//! GPU graph execution helpers for `GpuLayerRenderer`.

use std::error::Error;

use crate::model::Params;

use super::graph_ops::{DecodeBuffers, GraphBuffers, GraphOpUniforms};
use super::GpuLayerRenderer;

impl GpuLayerRenderer {
    /// Ensure aliased node-output buffers are allocated once for the active graph.
    pub(crate) fn ensure_node_alias_buffers(
        &mut self,
        luma_slots: usize,
        mask_slots: usize,
    ) -> Result<(), Box<dyn Error>> {
        if self.node_alias_luma_buffers.len() != luma_slots {
            self.node_alias_luma_buffers = (0..luma_slots)
                .map(|slot| {
                    self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(&format!("v2 node alias luma {slot}")),
                        size: self.output_size,
                        usage: wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_SRC
                            | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    })
                })
                .collect();
        }
        if self.node_alias_mask_buffers.len() != mask_slots {
            self.node_alias_mask_buffers = (0..mask_slots)
                .map(|slot| {
                    self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(&format!("v2 node alias mask {slot}")),
                        size: self.output_size,
                        usage: wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_SRC
                            | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    })
                })
                .collect();
        }
        Ok(())
    }

    /// Dispatch one fractal layer and write normalized luma into an alias output slot.
    pub(crate) fn render_generate_layer_to_alias(
        &mut self,
        params: &Params,
        input_base_slot: Option<usize>,
        output_slot: usize,
        opacity: f32,
        blend_mode: u32,
        contrast: f32,
    ) -> Result<(), Box<dyn Error>> {
        self.validate_params(params)?;
        let output = self.alias_luma(output_slot)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("v2 graph generate encoder"),
            });

        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(params));
        self.dispatch_main_pass(&mut encoder, params);
        let decode_buffers = DecodeBuffers {
            src_u32: &self.out_buffer,
            dst: &self.node_layer_temp_buffer,
        };
        self.graph_ops.encode_decode_layer(
            &self.device,
            &self.queue,
            &mut encoder,
            decode_buffers,
            self.width,
            self.height,
            contrast,
        );

        if let Some(base_slot) = input_base_slot {
            let base = self.alias_luma(base_slot)?;
            let blend_buffers = GraphBuffers {
                src0: base,
                src1: &self.node_layer_temp_buffer,
                src2: &self.node_layer_temp_buffer,
                dst: output,
            };
            let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
            uniforms.mode = blend_mode % 10;
            uniforms.p0 = opacity.clamp(0.0, 1.0);
            self.graph_ops.encode_blend(
                &self.device,
                &self.queue,
                &mut encoder,
                blend_buffers,
                uniforms,
            );
        } else {
            let copy_buffers = GraphBuffers {
                src0: &self.node_layer_temp_buffer,
                src1: &self.node_layer_temp_buffer,
                src2: &self.node_layer_temp_buffer,
                dst: output,
            };
            self.graph_ops.encode_copy(
                &self.device,
                &self.queue,
                &mut encoder,
                copy_buffers,
                self.width,
                self.height,
            );
        }

        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Render a deterministic value-noise source map into an alias slot.
    pub(crate) fn render_source_noise_to_alias(
        &mut self,
        output_mask: bool,
        output_slot: usize,
        seed: u32,
        scale: f32,
        octaves: u32,
        amplitude: f32,
    ) -> Result<(), Box<dyn Error>> {
        let output = if output_mask {
            self.alias_mask(output_slot)?
        } else {
            self.alias_luma(output_slot)?
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("v2 graph source noise encoder"),
            });
        // `source_noise` writes only `dst`, but the shared bind layout also exposes
        // read-only sources. Keep those bindings on a distinct scratch buffer so
        // `dst` is never bound as both STORAGE_READ and STORAGE_READ_WRITE.
        let buffers = GraphBuffers {
            src0: &self.node_layer_temp_buffer,
            src1: &self.node_layer_temp_buffer,
            src2: &self.node_layer_temp_buffer,
            dst: output,
        };
        let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
        uniforms.seed = seed;
        uniforms.octaves = octaves.max(1).min(8);
        uniforms.p0 = scale;
        uniforms.p1 = amplitude;
        self.graph_ops.encode_source_noise(
            &self.device,
            &self.queue,
            &mut encoder,
            buffers,
            uniforms,
        );
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Convert luma alias input into a mask alias output.
    pub(crate) fn render_mask_to_alias(
        &mut self,
        input_luma_slot: usize,
        output_mask_slot: usize,
        threshold: f32,
        softness: f32,
        invert: bool,
    ) -> Result<(), Box<dyn Error>> {
        let src = self.alias_luma(input_luma_slot)?;
        let dst = self.alias_mask(output_mask_slot)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("v2 graph mask encoder"),
            });
        let buffers = GraphBuffers {
            src0: src,
            src1: src,
            src2: src,
            dst,
        };
        let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
        uniforms.p0 = threshold;
        uniforms.p1 = softness;
        uniforms.flags = if invert { 0x2 } else { 0 };
        self.graph_ops
            .encode_mask(&self.device, &self.queue, &mut encoder, buffers, uniforms);
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Blend two luma alias inputs into a destination luma alias output.
    pub(crate) fn render_blend_to_alias(
        &mut self,
        base_slot: usize,
        top_slot: usize,
        mask_slot: Option<usize>,
        output_slot: usize,
        mode: u32,
        opacity: f32,
    ) -> Result<(), Box<dyn Error>> {
        let base = self.alias_luma(base_slot)?;
        let top = self.alias_luma(top_slot)?;
        let mask = match mask_slot {
            Some(slot) => self.alias_mask(slot)?,
            None => top,
        };
        let out = self.alias_luma(output_slot)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("v2 graph blend encoder"),
            });
        let buffers = GraphBuffers {
            src0: base,
            src1: top,
            src2: mask,
            dst: out,
        };
        let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
        uniforms.mode = mode % 10;
        uniforms.flags = u32::from(mask_slot.is_some());
        uniforms.p0 = opacity;
        self.graph_ops
            .encode_blend(&self.device, &self.queue, &mut encoder, buffers, uniforms);
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Apply tone mapping to one luma alias slot and write to output luma slot.
    pub(crate) fn render_tone_map_to_alias(
        &mut self,
        input_slot: usize,
        output_slot: usize,
        contrast: f32,
        low: f32,
        high: f32,
    ) -> Result<(), Box<dyn Error>> {
        let src = self.alias_luma(input_slot)?;
        let dst = self.alias_luma(output_slot)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("v2 graph tone map encoder"),
            });
        let buffers = GraphBuffers {
            src0: src,
            src1: src,
            src2: src,
            dst,
        };
        let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
        uniforms.p0 = contrast;
        uniforms.p1 = low;
        uniforms.p2 = high;
        self.graph_ops
            .encode_tone_map(&self.device, &self.queue, &mut encoder, buffers, uniforms);
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Apply warp/transform from one luma alias slot into another.
    pub(crate) fn render_warp_to_alias(
        &mut self,
        input_slot: usize,
        output_slot: usize,
        strength: f32,
        frequency: f32,
        phase: f32,
    ) -> Result<(), Box<dyn Error>> {
        let src = self.alias_luma(input_slot)?;
        let dst = self.alias_luma(output_slot)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("v2 graph warp encoder"),
            });
        let buffers = GraphBuffers {
            src0: src,
            src1: src,
            src2: src,
            dst,
        };
        let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
        uniforms.p0 = strength;
        uniforms.p1 = frequency;
        uniforms.p2 = phase;
        self.graph_ops
            .encode_warp(&self.device, &self.queue, &mut encoder, buffers, uniforms);
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Stage one luma alias slot as retained finalization input.
    pub(crate) fn stage_luma_alias_for_retained(
        &mut self,
        luma_slot: usize,
    ) -> Result<(), Box<dyn Error>> {
        let src = self.alias_luma(luma_slot)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("v2 graph output stage encoder"),
            });
        self.retained.encode_copy_from_luma(&mut encoder, src);
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    fn alias_luma(&self, slot: usize) -> Result<&wgpu::Buffer, Box<dyn Error>> {
        self.node_alias_luma_buffers
            .get(slot)
            .ok_or_else(|| format!("luma alias slot {slot} is out of range").into())
    }

    fn alias_mask(&self, slot: usize) -> Result<&wgpu::Buffer, Box<dyn Error>> {
        self.node_alias_mask_buffers
            .get(slot)
            .ok_or_else(|| format!("mask alias slot {slot} is out of range").into())
    }
}

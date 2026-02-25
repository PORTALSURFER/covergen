//! GPU graph execution helpers for `GpuLayerRenderer`.

use std::error::Error;

use crate::model::Params;
use crate::proc_graph::SopPrimitive;
use crate::sop::TopCameraRenderNode;

use super::graph_ops::{DecodeBuffers, GraphBuffers, GraphOpUniforms};
use super::{GpuLayerRenderer, GraphFrameContext};

impl GpuLayerRenderer {
    /// Start one frame-scoped graph execution context.
    pub(crate) fn begin_graph_frame(&self, label: &'static str) -> GraphFrameContext {
        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
        GraphFrameContext {
            encoder,
            encoded_ops: 0,
        }
    }

    /// Submit one recorded graph frame and return submit count (0 or 1).
    pub(crate) fn submit_graph_frame(&self, frame: GraphFrameContext) -> u32 {
        if frame.encoded_ops == 0 {
            return 0;
        }
        let mut encoder = frame.encoder;
        self.queue.submit(Some(encoder.finish()));
        1
    }

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

    /// Ensure persistent feedback buffers are allocated for stateful feedback nodes.
    pub(crate) fn ensure_node_feedback_buffers(
        &mut self,
        feedback_slots: usize,
    ) -> Result<(), Box<dyn Error>> {
        if self.node_feedback_buffers.len() != feedback_slots {
            self.node_feedback_buffers = (0..feedback_slots)
                .map(|slot| {
                    self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(&format!("v2 node feedback {slot}")),
                        size: self.output_size,
                        usage: wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_SRC
                            | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    })
                })
                .collect();
            self.reset_feedback_state()?;
        }
        Ok(())
    }

    /// Clear persistent stateful-feedback buffers before a new still/clip begins.
    pub(crate) fn reset_feedback_state(&mut self) -> Result<(), Box<dyn Error>> {
        if self.node_feedback_buffers.is_empty() {
            return Ok(());
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("v2 graph feedback reset encoder"),
            });
        for buffer in &self.node_feedback_buffers {
            encoder.copy_buffer_to_buffer(
                &self.node_feedback_clear_buffer,
                0,
                buffer,
                0,
                self.output_size,
            );
        }
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Dispatch one fractal layer and write normalized luma into an alias output slot.
    pub(crate) fn render_generate_layer_to_alias(
        &mut self,
        frame: &mut GraphFrameContext,
        params: &Params,
        input_base_slot: Option<usize>,
        output_slot: usize,
        opacity: f32,
        blend_mode: u32,
        contrast: f32,
    ) -> Result<(), Box<dyn Error>> {
        self.validate_params(params)?;
        let output = self.alias_luma(output_slot)?;

        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(params));
        self.dispatch_main_pass(&mut frame.encoder, params);
        let decode_buffers = DecodeBuffers {
            src_u32: &self.out_buffer,
            dst: &self.node_layer_temp_buffer,
        };
        self.graph_ops.encode_decode_layer(
            &self.device,
            &self.queue,
            &mut frame.encoder,
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
                &mut frame.encoder,
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
                &mut frame.encoder,
                copy_buffers,
                self.width,
                self.height,
            );
        }

        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Render a deterministic value-noise source map into an alias slot.
    pub(crate) fn render_source_noise_to_alias(
        &mut self,
        frame: &mut GraphFrameContext,
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
            &mut frame.encoder,
            buffers,
            uniforms,
        );
        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Convert luma alias input into a mask alias output.
    pub(crate) fn render_mask_to_alias(
        &mut self,
        frame: &mut GraphFrameContext,
        input_luma_slot: usize,
        output_mask_slot: usize,
        threshold: f32,
        softness: f32,
        invert: bool,
    ) -> Result<(), Box<dyn Error>> {
        let src = self.alias_luma(input_luma_slot)?;
        let dst = self.alias_mask(output_mask_slot)?;
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
            .encode_mask(&self.device, &self.queue, &mut frame.encoder, buffers, uniforms);
        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Blend two luma alias inputs into a destination luma alias output.
    pub(crate) fn render_blend_to_alias(
        &mut self,
        frame: &mut GraphFrameContext,
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
            .encode_blend(&self.device, &self.queue, &mut frame.encoder, buffers, uniforms);
        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Apply tone mapping to one luma alias slot and write to output luma slot.
    pub(crate) fn render_tone_map_to_alias(
        &mut self,
        frame: &mut GraphFrameContext,
        input_slot: usize,
        output_slot: usize,
        contrast: f32,
        low: f32,
        high: f32,
    ) -> Result<(), Box<dyn Error>> {
        let src = self.alias_luma(input_slot)?;
        let dst = self.alias_luma(output_slot)?;
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
            .encode_tone_map(&self.device, &self.queue, &mut frame.encoder, buffers, uniforms);
        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Apply warp/transform from one luma alias slot into another.
    pub(crate) fn render_warp_to_alias(
        &mut self,
        frame: &mut GraphFrameContext,
        input_slot: usize,
        output_slot: usize,
        strength: f32,
        frequency: f32,
        phase: f32,
    ) -> Result<(), Box<dyn Error>> {
        let src = self.alias_luma(input_slot)?;
        let dst = self.alias_luma(output_slot)?;
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
            .encode_warp(&self.device, &self.queue, &mut frame.encoder, buffers, uniforms);
        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Blend current input with persistent feedback state and update feedback memory.
    pub(crate) fn render_stateful_feedback_to_alias(
        &mut self,
        frame: &mut GraphFrameContext,
        input_slot: usize,
        output_slot: usize,
        feedback_slot: usize,
        mix: f32,
    ) -> Result<(), Box<dyn Error>> {
        let input = self.alias_luma(input_slot)?;
        let output = self.alias_luma(output_slot)?;
        let feedback = self.alias_feedback(feedback_slot)?;
        let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
        uniforms.p0 = mix.clamp(0.0, 1.0);
        self.graph_ops.encode_feedback_mix(
            &self.device,
            &self.queue,
            &mut frame.encoder,
            GraphBuffers {
                src0: input,
                src1: feedback,
                src2: feedback,
                dst: output,
            },
            uniforms,
        );
        self.graph_ops.encode_copy(
            &self.device,
            &self.queue,
            &mut frame.encoder,
            GraphBuffers {
                src0: output,
                src1: output,
                src2: output,
                dst: feedback,
            },
            self.width,
            self.height,
        );
        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Render a SOP primitive with camera controls directly into a luma alias slot.
    pub(crate) fn render_top_camera_to_alias(
        &mut self,
        frame: &mut GraphFrameContext,
        primitive: SopPrimitive,
        camera: TopCameraRenderNode,
        channel_mod: Option<f32>,
        output_slot: usize,
    ) -> Result<(), Box<dyn Error>> {
        let output = self.alias_luma(output_slot)?;
        let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
        uniforms.p0 = camera.exposure;
        uniforms.p1 = camera.gamma;
        uniforms.p2 = camera.zoom;
        uniforms.p3 = camera.pan_x;
        uniforms.p4 = camera.pan_y;
        uniforms.p5 = camera.rotate;
        uniforms.p6 = channel_mod.unwrap_or(1.0).clamp(0.2, 3.0);
        uniforms.flags = u32::from(camera.invert);
        match primitive {
            SopPrimitive::Circle(circle) => {
                uniforms.mode = 0;
                uniforms.p7 = circle.radius;
                uniforms.p8 = circle.feather;
                uniforms.p9 = circle.center_x;
                uniforms.p10 = circle.center_y;
            }
            SopPrimitive::Sphere(sphere) => {
                uniforms.mode = 1;
                uniforms.p7 = sphere.radius;
                uniforms.p8 = sphere.center_x;
                uniforms.p9 = sphere.center_y;
                uniforms.p10 = sphere.light_x;
                uniforms.p11 = sphere.light_y;
                uniforms.p12 = sphere.deform;
                uniforms.p13 = sphere.deform_freq;
                uniforms.p14 = sphere.deform_phase;
                uniforms.octaves = sphere.ambient.to_bits();
            }
        }
        self.graph_ops.encode_top_camera(
            &self.device,
            &self.queue,
            &mut frame.encoder,
            GraphBuffers {
                src0: &self.node_layer_temp_buffer,
                src1: &self.node_layer_temp_buffer,
                src2: &self.node_layer_temp_buffer,
                dst: output,
            },
            uniforms,
        );
        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Stage one luma alias slot as retained finalization input.
    pub(crate) fn stage_luma_alias_for_retained(
        &mut self,
        frame: &mut GraphFrameContext,
        luma_slot: usize,
    ) -> Result<(), Box<dyn Error>> {
        let src = self.alias_luma(luma_slot)?;
        self.retained.encode_copy_from_luma(&mut frame.encoder, src);
        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Explicitly composite primary and tap outputs before retained finalization.
    pub(crate) fn compose_outputs_to_retained(
        &mut self,
        frame: &mut GraphFrameContext,
        primary_slot: usize,
        tap_slots: &[(u8, usize)],
    ) -> Result<(), Box<dyn Error>> {
        if tap_slots.is_empty() {
            return self.stage_luma_alias_for_retained(frame, primary_slot);
        }

        let primary = self.alias_luma(primary_slot)?;
        self.graph_ops.encode_copy(
            &self.device,
            &self.queue,
            &mut frame.encoder,
            GraphBuffers {
                src0: primary,
                src1: primary,
                src2: primary,
                dst: &self.node_layer_temp_buffer,
            },
            self.width,
            self.height,
        );

        let mut active = &self.node_layer_temp_buffer;
        let mut scratch = &self.node_composite_temp_buffer;
        for (index, (tap_slot, alias_slot)) in tap_slots.iter().enumerate() {
            let tap = self.alias_luma(*alias_slot)?;
            let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
            uniforms.mode = 3; // Screen blend keeps taps additive but avoids hard clipping.
            uniforms.p0 = compositor_tap_opacity(index, *tap_slot);
            self.graph_ops.encode_blend(
                &self.device,
                &self.queue,
                &mut frame.encoder,
                GraphBuffers {
                    src0: active,
                    src1: tap,
                    src2: tap,
                    dst: scratch,
                },
                uniforms,
            );
            std::mem::swap(&mut active, &mut scratch);
        }

        self.retained.encode_copy_from_luma(&mut frame.encoder, active);
        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
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

    fn alias_feedback(&self, slot: usize) -> Result<&wgpu::Buffer, Box<dyn Error>> {
        self.node_feedback_buffers
            .get(slot)
            .ok_or_else(|| format!("feedback slot {slot} is out of range").into())
    }
}

fn compositor_tap_opacity(index: usize, tap_slot: u8) -> f32 {
    let order_decay = 0.20 / ((index + 1) as f32).sqrt();
    let slot_bias = (tap_slot % 3) as f32 * 0.02;
    (order_decay + slot_bias).clamp(0.08, 0.28)
}

//! GPU graph execution helpers for `GpuLayerRenderer`.

use std::error::Error;

use crate::model::Params;
use crate::proc_graph::SopPrimitive;
use crate::sop::TopCameraRenderNode;

use super::graph_ops::{DecodeBuffers, DecodeLayerDispatch, GraphBuffers, GraphOpUniforms};
use super::{
    BlendAliasDispatch, GenerateLayerAliasDispatch, GpuLayerRenderer, GraphFrameContext,
    GraphSubmitStats, SourceNoiseAliasDispatch,
};

impl GpuLayerRenderer {
    /// Start one frame-scoped graph execution context.
    pub(crate) fn begin_graph_frame(&mut self, label: &'static str) -> GraphFrameContext {
        self.graph_ops.begin_frame();
        self.main_pass_timestamps.begin_frame();
        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
        GraphFrameContext {
            encoder,
            encoded_ops: 0,
            upload_bytes: 0,
        }
    }

    /// Submit one recorded graph frame and return submit count (0 or 1).
    pub(crate) fn submit_graph_frame(&mut self, mut frame: GraphFrameContext) -> GraphSubmitStats {
        let bind_group_creates = self.graph_ops.frame_bind_group_creates();
        if frame.encoded_ops == 0 {
            return GraphSubmitStats {
                submit_count: 0,
                upload_bytes: frame.upload_bytes,
                bind_group_creates,
            };
        }
        self.main_pass_timestamps
            .resolve_and_reset(&mut frame.encoder);
        let encoder = frame.encoder;
        self.queue.submit(Some(encoder.finish()));
        GraphSubmitStats {
            submit_count: 1,
            upload_bytes: frame.upload_bytes,
            bind_group_creates,
        }
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
        dispatch: GenerateLayerAliasDispatch,
    ) -> Result<(), Box<dyn Error>> {
        self.validate_params(params)?;

        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(params));
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(std::mem::size_of::<Params>() as u64);
        self.dispatch_main_pass(&mut frame.encoder, params);
        let decode_buffers = DecodeBuffers {
            src_u32: &self.out_buffer,
            dst: &self.node_layer_temp_buffer,
        };
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(self.graph_ops.encode_decode_layer(
                &self.device,
                &self.queue,
                &mut frame.encoder,
                DecodeLayerDispatch {
                    buffers: decode_buffers,
                    width: self.width,
                    height: self.height,
                    contrast: dispatch.contrast,
                },
            ));

        if let Some(base_slot) = dispatch.input_base_slot {
            let base = self.alias_luma(base_slot)?;
            let blend_buffers = GraphBuffers {
                src0: base,
                src1: &self.node_layer_temp_buffer,
                src2: &self.node_layer_temp_buffer,
                dst: self.alias_luma(dispatch.output_slot)?,
            };
            let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
            uniforms.mode = dispatch.blend_mode % 10;
            uniforms.p0 = dispatch.opacity.clamp(0.0, 1.0);
            frame.upload_bytes = frame
                .upload_bytes
                .saturating_add(self.graph_ops.encode_blend(
                    &self.device,
                    &self.queue,
                    &mut frame.encoder,
                    blend_buffers,
                    uniforms,
                ));
        } else {
            let copy_buffers = GraphBuffers {
                src0: &self.node_layer_temp_buffer,
                src1: &self.node_layer_temp_buffer,
                src2: &self.node_layer_temp_buffer,
                dst: self.alias_luma(dispatch.output_slot)?,
            };
            frame.upload_bytes = frame
                .upload_bytes
                .saturating_add(self.graph_ops.encode_copy(
                    &self.device,
                    &self.queue,
                    &mut frame.encoder,
                    copy_buffers,
                    self.width,
                    self.height,
                ));
        }

        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Render a deterministic value-noise source map into an alias slot.
    pub(crate) fn render_source_noise_to_alias(
        &mut self,
        frame: &mut GraphFrameContext,
        dispatch: SourceNoiseAliasDispatch,
    ) -> Result<(), Box<dyn Error>> {
        let output = if dispatch.output_mask {
            self.alias_mask(dispatch.output_slot)?
        } else {
            self.alias_luma(dispatch.output_slot)?
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
        uniforms.seed = dispatch.seed;
        uniforms.octaves = dispatch.octaves.clamp(1, 8);
        uniforms.p0 = dispatch.scale;
        uniforms.p1 = dispatch.amplitude;
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(self.graph_ops.encode_source_noise(
                &self.device,
                &self.queue,
                &mut frame.encoder,
                buffers,
                uniforms,
            ));
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
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(self.graph_ops.encode_mask(
                &self.device,
                &self.queue,
                &mut frame.encoder,
                buffers,
                uniforms,
            ));
        frame.encoded_ops = frame.encoded_ops.saturating_add(1);
        Ok(())
    }

    /// Blend two luma alias inputs into a destination luma alias output.
    pub(crate) fn render_blend_to_alias(
        &mut self,
        frame: &mut GraphFrameContext,
        dispatch: BlendAliasDispatch,
    ) -> Result<(), Box<dyn Error>> {
        let base = self.alias_luma(dispatch.base_slot)?;
        let top = self.alias_luma(dispatch.top_slot)?;
        let mask = match dispatch.mask_slot {
            Some(slot) => self.alias_mask(slot)?,
            None => top,
        };
        let out = self.alias_luma(dispatch.output_slot)?;
        let buffers = GraphBuffers {
            src0: base,
            src1: top,
            src2: mask,
            dst: out,
        };
        let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
        uniforms.mode = dispatch.mode % 10;
        uniforms.flags = u32::from(dispatch.mask_slot.is_some());
        uniforms.p0 = dispatch.opacity;
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(self.graph_ops.encode_blend(
                &self.device,
                &self.queue,
                &mut frame.encoder,
                buffers,
                uniforms,
            ));
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
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(self.graph_ops.encode_tone_map(
                &self.device,
                &self.queue,
                &mut frame.encoder,
                buffers,
                uniforms,
            ));
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
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(self.graph_ops.encode_warp(
                &self.device,
                &self.queue,
                &mut frame.encoder,
                buffers,
                uniforms,
            ));
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
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(self.graph_ops.encode_feedback_mix(
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
            ));
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(self.graph_ops.encode_copy(
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
            ));
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
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(self.graph_ops.encode_top_camera(
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
            ));
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
        frame.upload_bytes = frame
            .upload_bytes
            .saturating_add(self.graph_ops.encode_copy(
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
            ));

        let mut active = &self.node_layer_temp_buffer;
        let mut scratch = &self.node_composite_temp_buffer;
        for (index, (tap_slot, alias_slot)) in tap_slots.iter().enumerate() {
            let tap = self.alias_luma(*alias_slot)?;
            let mut uniforms = GraphOpUniforms::sized(self.width, self.height);
            uniforms.mode = 3; // Screen blend keeps taps additive but avoids hard clipping.
            uniforms.p0 = compositor_tap_opacity(index, *tap_slot);
            frame.upload_bytes = frame
                .upload_bytes
                .saturating_add(self.graph_ops.encode_blend(
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
                ));
            std::mem::swap(&mut active, &mut scratch);
        }

        self.retained
            .encode_copy_from_luma(&mut frame.encoder, active);
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

#[cfg(test)]
mod tests {
    use super::compositor_tap_opacity;

    #[test]
    fn compositor_tap_opacity_stays_in_clamp_range() {
        for index in [0usize, 1, 2, 8, 32, 256, 2048] {
            for slot in 0u8..=8 {
                let value = compositor_tap_opacity(index, slot);
                assert!(
                    (0.08..=0.28).contains(&value),
                    "opacity should stay clamped: index={index} slot={slot} value={value}"
                );
            }
        }
    }

    #[test]
    fn compositor_tap_opacity_decays_with_tap_order() {
        let first = compositor_tap_opacity(0, 0);
        let later = compositor_tap_opacity(9, 0);
        let far_later = compositor_tap_opacity(999, 0);
        assert!(
            first > later,
            "expected first tap to have higher opacity: first={first} later={later}"
        );
        assert!(
            later >= far_later,
            "expected opacity to be non-increasing with order: later={later} far_later={far_later}"
        );
        assert!((far_later - 0.08).abs() < 1e-6);
    }

    #[test]
    fn compositor_tap_opacity_slot_bias_matches_expected_steps() {
        let base = compositor_tap_opacity(0, 0);
        let biased = compositor_tap_opacity(0, 2);
        assert!((base - 0.20).abs() < 1e-6);
        assert!((biased - 0.24).abs() < 1e-6);
    }
}

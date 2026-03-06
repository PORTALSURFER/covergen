//! Render-pass encoding helpers for staged tex-preview execution.

use super::*;

impl TexPreviewRenderer {
    /// Encode one render pass for one planned operation.
    pub(super) fn encode_pass_for_op(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        prepared: PreparedRenderOp,
        runtime_ops: &[TexViewerOp],
        source_target: Option<RenderTargetRef>,
    ) -> Option<()> {
        let clear_color = self.op_clear_color_for_planned(runtime_ops, prepared.planned_op);
        let timestamp_parts = self.op_pass_timestamps.next_render_pass_parts();
        let timestamp_writes = timestamp_parts.as_ref().map(|(query_set, begin, end)| {
            wgpu::RenderPassTimestampWrites {
                query_set: query_set.as_ref(),
                beginning_of_pass_write_index: Some(*begin),
                end_of_pass_write_index: Some(*end),
            }
        });
        let target_view = self.target_view(prepared.target)?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gui-tex-preview-op-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes,
        });
        match prepared.planned_op {
            PlannedRenderOp::Runtime { .. } => {
                let runtime_op = self.runtime_op_for_planned(runtime_ops, prepared.planned_op)?;
                match runtime_op {
                    TexViewerOp::Blend {
                        base_texture_node_id,
                        layer_texture_node_id,
                        ..
                    } => {
                        let base_bind_group = self
                            .blend_source_bind_group_for_texture(base_texture_node_id)
                            .or_else(|| {
                                source_target
                                    .and_then(|target_ref| self.target_bind_group(target_ref))
                            })?;
                        let layer_bind_group = layer_texture_node_id
                            .and_then(|id| self.blend_source_bind_group_for_texture(id))
                            .unwrap_or(self.dummy_bind_group.as_ref()?);
                        pass.set_pipeline(self.op_blend_pipeline.as_ref()?);
                        pass.set_bind_group(
                            0,
                            &self.op_uniform_bind_group,
                            &[prepared.dynamic_offset],
                        );
                        pass.set_bind_group(1, base_bind_group, &[]);
                        pass.set_bind_group(2, layer_bind_group, &[]);
                    }
                    TexViewerOp::DomainWarp {
                        base_texture_node_id,
                        warp_texture_node_id,
                        ..
                    } => {
                        let base_bind_group = self
                            .blend_source_bind_group_for_texture(base_texture_node_id)
                            .or_else(|| {
                                source_target
                                    .and_then(|target_ref| self.target_bind_group(target_ref))
                            })?;
                        let warp_bind_group = warp_texture_node_id
                            .and_then(|id| self.blend_source_bind_group_for_texture(id))
                            .unwrap_or(base_bind_group);
                        pass.set_pipeline(self.op_domain_warp_pipeline.as_ref()?);
                        pass.set_bind_group(
                            0,
                            &self.op_uniform_bind_group,
                            &[prepared.dynamic_offset],
                        );
                        pass.set_bind_group(1, base_bind_group, &[]);
                        pass.set_bind_group(2, warp_bind_group, &[]);
                    }
                    TexViewerOp::StoreTexture { .. } => {
                        return None;
                    }
                    _ => {
                        let descriptor = runtime_op_descriptor(runtime_op)?;
                        self.encode_runtime_pass_with_descriptor(
                            &mut pass,
                            descriptor,
                            source_target,
                            prepared.feedback_history_key,
                            prepared.dynamic_offset,
                        )?;
                    }
                }
            }
            PlannedRenderOp::TransformPair { .. } => {
                let src_target = source_target?;
                let src_bind_group = self.target_bind_group(src_target)?;
                pass.set_pipeline(self.op_transform_fused_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, src_bind_group, &[]);
                pass.set_bind_group(2, self.dummy_bind_group.as_ref()?, &[]);
            }
        }
        pass.draw(0..6, 0..1);
        Some(())
    }

    fn runtime_pipeline(&self, pipeline: RuntimeOpPipelineKind) -> Option<&wgpu::RenderPipeline> {
        match pipeline {
            RuntimeOpPipelineKind::Solid => self.op_solid_pipeline.as_ref(),
            RuntimeOpPipelineKind::Circle => self.op_circle_pipeline.as_ref(),
            RuntimeOpPipelineKind::Box => self.op_box_pipeline.as_ref(),
            RuntimeOpPipelineKind::Grid => self.op_grid_pipeline.as_ref(),
            RuntimeOpPipelineKind::Sphere => self.op_sphere_pipeline.as_ref(),
            RuntimeOpPipelineKind::SourceNoise => self.op_source_noise_pipeline.as_ref(),
            RuntimeOpPipelineKind::Transform => self.op_transform_pipeline.as_ref(),
            RuntimeOpPipelineKind::Level => self.op_level_pipeline.as_ref(),
            RuntimeOpPipelineKind::Mask => self.op_mask_pipeline.as_ref(),
            RuntimeOpPipelineKind::Morphology => self.op_morphology_pipeline.as_ref(),
            RuntimeOpPipelineKind::ToneMap => self.op_tone_map_pipeline.as_ref(),
            RuntimeOpPipelineKind::Feedback => self.op_feedback_pipeline.as_ref(),
            RuntimeOpPipelineKind::ReactionDiffusion => {
                self.op_reaction_diffusion_pipeline.as_ref()
            }
            RuntimeOpPipelineKind::DirectionalSmear => self.op_directional_smear_pipeline.as_ref(),
            RuntimeOpPipelineKind::WarpTransform => self.op_warp_transform_pipeline.as_ref(),
            RuntimeOpPipelineKind::PostProcess => self.op_post_process_pipeline.as_ref(),
        }
    }

    fn runtime_source_bind_group(
        &self,
        source: RuntimeSourceBinding,
        source_target: Option<RenderTargetRef>,
    ) -> Option<&wgpu::BindGroup> {
        match source {
            RuntimeSourceBinding::Dummy => self.dummy_bind_group.as_ref(),
            RuntimeSourceBinding::SourceTarget => {
                let source_target = source_target?;
                self.target_bind_group(source_target)
            }
        }
    }

    fn runtime_feedback_bind_group(
        &self,
        feedback: RuntimeFeedbackBinding,
        feedback_history_key: Option<FeedbackHistoryKey>,
    ) -> Option<&wgpu::BindGroup> {
        match feedback {
            RuntimeFeedbackBinding::Dummy => self.dummy_bind_group.as_ref(),
            RuntimeFeedbackBinding::HistoryRequired => {
                let history_key = feedback_history_key?;
                self.feedback_history_read_bind_group(history_key)
            }
        }
    }

    fn encode_runtime_pass_with_descriptor<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        descriptor: RuntimeOpDescriptor,
        source_target: Option<RenderTargetRef>,
        feedback_history_key: Option<FeedbackHistoryKey>,
        dynamic_offset: u32,
    ) -> Option<()> {
        pass.set_pipeline(self.runtime_pipeline(descriptor.pipeline)?);
        pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
        let source_bind_group =
            self.runtime_source_bind_group(descriptor.source_binding, source_target)?;
        let feedback_bind_group =
            self.runtime_feedback_bind_group(descriptor.feedback_binding, feedback_history_key)?;
        pass.set_bind_group(1, source_bind_group, &[]);
        pass.set_bind_group(2, feedback_bind_group, &[]);
        Some(())
    }
}

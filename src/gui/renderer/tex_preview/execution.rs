//! Frame-time tex preview preparation and GPU operation execution.

use crate::gui::geometry::Rect;
use crate::gui::tex_view::{TexViewerFrame, TexViewerOp, TexViewerPayload};
use std::collections::HashMap;

use super::super::viewer;
use super::execution_plan::{build_execution_plan, PlannedRenderOp, PlannedStep, TransformParams};
use super::pipeline::create_preview_texture_bundle;
use super::{
    CachedTextureSlot, FeedbackHistoryKey, FeedbackHistorySlot, RenderTargetRef, TexOpUniform,
    TexPreviewRenderer, PREVIEW_BG, TEX_PREVIEW_TEXTURE_FORMAT,
};

const TRANSPARENT_BG: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

const TEX_OP_UNIFORM_SIZE: usize = std::mem::size_of::<TexOpUniform>();

/// Deferred history write request for feedback nodes that bind external
/// accumulation textures.
#[derive(Clone, Copy, Debug)]
struct PendingExternalFeedbackWrite {
    history_key: FeedbackHistoryKey,
    fallback_source_target: RenderTargetRef,
}

/// Mutable per-frame dispatch state shared across staged execution helpers.
#[derive(Debug, Default)]
struct GpuDispatchState {
    source_target: Option<RenderTargetRef>,
    scratch_flip: bool,
    rendered_count: usize,
    pending_external_feedback_writes: HashMap<u32, Vec<PendingExternalFeedbackWrite>>,
}

/// Render-pass inputs resolved during target preparation.
#[derive(Clone, Copy, Debug)]
struct PreparedRenderOp {
    planned_op: PlannedRenderOp,
    feedback_history_key: Option<FeedbackHistoryKey>,
    target: RenderTargetRef,
    dynamic_offset: u32,
}

/// Shared per-frame context passed through staged dispatch helpers.
struct DispatchContext<'a> {
    device: &'a wgpu::Device,
    encoder: &'a mut wgpu::CommandEncoder,
    width: u32,
    height: u32,
}

impl TexPreviewRenderer {
    /// Prepare viewer resources and content for the current frame.
    pub(in crate::gui::renderer) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        tex_view: Option<TexViewerFrame<'_>>,
        export_preview_rect: Option<Rect>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> u64 {
        let Some(tex_view) = tex_view else {
            self.viewer_visible = false;
            self.export_preview_visible = false;
            return 0;
        };
        if tex_view.width == 0
            || tex_view.height == 0
            || tex_view.texture_width == 0
            || tex_view.texture_height == 0
        {
            self.viewer_visible = false;
            self.export_preview_visible = false;
            return 0;
        }
        let mut upload_bytes = 0u64;
        self.ensure_viewer_texture(device, tex_view.texture_width, tex_view.texture_height);
        let rect = Rect::new(
            tex_view.x,
            tex_view.y,
            tex_view.width as i32,
            tex_view.height as i32,
        );
        if self.cached_viewer_quad_rect != Some(rect) {
            let quad = viewer::quad_vertices(rect);
            upload_bytes = upload_bytes.saturating_add(std::mem::size_of_val(&quad) as u64);
            queue.write_buffer(&self.viewer_quad_buffer, 0, bytemuck::cast_slice(&quad));
            self.cached_viewer_quad_rect = Some(rect);
        }
        if let Some(preview_rect) = export_preview_rect {
            if preview_rect.w > 1 && preview_rect.h > 1 {
                if self.cached_export_preview_quad_rect != Some(preview_rect) {
                    let preview_quad = viewer::quad_vertices(preview_rect);
                    upload_bytes =
                        upload_bytes.saturating_add(std::mem::size_of_val(&preview_quad) as u64);
                    queue.write_buffer(
                        &self.export_preview_quad_buffer,
                        0,
                        bytemuck::cast_slice(&preview_quad),
                    );
                    self.cached_export_preview_quad_rect = Some(preview_rect);
                }
                self.export_preview_visible = true;
            } else {
                self.export_preview_visible = false;
                self.cached_export_preview_quad_rect = None;
            }
        } else {
            self.export_preview_visible = false;
            self.cached_export_preview_quad_rect = None;
        }

        let TexViewerPayload::GpuOps(ops) = tex_view.payload;
        if let Some(op_upload_bytes) = self.encode_gpu_ops(
            device,
            queue,
            encoder,
            ops,
            tex_view.texture_width,
            tex_view.texture_height,
            tex_view.ops_signature,
        ) {
            upload_bytes = upload_bytes.saturating_add(op_upload_bytes);
        } else {
            self.clear_viewer_target(encoder);
        }
        self.viewer_visible = true;
        upload_bytes
    }

    fn encode_gpu_ops(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        ops: &[TexViewerOp],
        width: u32,
        height: u32,
        ops_signature: u64,
    ) -> Option<u64> {
        if ops.is_empty() {
            return None;
        }
        self.op_pass_timestamps.begin_frame();
        let result = self.encode_gpu_ops_staged(
            device,
            queue,
            encoder,
            ops,
            width,
            height,
            ops_signature,
        );
        self.op_pass_timestamps.resolve_and_reset(encoder);
        result
    }

    /// Run staged tex-op dispatch: plan -> prepare targets -> encode passes -> finalize.
    fn encode_gpu_ops_staged(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        ops: &[TexViewerOp],
        width: u32,
        height: u32,
        ops_signature: u64,
    ) -> Option<u64> {
        let upload_bytes =
            self.plan_gpu_dispatch(device, queue, ops, width, height, ops_signature)?;
        let planned_steps = std::mem::take(&mut self.cached_plan_steps);
        let planned_render_ops = std::mem::take(&mut self.cached_plan_render_ops);
        let mut dispatch = DispatchContext {
            device,
            encoder,
            width,
            height,
        };
        let mut state = GpuDispatchState::default();
        for step in planned_steps.iter().copied() {
            self.dispatch_planned_step(
                &mut dispatch,
                step,
                planned_render_ops.as_slice(),
                &mut state,
            )?;
        }
        self.cached_plan_steps = planned_steps;
        self.cached_plan_render_ops = planned_render_ops;
        self.flush_pending_external_feedback_writes(
            dispatch.encoder,
            &mut state,
            dispatch.width,
            dispatch.height,
        )?;
        self.finalize_gpu_dispatch(
            dispatch.encoder,
            state.source_target,
            dispatch.width,
            dispatch.height,
        )?;
        Some(upload_bytes)
    }

    /// Build a render plan and allocate resources needed for this frame.
    fn plan_gpu_dispatch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ops: &[TexViewerOp],
        width: u32,
        height: u32,
        ops_signature: u64,
    ) -> Option<u64> {
        self.blend_source_aliases.clear();
        self.blend_alias_count_scratch_a = 0;
        self.blend_alias_count_scratch_b = 0;
        if self.cached_plan_signature != Some(ops_signature) {
            self.cached_plan_ops.clear();
            self.cached_plan_ops.extend_from_slice(ops);
            self.cached_plan_steps.clear();
            self.cached_plan_render_ops.clear();
            build_execution_plan(
                ops,
                &mut self.cached_plan_steps,
                &mut self.cached_plan_render_ops,
            );
            self.cached_plan_signature = Some(ops_signature);
            self.op_uniform_signature = None;
        }
        if self.cached_plan_render_ops.is_empty() {
            return None;
        }
        self.ensure_op_pipelines(device);
        self.ensure_dummy_bind_group(device);
        let render_op_count = self.cached_plan_render_ops.len();
        self.ensure_op_uniform_capacity(device, render_op_count);
        let upload_bytes = if self.op_uniform_signature != Some(ops_signature) {
            let render_ops = std::mem::take(&mut self.cached_plan_render_ops);
            let bytes = self.write_planned_op_uniforms(queue, render_ops.as_slice());
            self.cached_plan_render_ops = render_ops;
            self.op_uniform_signature = Some(ops_signature);
            bytes
        } else {
            0
        };
        if render_op_count > 1 {
            self.ensure_scratch_textures(device, width, height);
        }
        Some(upload_bytes)
    }

    /// Apply one planned step, including store-texture aliases and render passes.
    fn dispatch_planned_step(
        &mut self,
        dispatch: &mut DispatchContext<'_>,
        step: PlannedStep,
        render_ops: &[PlannedRenderOp],
        state: &mut GpuDispatchState,
    ) -> Option<()> {
        let render_index = match step {
            PlannedStep::Render { render_index } => render_index,
            PlannedStep::StoreTexture { texture_node_id } => {
                let src_target = state.source_target?;
                self.bind_blend_source_alias(texture_node_id, src_target);
                self.resolve_external_feedback_write_for_texture(
                    dispatch.encoder,
                    state,
                    texture_node_id,
                    src_target,
                    dispatch.width,
                    dispatch.height,
                )?;
                return Some(());
            }
        };
        let planned_op = *render_ops.get(render_index)?;
        let source_target_before = state.source_target;
        let prepared =
            self.prepare_targets_for_op(dispatch, planned_op, render_ops.len(), state)?;
        self.encode_pass_for_op(dispatch.encoder, prepared, source_target_before)?;
        if Self::is_feedback_history_tap_op(prepared.planned_op) {
            let history_key = prepared.feedback_history_key?;
            let source_target = source_target_before?;
            let frame_gap = Self::feedback_frame_gap_for_planned(prepared.planned_op);
            if self.should_write_feedback_history(history_key, frame_gap)? {
                if let Some(texture_node_id) =
                    Self::external_feedback_accumulation_texture(prepared.planned_op)
                {
                    state
                        .pending_external_feedback_writes
                        .entry(texture_node_id)
                        .or_default()
                        .push(PendingExternalFeedbackWrite {
                            history_key,
                            fallback_source_target: source_target,
                        });
                } else {
                    let history_write_target = self.feedback_history_write_target(history_key)?;
                    self.copy_target_to_target(
                        dispatch.encoder,
                        source_target,
                        history_write_target,
                        dispatch.width,
                        dispatch.height,
                    );
                    self.swap_feedback_history(history_key)?;
                }
            }
            state.source_target = Some(prepared.target);
        } else {
            state.source_target = if let Some(history_key) = prepared.feedback_history_key {
                self.swap_feedback_history(history_key)
            } else {
                Some(prepared.target)
            };
        }
        state.rendered_count = state.rendered_count.saturating_add(1);
        Some(())
    }

    /// Resolve render target routing and per-op dynamic uniform offset.
    fn prepare_targets_for_op(
        &mut self,
        dispatch: &mut DispatchContext<'_>,
        planned_op: PlannedRenderOp,
        render_op_count: usize,
        state: &mut GpuDispatchState,
    ) -> Option<PreparedRenderOp> {
        let feedback_history_key = Self::feedback_key_for_planned(planned_op);
        if let Some(history_key) = feedback_history_key {
            self.ensure_feedback_history_slot(
                dispatch.device,
                dispatch.encoder,
                history_key,
                dispatch.width,
                dispatch.height,
            );
        }
        let last = state.rendered_count.saturating_add(1) == render_op_count;
        let mut target = if last {
            RenderTargetRef::Viewer
        } else {
            self.choose_intermediate_target(&mut state.scratch_flip)
        };
        if feedback_history_key.is_some() && !Self::is_feedback_history_tap_op(planned_op) {
            let history_key = feedback_history_key?;
            target = self.feedback_history_write_target(history_key)?;
        }
        self.materialize_blend_source_aliases_for_target(
            dispatch.device,
            dispatch.encoder,
            target,
            dispatch.width,
            dispatch.height,
        );
        let uniform_offset = self.op_uniform_offset(state.rendered_count);
        let dynamic_offset = u32::try_from(uniform_offset).ok()?;
        Some(PreparedRenderOp {
            planned_op,
            feedback_history_key,
            target,
            dynamic_offset,
        })
    }

    /// Encode one render pass for one planned operation.
    fn encode_pass_for_op(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        prepared: PreparedRenderOp,
        source_target: Option<RenderTargetRef>,
    ) -> Option<()> {
        let clear_color = Self::op_clear_color_for_planned(prepared.planned_op);
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
            PlannedRenderOp::Runtime(TexViewerOp::Solid { .. }) => {
                pass.set_pipeline(self.op_solid_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, self.dummy_bind_group.as_ref()?, &[]);
                pass.set_bind_group(2, self.dummy_bind_group.as_ref()?, &[]);
            }
            PlannedRenderOp::Runtime(TexViewerOp::Circle { .. }) => {
                pass.set_pipeline(self.op_circle_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, self.dummy_bind_group.as_ref()?, &[]);
                pass.set_bind_group(2, self.dummy_bind_group.as_ref()?, &[]);
            }
            PlannedRenderOp::Runtime(TexViewerOp::Sphere { .. }) => {
                pass.set_pipeline(self.op_sphere_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, self.dummy_bind_group.as_ref()?, &[]);
                pass.set_bind_group(2, self.dummy_bind_group.as_ref()?, &[]);
            }
            PlannedRenderOp::Runtime(TexViewerOp::Transform { .. }) => {
                let src_target = source_target?;
                let src_bind_group = self.target_bind_group(src_target)?;
                pass.set_pipeline(self.op_transform_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, src_bind_group, &[]);
                pass.set_bind_group(2, self.dummy_bind_group.as_ref()?, &[]);
            }
            PlannedRenderOp::Runtime(TexViewerOp::Level { .. }) => {
                let src_target = source_target?;
                let src_bind_group = self.target_bind_group(src_target)?;
                pass.set_pipeline(self.op_level_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, src_bind_group, &[]);
                pass.set_bind_group(2, self.dummy_bind_group.as_ref()?, &[]);
            }
            PlannedRenderOp::TransformPair { .. } => {
                let src_target = source_target?;
                let src_bind_group = self.target_bind_group(src_target)?;
                pass.set_pipeline(self.op_transform_fused_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, src_bind_group, &[]);
                pass.set_bind_group(2, self.dummy_bind_group.as_ref()?, &[]);
            }
            PlannedRenderOp::Runtime(TexViewerOp::Feedback { .. }) => {
                let src_target = source_target?;
                let src_bind_group = self.target_bind_group(src_target)?;
                let history_key = prepared.feedback_history_key?;
                let history_bind_group = self.feedback_history_read_bind_group(history_key)?;
                pass.set_pipeline(self.op_feedback_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, src_bind_group, &[]);
                pass.set_bind_group(2, history_bind_group, &[]);
            }
            PlannedRenderOp::Runtime(TexViewerOp::ReactionDiffusion { .. }) => {
                let src_target = source_target?;
                let src_bind_group = self.target_bind_group(src_target)?;
                let history_key = prepared.feedback_history_key?;
                let history_bind_group = self.feedback_history_read_bind_group(history_key)?;
                pass.set_pipeline(self.op_reaction_diffusion_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, src_bind_group, &[]);
                pass.set_bind_group(2, history_bind_group, &[]);
            }
            PlannedRenderOp::Runtime(TexViewerOp::PostProcess { history, .. }) => {
                let src_target = source_target?;
                let src_bind_group = self.target_bind_group(src_target)?;
                let feedback_bind_group = if history.is_some() {
                    let history_key = prepared.feedback_history_key?;
                    self.feedback_history_read_bind_group(history_key)?
                } else {
                    self.dummy_bind_group.as_ref()?
                };
                pass.set_pipeline(self.op_post_process_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, src_bind_group, &[]);
                pass.set_bind_group(2, feedback_bind_group, &[]);
            }
            PlannedRenderOp::Runtime(TexViewerOp::Blend {
                base_texture_node_id,
                layer_texture_node_id,
                ..
            }) => {
                let base_bind_group = self
                    .blend_source_bind_group_for_texture(base_texture_node_id)
                    .or_else(|| {
                        source_target.and_then(|target_ref| self.target_bind_group(target_ref))
                    })?;
                let layer_bind_group = layer_texture_node_id
                    .and_then(|id| self.blend_source_bind_group_for_texture(id))
                    .unwrap_or(self.dummy_bind_group.as_ref()?);
                pass.set_pipeline(self.op_blend_pipeline.as_ref()?);
                pass.set_bind_group(0, &self.op_uniform_bind_group, &[prepared.dynamic_offset]);
                pass.set_bind_group(1, base_bind_group, &[]);
                pass.set_bind_group(2, layer_bind_group, &[]);
            }
            PlannedRenderOp::Runtime(TexViewerOp::StoreTexture { .. }) => {
                return None;
            }
        }
        pass.draw(0..6, 0..1);
        Some(())
    }

    /// Copy final render target into viewer output.
    fn finalize_gpu_dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        final_target: Option<RenderTargetRef>,
        width: u32,
        height: u32,
    ) -> Option<()> {
        let final_target = final_target?;
        self.copy_target_to_viewer(encoder, final_target, width, height);
        Some(())
    }

    fn op_uniform_for_fused_transform_pair(
        first: TransformParams,
        second: TransformParams,
    ) -> TexOpUniform {
        TexOpUniform {
            p0: [first.brightness, first.gain_r, first.gain_g, first.gain_b],
            p1: [first.alpha_mul, 0.0, 0.0, 0.0],
            p2: [
                second.brightness,
                second.gain_r,
                second.gain_g,
                second.gain_b,
            ],
            p3: [second.alpha_mul, 0.0, 0.0, 0.0],
            p4: [0.0; 4],
        }
    }

    fn op_uniform_for_planned(op: PlannedRenderOp) -> TexOpUniform {
        match op {
            PlannedRenderOp::Runtime(runtime_op) => match runtime_op {
                TexViewerOp::Solid { .. } => TexOpUniform::solid(runtime_op),
                TexViewerOp::Circle { .. } => TexOpUniform::circle(runtime_op),
                TexViewerOp::Sphere { .. } => TexOpUniform::sphere(runtime_op),
                TexViewerOp::Transform { .. } => TexOpUniform::transform(runtime_op),
                TexViewerOp::Level { .. } => TexOpUniform::level(runtime_op),
                TexViewerOp::Feedback { .. } => TexOpUniform::feedback(runtime_op),
                TexViewerOp::ReactionDiffusion { .. } => {
                    TexOpUniform::reaction_diffusion(runtime_op)
                }
                TexViewerOp::PostProcess { .. } => TexOpUniform::post_process(runtime_op),
                TexViewerOp::Blend { .. } => TexOpUniform::blend(runtime_op),
                TexViewerOp::StoreTexture { .. } => TexOpUniform::solid(runtime_op),
            },
            PlannedRenderOp::TransformPair { first, second } => {
                Self::op_uniform_for_fused_transform_pair(first, second)
            }
        }
    }

    fn write_planned_op_uniforms(
        &mut self,
        queue: &wgpu::Queue,
        planned_render_ops: &[PlannedRenderOp],
    ) -> u64 {
        if planned_render_ops.is_empty() {
            return 0;
        }
        let stride = self.op_uniform_stride as usize;
        let upload_len = stride.saturating_mul(planned_render_ops.len());
        self.op_uniform_staging.resize(upload_len, 0);
        for (index, op) in planned_render_ops.iter().copied().enumerate() {
            let offset = stride.saturating_mul(index);
            let chunk = &mut self.op_uniform_staging[offset..offset + stride];
            chunk.fill(0);
            let uniform = Self::op_uniform_for_planned(op);
            chunk[..TEX_OP_UNIFORM_SIZE].copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        queue.write_buffer(
            &self.op_uniform_buffer,
            0,
            &self.op_uniform_staging[..upload_len],
        );
        upload_len as u64
    }

    fn feedback_key_for_planned(op: PlannedRenderOp) -> Option<FeedbackHistoryKey> {
        let PlannedRenderOp::Runtime(runtime_op) = op else {
            return None;
        };
        match runtime_op {
            TexViewerOp::Feedback { history, .. } => {
                Some(FeedbackHistoryKey::from_binding(history))
            }
            TexViewerOp::ReactionDiffusion { history, .. } => {
                Some(FeedbackHistoryKey::from_binding(history))
            }
            TexViewerOp::PostProcess {
                history: Some(history),
                ..
            } => Some(FeedbackHistoryKey::from_binding(history)),
            _ => None,
        }
    }

    fn op_clear_color_for_planned(op: PlannedRenderOp) -> wgpu::Color {
        let PlannedRenderOp::Runtime(runtime_op) = op else {
            return PREVIEW_BG;
        };
        op_clear_color(runtime_op)
    }

    fn is_feedback_history_tap_op(op: PlannedRenderOp) -> bool {
        matches!(op, PlannedRenderOp::Runtime(TexViewerOp::Feedback { .. }))
    }

    fn external_feedback_accumulation_texture(op: PlannedRenderOp) -> Option<u32> {
        let PlannedRenderOp::Runtime(runtime_op) = op else {
            return None;
        };
        let TexViewerOp::Feedback { history, .. } = runtime_op else {
            return None;
        };
        let crate::gui::runtime::TexRuntimeFeedbackHistoryBinding::External { texture_node_id } =
            history
        else {
            return None;
        };
        Some(texture_node_id)
    }

    fn feedback_frame_gap_for_planned(op: PlannedRenderOp) -> u32 {
        let PlannedRenderOp::Runtime(runtime_op) = op else {
            return 0;
        };
        let TexViewerOp::Feedback { frame_gap, .. } = runtime_op else {
            return 0;
        };
        frame_gap
    }

    fn should_write_feedback_history(
        &mut self,
        key: FeedbackHistoryKey,
        frame_gap: u32,
    ) -> Option<bool> {
        let history = self.feedback_history.get_mut(&key)?;
        if history.configured_gap != frame_gap {
            history.configured_gap = frame_gap;
            history.write_cooldown = 0;
        }
        Some(consume_feedback_write_cooldown(
            &mut history.write_cooldown,
            frame_gap,
        ))
    }

    fn resolve_external_feedback_write_for_texture(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        state: &mut GpuDispatchState,
        texture_node_id: u32,
        source_target: RenderTargetRef,
        width: u32,
        height: u32,
    ) -> Option<()> {
        let Some(pending_writes) = state
            .pending_external_feedback_writes
            .remove(&texture_node_id)
        else {
            return Some(());
        };
        for pending in pending_writes {
            let history_write_target = self.feedback_history_write_target(pending.history_key)?;
            self.copy_target_to_target(encoder, source_target, history_write_target, width, height);
            self.swap_feedback_history(pending.history_key)?;
        }
        Some(())
    }

    fn flush_pending_external_feedback_writes(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        state: &mut GpuDispatchState,
        width: u32,
        height: u32,
    ) -> Option<()> {
        for pending_writes in state
            .pending_external_feedback_writes
            .drain()
            .map(|(_, writes)| writes)
        {
            for pending in pending_writes {
                let history_write_target =
                    self.feedback_history_write_target(pending.history_key)?;
                self.copy_target_to_target(
                    encoder,
                    pending.fallback_source_target,
                    history_write_target,
                    width,
                    height,
                );
                self.swap_feedback_history(pending.history_key)?;
            }
        }
        Some(())
    }

    fn choose_intermediate_target(&self, scratch_flip: &mut bool) -> RenderTargetRef {
        let preferred = if *scratch_flip {
            RenderTargetRef::ScratchB
        } else {
            RenderTargetRef::ScratchA
        };
        *scratch_flip = !*scratch_flip;
        if !self.has_blend_alias_target(preferred) {
            return preferred;
        }
        let alternate = match preferred {
            RenderTargetRef::ScratchA => RenderTargetRef::ScratchB,
            RenderTargetRef::ScratchB => RenderTargetRef::ScratchA,
            _ => preferred,
        };
        if !self.has_blend_alias_target(alternate) {
            return alternate;
        }
        preferred
    }

    fn has_blend_alias_target(&self, target: RenderTargetRef) -> bool {
        match target {
            RenderTargetRef::ScratchA => self.blend_alias_count_scratch_a > 0,
            RenderTargetRef::ScratchB => self.blend_alias_count_scratch_b > 0,
            _ => self
                .blend_source_aliases
                .values()
                .copied()
                .any(|alias| alias == target),
        }
    }

    fn bind_blend_source_alias(&mut self, texture_node_id: u32, target: RenderTargetRef) {
        if let Some(previous) = self.blend_source_aliases.insert(texture_node_id, target) {
            self.decrement_blend_alias_target_count(previous);
        }
        self.increment_blend_alias_target_count(target);
    }

    fn materialize_blend_source_aliases_for_target(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: RenderTargetRef,
        width: u32,
        height: u32,
    ) {
        let mut materialize = std::mem::take(&mut self.blend_alias_materialize_scratch);
        materialize.clear();
        for (&texture_node_id, &alias_target) in self.blend_source_aliases.iter() {
            if alias_target == target {
                materialize.push(texture_node_id);
            }
        }
        for texture_node_id in materialize.iter().copied() {
            self.ensure_blend_source_slot(device, encoder, texture_node_id, width, height);
            self.copy_target_to_blend_source(encoder, target, texture_node_id, width, height);
            if let Some(alias_target) = self.blend_source_aliases.remove(&texture_node_id) {
                self.decrement_blend_alias_target_count(alias_target);
            }
        }
        materialize.clear();
        self.blend_alias_materialize_scratch = materialize;
    }

    fn increment_blend_alias_target_count(&mut self, target: RenderTargetRef) {
        match target {
            RenderTargetRef::ScratchA => {
                self.blend_alias_count_scratch_a =
                    self.blend_alias_count_scratch_a.saturating_add(1);
            }
            RenderTargetRef::ScratchB => {
                self.blend_alias_count_scratch_b =
                    self.blend_alias_count_scratch_b.saturating_add(1);
            }
            _ => {}
        }
    }

    fn decrement_blend_alias_target_count(&mut self, target: RenderTargetRef) {
        match target {
            RenderTargetRef::ScratchA => {
                self.blend_alias_count_scratch_a =
                    self.blend_alias_count_scratch_a.saturating_sub(1);
            }
            RenderTargetRef::ScratchB => {
                self.blend_alias_count_scratch_b =
                    self.blend_alias_count_scratch_b.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn clear_viewer_target(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(view) = self.viewer_texture_view.as_ref() else {
            return;
        };
        let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gui-tex-preview-clear-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(PREVIEW_BG),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
    }

    fn ensure_viewer_texture(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.viewer_texture_size == (width, height) && self.viewer_bind_group.is_some() {
            return;
        }
        self.viewer_texture_size = (width, height);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui-tex-viewer-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEX_PREVIEW_TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = viewer::create_texture_bind_group(
            device,
            &self.viewer_texture_layout,
            &texture_view,
            &self.viewer_sampler,
        );
        self.viewer_texture = Some(texture);
        self.viewer_texture_view = Some(texture_view);
        self.viewer_bind_group = Some(bind_group);
    }

    fn ensure_scratch_textures(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.scratch_texture_size == (width, height)
            && self.scratch_bind_group_a.is_some()
            && self.scratch_bind_group_b.is_some()
        {
            return;
        }
        self.scratch_texture_size = (width, height);
        let (a_texture, a_view, a_bind) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-tex-preview-scratch-a",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        let (b_texture, b_view, b_bind) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-tex-preview-scratch-b",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        self.scratch_texture_a = Some(a_texture);
        self.scratch_view_a = Some(a_view);
        self.scratch_bind_group_a = Some(a_bind);
        self.scratch_texture_b = Some(b_texture);
        self.scratch_view_b = Some(b_view);
        self.scratch_bind_group_b = Some(b_bind);
    }

    fn target_view(&self, target: RenderTargetRef) -> Option<&wgpu::TextureView> {
        match target {
            RenderTargetRef::Viewer => self.viewer_texture_view.as_ref(),
            RenderTargetRef::ScratchA => self.scratch_view_a.as_ref(),
            RenderTargetRef::ScratchB => self.scratch_view_b.as_ref(),
            RenderTargetRef::FeedbackHistory { key, slot_index } => self
                .feedback_history
                .get(&key)
                .and_then(|history| history.slots.get(slot_index))
                .map(|slot| &slot.view),
        }
    }

    fn target_bind_group(&self, target: RenderTargetRef) -> Option<&wgpu::BindGroup> {
        match target {
            RenderTargetRef::Viewer => self.viewer_bind_group.as_ref(),
            RenderTargetRef::ScratchA => self.scratch_bind_group_a.as_ref(),
            RenderTargetRef::ScratchB => self.scratch_bind_group_b.as_ref(),
            RenderTargetRef::FeedbackHistory { key, slot_index } => self
                .feedback_history
                .get(&key)
                .and_then(|history| history.slots.get(slot_index))
                .map(|slot| &slot.bind_group),
        }
    }

    fn target_texture(&self, target: RenderTargetRef) -> Option<&wgpu::Texture> {
        match target {
            RenderTargetRef::Viewer => self.viewer_texture.as_ref(),
            RenderTargetRef::ScratchA => self.scratch_texture_a.as_ref(),
            RenderTargetRef::ScratchB => self.scratch_texture_b.as_ref(),
            RenderTargetRef::FeedbackHistory { key, slot_index } => self
                .feedback_history
                .get(&key)
                .and_then(|history| history.slots.get(slot_index))
                .map(|slot| &slot.texture),
        }
    }

    fn ensure_feedback_history_slot(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        key: FeedbackHistoryKey,
        width: u32,
        height: u32,
    ) {
        if self
            .feedback_history
            .get(&key)
            .map(|history| history.slots[0].size == (width, height))
            .unwrap_or(false)
        {
            return;
        }
        let (texture_a, view_a, bind_group_a) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-tex-preview-feedback-history-a",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        let (texture_b, view_b, bind_group_b) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-tex-preview-feedback-history-b",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        for view in [&view_a, &view_b] {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui-tex-preview-feedback-history-clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(TRANSPARENT_BG),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }
        self.feedback_history.insert(
            key,
            FeedbackHistorySlot {
                slots: [
                    CachedTextureSlot {
                        texture: texture_a,
                        view: view_a,
                        bind_group: bind_group_a,
                        size: (width, height),
                    },
                    CachedTextureSlot {
                        texture: texture_b,
                        view: view_b,
                        bind_group: bind_group_b,
                        size: (width, height),
                    },
                ],
                read_index: 0,
                write_cooldown: 0,
                configured_gap: 0,
            },
        );
    }

    fn feedback_history_read_bind_group(
        &self,
        key: FeedbackHistoryKey,
    ) -> Option<&wgpu::BindGroup> {
        let history = self.feedback_history.get(&key)?;
        history
            .slots
            .get(history.read_index)
            .map(|slot| &slot.bind_group)
    }

    fn feedback_history_write_target(&self, key: FeedbackHistoryKey) -> Option<RenderTargetRef> {
        let history = self.feedback_history.get(&key)?;
        let write_index = 1usize.saturating_sub(history.read_index);
        Some(RenderTargetRef::FeedbackHistory {
            key,
            slot_index: write_index,
        })
    }

    fn swap_feedback_history(&mut self, key: FeedbackHistoryKey) -> Option<RenderTargetRef> {
        let history = self.feedback_history.get_mut(&key)?;
        history.read_index = 1usize.saturating_sub(history.read_index);
        Some(RenderTargetRef::FeedbackHistory {
            key,
            slot_index: history.read_index,
        })
    }

    fn copy_target_to_viewer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: RenderTargetRef,
        width: u32,
        height: u32,
    ) {
        let Some(src_texture) = self.target_texture(target) else {
            return;
        };
        let Some(dst_texture) = self.viewer_texture.as_ref() else {
            return;
        };
        if matches!(target, RenderTargetRef::Viewer) {
            return;
        }
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: src_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: dst_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn copy_target_to_blend_source(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: RenderTargetRef,
        texture_node_id: u32,
        width: u32,
        height: u32,
    ) {
        let Some(src_texture) = self.target_texture(target) else {
            return;
        };
        let Some(slot) = self.blend_source_slots.get(&texture_node_id) else {
            return;
        };
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: src_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: &slot.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn copy_target_to_target(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        src_target: RenderTargetRef,
        dst_target: RenderTargetRef,
        width: u32,
        height: u32,
    ) {
        if src_target == dst_target {
            return;
        }
        let Some(src_texture) = self.target_texture(src_target) else {
            return;
        };
        let Some(dst_texture) = self.target_texture(dst_target) else {
            return;
        };
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: src_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: dst_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn ensure_blend_source_slot(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture_node_id: u32,
        width: u32,
        height: u32,
    ) {
        if self
            .blend_source_slots
            .get(&texture_node_id)
            .map(|slot| slot.size == (width, height))
            .unwrap_or(false)
        {
            return;
        }
        let (texture, view, bind_group) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-tex-preview-blend-source",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui-tex-preview-blend-source-clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(TRANSPARENT_BG),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }
        self.blend_source_slots.insert(
            texture_node_id,
            CachedTextureSlot {
                texture,
                view,
                bind_group,
                size: (width, height),
            },
        );
    }

    fn blend_source_bind_group(&self, texture_node_id: u32) -> Option<&wgpu::BindGroup> {
        self.blend_source_slots
            .get(&texture_node_id)
            .map(|slot| &slot.bind_group)
    }

    fn blend_source_bind_group_for_texture(
        &self,
        texture_node_id: u32,
    ) -> Option<&wgpu::BindGroup> {
        self.blend_source_aliases
            .get(&texture_node_id)
            .copied()
            .and_then(|target| self.target_bind_group(target))
            .or_else(|| self.blend_source_bind_group(texture_node_id))
    }
}

fn op_clear_color(op: TexViewerOp) -> wgpu::Color {
    match op {
        TexViewerOp::Sphere {
            alpha_clip: true, ..
        }
        | TexViewerOp::Circle {
            alpha_clip: true, ..
        } => TRANSPARENT_BG,
        _ => PREVIEW_BG,
    }
}

fn consume_feedback_write_cooldown(write_cooldown: &mut u32, frame_gap: u32) -> bool {
    if *write_cooldown == 0 {
        *write_cooldown = frame_gap;
        true
    } else {
        *write_cooldown = (*write_cooldown).saturating_sub(1);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{consume_feedback_write_cooldown, PlannedRenderOp, TexPreviewRenderer};
    use crate::gui::runtime::{PostProcessCategory, TexRuntimeFeedbackHistoryBinding};
    use crate::gui::tex_view::TexViewerOp;

    #[test]
    fn feedback_ops_use_history_tap_mode() {
        let op = PlannedRenderOp::Runtime(TexViewerOp::Feedback {
            mix: 1.0,
            frame_gap: 0,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 11,
            },
        });
        assert!(TexPreviewRenderer::is_feedback_history_tap_op(op));
    }

    #[test]
    fn temporal_history_ops_except_feedback_keep_history_render_target_mode() {
        let reaction = PlannedRenderOp::Runtime(TexViewerOp::ReactionDiffusion {
            diffusion_a: 1.0,
            diffusion_b: 0.5,
            feed: 0.06,
            kill: 0.04,
            dt: 1.0,
            seed_mix: 0.25,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 22,
            },
        });
        let post = PlannedRenderOp::Runtime(TexViewerOp::PostProcess {
            category: PostProcessCategory::Temporal,
            effect: 0.0,
            amount: 0.5,
            scale: 0.5,
            threshold: 0.0,
            speed: 0.0,
            time: 1.0,
            history: Some(TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 44,
            }),
        });
        assert!(!TexPreviewRenderer::is_feedback_history_tap_op(reaction));
        assert!(!TexPreviewRenderer::is_feedback_history_tap_op(post));
    }

    #[test]
    fn external_feedback_accumulation_texture_detects_external_binding() {
        let op = PlannedRenderOp::Runtime(TexViewerOp::Feedback {
            mix: 1.0,
            frame_gap: 0,
            history: TexRuntimeFeedbackHistoryBinding::External {
                texture_node_id: 77,
            },
        });
        assert_eq!(
            TexPreviewRenderer::external_feedback_accumulation_texture(op),
            Some(77)
        );
    }

    #[test]
    fn external_feedback_accumulation_texture_ignores_internal_and_non_feedback_ops() {
        let internal_feedback = PlannedRenderOp::Runtime(TexViewerOp::Feedback {
            mix: 0.8,
            frame_gap: 0,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 5,
            },
        });
        let reaction = PlannedRenderOp::Runtime(TexViewerOp::ReactionDiffusion {
            diffusion_a: 1.0,
            diffusion_b: 0.5,
            feed: 0.06,
            kill: 0.04,
            dt: 1.0,
            seed_mix: 0.25,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 22,
            },
        });
        assert_eq!(
            TexPreviewRenderer::external_feedback_accumulation_texture(internal_feedback),
            None
        );
        assert_eq!(
            TexPreviewRenderer::external_feedback_accumulation_texture(reaction),
            None
        );
    }

    #[test]
    fn feedback_write_cooldown_obeys_frame_gap_steps() {
        let mut cooldown = 0u32;
        assert!(consume_feedback_write_cooldown(&mut cooldown, 2));
        assert_eq!(cooldown, 2);
        assert!(!consume_feedback_write_cooldown(&mut cooldown, 2));
        assert_eq!(cooldown, 1);
        assert!(!consume_feedback_write_cooldown(&mut cooldown, 2));
        assert_eq!(cooldown, 0);
        assert!(consume_feedback_write_cooldown(&mut cooldown, 2));
        assert_eq!(cooldown, 2);
    }
}

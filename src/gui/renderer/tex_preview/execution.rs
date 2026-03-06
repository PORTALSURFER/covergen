//! Frame-time tex preview preparation and GPU operation execution.

mod history_stage;
mod op_metadata;
mod pass_stage;
mod targets_stage;

use crate::gui::geometry::Rect;
use crate::gui::tex_view::{TexViewerFrame, TexViewerOp, TexViewerPayload};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use super::super::viewer;
use super::execution_plan::{build_execution_plan, PlannedRenderOp, PlannedStep};
use super::pipeline::create_preview_texture_bundle;
use super::{
    CachedTextureSlot, FeedbackHistoryKey, FeedbackHistorySlot, RenderTargetRef, TexOpUniform,
    TexPreviewRenderer, PREVIEW_BG, TEX_PREVIEW_TEXTURE_FORMAT,
};

const TEX_OP_UNIFORM_SIZE: usize = std::mem::size_of::<TexOpUniform>();
const TRANSPARENT_BG: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

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

/// One frame's GPU-op dispatch request payload.
#[derive(Clone, Copy, Debug)]
struct GpuOpsRequest<'a> {
    ops: &'a [TexViewerOp],
    width: u32,
    height: u32,
    plan_signature: u64,
    uniform_signature: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeOpPipelineKind {
    Solid,
    Circle,
    Box,
    Grid,
    Sphere,
    SourceNoise,
    Transform2D,
    ColorAdjust,
    Level,
    Mask,
    Morphology,
    ToneMap,
    Feedback,
    ReactionDiffusion,
    DirectionalSmear,
    WarpTransform,
    PostProcess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeSourceBinding {
    Dummy,
    SourceTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeFeedbackBinding {
    Dummy,
    HistoryRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RuntimeOpDescriptor {
    pipeline: RuntimeOpPipelineKind,
    source_binding: RuntimeSourceBinding,
    feedback_binding: RuntimeFeedbackBinding,
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
        let request = GpuOpsRequest {
            ops,
            width: tex_view.texture_width,
            height: tex_view.texture_height,
            plan_signature: tex_view.ops_plan_signature,
            uniform_signature: tex_view.ops_uniform_signature,
        };
        if let Some(op_upload_bytes) = self.encode_gpu_ops(device, queue, encoder, request) {
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
        request: GpuOpsRequest<'_>,
    ) -> Option<u64> {
        if request.ops.is_empty() {
            return None;
        }
        self.op_pass_timestamps.begin_frame();
        let result = self.encode_gpu_ops_staged(device, queue, encoder, request);
        self.op_pass_timestamps.resolve_and_reset(encoder);
        result
    }

    /// Run staged tex-op dispatch: plan -> prepare targets -> encode passes -> finalize.
    fn encode_gpu_ops_staged(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        request: GpuOpsRequest<'_>,
    ) -> Option<u64> {
        let upload_bytes = self.plan_gpu_dispatch(device, queue, request)?;
        let planned_steps = std::mem::take(&mut self.cached_plan_steps);
        let planned_render_ops = std::mem::take(&mut self.cached_plan_render_ops);
        let mut dispatch = DispatchContext {
            device,
            encoder,
            width: request.width,
            height: request.height,
        };
        let mut state = GpuDispatchState::default();
        for step in planned_steps.iter().copied() {
            self.dispatch_planned_step(
                &mut dispatch,
                step,
                request.ops,
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
        request: GpuOpsRequest<'_>,
    ) -> Option<u64> {
        self.blend_source_aliases.clear();
        self.blend_source_aliases_by_target.clear();
        let plan_changed = self.cached_plan_signature != Some(request.plan_signature);
        if plan_changed {
            self.cached_plan_ops.clear();
            self.cached_plan_ops.extend_from_slice(request.ops);
            self.cached_plan_steps.clear();
            self.cached_plan_render_ops.clear();
            build_execution_plan(
                request.ops,
                &mut self.cached_plan_steps,
                &mut self.cached_plan_render_ops,
            );
            self.cached_plan_signature = Some(request.plan_signature);
            self.op_uniform_signature = None;
            self.prune_texture_caches_for_plan();
        }
        if self.cached_plan_render_ops.is_empty() {
            return None;
        }
        self.ensure_op_pipelines(device);
        self.ensure_dummy_bind_group(device);
        let render_op_count = self.cached_plan_render_ops.len();
        self.ensure_op_uniform_capacity(device, render_op_count);
        let upload_bytes = if self.op_uniform_signature != Some(request.uniform_signature) {
            let render_ops = std::mem::take(&mut self.cached_plan_render_ops);
            let bytes = self.write_planned_op_uniforms(queue, request.ops, render_ops.as_slice());
            self.cached_plan_render_ops = render_ops;
            self.op_uniform_signature = Some(request.uniform_signature);
            bytes
        } else {
            0
        };
        if render_op_count > 1 {
            self.ensure_scratch_textures(device, request.width, request.height);
        }
        Some(upload_bytes)
    }

    /// Apply one planned step, including store-texture aliases and render passes.
    fn dispatch_planned_step(
        &mut self,
        dispatch: &mut DispatchContext<'_>,
        step: PlannedStep,
        runtime_ops: &[TexViewerOp],
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
        let prepared = self.prepare_targets_for_op(
            dispatch,
            planned_op,
            runtime_ops,
            render_ops.len(),
            state,
        )?;
        self.encode_pass_for_op(
            dispatch.encoder,
            prepared,
            runtime_ops,
            source_target_before,
        )?;
        if self.is_feedback_history_tap_op(runtime_ops, prepared.planned_op) {
            let history_key = prepared.feedback_history_key?;
            let source_target = source_target_before?;
            let frame_gap = self.feedback_frame_gap_for_planned(runtime_ops, prepared.planned_op);
            if self.should_write_feedback_history(history_key, frame_gap)? {
                if let Some(texture_node_id) =
                    self.external_feedback_accumulation_texture(runtime_ops, prepared.planned_op)
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
        runtime_ops: &[TexViewerOp],
        render_op_count: usize,
        state: &mut GpuDispatchState,
    ) -> Option<PreparedRenderOp> {
        let feedback_history_key = self.feedback_key_for_planned(runtime_ops, planned_op);
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
            self.choose_intermediate_target(&mut state.scratch_flip, state.source_target)
        };
        if feedback_history_key.is_some()
            && !self.is_feedback_history_tap_op(runtime_ops, planned_op)
        {
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

    fn op_uniform_for_fused_color_adjust_pair(first: [f32; 5], second: [f32; 5]) -> TexOpUniform {
        TexOpUniform {
            p0: [first[0], first[1], first[2], first[3]],
            p1: [first[4], 0.0, 0.0, 0.0],
            p2: [second[0], second[1], second[2], second[3]],
            p3: [second[4], 0.0, 0.0, 0.0],
            p4: [0.0; 4],
        }
    }

    fn color_adjust_components(op: TexViewerOp) -> Option<[f32; 5]> {
        let TexViewerOp::ColorAdjust {
            brightness,
            gain_r,
            gain_g,
            gain_b,
            alpha_mul,
        } = op
        else {
            return None;
        };
        Some([brightness, gain_r, gain_g, gain_b, alpha_mul])
    }

    fn runtime_op_for_planned(
        &self,
        runtime_ops: &[TexViewerOp],
        op: PlannedRenderOp,
    ) -> Option<TexViewerOp> {
        let PlannedRenderOp::Runtime { op_index } = op else {
            return None;
        };
        runtime_ops.get(op_index).copied()
    }

    fn op_uniform_for_planned(
        &self,
        runtime_ops: &[TexViewerOp],
        op: PlannedRenderOp,
    ) -> Option<TexOpUniform> {
        match op {
            PlannedRenderOp::Runtime { .. } => {
                let runtime_op = self.runtime_op_for_planned(runtime_ops, op)?;
                Some(op_uniform_for_runtime_op(runtime_op))
            }
            PlannedRenderOp::ColorAdjustPair {
                first_op_index,
                second_op_index,
            } => {
                let first = runtime_ops.get(first_op_index).copied()?;
                let second = runtime_ops.get(second_op_index).copied()?;
                let first = Self::color_adjust_components(first)?;
                let second = Self::color_adjust_components(second)?;
                Some(Self::op_uniform_for_fused_color_adjust_pair(first, second))
            }
        }
    }

    fn write_planned_op_uniforms(
        &mut self,
        queue: &wgpu::Queue,
        runtime_ops: &[TexViewerOp],
        planned_render_ops: &[PlannedRenderOp],
    ) -> u64 {
        if planned_render_ops.is_empty() {
            return 0;
        }
        let stride = self.op_uniform_stride as usize;
        let upload_len = stride.saturating_mul(planned_render_ops.len());
        self.op_uniform_staging.resize(upload_len, 0);
        for (index, op) in planned_render_ops.iter().copied().enumerate() {
            let Some(uniform) = self.op_uniform_for_planned(runtime_ops, op) else {
                continue;
            };
            let offset = stride.saturating_mul(index);
            let chunk = &mut self.op_uniform_staging[offset..offset + stride];
            chunk.fill(0);
            chunk[..TEX_OP_UNIFORM_SIZE].copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        queue.write_buffer(
            &self.op_uniform_buffer,
            0,
            &self.op_uniform_staging[..upload_len],
        );
        upload_len as u64
    }

    fn feedback_key_for_planned(
        &self,
        runtime_ops: &[TexViewerOp],
        op: PlannedRenderOp,
    ) -> Option<FeedbackHistoryKey> {
        self.runtime_op_for_planned(runtime_ops, op)
            .and_then(feedback_key_for_runtime_op)
    }

    fn op_clear_color_for_planned(
        &self,
        runtime_ops: &[TexViewerOp],
        op: PlannedRenderOp,
    ) -> wgpu::Color {
        let Some(runtime_op) = self.runtime_op_for_planned(runtime_ops, op) else {
            return PREVIEW_BG;
        };
        op_clear_color(runtime_op)
    }

    fn is_feedback_history_tap_op(&self, runtime_ops: &[TexViewerOp], op: PlannedRenderOp) -> bool {
        self.runtime_op_for_planned(runtime_ops, op)
            .map(is_feedback_history_tap_runtime_op)
            .unwrap_or(false)
    }

    fn external_feedback_accumulation_texture(
        &self,
        runtime_ops: &[TexViewerOp],
        op: PlannedRenderOp,
    ) -> Option<u32> {
        self.runtime_op_for_planned(runtime_ops, op)
            .and_then(external_feedback_accumulation_texture_for_runtime_op)
    }

    fn feedback_frame_gap_for_planned(
        &self,
        runtime_ops: &[TexViewerOp],
        op: PlannedRenderOp,
    ) -> u32 {
        self.runtime_op_for_planned(runtime_ops, op)
            .map(feedback_frame_gap_for_runtime_op)
            .unwrap_or(0)
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

    fn choose_intermediate_target(
        &self,
        scratch_flip: &mut bool,
        source_target: Option<RenderTargetRef>,
    ) -> RenderTargetRef {
        choose_intermediate_target_impl(scratch_flip, source_target)
    }

    fn bind_blend_source_alias(&mut self, texture_node_id: u32, target: RenderTargetRef) {
        if let Some(previous) = self.blend_source_aliases.insert(texture_node_id, target) {
            self.remove_blend_alias_from_target(previous, texture_node_id);
        }
        self.blend_source_aliases_by_target
            .entry(target)
            .or_default()
            .push(texture_node_id);
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
        if let Some(source_ids) = self.blend_source_aliases_by_target.get_mut(&target) {
            materialize.extend(source_ids.iter().copied());
            source_ids.clear();
        }
        for texture_node_id in materialize.iter().copied() {
            self.ensure_blend_source_slot(device, encoder, texture_node_id, width, height);
            self.copy_target_to_blend_source(encoder, target, texture_node_id, width, height);
            let _ = self.blend_source_aliases.remove(&texture_node_id);
        }
        materialize.clear();
        self.blend_alias_materialize_scratch = materialize;
    }

    fn remove_blend_alias_from_target(&mut self, target: RenderTargetRef, texture_node_id: u32) {
        let Some(source_ids) = self.blend_source_aliases_by_target.get_mut(&target) else {
            return;
        };
        if let Some(index) = source_ids
            .iter()
            .position(|existing| *existing == texture_node_id)
        {
            source_ids.swap_remove(index);
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

    fn prune_texture_caches_for_plan(&mut self) {
        let (active_feedback, active_blend_sources) = collect_active_cache_keys(
            self.cached_plan_steps.as_slice(),
            self.cached_plan_render_ops.as_slice(),
            self.cached_plan_ops.as_slice(),
        );
        prune_keyed_cache(&mut self.feedback_history, &active_feedback);
        prune_keyed_cache(&mut self.blend_source_slots, &active_blend_sources);
    }
}

fn collect_active_cache_keys(
    planned_steps: &[PlannedStep],
    planned_render_ops: &[PlannedRenderOp],
    planned_ops: &[TexViewerOp],
) -> (HashSet<FeedbackHistoryKey>, HashSet<u32>) {
    let mut active_feedback = HashSet::new();
    let mut active_blend_sources = HashSet::new();
    for op in planned_render_ops {
        let runtime_op = match *op {
            PlannedRenderOp::Runtime { op_index } => planned_ops.get(op_index).copied(),
            PlannedRenderOp::ColorAdjustPair { .. } => None,
        };
        let Some(runtime_op) = runtime_op else {
            continue;
        };
        if let Some(key) = feedback_key_for_runtime_op(runtime_op) {
            let _ = active_feedback.insert(key);
        }
        match runtime_op {
            TexViewerOp::Blend {
                base_texture_node_id,
                layer_texture_node_id,
                ..
            } => {
                let _ = active_blend_sources.insert(base_texture_node_id);
                if let Some(layer_id) = layer_texture_node_id {
                    let _ = active_blend_sources.insert(layer_id);
                }
            }
            TexViewerOp::DomainWarp {
                base_texture_node_id,
                warp_texture_node_id,
                ..
            } => {
                let _ = active_blend_sources.insert(base_texture_node_id);
                if let Some(warp_id) = warp_texture_node_id {
                    let _ = active_blend_sources.insert(warp_id);
                }
            }
            _ => {}
        }
    }
    for step in planned_steps {
        let PlannedStep::StoreTexture { texture_node_id } = *step else {
            continue;
        };
        let _ = active_blend_sources.insert(texture_node_id);
    }
    (active_feedback, active_blend_sources)
}

fn runtime_op_descriptor(runtime_op: TexViewerOp) -> Option<RuntimeOpDescriptor> {
    op_metadata::runtime_op_descriptor(runtime_op)
}

fn feedback_key_for_runtime_op(runtime_op: TexViewerOp) -> Option<FeedbackHistoryKey> {
    op_metadata::feedback_key_for_runtime_op(runtime_op)
}

fn is_feedback_history_tap_runtime_op(runtime_op: TexViewerOp) -> bool {
    op_metadata::is_feedback_history_tap_runtime_op(runtime_op)
}

fn external_feedback_accumulation_texture_for_runtime_op(runtime_op: TexViewerOp) -> Option<u32> {
    op_metadata::external_feedback_accumulation_texture_for_runtime_op(runtime_op)
}

fn feedback_frame_gap_for_runtime_op(runtime_op: TexViewerOp) -> u32 {
    op_metadata::feedback_frame_gap_for_runtime_op(runtime_op)
}

fn op_clear_color(op: TexViewerOp) -> wgpu::Color {
    op_metadata::op_clear_color(op)
}

fn op_uniform_for_runtime_op(runtime_op: TexViewerOp) -> TexOpUniform {
    op_metadata::op_uniform_for_runtime_op(runtime_op)
}

fn consume_feedback_write_cooldown(write_cooldown: &mut u32, frame_gap: u32) -> bool {
    history_stage::consume_feedback_write_cooldown(write_cooldown, frame_gap)
}

fn prune_keyed_cache<K, V>(cache: &mut HashMap<K, V>, active_keys: &HashSet<K>)
where
    K: Eq + Hash,
{
    cache.retain(|key, _| active_keys.contains(key));
}

fn choose_intermediate_target_impl(
    scratch_flip: &mut bool,
    source_target: Option<RenderTargetRef>,
) -> RenderTargetRef {
    let preferred = if *scratch_flip {
        RenderTargetRef::ScratchB
    } else {
        RenderTargetRef::ScratchA
    };
    *scratch_flip = !*scratch_flip;
    let target_is_available = |target| source_target != Some(target);
    if target_is_available(preferred) {
        return preferred;
    }
    let alternate = match preferred {
        RenderTargetRef::ScratchA => RenderTargetRef::ScratchB,
        RenderTargetRef::ScratchB => RenderTargetRef::ScratchA,
        _ => preferred,
    };
    if target_is_available(alternate) {
        return alternate;
    }
    if target_is_available(RenderTargetRef::Viewer) {
        return RenderTargetRef::Viewer;
    }
    preferred
}

#[cfg(test)]
mod tests {
    use super::{
        choose_intermediate_target_impl, collect_active_cache_keys,
        consume_feedback_write_cooldown, external_feedback_accumulation_texture_for_runtime_op,
        is_feedback_history_tap_runtime_op, prune_keyed_cache, runtime_op_descriptor,
        PlannedRenderOp, PlannedStep, RuntimeFeedbackBinding, RuntimeOpPipelineKind,
        RuntimeSourceBinding,
    };
    use crate::gui::renderer::tex_preview::RenderTargetRef;
    use crate::gui::runtime::{PostProcessCategory, TexRuntimeFeedbackHistoryBinding};
    use crate::gui::tex_view::TexViewerOp;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn feedback_ops_use_history_tap_mode() {
        let op = TexViewerOp::Feedback {
            mix: 1.0,
            frame_gap: 0,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 11,
            },
        };
        assert!(is_feedback_history_tap_runtime_op(op));
    }

    #[test]
    fn temporal_history_ops_except_feedback_keep_history_render_target_mode() {
        let reaction = TexViewerOp::ReactionDiffusion {
            diffusion_a: 1.0,
            diffusion_b: 0.5,
            feed: 0.06,
            kill: 0.04,
            dt: 1.0,
            seed_mix: 0.25,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 22,
            },
        };
        let post = TexViewerOp::PostProcess {
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
        };
        assert!(!is_feedback_history_tap_runtime_op(reaction));
        assert!(!is_feedback_history_tap_runtime_op(post));
    }

    #[test]
    fn external_feedback_accumulation_texture_detects_external_binding() {
        let op = TexViewerOp::Feedback {
            mix: 1.0,
            frame_gap: 0,
            history: TexRuntimeFeedbackHistoryBinding::External {
                texture_node_id: 77,
            },
        };
        assert_eq!(
            external_feedback_accumulation_texture_for_runtime_op(op),
            Some(77)
        );
    }

    #[test]
    fn external_feedback_accumulation_texture_ignores_internal_and_non_feedback_ops() {
        let internal_feedback = TexViewerOp::Feedback {
            mix: 0.8,
            frame_gap: 0,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 5,
            },
        };
        let reaction = TexViewerOp::ReactionDiffusion {
            diffusion_a: 1.0,
            diffusion_b: 0.5,
            feed: 0.06,
            kill: 0.04,
            dt: 1.0,
            seed_mix: 0.25,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 22,
            },
        };
        assert_eq!(
            external_feedback_accumulation_texture_for_runtime_op(internal_feedback),
            None
        );
        assert_eq!(
            external_feedback_accumulation_texture_for_runtime_op(reaction),
            None
        );
    }

    #[test]
    fn active_cache_keys_include_feedback_and_blend_sources() {
        let planned_ops = vec![
            TexViewerOp::Feedback {
                mix: 0.5,
                frame_gap: 0,
                history: TexRuntimeFeedbackHistoryBinding::Internal {
                    feedback_node_id: 12,
                },
            },
            TexViewerOp::PostProcess {
                category: PostProcessCategory::Temporal,
                effect: 0.0,
                amount: 0.5,
                scale: 0.5,
                threshold: 0.0,
                speed: 0.0,
                time: 0.0,
                history: Some(TexRuntimeFeedbackHistoryBinding::External {
                    texture_node_id: 44,
                }),
            },
            TexViewerOp::Blend {
                mode: 0.0,
                opacity: 1.0,
                bg_r: 0.0,
                bg_g: 0.0,
                bg_b: 0.0,
                bg_a: 0.0,
                base_texture_node_id: 7,
                layer_texture_node_id: Some(9),
            },
            TexViewerOp::DomainWarp {
                strength: 0.4,
                frequency: 3.0,
                rotation: 15.0,
                octaves: 4.0,
                base_texture_node_id: 11,
                warp_texture_node_id: Some(13),
            },
        ];
        let render_ops = vec![
            PlannedRenderOp::Runtime { op_index: 0 },
            PlannedRenderOp::Runtime { op_index: 1 },
            PlannedRenderOp::Runtime { op_index: 2 },
            PlannedRenderOp::Runtime { op_index: 3 },
        ];
        let steps = vec![
            PlannedStep::StoreTexture { texture_node_id: 7 },
            PlannedStep::StoreTexture {
                texture_node_id: 13,
            },
        ];

        let (active_feedback, active_blend) = collect_active_cache_keys(
            steps.as_slice(),
            render_ops.as_slice(),
            planned_ops.as_slice(),
        );
        assert_eq!(active_feedback.len(), 2);
        assert_eq!(active_blend.len(), 4);
        assert!(active_blend.contains(&7));
        assert!(active_blend.contains(&9));
        assert!(active_blend.contains(&11));
        assert!(active_blend.contains(&13));
    }

    #[test]
    fn prune_keyed_cache_removes_stale_keys() {
        let mut cache = HashMap::from([(1u32, "a"), (2u32, "b"), (3u32, "c")]);
        let active = HashSet::from([2u32, 3u32]);
        prune_keyed_cache(&mut cache, &active);
        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key(&2));
        assert!(cache.contains_key(&3));
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

    #[test]
    fn runtime_op_descriptor_maps_bindings_by_op_family() {
        let transform = runtime_op_descriptor(TexViewerOp::Transform2D {
            offset_x: 0.0,
            offset_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotate_deg: 0.0,
            pivot_x: 0.5,
            pivot_y: 0.5,
        })
        .expect("transform descriptor");
        assert_eq!(transform.pipeline, RuntimeOpPipelineKind::Transform2D);
        assert_eq!(transform.source_binding, RuntimeSourceBinding::SourceTarget);
        assert_eq!(transform.feedback_binding, RuntimeFeedbackBinding::Dummy);

        let feedback = runtime_op_descriptor(TexViewerOp::Feedback {
            mix: 0.5,
            frame_gap: 0,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 1,
            },
        })
        .expect("feedback descriptor");
        assert_eq!(feedback.pipeline, RuntimeOpPipelineKind::Feedback);
        assert_eq!(feedback.source_binding, RuntimeSourceBinding::SourceTarget);
        assert_eq!(
            feedback.feedback_binding,
            RuntimeFeedbackBinding::HistoryRequired
        );
    }

    #[test]
    fn runtime_op_descriptor_skips_non_render_step_ops() {
        assert!(runtime_op_descriptor(TexViewerOp::StoreTexture { texture_node_id: 1 }).is_none());
    }

    #[test]
    fn intermediate_target_selection_avoids_current_source_target() {
        let mut scratch_flip = true;
        let chosen =
            choose_intermediate_target_impl(&mut scratch_flip, Some(RenderTargetRef::ScratchB));
        assert_eq!(chosen, RenderTargetRef::ScratchA);
    }

    #[test]
    fn intermediate_target_selection_reuses_non_source_scratch_targets() {
        let mut scratch_flip = false;
        let chosen =
            choose_intermediate_target_impl(&mut scratch_flip, Some(RenderTargetRef::ScratchB));
        assert_eq!(chosen, RenderTargetRef::ScratchA);
    }
}

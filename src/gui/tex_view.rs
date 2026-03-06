//! GUI tex preview planning with compiled GPU-runtime evaluation.
//!
//! The generator caches one compiled render chain and frame-keyed operation
//! payload so the renderer executes a single GPU-only preview path.

mod signature;

use self::signature::ops_signatures;
use super::project::{GuiProject, SignalEvalStack};
use super::runtime::{GuiCompiledRuntime, TexRuntimeFrameContext};
use super::timeline::editor_panel_height;

/// Re-exported tex operation type consumed by preview rendering.
pub(crate) use super::runtime::TexRuntimeOp as TexViewerOp;

/// tex viewer payload consumed by the GUI renderer.
pub(crate) enum TexViewerPayload<'a> {
    /// GPU operation chain executed into the viewer target.
    GpuOps(&'a [TexViewerOp]),
}

/// Borrowed frame payload for one tex viewer render.
pub(crate) struct TexViewerFrame<'a> {
    /// Panel-space x-offset of fitted preview quad.
    pub(crate) x: i32,
    /// Panel-space y-offset of fitted preview quad.
    pub(crate) y: i32,
    /// Panel-space fitted preview quad width.
    pub(crate) width: u32,
    /// Panel-space fitted preview quad height.
    pub(crate) height: u32,
    /// Backing GPU texture width used for tex evaluation.
    pub(crate) texture_width: u32,
    /// Backing GPU texture height used for tex evaluation.
    pub(crate) texture_height: u32,
    /// Signature for operation-structure cache reuse in tex-preview planning.
    pub(crate) ops_plan_signature: u64,
    /// Signature for full operation payload cache reuse in uniform uploads.
    pub(crate) ops_uniform_signature: u64,
    /// Borrowed operation payload consumed by the current viewer frame.
    pub(crate) payload: TexViewerPayload<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ViewerCacheKey {
    panel_width: u32,
    panel_height: u32,
    view_width: u32,
    view_height: u32,
    texture_width: u32,
    texture_height: u32,
    render_signature: u64,
    tex_eval_epoch: u64,
    frame_index: u32,
}

impl ViewerCacheKey {
    /// Return true when all inputs that can change texture-size evaluation are unchanged.
    fn matches_runtime_size_inputs(
        &self,
        panel_width: u32,
        panel_height: u32,
        render_signature: u64,
        tex_eval_epoch: u64,
        frame_index: u32,
    ) -> bool {
        self.panel_width == panel_width
            && self.panel_height == panel_height
            && self.render_signature == render_signature
            && self.tex_eval_epoch == tex_eval_epoch
            && self.frame_index == frame_index
    }
}

/// Cached tex preview payload producer.
#[derive(Debug, Default)]
pub(crate) struct TexViewerGenerator {
    key: Option<ViewerCacheKey>,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    compiled_epoch: Option<u64>,
    compiled_render_signature: Option<u64>,
    compiled_runtime: Option<GuiCompiledRuntime>,
    ops: Vec<TexViewerOp>,
    ops_plan_signature: u64,
    ops_uniform_signature: u64,
    eval_stack: SignalEvalStack,
}

/// Immutable update inputs for one tex viewer cache tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TexViewerUpdate {
    /// Current viewport width in physical pixels.
    pub(crate) viewport_width: usize,
    /// Current viewport height in physical pixels.
    pub(crate) viewport_height: usize,
    /// Left panel width in physical pixels.
    pub(crate) panel_width: usize,
    /// Current timeline frame index.
    pub(crate) frame_index: u32,
    /// Total timeline frame count.
    pub(crate) timeline_total_frames: u32,
    /// Timeline playback rate used for time conversion.
    pub(crate) timeline_fps: u32,
    /// Epoch token for tex evaluation invalidation.
    pub(crate) tex_eval_epoch: u64,
}

impl TexViewerGenerator {
    /// Update cached viewer payload for current panel split and graph state.
    pub(crate) fn update(&mut self, project: &GuiProject, update: TexViewerUpdate) {
        let render_signature = project.render_signature();
        let panel_w = update.viewport_width.saturating_sub(update.panel_width) as u32;
        let panel_h = editor_panel_height(update.viewport_height) as u32;
        let dynamic_frame = update.frame_index;
        if self.compiled_epoch != Some(update.tex_eval_epoch)
            || self.compiled_render_signature != Some(render_signature)
        {
            self.compiled_runtime = GuiCompiledRuntime::compile(project);
            self.compiled_epoch = Some(update.tex_eval_epoch);
            self.compiled_render_signature = Some(render_signature);
        }
        if let Some(key) = self.key.filter(|key| {
            key.matches_runtime_size_inputs(
                panel_w,
                panel_h,
                render_signature,
                update.tex_eval_epoch,
                dynamic_frame,
            )
        }) {
            self.x =
                update.panel_width as i32 + (panel_w.saturating_sub(key.view_width) / 2) as i32;
            self.y = (panel_h.saturating_sub(key.view_height) / 2) as i32;
            return;
        }
        let time_secs = update.frame_index as f32 / update.timeline_fps.max(1) as f32;
        let (texture_width, texture_height) = self
            .compiled_runtime
            .as_ref()
            .map(|runtime| runtime.output_texture_size(project, time_secs, &mut self.eval_stack))
            .unwrap_or((project.preview_width.max(1), project.preview_height.max(1)));
        let (view_width, view_height) =
            fit_aspect_in_rect(panel_w, panel_h, texture_width, texture_height);
        let x = update.panel_width as i32 + (panel_w.saturating_sub(view_width) / 2) as i32;
        let y = (panel_h.saturating_sub(view_height) / 2) as i32;
        let key = ViewerCacheKey {
            panel_width: panel_w,
            panel_height: panel_h,
            view_width,
            view_height,
            texture_width,
            texture_height,
            render_signature,
            tex_eval_epoch: update.tex_eval_epoch,
            frame_index: dynamic_frame,
        };
        self.x = x;
        self.y = y;
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.width = view_width;
        self.height = view_height;

        self.ops.clear();
        if let Some(compiled_runtime) = &self.compiled_runtime {
            compiled_runtime.evaluate_ops_with_frame(
                project,
                time_secs,
                Some(TexRuntimeFrameContext {
                    frame_index: dynamic_frame,
                    frame_total: update.timeline_total_frames.max(1),
                }),
                &mut self.eval_stack,
                &mut self.ops,
            );
        }
        (self.ops_plan_signature, self.ops_uniform_signature) = ops_signatures(self.ops.as_slice());
    }

    /// Return current frame payload, if viewer dimensions are valid.
    pub(crate) fn frame(&self) -> Option<TexViewerFrame<'_>> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        Some(TexViewerFrame {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            texture_width: self
                .key
                .map(|key| key.texture_width)
                .unwrap_or(self.width.max(1)),
            texture_height: self
                .key
                .map(|key| key.texture_height)
                .unwrap_or(self.height.max(1)),
            ops_plan_signature: self.ops_plan_signature,
            ops_uniform_signature: self.ops_uniform_signature,
            payload: TexViewerPayload::GpuOps(self.ops.as_slice()),
        })
    }
}

fn fit_aspect_in_rect(avail_w: u32, avail_h: u32, texture_w: u32, texture_h: u32) -> (u32, u32) {
    if avail_w == 0 || avail_h == 0 || texture_w == 0 || texture_h == 0 {
        return (0, 0);
    }
    if (avail_w as u64) * (texture_h as u64) <= (avail_h as u64) * (texture_w as u64) {
        let h = ((avail_w as u64) * (texture_h as u64) / (texture_w as u64)) as u32;
        (avail_w.max(1), h.max(1))
    } else {
        let w = ((avail_h as u64) * (texture_w as u64) / (texture_h as u64)) as u32;
        (w.max(1), avail_h.max(1))
    }
}

#[cfg(test)]
#[allow(clippy::infallible_destructuring_match)]
mod tests;

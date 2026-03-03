//! GUI tex preview planning with compiled GPU-runtime evaluation.
//!
//! The generator caches one compiled render chain and frame-keyed operation
//! payload so the renderer executes a single GPU-only preview path.

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
        let dynamic_frame = if project.has_signal_bindings() || project.has_temporal_nodes() {
            update.frame_index
        } else {
            0
        };
        if self.compiled_epoch != Some(update.tex_eval_epoch)
            || self.compiled_render_signature != Some(render_signature)
        {
            self.compiled_runtime = GuiCompiledRuntime::compile(project);
            self.compiled_epoch = Some(update.tex_eval_epoch);
            self.compiled_render_signature = Some(render_signature);
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
            ops_plan_signature: ops_plan_signature(self.ops.as_slice()),
            ops_uniform_signature: ops_uniform_signature(self.ops.as_slice()),
            payload: TexViewerPayload::GpuOps(self.ops.as_slice()),
        })
    }
}

fn ops_plan_signature(ops: &[TexViewerOp]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for op in ops {
        match *op {
            TexViewerOp::Solid { .. } => hash = fnv1a(hash, 1),
            TexViewerOp::Circle { .. } => hash = fnv1a(hash, 2),
            TexViewerOp::Sphere { .. } => hash = fnv1a(hash, 3),
            TexViewerOp::Transform { .. } => hash = fnv1a(hash, 4),
            TexViewerOp::Level { .. } => hash = fnv1a(hash, 5),
            TexViewerOp::Feedback { history, .. } => {
                hash = fnv1a(hash, 6);
                hash = hash_feedback_binding(hash, history);
            }
            TexViewerOp::ReactionDiffusion { history, .. } => {
                hash = fnv1a(hash, 7);
                hash = hash_feedback_binding(hash, history);
            }
            TexViewerOp::PostProcess {
                category, history, ..
            } => {
                hash = fnv1a(hash, 8);
                hash = fnv1a(hash, category as u64);
                hash = match history {
                    Some(binding) => hash_feedback_binding(fnv1a(hash, 1), binding),
                    None => fnv1a(hash, 0),
                };
            }
            TexViewerOp::StoreTexture { texture_node_id } => {
                hash = fnv1a(hash, 9);
                hash = fnv1a(hash, texture_node_id as u64);
            }
            TexViewerOp::Blend {
                base_texture_node_id,
                layer_texture_node_id,
                ..
            } => {
                hash = fnv1a(hash, 10);
                hash = fnv1a(hash, base_texture_node_id as u64);
                hash = fnv1a(hash, layer_texture_node_id.unwrap_or(0) as u64);
            }
        }
    }
    hash
}

fn ops_uniform_signature(ops: &[TexViewerOp]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for op in ops {
        match *op {
            TexViewerOp::Solid {
                color_r,
                color_g,
                color_b,
                alpha,
            } => {
                hash = fnv1a(hash, 1);
                hash = hash_f32(hash, color_r);
                hash = hash_f32(hash, color_g);
                hash = hash_f32(hash, color_b);
                hash = hash_f32(hash, alpha);
            }
            TexViewerOp::Circle {
                center_x,
                center_y,
                radius,
                feather,
                line_width,
                noise_amount,
                noise_freq,
                noise_phase,
                noise_twist,
                noise_stretch,
                arc_start_deg,
                arc_end_deg,
                segment_count,
                arc_open,
                color_r,
                color_g,
                color_b,
                alpha,
                alpha_clip,
            } => {
                hash = fnv1a(hash, 2);
                for value in [
                    center_x,
                    center_y,
                    radius,
                    feather,
                    line_width,
                    noise_amount,
                    noise_freq,
                    noise_phase,
                    noise_twist,
                    noise_stretch,
                    arc_start_deg,
                    arc_end_deg,
                    segment_count,
                    arc_open,
                    color_r,
                    color_g,
                    color_b,
                    alpha,
                ] {
                    hash = hash_f32(hash, value);
                }
                hash = fnv1a(hash, alpha_clip as u64);
            }
            TexViewerOp::Sphere {
                center_x,
                center_y,
                radius,
                edge_softness,
                noise_amount,
                noise_freq,
                noise_phase,
                noise_twist,
                noise_stretch,
                light_x,
                light_y,
                light_z,
                ambient,
                color_r,
                color_g,
                color_b,
                alpha,
                alpha_clip,
            } => {
                hash = fnv1a(hash, 3);
                for value in [
                    center_x,
                    center_y,
                    radius,
                    edge_softness,
                    noise_amount,
                    noise_freq,
                    noise_phase,
                    noise_twist,
                    noise_stretch,
                    light_x,
                    light_y,
                    light_z,
                    ambient,
                    color_r,
                    color_g,
                    color_b,
                    alpha,
                ] {
                    hash = hash_f32(hash, value);
                }
                hash = fnv1a(hash, alpha_clip as u64);
            }
            TexViewerOp::Transform {
                brightness,
                gain_r,
                gain_g,
                gain_b,
                alpha_mul,
            } => {
                hash = fnv1a(hash, 4);
                for value in [brightness, gain_r, gain_g, gain_b, alpha_mul] {
                    hash = hash_f32(hash, value);
                }
            }
            TexViewerOp::Level {
                in_low,
                in_high,
                gamma,
                out_low,
                out_high,
            } => {
                hash = fnv1a(hash, 5);
                for value in [in_low, in_high, gamma, out_low, out_high] {
                    hash = hash_f32(hash, value);
                }
            }
            TexViewerOp::Feedback {
                mix,
                frame_gap,
                history,
            } => {
                hash = fnv1a(hash, 6);
                hash = hash_f32(hash, mix);
                hash = fnv1a(hash, frame_gap as u64);
                hash = hash_feedback_binding(hash, history);
            }
            TexViewerOp::ReactionDiffusion {
                diffusion_a,
                diffusion_b,
                feed,
                kill,
                dt,
                seed_mix,
                history,
            } => {
                hash = fnv1a(hash, 7);
                for value in [diffusion_a, diffusion_b, feed, kill, dt, seed_mix] {
                    hash = hash_f32(hash, value);
                }
                hash = hash_feedback_binding(hash, history);
            }
            TexViewerOp::PostProcess {
                category,
                effect,
                amount,
                scale,
                threshold,
                speed,
                time,
                history,
            } => {
                hash = fnv1a(hash, 8);
                hash = fnv1a(hash, category as u64);
                for value in [effect, amount, scale, threshold, speed, time] {
                    hash = hash_f32(hash, value);
                }
                hash = match history {
                    Some(binding) => hash_feedback_binding(fnv1a(hash, 1), binding),
                    None => fnv1a(hash, 0),
                };
            }
            TexViewerOp::StoreTexture { texture_node_id } => {
                hash = fnv1a(hash, 9);
                hash = fnv1a(hash, texture_node_id as u64);
            }
            TexViewerOp::Blend {
                mode,
                opacity,
                bg_r,
                bg_g,
                bg_b,
                bg_a,
                base_texture_node_id,
                layer_texture_node_id,
            } => {
                hash = fnv1a(hash, 10);
                for value in [mode, opacity, bg_r, bg_g, bg_b, bg_a] {
                    hash = hash_f32(hash, value);
                }
                hash = fnv1a(hash, base_texture_node_id as u64);
                hash = fnv1a(hash, layer_texture_node_id.unwrap_or(0) as u64);
            }
        }
    }
    hash
}

fn hash_feedback_binding(
    mut hash: u64,
    binding: crate::gui::runtime::TexRuntimeFeedbackHistoryBinding,
) -> u64 {
    hash = match binding {
        crate::gui::runtime::TexRuntimeFeedbackHistoryBinding::Internal { feedback_node_id } => {
            fnv1a(hash, 1 ^ feedback_node_id as u64)
        }
        crate::gui::runtime::TexRuntimeFeedbackHistoryBinding::External { texture_node_id } => {
            fnv1a(hash, 2 ^ texture_node_id as u64)
        }
    };
    hash
}

fn hash_f32(hash: u64, value: f32) -> u64 {
    fnv1a(hash, value.to_bits() as u64)
}

fn fnv1a(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(0x100000001b3)
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

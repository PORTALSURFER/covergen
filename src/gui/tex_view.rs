//! GUI tex preview planning with compiled GPU-runtime evaluation.
//!
//! The generator caches one compiled render chain and frame-keyed operation
//! payload so the renderer executes a single GPU-only preview path.

use super::project::GuiProject;
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
    compiled_runtime: Option<GuiCompiledRuntime>,
    ops: Vec<TexViewerOp>,
    eval_stack: Vec<u32>,
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
        let panel_w = update.viewport_width.saturating_sub(update.panel_width) as u32;
        let panel_h = editor_panel_height(update.viewport_height) as u32;
        let dynamic_frame = if project.has_signal_bindings() || project.has_temporal_nodes() {
            update.frame_index
        } else {
            0
        };
        if self.compiled_epoch != Some(update.tex_eval_epoch) {
            self.compiled_runtime = GuiCompiledRuntime::compile(project);
            self.compiled_epoch = Some(update.tex_eval_epoch);
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
mod tests {
    use super::{TexViewerGenerator, TexViewerOp, TexViewerPayload, TexViewerUpdate};
    use crate::gui::project::{GuiProject, ProjectNodeKind};
    use crate::gui::timeline::editor_panel_height;

    #[test]
    fn supported_graph_emits_gpu_ops_payload() {
        let mut project = GuiProject::new_empty(640, 480);
        let tex_source = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(tex_source, out));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TexViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
    }

    #[test]
    fn transform_chain_produces_solid_then_transform_ops() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
        assert!(project.connect_image_link(solid, xform));
        assert!(project.connect_image_link(xform, out));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TexViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
        assert!(matches!(ops[1], TexViewerOp::Transform { .. }));
    }

    #[test]
    fn level_chain_produces_solid_then_level_ops() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
        let level = project.add_node(ProjectNodeKind::TexLevel, 180, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
        assert!(project.connect_image_link(solid, level));
        assert!(project.connect_image_link(level, out));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TexViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
        assert!(matches!(ops[1], TexViewerOp::Level { .. }));
    }

    #[test]
    fn feedback_chain_produces_solid_then_feedback_ops() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
        assert!(project.connect_image_link(solid, feedback));
        assert!(project.connect_image_link(feedback, out));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TexViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
        assert!(matches!(ops[1], TexViewerOp::Feedback { .. }));
    }

    #[test]
    fn lfo_binding_changes_gpu_op_parameter_over_time() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 180, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
        assert!(project.connect_image_link(solid, out));
        assert!(project.toggle_node_expanded(solid, 420, 480));
        assert!(project.connect_image_link(lfo, solid));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let r0 = match viewer.frame().expect("frame0").payload {
            TexViewerPayload::GpuOps(ops) => match ops[0] {
                TexViewerOp::Solid { color_r, .. } => color_r,
                _ => panic!("first op should be solid"),
            },
        };
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 60,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let r1 = match viewer.frame().expect("frame1").payload {
            TexViewerPayload::GpuOps(ops) => match ops[0] {
                TexViewerOp::Solid { color_r, .. } => color_r,
                _ => panic!("first op should be solid"),
            },
        };
        assert_ne!(r0, r1);
    }

    #[test]
    fn circle_node_emits_circle_op() {
        let mut project = GuiProject::new_empty(640, 480);
        let circle = project.add_node(ProjectNodeKind::TexCircle, 60, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(circle, out));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TexViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TexViewerOp::Circle { .. }));
    }

    #[test]
    fn sphere_buffer_pipeline_emits_sphere_op() {
        let mut project = GuiProject::new_empty(640, 480);
        let sphere = project.add_node(ProjectNodeKind::BufSphere, 60, 80, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 220, 80, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 380, 80, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 540, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 700, 80, 420, 480);
        assert!(project.connect_image_link(sphere, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TexViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TexViewerOp::Sphere { .. }));
    }

    #[test]
    fn scene_pass_resolution_overrides_output_texture_size() {
        let mut project = GuiProject::new_empty(640, 480);
        let sphere = project.add_node(ProjectNodeKind::BufSphere, 60, 80, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 220, 80, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 380, 80, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 540, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 700, 80, 420, 480);
        assert!(project.connect_image_link(sphere, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));
        assert!(project.set_param_value(pass, 0, 1024.0));
        assert!(project.set_param_value(pass, 1, 256.0));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 1200,
                viewport_height: 700,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        assert_eq!(frame.texture_width, 1024);
        assert_eq!(frame.texture_height, 256);
    }

    #[test]
    fn circle_nurbs_buffer_pipeline_emits_circle_op() {
        let mut project = GuiProject::new_empty(640, 480);
        let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 60, 80, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 220, 80, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 380, 80, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 540, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 700, 80, 420, 480);
        assert!(project.connect_image_link(circle, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TexViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TexViewerOp::Circle { .. }));
    }

    #[test]
    fn buffer_noise_chain_emits_scene_op() {
        let mut project = GuiProject::new_empty(640, 480);
        let sphere = project.add_node(ProjectNodeKind::BufSphere, 60, 80, 420, 480);
        let noise = project.add_node(ProjectNodeKind::BufNoise, 220, 80, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 380, 80, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 540, 80, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 700, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 860, 80, 420, 480);
        assert!(project.connect_image_link(sphere, noise));
        assert!(project.connect_image_link(noise, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 1200,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TexViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TexViewerOp::Sphere { .. }));
    }

    #[test]
    fn buffer_noise_chain_remains_time_dynamic_without_signal_bindings() {
        let mut project = GuiProject::new_empty(640, 480);
        let sphere = project.add_node(ProjectNodeKind::BufSphere, 60, 80, 420, 480);
        let noise = project.add_node(ProjectNodeKind::BufNoise, 220, 80, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 380, 80, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 540, 80, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 700, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 860, 80, 420, 480);
        assert!(project.connect_image_link(sphere, noise));
        assert!(project.connect_image_link(noise, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));
        assert!(!project.has_signal_bindings());

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 1200,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let phase_t0 = match viewer.frame().expect("frame0").payload {
            TexViewerPayload::GpuOps(ops) => match ops[0] {
                TexViewerOp::Sphere { noise_phase, .. } => noise_phase,
                _ => panic!("expected sphere op"),
            },
        };

        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 1200,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 60,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let phase_t1 = match viewer.frame().expect("frame1").payload {
            TexViewerPayload::GpuOps(ops) => match ops[0] {
                TexViewerOp::Sphere { noise_phase, .. } => noise_phase,
                _ => panic!("expected sphere op"),
            },
        };

        assert_ne!(phase_t0, phase_t1);
    }

    #[test]
    fn ui_only_state_changes_do_not_invalidate_preview_cache_key() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(solid, out));

        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let base_key = viewer.key;

        assert!(project.toggle_node_expanded(solid, 420, 480));
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        assert_eq!(viewer.key, base_key);

        assert!(project.select_next_param(solid));
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        assert_eq!(viewer.key, base_key);
    }

    #[test]
    fn disconnected_graph_returns_empty_gpu_payload() {
        let project = GuiProject::new_empty(640, 480);
        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 960,
                viewport_height: 540,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TexViewerPayload::GpuOps(ops) => ops,
        };
        assert!(ops.is_empty());
    }

    #[test]
    fn viewer_frame_fits_texture_aspect_inside_output_panel() {
        let project = GuiProject::new_empty(1920, 1080);
        let mut viewer = TexViewerGenerator::default();
        viewer.update(
            &project,
            TexViewerUpdate {
                viewport_width: 1200,
                viewport_height: 900,
                panel_width: 420,
                frame_index: 0,
                timeline_total_frames: 1_800,
                timeline_fps: 60,
                tex_eval_epoch: project.invalidation().tex_eval,
            },
        );
        let frame = viewer.frame().expect("viewer frame should exist");
        assert_eq!(frame.texture_width, 1920);
        assert_eq!(frame.texture_height, 1080);
        assert_eq!(frame.width, 780);
        assert_eq!(frame.height, 438);
        assert_eq!(frame.x, 420);
        let expected_y = ((editor_panel_height(900) as u32 - frame.height) / 2) as i32;
        assert_eq!(frame.y, expected_y);
    }
}

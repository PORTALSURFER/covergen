//! GUI TOP preview planning with compiled GPU-runtime evaluation.
//!
//! The generator caches one compiled render chain and frame-keyed operation
//! payload so the renderer executes a single GPU-only preview path.

use super::project::GuiProject;
use super::runtime::{GuiCompiledRuntime, TopRuntimeFrameContext};
use super::timeline::{editor_panel_height, TIMELINE_TOTAL_FRAMES};

/// Re-exported TOP operation type consumed by preview rendering.
pub(crate) use super::runtime::TopRuntimeOp as TopViewerOp;

/// TOP viewer payload consumed by the GUI renderer.
pub(crate) enum TopViewerPayload<'a> {
    /// GPU operation chain executed into the viewer target.
    GpuOps(&'a [TopViewerOp]),
}

/// Borrowed frame payload for one TOP viewer render.
pub(crate) struct TopViewerFrame<'a> {
    /// Panel-space x-offset of fitted preview quad.
    pub(crate) x: i32,
    /// Panel-space y-offset of fitted preview quad.
    pub(crate) y: i32,
    /// Panel-space fitted preview quad width.
    pub(crate) width: u32,
    /// Panel-space fitted preview quad height.
    pub(crate) height: u32,
    /// Backing GPU texture width used for TOP evaluation.
    pub(crate) texture_width: u32,
    /// Backing GPU texture height used for TOP evaluation.
    pub(crate) texture_height: u32,
    pub(crate) payload: TopViewerPayload<'a>,
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
    frame_index: u32,
}

/// Cached TOP preview payload producer.
#[derive(Debug, Default)]
pub(crate) struct TopViewerGenerator {
    key: Option<ViewerCacheKey>,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    compiled_signature: Option<u64>,
    compiled_runtime: Option<GuiCompiledRuntime>,
    ops: Vec<TopViewerOp>,
    eval_stack: Vec<u32>,
}

impl TopViewerGenerator {
    /// Update cached viewer payload for current panel split and graph state.
    pub(crate) fn update(
        &mut self,
        project: &GuiProject,
        viewport_width: usize,
        viewport_height: usize,
        panel_width: usize,
        frame_index: u32,
        timeline_fps: u32,
    ) {
        let panel_w = viewport_width.saturating_sub(panel_width) as u32;
        let panel_h = editor_panel_height(viewport_height) as u32;
        let render_signature = project.render_signature();
        let dynamic_frame = if project.has_signal_bindings() || project.has_temporal_nodes() {
            frame_index
        } else {
            0
        };
        if self.compiled_signature != Some(render_signature) {
            self.compiled_runtime = GuiCompiledRuntime::compile(project);
            self.compiled_signature = Some(render_signature);
        }
        let time_secs = frame_index as f32 / timeline_fps.max(1) as f32;
        let (texture_width, texture_height) = self
            .compiled_runtime
            .as_ref()
            .map(|runtime| runtime.output_texture_size(project, time_secs, &mut self.eval_stack))
            .unwrap_or((project.preview_width.max(1), project.preview_height.max(1)));
        let (view_width, view_height) =
            fit_aspect_in_rect(panel_w, panel_h, texture_width, texture_height);
        let x = panel_width as i32 + (panel_w.saturating_sub(view_width) / 2) as i32;
        let y = (panel_h.saturating_sub(view_height) / 2) as i32;
        let key = ViewerCacheKey {
            panel_width: panel_w,
            panel_height: panel_h,
            view_width,
            view_height,
            texture_width,
            texture_height,
            render_signature,
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
                Some(TopRuntimeFrameContext {
                    frame_index: dynamic_frame,
                    frame_total: TIMELINE_TOTAL_FRAMES,
                }),
                &mut self.eval_stack,
                &mut self.ops,
            );
        }
    }

    /// Return current frame payload, if viewer dimensions are valid.
    pub(crate) fn frame(&self) -> Option<TopViewerFrame<'_>> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        Some(TopViewerFrame {
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
            payload: TopViewerPayload::GpuOps(self.ops.as_slice()),
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
    use super::{TopViewerGenerator, TopViewerOp, TopViewerPayload};
    use crate::gui::project::{GuiProject, ProjectNodeKind};

    #[test]
    fn supported_graph_emits_gpu_ops_payload() {
        let mut project = GuiProject::new_empty(640, 480);
        let top = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(top, out));

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TopViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TopViewerOp::Solid { .. }));
    }

    #[test]
    fn transform_chain_produces_solid_then_transform_ops() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
        assert!(project.connect_image_link(solid, xform));
        assert!(project.connect_image_link(xform, out));

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TopViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], TopViewerOp::Solid { .. }));
        assert!(matches!(ops[1], TopViewerOp::Transform { .. }));
    }

    #[test]
    fn feedback_chain_produces_solid_then_feedback_ops() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
        assert!(project.connect_image_link(solid, feedback));
        assert!(project.connect_image_link(feedback, out));

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TopViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], TopViewerOp::Solid { .. }));
        assert!(matches!(ops[1], TopViewerOp::Feedback { .. }));
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

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let r0 = match viewer.frame().expect("frame0").payload {
            TopViewerPayload::GpuOps(ops) => match ops[0] {
                TopViewerOp::Solid { color_r, .. } => color_r,
                _ => panic!("first op should be solid"),
            },
        };
        viewer.update(&project, 960, 540, 420, 60, 60);
        let r1 = match viewer.frame().expect("frame1").payload {
            TopViewerPayload::GpuOps(ops) => match ops[0] {
                TopViewerOp::Solid { color_r, .. } => color_r,
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

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TopViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TopViewerOp::Circle { .. }));
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

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TopViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TopViewerOp::Sphere { .. }));
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

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 1200, 700, 420, 0, 60);
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

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TopViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TopViewerOp::Circle { .. }));
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

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 1200, 540, 420, 0, 60);
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TopViewerPayload::GpuOps(ops) => ops,
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TopViewerOp::Sphere { .. }));
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

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 1200, 540, 420, 0, 60);
        let phase_t0 = match viewer.frame().expect("frame0").payload {
            TopViewerPayload::GpuOps(ops) => match ops[0] {
                TopViewerOp::Sphere { noise_phase, .. } => noise_phase,
                _ => panic!("expected sphere op"),
            },
        };

        viewer.update(&project, 1200, 540, 420, 60, 60);
        let phase_t1 = match viewer.frame().expect("frame1").payload {
            TopViewerPayload::GpuOps(ops) => match ops[0] {
                TopViewerOp::Sphere { noise_phase, .. } => noise_phase,
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

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let base_key = viewer.key;

        assert!(project.toggle_node_expanded(solid, 420, 480));
        viewer.update(&project, 960, 540, 420, 0, 60);
        assert_eq!(viewer.key, base_key);

        assert!(project.select_next_param(solid));
        viewer.update(&project, 960, 540, 420, 0, 60);
        assert_eq!(viewer.key, base_key);
    }

    #[test]
    fn disconnected_graph_returns_empty_gpu_payload() {
        let project = GuiProject::new_empty(640, 480);
        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let frame = viewer.frame().expect("viewer frame should exist");
        let ops = match frame.payload {
            TopViewerPayload::GpuOps(ops) => ops,
        };
        assert!(ops.is_empty());
    }

    #[test]
    fn viewer_frame_fits_texture_aspect_inside_output_panel() {
        let project = GuiProject::new_empty(1920, 1080);
        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 1200, 900, 420, 0, 60);
        let frame = viewer.frame().expect("viewer frame should exist");
        assert_eq!(frame.texture_width, 1920);
        assert_eq!(frame.texture_height, 1080);
        assert_eq!(frame.width, 780);
        assert_eq!(frame.height, 438);
        assert_eq!(frame.x, 420);
        assert_eq!(frame.y, 216);
    }
}

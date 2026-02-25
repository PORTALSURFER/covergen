//! GUI TOP preview planning with compiled GPU-runtime evaluation.
//!
//! The generator caches one compiled render chain and frame-keyed operation
//! payload so the renderer executes a single GPU-only preview path.

use super::project::GuiProject;
use super::runtime::GuiCompiledRuntime;

/// Re-exported TOP operation type consumed by preview rendering.
pub(crate) use super::runtime::TopRuntimeOp as TopViewerOp;

/// TOP viewer payload consumed by the GUI renderer.
pub(crate) enum TopViewerPayload<'a> {
    /// GPU operation chain executed into the viewer target.
    GpuOps(&'a [TopViewerOp]),
}

/// Borrowed frame payload for one TOP viewer render.
pub(crate) struct TopViewerFrame<'a> {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) payload: TopViewerPayload<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ViewerCacheKey {
    width: u32,
    height: u32,
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
        let width = viewport_width.saturating_sub(panel_width) as u32;
        let height = viewport_height as u32;
        let render_signature = project.render_signature();
        let dynamic_frame = if project.has_signal_bindings() {
            frame_index
        } else {
            0
        };
        let key = ViewerCacheKey {
            width,
            height,
            render_signature,
            frame_index: dynamic_frame,
        };
        self.x = panel_width as i32;
        self.y = 0;
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.width = width;
        self.height = height;

        if self.compiled_signature != Some(render_signature) {
            self.compiled_runtime = GuiCompiledRuntime::compile(project);
            self.compiled_signature = Some(render_signature);
        }

        self.ops.clear();
        if let Some(compiled_runtime) = &self.compiled_runtime {
            let time_secs = frame_index as f32 / timeline_fps.max(1) as f32;
            compiled_runtime.evaluate_ops(project, time_secs, &mut self.eval_stack, &mut self.ops);
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
            payload: TopViewerPayload::GpuOps(self.ops.as_slice()),
        })
    }
}

#[cfg(test)]
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
}

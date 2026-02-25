//! GUI TOP preview planning with GPU-first evaluation.
//!
//! The generator produces a cached per-frame preview payload. For supported
//! node chains, the payload is a GPU operation list executed directly in the
//! renderer. Unsupported chains fall back to CPU rasterization.

use super::project::{GuiProject, ProjectNodeKind};
use super::top_view_cpu::generate_output_pixels;

/// One GPU-evaluable preview operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TopViewerOp {
    /// `tex.solid` operation.
    Solid {
        center_x: f32,
        center_y: f32,
        radius: f32,
        feather: f32,
        color_r: f32,
        color_g: f32,
        color_b: f32,
        alpha: f32,
    },
    /// `tex.transform_2d` operation.
    Transform {
        brightness: f32,
        gain_r: f32,
        gain_g: f32,
        gain_b: f32,
        alpha_mul: f32,
    },
}

/// TOP viewer payload consumed by the GUI renderer.
pub(crate) enum TopViewerPayload<'a> {
    /// CPU-generated RGBA8 pixels.
    CpuRgba8(&'a [u8]),
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
    graph_signature: u64,
    frame_index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewerContentMode {
    Cpu,
    Gpu,
}

/// Cached TOP preview payload producer.
#[derive(Debug)]
pub(crate) struct TopViewerGenerator {
    key: Option<ViewerCacheKey>,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    content_mode: ViewerContentMode,
    pixels: Vec<u8>,
    scratch: Vec<u8>,
    ops: Vec<TopViewerOp>,
    eval_stack: Vec<u32>,
}

impl Default for TopViewerGenerator {
    fn default() -> Self {
        Self {
            key: None,
            width: 0,
            height: 0,
            x: 0,
            y: 0,
            content_mode: ViewerContentMode::Cpu,
            pixels: Vec::new(),
            scratch: Vec::new(),
            ops: Vec::new(),
            eval_stack: Vec::new(),
        }
    }
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
        let graph_signature = project.graph_signature();
        let dynamic_frame = if project.has_signal_bindings() {
            frame_index
        } else {
            0
        };
        let key = ViewerCacheKey {
            width,
            height,
            graph_signature,
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
        let time_secs = frame_index as f32 / timeline_fps.max(1) as f32;
        if self.build_gpu_ops(project, time_secs) {
            self.content_mode = ViewerContentMode::Gpu;
            return;
        }

        self.content_mode = ViewerContentMode::Cpu;
        let pixel_count = width.saturating_mul(height).saturating_mul(4) as usize;
        self.pixels.resize(pixel_count, 0);
        self.scratch.resize(pixel_count, 0);
        self.eval_stack.clear();
        generate_output_pixels(
            &mut self.pixels,
            &mut self.scratch,
            width,
            height,
            project,
            time_secs,
            &mut self.eval_stack,
        );
    }

    /// Return current frame payload, if viewer dimensions are valid.
    pub(crate) fn frame(&self) -> Option<TopViewerFrame<'_>> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let payload = match self.content_mode {
            ViewerContentMode::Cpu => {
                if self.pixels.is_empty() {
                    return None;
                }
                TopViewerPayload::CpuRgba8(self.pixels.as_slice())
            }
            ViewerContentMode::Gpu => {
                if self.ops.is_empty() {
                    return None;
                }
                TopViewerPayload::GpuOps(self.ops.as_slice())
            }
        };
        Some(TopViewerFrame {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            payload,
        })
    }

    fn build_gpu_ops(&mut self, project: &GuiProject, time_secs: f32) -> bool {
        self.ops.clear();
        self.eval_stack.clear();
        let Some(output_source_id) = project.window_out_input_node_id() else {
            return false;
        };
        collect_gpu_ops(
            project,
            output_source_id,
            time_secs,
            &mut self.ops,
            &mut self.eval_stack,
        )
    }
}

fn collect_gpu_ops(
    project: &GuiProject,
    node_id: u32,
    time_secs: f32,
    out_ops: &mut Vec<TopViewerOp>,
    eval_stack: &mut Vec<u32>,
) -> bool {
    if eval_stack.contains(&node_id) {
        return false;
    }
    let Some(node) = project.node(node_id) else {
        return false;
    };
    eval_stack.push(node_id);
    let ok = match node.kind() {
        ProjectNodeKind::TexSolid => {
            out_ops.push(TopViewerOp::Solid {
                center_x: project
                    .node_param_value(node_id, "center_x", time_secs, eval_stack)
                    .unwrap_or(0.5),
                center_y: project
                    .node_param_value(node_id, "center_y", time_secs, eval_stack)
                    .unwrap_or(0.5),
                radius: project
                    .node_param_value(node_id, "radius", time_secs, eval_stack)
                    .unwrap_or(0.24),
                feather: project
                    .node_param_value(node_id, "feather", time_secs, eval_stack)
                    .unwrap_or(0.06),
                color_r: project
                    .node_param_value(node_id, "color_r", time_secs, eval_stack)
                    .unwrap_or(0.9),
                color_g: project
                    .node_param_value(node_id, "color_g", time_secs, eval_stack)
                    .unwrap_or(0.9),
                color_b: project
                    .node_param_value(node_id, "color_b", time_secs, eval_stack)
                    .unwrap_or(0.9),
                alpha: project
                    .node_param_value(node_id, "alpha", time_secs, eval_stack)
                    .unwrap_or(1.0),
            });
            true
        }
        ProjectNodeKind::TexTransform2D => {
            if let Some(source_id) = project.input_source_node_id(node_id) {
                if !collect_gpu_ops(project, source_id, time_secs, out_ops, eval_stack) {
                    false
                } else {
                    out_ops.push(TopViewerOp::Transform {
                        brightness: project
                            .node_param_value(node_id, "brightness", time_secs, eval_stack)
                            .unwrap_or(1.08),
                        gain_r: project
                            .node_param_value(node_id, "gain_r", time_secs, eval_stack)
                            .unwrap_or(0.45),
                        gain_g: project
                            .node_param_value(node_id, "gain_g", time_secs, eval_stack)
                            .unwrap_or(0.8),
                        gain_b: project
                            .node_param_value(node_id, "gain_b", time_secs, eval_stack)
                            .unwrap_or(1.0),
                        alpha_mul: project
                            .node_param_value(node_id, "alpha_mul", time_secs, eval_stack)
                            .unwrap_or(0.8),
                    });
                    true
                }
            } else {
                false
            }
        }
        ProjectNodeKind::CtlLfo | ProjectNodeKind::IoWindowOut => false,
    };
    eval_stack.pop();
    ok
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
        match frame.payload {
            TopViewerPayload::GpuOps(ops) => {
                assert_eq!(ops.len(), 1);
                assert!(matches!(ops[0], TopViewerOp::Solid { .. }));
            }
            TopViewerPayload::CpuRgba8(_) => panic!("expected GPU operation payload"),
        }
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
            TopViewerPayload::CpuRgba8(_) => panic!("expected GPU operation payload"),
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
        assert!(project.select_next_param(solid));
        assert!(project.select_next_param(solid));
        assert!(project.select_next_param(solid));
        assert!(project.select_next_param(solid));
        assert!(project.connect_image_link(lfo, solid));

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let r0 = match viewer.frame().expect("frame0").payload {
            TopViewerPayload::GpuOps(ops) => match ops[0] {
                TopViewerOp::Solid { color_r, .. } => color_r,
                _ => panic!("first op should be solid"),
            },
            TopViewerPayload::CpuRgba8(_) => panic!("expected GPU operation payload"),
        };
        viewer.update(&project, 960, 540, 420, 60, 60);
        let r1 = match viewer.frame().expect("frame1").payload {
            TopViewerPayload::GpuOps(ops) => match ops[0] {
                TopViewerOp::Solid { color_r, .. } => color_r,
                _ => panic!("first op should be solid"),
            },
            TopViewerPayload::CpuRgba8(_) => panic!("expected GPU operation payload"),
        };
        assert_ne!(r0, r1);
    }

    #[test]
    fn disconnected_graph_uses_cpu_fallback_payload() {
        let project = GuiProject::new_empty(640, 480);
        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420, 0, 60);
        let frame = viewer.frame().expect("viewer frame should exist");
        match frame.payload {
            TopViewerPayload::CpuRgba8(bytes) => assert!(!bytes.is_empty()),
            TopViewerPayload::GpuOps(_) => panic!("expected CPU fallback payload"),
        }
    }
}

//! CPU-side TOP viewer buffer generation for GUI preview.
//!
//! The preview evaluator currently supports a small typed subset:
//!
//! - `tex.solid`: generates a circle texture.
//! - `tex.transform_2d`: mutates incoming texture color and alpha.
//! - `io.window_out`: output sink.
//!
//! Evaluation is pull-based from `io.window_out`.

use super::project::{GuiProject, ProjectNodeKind};

/// Borrowed output buffer view consumed by the GUI renderer.
pub(crate) struct TopViewerFrame<'a> {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba8: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ViewerCacheKey {
    width: u32,
    height: u32,
    graph_signature: u64,
}

/// Cached TOP preview buffer producer.
#[derive(Debug, Default)]
pub(crate) struct TopViewerGenerator {
    key: Option<ViewerCacheKey>,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    pixels: Vec<u8>,
    scratch: Vec<u8>,
    eval_stack: Vec<u32>,
}

impl TopViewerGenerator {
    /// Update cached viewer buffer for current panel split and project wiring.
    pub(crate) fn update(
        &mut self,
        project: &GuiProject,
        viewport_width: usize,
        viewport_height: usize,
        panel_width: usize,
    ) {
        let width = viewport_width.saturating_sub(panel_width) as u32;
        let height = viewport_height as u32;
        let graph_signature = project.graph_signature();
        let key = ViewerCacheKey {
            width,
            height,
            graph_signature,
        };
        self.x = panel_width as i32;
        self.y = 0;
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.width = width;
        self.height = height;
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
            &mut self.eval_stack,
        );
    }

    /// Return current frame view, if viewer dimensions are valid.
    pub(crate) fn frame(&self) -> Option<TopViewerFrame<'_>> {
        if self.width == 0 || self.height == 0 || self.pixels.is_empty() {
            return None;
        }
        Some(TopViewerFrame {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            rgba8: self.pixels.as_slice(),
        })
    }
}

fn generate_output_pixels(
    out: &mut [u8],
    scratch: &mut [u8],
    width: u32,
    height: u32,
    project: &GuiProject,
    eval_stack: &mut Vec<u32>,
) {
    fill_rgba(out, 8, 8, 8, 255);
    if width == 0 || height == 0 {
        return;
    }
    let Some(output_source_id) = project.window_out_input_node_id() else {
        return;
    };
    let rendered = render_node(
        project,
        output_source_id,
        width,
        height,
        out,
        scratch,
        eval_stack,
    );
    if !rendered {
        fill_rgba(out, 8, 8, 8, 255);
    }
}

fn fill_rgba(out: &mut [u8], r: u8, g: u8, b: u8, a: u8) {
    for px in out.chunks_exact_mut(4) {
        px[0] = r;
        px[1] = g;
        px[2] = b;
        px[3] = a;
    }
}

fn draw_circle(out: &mut [u8], width: u32, height: u32) {
    let cx = (width as f32) * 0.5;
    let cy = (height as f32) * 0.5;
    let radius = (width.min(height) as f32) * 0.24;
    let feather = radius * 0.06;
    if radius <= 1.0 {
        return;
    }
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let alpha = smoothstep(radius + feather, radius - feather, dist);
            if alpha <= 0.0 {
                continue;
            }
            let idx = ((y * width + x) * 4) as usize;
            let base = out[idx] as f32;
            let lit = base + alpha * 228.0;
            let c = lit.clamp(0.0, 255.0) as u8;
            out[idx] = c;
            out[idx + 1] = c;
            out[idx + 2] = c;
            out[idx + 3] = 255;
        }
    }
}

fn render_node(
    project: &GuiProject,
    node_id: u32,
    width: u32,
    height: u32,
    out: &mut [u8],
    scratch: &mut [u8],
    eval_stack: &mut Vec<u32>,
) -> bool {
    if eval_stack.contains(&node_id) {
        return false;
    }
    let Some(node) = project.node(node_id) else {
        return false;
    };
    eval_stack.push(node_id);
    let rendered = match node.kind() {
        ProjectNodeKind::TexSolid => {
            fill_rgba(out, 8, 8, 8, 255);
            draw_circle(out, width, height);
            true
        }
        ProjectNodeKind::TexTransform2D => {
            let Some(source_id) = project.input_source_node_id(node_id) else {
                false
            } else if !render_node(project, source_id, width, height, scratch, out, eval_stack) {
                false
            } else {
                out.copy_from_slice(scratch);
                apply_tex_transform(out);
                true
            }
        }
        ProjectNodeKind::IoWindowOut => false,
    };
    eval_stack.pop();
    rendered
}

fn apply_tex_transform(out: &mut [u8]) {
    const TINT_R: f32 = 0.45;
    const TINT_G: f32 = 0.8;
    const TINT_B: f32 = 1.0;
    const BRIGHTNESS: f32 = 1.08;
    const ALPHA_MUL: f32 = 0.8;

    for px in out.chunks_exact_mut(4) {
        let r = ((px[0] as f32) * TINT_R * BRIGHTNESS).clamp(0.0, 255.0);
        let g = ((px[1] as f32) * TINT_G * BRIGHTNESS).clamp(0.0, 255.0);
        let b = ((px[2] as f32) * TINT_B * BRIGHTNESS).clamp(0.0, 255.0);
        let a = ((px[3] as f32) * ALPHA_MUL).clamp(0.0, 255.0);
        px[0] = r as u8;
        px[1] = g as u8;
        px[2] = b as u8;
        px[3] = a as u8;
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::TopViewerGenerator;
    use crate::gui::project::{GuiProject, ProjectNodeKind};

    #[test]
    fn viewer_generates_pixels_when_solid_is_connected_to_window_out() {
        let mut project = GuiProject::new_empty(640, 480);
        let top = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(top, out));
        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420);
        let frame = viewer.frame().expect("viewer frame should exist");
        assert_eq!(frame.width, 540);
        assert_eq!(frame.height, 540);
        assert!(!frame.rgba8.is_empty());
    }

    #[test]
    fn transform_node_mutates_color_and_alpha() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
        assert!(project.connect_image_link(solid, xform));
        assert!(project.connect_image_link(xform, out));

        let mut viewer = TopViewerGenerator::default();
        viewer.update(&project, 960, 540, 420);
        let frame = viewer.frame().expect("viewer frame should exist");
        let cx = frame.width / 2;
        let cy = frame.height / 2;
        let idx = ((cy * frame.width + cx) * 4) as usize;
        let r = frame.rgba8[idx];
        let g = frame.rgba8[idx + 1];
        let b = frame.rgba8[idx + 2];
        let a = frame.rgba8[idx + 3];
        assert!(r != g || g != b);
        assert!(a < 255);
    }
}

//! CPU-side TOP viewer buffer generation for GUI preview.
//!
//! The current implementation keeps scope intentionally small: when an
//! `io.window_out` node is wired from a `tex.solid` source, it produces a single
//! circle buffer that is uploaded to the right-side TOP viewer.

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
    source_kind: Option<ProjectNodeKind>,
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
        let source_kind = project.output_source_kind();
        let key = ViewerCacheKey {
            width,
            height,
            source_kind,
        };
        self.x = panel_width as i32;
        self.y = 0;
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.width = width;
        self.height = height;
        self.pixels
            .resize(width.saturating_mul(height).saturating_mul(4) as usize, 0);
        generate_output_pixels(&mut self.pixels, width, height, source_kind);
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
    width: u32,
    height: u32,
    source_kind: Option<ProjectNodeKind>,
) {
    fill_rgba(out, 8, 8, 8, 255);
    if width == 0 || height == 0 {
        return;
    }
    if source_kind != Some(ProjectNodeKind::TexSolid) {
        return;
    }
    draw_circle(out, width, height);
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

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::TopViewerGenerator;
    use crate::gui::project::{GuiProject, ProjectNodeKind};

    #[test]
    fn viewer_generates_pixels_when_top_is_connected_to_output() {
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
}

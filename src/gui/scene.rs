//! Retained-style scene assembly for the GPU GUI renderer.
//!
//! The builder produces simple colored rectangles and line segments each frame.
//! Rendering stays on GPU; only lightweight geometry lists are rebuilt.

use super::geometry::Rect;
use super::project::{GuiProject, ProjectNode, ProjectNodeKind, NODE_HEIGHT, NODE_WIDTH};
use super::state::{PreviewState, ADD_NODE_OPTIONS, MENU_INNER_PADDING};
use super::text::GuiTextRenderer;

const PREVIEW_BG: Color = Color::argb(0xFF0A0D12);
const PANEL_BG: Color = Color::argb(0xFF111318);
const GRID_COLOR: Color = Color::argb(0xFF1B2028);
const BORDER_COLOR: Color = Color::argb(0xFF2A313A);
const EDGE_COLOR: Color = Color::argb(0xFF4A5564);
const NODE_BODY: Color = Color::argb(0xFF151A22);
const NODE_DRAG: Color = Color::argb(0xFFF59E0B);
const NODE_HOVER: Color = Color::argb(0xFF22D3EE);
const MENU_BG: Color = Color::argb(0xFF1D2430);
const MENU_SELECTED: Color = Color::argb(0xFF334155);
const MENU_BORDER: Color = Color::argb(0xFF475569);
const HEADER_BG: Color = Color::argb(0xFF202631);
const HEADER_TEXT: Color = Color::argb(0xFFE2E8F0);
const NODE_TEXT: Color = Color::argb(0xFFCBD5E1);
const MENU_TEXT: Color = Color::argb(0xFFF1F5F9);

/// RGBA color with normalized float channels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Color {
    pub(crate) r: f32,
    pub(crate) g: f32,
    pub(crate) b: f32,
    pub(crate) a: f32,
}

impl Color {
    /// Build color from one `0xAARRGGBB` integer literal.
    pub(crate) const fn argb(raw: u32) -> Self {
        let a = ((raw >> 24) & 0xFF) as f32 / 255.0;
        let r = ((raw >> 16) & 0xFF) as f32 / 255.0;
        let g = ((raw >> 8) & 0xFF) as f32 / 255.0;
        let b = (raw & 0xFF) as f32 / 255.0;
        Self { r, g, b, a }
    }
}

/// Filled rectangle primitive.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ColoredRect {
    pub(crate) rect: Rect,
    pub(crate) color: Color,
}

/// Line segment primitive.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ColoredLine {
    pub(crate) x0: i32,
    pub(crate) y0: i32,
    pub(crate) x1: i32,
    pub(crate) y1: i32,
    pub(crate) color: Color,
}

/// One frame of GPU scene primitives.
#[derive(Debug, Default)]
pub(crate) struct SceneFrame {
    pub(crate) clear: Option<Color>,
    pub(crate) rects: Vec<ColoredRect>,
    pub(crate) lines: Vec<ColoredLine>,
}

/// Stateful scene builder that reuses allocation capacity across frames.
pub(crate) struct SceneBuilder {
    frame: SceneFrame,
    static_rects: Vec<ColoredRect>,
    static_lines: Vec<ColoredLine>,
    cached_static_key: Option<(usize, usize, usize)>,
    edge_anchors: Vec<(u32, i32, i32)>,
    text_renderer: GuiTextRenderer,
}

impl Default for SceneBuilder {
    fn default() -> Self {
        Self {
            frame: SceneFrame::default(),
            static_rects: Vec::new(),
            static_lines: Vec::new(),
            cached_static_key: None,
            edge_anchors: Vec::new(),
            text_renderer: GuiTextRenderer::default(),
        }
    }
}

impl SceneBuilder {
    /// Build one frame of editor scene geometry.
    pub(crate) fn build(
        &mut self,
        project: &GuiProject,
        state: &PreviewState,
        width: usize,
        height: usize,
        panel_width: usize,
    ) -> &SceneFrame {
        self.rebuild_static_if_needed(width, height, panel_width);
        self.frame.clear = Some(PREVIEW_BG);
        self.frame.rects.clear();
        self.frame.lines.clear();
        self.frame.rects.extend_from_slice(&self.static_rects);
        self.frame.lines.extend_from_slice(&self.static_lines);

        self.push_header(project);
        self.push_edges(project);
        self.push_nodes(project, state);
        self.push_menu(state);
        &self.frame
    }

    fn rebuild_static_if_needed(&mut self, width: usize, height: usize, panel_width: usize) {
        let key = (width, height, panel_width);
        if self.cached_static_key == Some(key) {
            return;
        }
        self.cached_static_key = Some(key);
        self.static_rects.clear();
        self.static_lines.clear();
        self.static_rects.push(ColoredRect {
            rect: Rect::new(0, 0, panel_width as i32, height as i32),
            color: PANEL_BG,
        });
        Self::push_grid_into(
            &mut self.static_lines,
            panel_width as i32,
            height as i32,
            20,
        );
        Self::push_divider_into(&mut self.static_lines, panel_width as i32, height as i32);
    }

    fn push_header(&mut self, project: &GuiProject) {
        let w = 380;
        let rect = Rect::new(8, 8, w, 24);
        self.push_rect(rect, HEADER_BG);
        self.push_border(rect, BORDER_COLOR);
        self.push_text(rect.x + 8, rect.y + 7, project.name.as_str(), HEADER_TEXT);
    }

    fn push_edges(&mut self, project: &GuiProject) {
        if project.edge_count() == 0 {
            return;
        }
        self.edge_anchors.clear();
        self.edge_anchors.extend(project.nodes().iter().map(|node| {
            (
                node.id(),
                node.x() + NODE_WIDTH / 2,
                node.y() + NODE_HEIGHT / 2,
            )
        }));
        for node in project.nodes() {
            let Some((to_x, to_y)) = self.anchor_for(node.id()) else {
                continue;
            };
            for input in node.inputs() {
                let Some((from_x, from_y)) = self.anchor_for(*input) else {
                    continue;
                };
                self.push_line(from_x, from_y, to_x, to_y, EDGE_COLOR);
            }
        }
    }

    fn push_nodes(&mut self, project: &GuiProject, state: &PreviewState) {
        for node in project.nodes() {
            let rect = node_rect(node);
            self.push_rect(rect, NODE_BODY);
            self.push_rect(
                Rect::new(rect.x, rect.y, rect.w, 8),
                node_top_color(node.kind()),
            );
            let border = if state.drag.map(|drag| drag.node_id) == Some(node.id()) {
                NODE_DRAG
            } else if state.hover_node == Some(node.id()) {
                NODE_HOVER
            } else {
                BORDER_COLOR
            };
            self.push_border(rect, border);
            self.push_text(rect.x + 8, rect.y + 18, node.kind().label(), NODE_TEXT);
        }
    }

    fn push_menu(&mut self, state: &PreviewState) {
        if !state.menu.open {
            return;
        }
        let rect = state.menu.rect();
        self.push_rect(rect, MENU_BG);
        self.push_border(rect, MENU_BORDER);
        self.push_text(rect.x + MENU_INNER_PADDING + 6, rect.y + 7, "Add Node", MENU_TEXT);
        for (index, option) in ADD_NODE_OPTIONS.iter().copied().enumerate() {
            let Some(item) = state.menu.item_rect(index) else {
                continue;
            };
            if index == state.menu.selected || state.hover_menu_item == Some(index) {
                self.push_rect(item, MENU_SELECTED);
            }
            self.push_text(item.x + 6, item.y + 8, option.label(), MENU_TEXT);
        }
    }

    fn push_grid_into(out: &mut Vec<ColoredLine>, panel_width: i32, panel_height: i32, step: i32) {
        let mut x = 0;
        while x < panel_width {
            out.push(ColoredLine {
                x0: x,
                y0: 0,
                x1: x,
                y1: panel_height - 1,
                color: GRID_COLOR,
            });
            x += step;
        }
        let mut y = 0;
        while y < panel_height {
            out.push(ColoredLine {
                x0: 0,
                y0: y,
                x1: panel_width - 1,
                y1: y,
                color: GRID_COLOR,
            });
            y += step;
        }
    }

    fn push_divider_into(out: &mut Vec<ColoredLine>, panel_width: i32, panel_height: i32) {
        let x = panel_width - 1;
        out.push(ColoredLine {
            x0: x,
            y0: 0,
            x1: x,
            y1: panel_height - 1,
            color: BORDER_COLOR,
        });
    }

    fn push_rect(&mut self, rect: Rect, color: Color) {
        self.frame.rects.push(ColoredRect { rect, color });
    }

    fn push_border(&mut self, rect: Rect, color: Color) {
        let x0 = rect.x;
        let y0 = rect.y;
        let x1 = rect.x + rect.w - 1;
        let y1 = rect.y + rect.h - 1;
        self.push_line(x0, y0, x1, y0, color);
        self.push_line(x1, y0, x1, y1, color);
        self.push_line(x1, y1, x0, y1, color);
        self.push_line(x0, y1, x0, y0, color);
    }

    fn push_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        self.frame.lines.push(ColoredLine {
            x0,
            y0,
            x1,
            y1,
            color,
        });
    }

    fn push_text(&mut self, x: i32, y: i32, text: &str, color: Color) {
        let out = &mut self.frame.rects;
        self.text_renderer.push_text(out, x, y, text, color);
    }

    fn anchor_for(&self, node_id: u32) -> Option<(i32, i32)> {
        self.edge_anchors
            .iter()
            .find(|(id, _, _)| *id == node_id)
            .map(|(_, x, y)| (*x, *y))
    }
}

fn node_rect(node: &ProjectNode) -> Rect {
    Rect::new(node.x(), node.y(), NODE_WIDTH, NODE_HEIGHT)
}

fn node_top_color(kind: ProjectNodeKind) -> Color {
    if kind.is_top_like() {
        Color::argb(0xFF3B82F6)
    } else {
        Color::argb(0xFFEF4444)
    }
}

//! Retained-style scene assembly for the GPU GUI renderer.
//!
//! The builder produces simple colored rectangles and line segments each frame.
//! Rendering stays on GPU; only lightweight geometry lists are rebuilt.

use super::geometry::Rect;
use super::project::{
    input_pin_center, node_expand_toggle_rect, node_param_row_rect, node_param_value_rect,
    output_pin_center, pin_rect, GuiProject, ProjectNode, ProjectNodeKind, NODE_WIDTH,
};
use super::state::{PreviewState, RightMarqueeState, ADD_NODE_OPTIONS, MENU_INNER_PADDING};
use super::text::GuiTextRenderer;
use super::theme::AGIO;

const PREVIEW_BG: Color = Color::argb(AGIO.preview_bg);
const PANEL_BG: Color = Color::argb(AGIO.panel_bg);
const BORDER_COLOR: Color = Color::argb(AGIO.border);
const EDGE_COLOR: Color = Color::argb(AGIO.highlight_accent);
const NODE_BODY: Color = Color::argb(AGIO.node_body);
const NODE_DRAG: Color = Color::argb(AGIO.highlight_warning);
const NODE_HOVER: Color = Color::argb(AGIO.highlight_focus);
const NODE_SELECTED: Color = Color::argb(AGIO.highlight_selection);
const MENU_BG: Color = Color::argb(AGIO.menu_bg);
const MENU_SELECTED: Color = Color::argb(AGIO.highlight_selection);
const MENU_BORDER: Color = Color::argb(AGIO.border);
const HEADER_BG: Color = Color::argb(AGIO.header_bg);
const HEADER_TEXT: Color = Color::argb(AGIO.header_text);
const NODE_TEXT: Color = Color::argb(AGIO.node_text);
const MENU_TEXT: Color = Color::argb(AGIO.menu_text);
const PIN_BODY: Color = Color::argb(AGIO.highlight_selection);
const PIN_HOVER: Color = Color::argb(AGIO.highlight_focus);
const PARAM_SELECTED: Color = Color::argb(0x33262F3A);
const TOGGLE_BG: Color = Color::argb(0xFF121212);
const TOGGLE_BORDER: Color = Color::argb(AGIO.border);
const TOGGLE_ACTIVE_BG: Color = Color::argb(0x663B82F6);
const TOGGLE_ICON: Color = Color::argb(AGIO.menu_text);
const PARAM_VALUE_BG: Color = Color::argb(0xFF101010);
const PARAM_VALUE_BORDER: Color = Color::argb(AGIO.border);
const PARAM_VALUE_ACTIVE: Color = Color::argb(AGIO.highlight_focus);
const PARAM_VALUE_TEXT: Color = Color::argb(AGIO.menu_text);
const CUT_EDGE_COLOR: Color = Color::argb(AGIO.highlight_warning);
const CUT_LINE_COLOR: Color = Color::argb(AGIO.highlight_warning);
const MARQUEE_FILL: Color = Color::argb(0x223B82F6);
const MARQUEE_BORDER: Color = Color::argb(AGIO.highlight_selection);
const GRAPH_TEXT_HIDE_ZOOM: f32 = 0.58;

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
    text_renderer: GuiTextRenderer,
}

impl Default for SceneBuilder {
    fn default() -> Self {
        Self {
            frame: SceneFrame::default(),
            static_rects: Vec::new(),
            static_lines: Vec::new(),
            cached_static_key: None,
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
        self.push_edges(project, state);
        self.push_nodes(project, state);
        self.push_right_marquee(state);
        self.push_link_cut(state);
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
        Self::push_divider_into(&mut self.static_lines, panel_width as i32, height as i32);
    }

    fn push_header(&mut self, project: &GuiProject) {
        let w = 380;
        let rect = Rect::new(8, 8, w, 24);
        self.push_rect(rect, HEADER_BG);
        self.push_border(rect, BORDER_COLOR);
        self.push_text(rect.x + 8, rect.y + 7, project.name.as_str(), HEADER_TEXT);
    }

    fn push_edges(&mut self, project: &GuiProject, state: &PreviewState) {
        if project.edge_count() == 0 {
            self.push_wire_drag(project, state);
            return;
        }
        for target in project.nodes() {
            let Some((to_x, to_y)) = input_pin_center(target) else {
                continue;
            };
            let (to_x, to_y) = graph_point_to_panel(to_x, to_y, state);
            for source_id in target.inputs() {
                let Some(source) = project.node(*source_id) else {
                    continue;
                };
                let Some((from_x, from_y)) = output_pin_center(source) else {
                    continue;
                };
                let (from_x, from_y) = graph_point_to_panel(from_x, from_y, state);
                let color = if edge_intersects_cut_line(state, from_x, from_y, to_x, to_y) {
                    CUT_EDGE_COLOR
                } else {
                    EDGE_COLOR
                };
                self.push_line(from_x, from_y, to_x, to_y, color);
            }
        }
        self.push_wire_drag(project, state);
    }

    fn push_nodes(&mut self, project: &GuiProject, state: &PreviewState) {
        for node in project.nodes() {
            let rect = node_rect(node, state);
            self.push_rect(rect, NODE_BODY);
            let top_h = (8.0 * state.zoom).round().max(2.0) as i32;
            self.push_rect(
                Rect::new(rect.x, rect.y, rect.w, top_h.min(rect.h)),
                node_top_color(node.kind()),
            );
            let border = if state.drag.map(|drag| drag.node_id) == Some(node.id()) {
                NODE_DRAG
            } else if state.hover_node == Some(node.id()) {
                NODE_HOVER
            } else if state.selected_nodes.contains(&node.id()) {
                NODE_SELECTED
            } else {
                BORDER_COLOR
            };
            self.push_border(rect, border);
            self.push_graph_text(rect.x + 8, rect.y + 18, node.kind().label(), NODE_TEXT, state);
            self.push_node_toggle(node, state);
            if node.expanded() {
                self.push_node_params(project, node, state);
            }
            self.push_pins(node, state);
        }
    }

    fn push_node_toggle(&mut self, node: &ProjectNode, state: &PreviewState) {
        let Some(toggle_world) = node_expand_toggle_rect(node) else {
            return;
        };
        let toggle = graph_rect_to_panel(toggle_world, state);
        let bg = if node.expanded() {
            TOGGLE_ACTIVE_BG
        } else {
            TOGGLE_BG
        };
        self.push_rect(toggle, bg);
        self.push_border(toggle, TOGGLE_BORDER);
        if toggle.w < 4 || toggle.h < 4 {
            return;
        }
        let cx = toggle.x + toggle.w / 2;
        let cy = toggle.y + toggle.h / 2;
        self.push_line(toggle.x + 2, cy, toggle.x + toggle.w - 3, cy, TOGGLE_ICON);
        if !node.expanded() {
            self.push_line(cx, toggle.y + 2, cx, toggle.y + toggle.h - 3, TOGGLE_ICON);
        }
    }

    fn push_node_params(
        &mut self,
        project: &GuiProject,
        node: &ProjectNode,
        state: &PreviewState,
    ) {
        let Some(rows) = project.node_param_views(node.id()) else {
            return;
        };
        if rows.is_empty() {
            return;
        }
        for (index, row) in rows.iter().enumerate() {
            let Some(row_world) = node_param_row_rect(node, index) else {
                continue;
            };
            let row_rect = graph_rect_to_panel(row_world, state);
            if row.selected {
                self.push_rect(
                    Rect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h),
                    PARAM_SELECTED,
                );
            }
            let label = if row.bound {
                format!("{} *", row.label)
            } else {
                row.label.to_string()
            };
            self.push_graph_text(
                row_rect.x + 4,
                row_rect.y + row_rect.h.saturating_sub(6),
                label.as_str(),
                NODE_TEXT,
                state,
            );
            let Some(value_world) = node_param_value_rect(node, index) else {
                continue;
            };
            let value_rect = graph_rect_to_panel(value_world, state);
            self.push_rect(value_rect, PARAM_VALUE_BG);
            let editing = state
                .param_edit
                .as_ref()
                .map(|edit| edit.node_id == node.id() && edit.param_index == index)
                .unwrap_or(false);
            self.push_border(
                value_rect,
                if editing {
                    PARAM_VALUE_ACTIVE
                } else {
                    PARAM_VALUE_BORDER
                },
            );
            let value_text = if let Some(edit) = state.param_edit.as_ref() {
                if edit.node_id == node.id() && edit.param_index == index {
                    edit.buffer.clone()
                } else {
                    format!("{:.3}", row.value)
                }
            } else {
                format!("{:.3}", row.value)
            };
            self.push_graph_text(
                value_rect.x + 4,
                value_rect.y + value_rect.h.saturating_sub(4),
                value_text.as_str(),
                PARAM_VALUE_TEXT,
                state,
            );
        }
    }

    fn push_menu(&mut self, state: &PreviewState) {
        if !state.menu.open {
            return;
        }
        let rect = state.menu.rect();
        self.push_rect(rect, MENU_BG);
        self.push_border(rect, MENU_BORDER);
        self.push_text(
            rect.x + MENU_INNER_PADDING + 6,
            rect.y + 7,
            "Add Node",
            MENU_TEXT,
        );
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

    fn push_link_cut(&mut self, state: &PreviewState) {
        let Some(cut) = state.link_cut else {
            return;
        };
        self.push_line(
            cut.start_x,
            cut.start_y,
            cut.cursor_x,
            cut.cursor_y,
            CUT_LINE_COLOR,
        );
    }

    fn push_right_marquee(&mut self, state: &PreviewState) {
        let Some(marquee) = state.right_marquee else {
            return;
        };
        let Some(rect) = marquee_panel_rect(marquee) else {
            return;
        };
        self.push_rect(rect, MARQUEE_FILL);
        self.push_border(rect, MARQUEE_BORDER);
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

    fn push_pins(&mut self, node: &ProjectNode, state: &PreviewState) {
        if let Some((cx, cy)) = output_pin_center(node) {
            let (cx, cy) = graph_point_to_panel(cx, cy, state);
            let color = if state.hover_output_pin == Some(node.id())
                || state.wire_drag.map(|wire| wire.source_node_id) == Some(node.id())
            {
                PIN_HOVER
            } else {
                PIN_BODY
            };
            self.push_rect(pin_rect(cx, cy), color);
        }
        if let Some((cx, cy)) = input_pin_center(node) {
            let (cx, cy) = graph_point_to_panel(cx, cy, state);
            let color = if state.hover_input_pin == Some(node.id()) {
                PIN_HOVER
            } else {
                PIN_BODY
            };
            self.push_rect(pin_rect(cx, cy), color);
        }
    }

    fn push_wire_drag(&mut self, project: &GuiProject, state: &PreviewState) {
        let Some(wire) = state.wire_drag else {
            return;
        };
        let Some(source) = project.node(wire.source_node_id) else {
            return;
        };
        let Some((x0, y0)) = output_pin_center(source) else {
            return;
        };
        let (x0, y0) = graph_point_to_panel(x0, y0, state);
        let (x1, y1) = if let Some(target_id) = state.hover_input_pin {
            if let Some(target_node) = project.node(target_id) {
                input_pin_center(target_node)
                    .map(|(x, y)| graph_point_to_panel(x, y, state))
                    .unwrap_or((wire.cursor_x, wire.cursor_y))
            } else {
                (wire.cursor_x, wire.cursor_y)
            }
        } else {
            (wire.cursor_x, wire.cursor_y)
        };
        self.push_line(x0, y0, x1, y1, PIN_HOVER);
    }

    fn push_text(&mut self, x: i32, y: i32, text: &str, color: Color) {
        let out = &mut self.frame.rects;
        self.text_renderer.push_text(out, x, y, text, color);
    }

    fn push_graph_text(&mut self, x: i32, y: i32, text: &str, color: Color, state: &PreviewState) {
        if state.zoom < GRAPH_TEXT_HIDE_ZOOM {
            return;
        }
        let out = &mut self.frame.rects;
        self.text_renderer
            .push_text_scaled(out, x, y, text, color, state.zoom);
    }
}

fn node_rect(node: &ProjectNode, state: &PreviewState) -> Rect {
    graph_rect_to_panel(
        Rect::new(node.x(), node.y(), NODE_WIDTH, node.card_height()),
        state,
    )
}

fn graph_rect_to_panel(rect: Rect, state: &PreviewState) -> Rect {
    let x = (rect.x as f32 * state.zoom + state.pan_x).round() as i32;
    let y = (rect.y as f32 * state.zoom + state.pan_y).round() as i32;
    let w = (rect.w as f32 * state.zoom).round().max(1.0) as i32;
    let h = (rect.h as f32 * state.zoom).round().max(1.0) as i32;
    Rect::new(x, y, w, h)
}

fn graph_point_to_panel(x: i32, y: i32, state: &PreviewState) -> (i32, i32) {
    let sx = (x as f32 * state.zoom + state.pan_x).round() as i32;
    let sy = (y as f32 * state.zoom + state.pan_y).round() as i32;
    (sx, sy)
}

fn marquee_panel_rect(marquee: RightMarqueeState) -> Option<Rect> {
    let x0 = marquee.start_x.min(marquee.cursor_x);
    let y0 = marquee.start_y.min(marquee.cursor_y);
    let x1 = marquee.start_x.max(marquee.cursor_x);
    let y1 = marquee.start_y.max(marquee.cursor_y);
    let w = x1 - x0;
    let h = y1 - y0;
    if w <= 4 || h <= 4 {
        return None;
    }
    Some(Rect::new(x0, y0, w, h))
}

fn node_top_color(kind: ProjectNodeKind) -> Color {
    match kind {
        ProjectNodeKind::TexSolid => Color::argb(AGIO.node_header_tex_solid),
        ProjectNodeKind::TexTransform2D => Color::argb(AGIO.node_header_tex_transform_2d),
        ProjectNodeKind::CtlLfo => Color::argb(AGIO.node_header_ctl_lfo),
        ProjectNodeKind::IoWindowOut => Color::argb(AGIO.node_header_io_window_out),
    }
}

fn edge_intersects_cut_line(state: &PreviewState, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    let Some(cut) = state.link_cut else {
        return false;
    };
    segments_intersect(
        cut.start_x,
        cut.start_y,
        cut.cursor_x,
        cut.cursor_y,
        x0,
        y0,
        x1,
        y1,
    )
}

fn segments_intersect(
    ax: i32,
    ay: i32,
    bx: i32,
    by: i32,
    cx: i32,
    cy: i32,
    dx: i32,
    dy: i32,
) -> bool {
    let o1 = orient(ax, ay, bx, by, cx, cy);
    let o2 = orient(ax, ay, bx, by, dx, dy);
    let o3 = orient(cx, cy, dx, dy, ax, ay);
    let o4 = orient(cx, cy, dx, dy, bx, by);
    if o1 == 0 && on_segment(ax, ay, bx, by, cx, cy) {
        return true;
    }
    if o2 == 0 && on_segment(ax, ay, bx, by, dx, dy) {
        return true;
    }
    if o3 == 0 && on_segment(cx, cy, dx, dy, ax, ay) {
        return true;
    }
    if o4 == 0 && on_segment(cx, cy, dx, dy, bx, by) {
        return true;
    }
    (o1 > 0) != (o2 > 0) && (o3 > 0) != (o4 > 0)
}

fn orient(ax: i32, ay: i32, bx: i32, by: i32, cx: i32, cy: i32) -> i64 {
    let abx = (bx - ax) as i64;
    let aby = (by - ay) as i64;
    let acx = (cx - ax) as i64;
    let acy = (cy - ay) as i64;
    abx * acy - aby * acx
}

fn on_segment(ax: i32, ay: i32, bx: i32, by: i32, px: i32, py: i32) -> bool {
    px >= ax.min(bx) && px <= ax.max(bx) && py >= ay.min(by) && py <= ay.max(by)
}

//! Retained-style scene assembly for the GPU GUI renderer.
//!
//! The builder partitions GUI geometry into retained layers and marks only
//! changed layers dirty each update (`static_panel`, `edges`, `nodes`,
//! `overlays`). Rendering stays on GPU and unchanged layers are reused.

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
const PARAM_VALUE_SELECTION: Color = Color::argb(0x664A88D9);
const PARAM_VALUE_CARET: Color = Color::argb(0xFFE2E2E2);
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
    pub(crate) static_panel: SceneLayer,
    pub(crate) edges: SceneLayer,
    pub(crate) nodes: SceneLayer,
    pub(crate) overlays: SceneLayer,
    pub(crate) dirty: SceneLayerDirty,
    pub(crate) ui_alloc_bytes: u64,
}

/// One retained GUI geometry layer.
#[derive(Debug, Default)]
pub(crate) struct SceneLayer {
    pub(crate) rects: Vec<ColoredRect>,
    pub(crate) lines: Vec<ColoredLine>,
}

/// Dirty flags used to invalidate retained GUI geometry layers.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SceneLayerDirty {
    pub(crate) static_panel: bool,
    pub(crate) edges: bool,
    pub(crate) nodes: bool,
    pub(crate) overlays: bool,
}

impl SceneLayerDirty {
    /// Return true when any retained layer needs a GPU buffer update.
    pub(crate) fn any(self) -> bool {
        self.static_panel || self.edges || self.nodes || self.overlays
    }
}

#[derive(Clone, Copy, Debug, Default)]
enum ActiveLayer {
    StaticPanel,
    Edges,
    #[default]
    Nodes,
    Overlays,
}

/// Stateful scene builder that reuses allocation capacity across frames.
pub(crate) struct SceneBuilder {
    frame: SceneFrame,
    active_layer: ActiveLayer,
    cached_static_key: Option<(usize, usize, usize)>,
    cached_nodes_key: Option<u64>,
    cached_edges_key: Option<u64>,
    cached_overlays_key: Option<u64>,
    text_renderer: GuiTextRenderer,
    label_scratch: String,
    fitted_label_scratch: String,
    frame_alloc_bytes: u64,
}

impl Default for SceneBuilder {
    fn default() -> Self {
        Self {
            frame: SceneFrame::default(),
            active_layer: ActiveLayer::default(),
            cached_static_key: None,
            cached_nodes_key: None,
            cached_edges_key: None,
            cached_overlays_key: None,
            text_renderer: GuiTextRenderer::default(),
            label_scratch: String::new(),
            fitted_label_scratch: String::new(),
            frame_alloc_bytes: 0,
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
        self.frame.clear = Some(PREVIEW_BG);
        self.frame.dirty = SceneLayerDirty::default();
        self.frame_alloc_bytes = 0;

        self.rebuild_static_if_needed(width, height, panel_width);

        let nodes_key = nodes_layer_key(project, state);
        if self.cached_nodes_key != Some(nodes_key) {
            self.cached_nodes_key = Some(nodes_key);
            self.frame.dirty.nodes = true;
            self.rebuild_nodes_layer(project, state);
        }

        let edges_key = edges_layer_key(project, state);
        if self.cached_edges_key != Some(edges_key) {
            self.cached_edges_key = Some(edges_key);
            self.frame.dirty.edges = true;
            self.rebuild_edges_layer(project, state);
        }

        let overlays_key = overlays_layer_key(project, state);
        if self.cached_overlays_key != Some(overlays_key) {
            self.cached_overlays_key = Some(overlays_key);
            self.frame.dirty.overlays = true;
            self.rebuild_overlays_layer(project, state);
        }
        self.frame.ui_alloc_bytes = self.frame_alloc_bytes;
        &self.frame
    }

    fn rebuild_static_if_needed(&mut self, width: usize, height: usize, panel_width: usize) {
        let key = (width, height, panel_width);
        if self.cached_static_key == Some(key) {
            return;
        }
        self.cached_static_key = Some(key);
        self.frame.dirty.static_panel = true;
        let before = self.layer_capacity(ActiveLayer::StaticPanel);
        self.set_active_layer(ActiveLayer::StaticPanel);
        self.clear_active_layer();
        self.push_rect(Rect::new(0, 0, panel_width as i32, height as i32), PANEL_BG);
        let x = panel_width as i32 - 1;
        self.push_line(x, 0, x, height as i32 - 1, BORDER_COLOR);
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::StaticPanel));
    }

    fn rebuild_nodes_layer(&mut self, project: &GuiProject, state: &PreviewState) {
        let before = self.layer_capacity(ActiveLayer::Nodes);
        self.set_active_layer(ActiveLayer::Nodes);
        self.clear_active_layer();
        self.push_header(project);
        self.push_nodes(project, state);
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::Nodes));
    }

    fn rebuild_edges_layer(&mut self, project: &GuiProject, state: &PreviewState) {
        let before = self.layer_capacity(ActiveLayer::Edges);
        self.set_active_layer(ActiveLayer::Edges);
        self.clear_active_layer();
        self.push_edges(project, state);
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::Edges));
    }

    fn rebuild_overlays_layer(&mut self, project: &GuiProject, state: &PreviewState) {
        let before = self.layer_capacity(ActiveLayer::Overlays);
        self.set_active_layer(ActiveLayer::Overlays);
        self.clear_active_layer();
        self.push_wire_drag(project, state);
        self.push_right_marquee(state);
        self.push_link_cut(state);
        self.push_menu(state);
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::Overlays));
    }

    fn set_active_layer(&mut self, layer: ActiveLayer) {
        self.active_layer = layer;
    }

    fn clear_active_layer(&mut self) {
        let layer = active_scene_layer_mut(&mut self.frame, self.active_layer);
        layer.rects.clear();
        layer.lines.clear();
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
            // Anchor title text in graph space so it stays visually locked to the
            // node card under pan/zoom and long-distance canvas movement.
            let (title_x, title_y) = graph_point_to_panel(node.x() + 8, node.y() + 18, state);
            self.push_graph_text(title_x, title_y, node.kind().label(), NODE_TEXT, state);
            self.push_node_toggle(node, state);
            if node.expanded() {
                self.push_node_params(node, state);
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

    fn push_node_params(&mut self, node: &ProjectNode, state: &PreviewState) {
        if node.param_count() == 0 {
            return;
        }
        let mut label_scratch = std::mem::take(&mut self.label_scratch);
        let mut fitted_label_scratch = std::mem::take(&mut self.fitted_label_scratch);
        for (index, row) in node.param_views().enumerate() {
            let Some(row_world) = node_param_row_rect(node, index) else {
                continue;
            };
            let row_rect = graph_rect_to_panel(row_world, state);
            let Some(value_world) = node_param_value_rect(node, index) else {
                continue;
            };
            let value_rect = graph_rect_to_panel(value_world, state);
            if row.selected {
                self.push_rect(
                    Rect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h),
                    PARAM_SELECTED,
                );
            }
            label_scratch.clear();
            label_scratch.push_str(row.label);
            if row.bound {
                label_scratch.push_str(" *");
            }
            let label_x = row_rect.x + 4;
            let label_max_w = (value_rect.x - label_x - 4).max(0);
            let fitted_label = self.fit_graph_text_into(
                label_scratch.as_str(),
                label_max_w,
                state,
                &mut fitted_label_scratch,
            );
            let label_rect = Rect::new(label_x, row_rect.y, label_max_w, row_rect.h);
            self.push_graph_text_in_rect(label_rect, 0, fitted_label, NODE_TEXT, state);
            self.push_rect(value_rect, PARAM_VALUE_BG);
            let editing = state
                .param_edit
                .as_ref()
                .map(|edit| edit.node_id == node.id() && edit.param_index == index)
                .unwrap_or(false);
            let active_edit = state
                .param_edit
                .as_ref()
                .filter(|edit| edit.node_id == node.id() && edit.param_index == index);
            let value_text = active_edit
                .map(|edit| edit.buffer.as_str())
                .unwrap_or(row.value_text);
            self.push_value_editor_text(value_rect, value_text, active_edit, state);
            self.push_border(
                value_rect,
                if editing {
                    PARAM_VALUE_ACTIVE
                } else {
                    PARAM_VALUE_BORDER
                },
            );
        }
        self.label_scratch = label_scratch;
        self.fitted_label_scratch = fitted_label_scratch;
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

    fn push_rect(&mut self, rect: Rect, color: Color) {
        active_scene_layer_mut(&mut self.frame, self.active_layer)
            .rects
            .push(ColoredRect { rect, color });
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
        active_scene_layer_mut(&mut self.frame, self.active_layer)
            .lines
            .push(ColoredLine {
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
        let out = &mut active_scene_layer_mut(&mut self.frame, self.active_layer).rects;
        self.text_renderer.push_text(out, x, y, text, color);
    }

    fn push_graph_text(&mut self, x: i32, y: i32, text: &str, color: Color, state: &PreviewState) {
        if state.zoom < GRAPH_TEXT_HIDE_ZOOM {
            return;
        }
        let out = &mut active_scene_layer_mut(&mut self.frame, self.active_layer).rects;
        self.text_renderer
            .push_text_scaled(out, x, y, text, color, state.zoom);
    }

    fn push_graph_text_in_rect(
        &mut self,
        rect: Rect,
        left_pad: i32,
        text: &str,
        color: Color,
        state: &PreviewState,
    ) {
        if state.zoom < GRAPH_TEXT_HIDE_ZOOM || rect.w <= 0 || rect.h <= 0 || text.is_empty() {
            return;
        }
        let metrics = self.text_renderer.metrics_scaled(state.zoom);
        let x = rect.x + left_pad;
        let y = rect.y + ((rect.h - metrics.line_height_px).max(0) / 2);
        let out = &mut active_scene_layer_mut(&mut self.frame, self.active_layer).rects;
        self.text_renderer
            .push_text_scaled(out, x, y, text, color, state.zoom);
    }

    fn push_value_editor_text(
        &mut self,
        value_rect: Rect,
        text: &str,
        edit: Option<&super::state::ParamEditState>,
        state: &PreviewState,
    ) {
        if state.zoom < GRAPH_TEXT_HIDE_ZOOM {
            return;
        }
        let metrics = self.text_renderer.metrics_scaled(state.zoom);
        let text_x = value_rect.x + 4;
        let text_y = value_rect.y + ((value_rect.h - metrics.line_height_px).max(0) / 2);
        if let Some(edit_state) = edit {
            let mut cursor = edit_state.cursor.min(text.len());
            let mut anchor = edit_state.anchor.min(text.len());
            if anchor > cursor {
                std::mem::swap(&mut anchor, &mut cursor);
            }
            if anchor != cursor {
                let start_w = self.text_renderer.cursor_offset(text, anchor, state.zoom);
                let end_w = self.text_renderer.cursor_offset(text, cursor, state.zoom);
                let highlight_x = text_x + start_w;
                let highlight_w = (end_w - start_w).max(1);
                let left = highlight_x.max(value_rect.x + 1);
                let right = (highlight_x + highlight_w).min(value_rect.x + value_rect.w - 1);
                let clamped = Rect::new(left, text_y, right - left, metrics.line_height_px.max(1));
                if clamped.w > 0 && clamped.h > 0 {
                    self.push_rect(clamped, PARAM_VALUE_SELECTION);
                }
            }
        }
        let out = &mut active_scene_layer_mut(&mut self.frame, self.active_layer).rects;
        self.text_renderer.push_text_scaled(
            out,
            text_x,
            text_y,
            text,
            PARAM_VALUE_TEXT,
            state.zoom,
        );
        if let Some(edit_state) = edit {
            let caret_index = edit_state.cursor.min(text.len());
            let caret_x = text_x
                + self
                    .text_renderer
                    .cursor_offset(text, caret_index, state.zoom);
            let caret_top = text_y;
            let caret_bottom = text_y + metrics.line_height_px.max(1) - 1;
            self.push_line(caret_x, caret_top, caret_x, caret_bottom, PARAM_VALUE_CARET);
        }
    }

    fn fit_graph_text_into<'a>(
        &self,
        text: &'a str,
        max_width: i32,
        state: &PreviewState,
        out: &'a mut String,
    ) -> &'a str {
        if max_width <= 0 || text.is_empty() {
            return "";
        }
        let scale = state.zoom;
        let full_w = self.text_renderer.measure_text_width(text, scale);
        if full_w <= max_width {
            return text;
        }
        let ellipsis = "...";
        let ellipsis_w = self.text_renderer.measure_text_width(ellipsis, scale);
        if ellipsis_w > max_width {
            return "";
        }
        let mut width = 0;
        let mut end_byte = 0usize;
        for (byte_index, ch) in text.char_indices() {
            let ch_w = self.text_renderer.measure_char_width(ch, scale);
            if width + ch_w + ellipsis_w > max_width {
                break;
            }
            end_byte = byte_index + ch.len_utf8();
            width += ch_w;
        }
        out.clear();
        out.push_str(&text[..end_byte]);
        out.push_str(ellipsis);
        out.as_str()
    }

    fn layer_capacity(&self, layer: ActiveLayer) -> (usize, usize) {
        let data = match layer {
            ActiveLayer::StaticPanel => &self.frame.static_panel,
            ActiveLayer::Edges => &self.frame.edges,
            ActiveLayer::Nodes => &self.frame.nodes,
            ActiveLayer::Overlays => &self.frame.overlays,
        };
        (data.rects.capacity(), data.lines.capacity())
    }

    fn bump_layer_alloc_growth(&mut self, before: (usize, usize), after: (usize, usize)) {
        let rect_growth = after
            .0
            .saturating_sub(before.0)
            .saturating_mul(std::mem::size_of::<ColoredRect>());
        let line_growth = after
            .1
            .saturating_sub(before.1)
            .saturating_mul(std::mem::size_of::<ColoredLine>());
        self.frame_alloc_bytes = self
            .frame_alloc_bytes
            .saturating_add((rect_growth + line_growth) as u64);
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

fn active_scene_layer_mut(frame: &mut SceneFrame, layer: ActiveLayer) -> &mut SceneLayer {
    match layer {
        ActiveLayer::StaticPanel => &mut frame.static_panel,
        ActiveLayer::Edges => &mut frame.edges,
        ActiveLayer::Nodes => &mut frame.nodes,
        ActiveLayer::Overlays => &mut frame.overlays,
    }
}

fn nodes_layer_key(project: &GuiProject, state: &PreviewState) -> u64 {
    let mut hash = hash_start();
    hash = hash_u64(hash, project.graph_signature());
    hash = hash_f32(hash, state.pan_x);
    hash = hash_f32(hash, state.pan_y);
    hash = hash_f32(hash, state.zoom);
    hash = hash_opt_u32(hash, state.hover_node);
    hash = hash_opt_u32(hash, state.hover_output_pin);
    hash = hash_opt_u32(hash, state.hover_input_pin);
    hash = hash_opt_u32(hash, state.wire_drag.map(|wire| wire.source_node_id));
    hash = hash_opt_u32(hash, state.drag.map(|drag| drag.node_id));
    for selected in &state.selected_nodes {
        hash = hash_u64(hash, *selected as u64);
    }
    if let Some(edit) = state.param_edit.as_ref() {
        hash = hash_u64(hash, edit.node_id as u64);
        hash = hash_u64(hash, edit.param_index as u64);
        hash = hash_u64(hash, edit.cursor as u64);
        hash = hash_u64(hash, edit.anchor as u64);
        for byte in edit.buffer.as_bytes() {
            hash = hash_u64(hash, *byte as u64);
        }
    }
    for byte in project.name.as_bytes() {
        hash = hash_u64(hash, *byte as u64);
    }
    hash
}

fn edges_layer_key(project: &GuiProject, state: &PreviewState) -> u64 {
    let mut hash = hash_start();
    hash = hash_f32(hash, state.pan_x);
    hash = hash_f32(hash, state.pan_y);
    hash = hash_f32(hash, state.zoom);
    hash = hash_opt_cut_line(hash, state.link_cut);
    for node in project.nodes() {
        hash = hash_u64(hash, node.id() as u64);
        hash = hash_i32(hash, node.x());
        hash = hash_i32(hash, node.y());
        for input in node.inputs() {
            hash = hash_u64(hash, *input as u64);
        }
        hash = hash_u64(hash, 0xff);
    }
    hash
}

fn overlays_layer_key(project: &GuiProject, state: &PreviewState) -> u64 {
    let mut hash = hash_start();
    hash = hash_u64(hash, project.ui_signature());
    hash = hash_f32(hash, state.pan_x);
    hash = hash_f32(hash, state.pan_y);
    hash = hash_f32(hash, state.zoom);
    hash = hash_opt_u32(hash, state.hover_input_pin);
    hash = hash_opt_wire(hash, state.wire_drag);
    hash = hash_opt_cut_line(hash, state.link_cut);
    hash = hash_opt_marquee(hash, state.right_marquee);
    hash = hash_u64(hash, state.menu.open as u64);
    hash = hash_i32(hash, state.menu.x);
    hash = hash_i32(hash, state.menu.y);
    hash = hash_u64(hash, state.menu.selected as u64);
    if let Some(index) = state.hover_menu_item {
        hash = hash_u64(hash, index as u64);
    }
    hash
}

fn hash_start() -> u64 {
    0xcbf29ce484222325
}

fn hash_u64(seed: u64, value: u64) -> u64 {
    seed.wrapping_mul(0x100000001b3) ^ value
}

fn hash_i32(seed: u64, value: i32) -> u64 {
    hash_u64(seed, value as i64 as u64)
}

fn hash_f32(seed: u64, value: f32) -> u64 {
    hash_u64(seed, value.to_bits() as u64)
}

fn hash_opt_u32(seed: u64, value: Option<u32>) -> u64 {
    match value {
        Some(v) => hash_u64(seed, v as u64),
        None => hash_u64(seed, u64::MAX),
    }
}

fn hash_opt_wire(seed: u64, value: Option<super::state::WireDragState>) -> u64 {
    let Some(wire) = value else {
        return hash_u64(seed, u64::MAX - 1);
    };
    let hash = hash_u64(seed, wire.source_node_id as u64);
    let hash = hash_i32(hash, wire.cursor_x);
    hash_i32(hash, wire.cursor_y)
}

fn hash_opt_cut_line(seed: u64, value: Option<super::state::LinkCutState>) -> u64 {
    let Some(cut) = value else {
        return hash_u64(seed, u64::MAX - 2);
    };
    let hash = hash_i32(seed, cut.start_x);
    let hash = hash_i32(hash, cut.start_y);
    let hash = hash_i32(hash, cut.cursor_x);
    hash_i32(hash, cut.cursor_y)
}

fn hash_opt_marquee(seed: u64, value: Option<RightMarqueeState>) -> u64 {
    let Some(marquee) = value else {
        return hash_u64(seed, u64::MAX - 3);
    };
    let hash = hash_i32(seed, marquee.start_x);
    let hash = hash_i32(hash, marquee.start_y);
    let hash = hash_i32(hash, marquee.cursor_x);
    hash_i32(hash, marquee.cursor_y)
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

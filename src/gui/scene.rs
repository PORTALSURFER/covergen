//! Retained-style scene assembly for the GPU GUI renderer.
//!
//! The builder partitions GUI geometry into retained layers and marks only
//! changed layers dirty each update (`static_panel`, `edges`, `nodes`,
//! `overlays`). Rendering stays on GPU and unchanged layers are reused.

mod wire_route;

use std::fmt::Write as _;

use super::geometry::Rect;
use super::project::{
    input_pin_center, node_expand_toggle_rect, node_param_dropdown_rect, node_param_row_rect,
    node_param_value_rect, output_pin_center, pin_rect, GuiProject, ProjectNode, ProjectNodeKind,
    ResourceKind, NODE_WIDTH,
};
use super::state::{
    AddNodeCategory, AddNodeMenuEntry, PreviewState, RightMarqueeState, ADD_NODE_OPTIONS,
    MENU_BLOCK_GAP, MENU_INNER_PADDING,
};
use super::text::GuiTextRenderer;
use super::theme::AGIO;
use super::timeline::{
    pause_button_rect, play_button_rect, timeline_rect, track_rect, track_x_for_frame,
    TIMELINE_END_FRAME, TIMELINE_START_FRAME,
};

const PREVIEW_BG: Color = Color::argb(AGIO.preview_bg);
const PANEL_BG: Color = Color::argb(AGIO.panel_bg);
const BORDER_COLOR: Color = Color::argb(AGIO.border);
const EDGE_COLOR: Color = Color::argb(AGIO.highlight_accent);
const PARAM_EDGE_COLOR: Color = Color::argb(AGIO.highlight_error);
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
const MENU_CATEGORY_TEXT: Color = Color::argb(0xFFBEBEBE);
const MENU_CATEGORY_CHIP_TEXT: Color = Color::argb(0xFF111111);
const MENU_CATEGORY_CHIP_BORDER: Color = Color::argb(0xFF0A0A0A);
const MENU_SEARCH_BG: Color = Color::argb(0xFF121212);
const PIN_BODY: Color = Color::argb(AGIO.highlight_selection);
const PIN_HOVER: Color = Color::argb(AGIO.highlight_focus);
const PARAM_SELECTED: Color = Color::argb(0x33262F3A);
const PARAM_BIND_HOVER: Color = Color::argb(0x3342A5F5);
const TOGGLE_BG: Color = Color::argb(0xFF121212);
const TOGGLE_BORDER: Color = Color::argb(AGIO.border);
const TOGGLE_ACTIVE_BG: Color = Color::argb(0x663B82F6);
const TOGGLE_ICON: Color = Color::argb(AGIO.menu_text);
const PARAM_VALUE_BG: Color = Color::argb(0xFF101010);
const PARAM_VALUE_BORDER: Color = Color::argb(AGIO.border);
const PARAM_VALUE_ACTIVE: Color = Color::argb(AGIO.highlight_focus);
const PARAM_VALUE_SELECTION: Color = Color::argb(0x664A88D9);
const PARAM_VALUE_CARET: Color = Color::argb(0xFFE2E2E2);
const PARAM_DROPDOWN_BG: Color = Color::argb(0xFF0E0E0E);
const PARAM_DROPDOWN_SELECTED: Color = Color::argb(0x663B82F6);
const PARAM_DROPDOWN_HOVER: Color = Color::argb(0x3342A5F5);
const CUT_EDGE_COLOR: Color = Color::argb(AGIO.highlight_warning);
const CUT_LINE_COLOR: Color = Color::argb(AGIO.highlight_warning);
const MARQUEE_FILL: Color = Color::argb(0x223B82F6);
const MARQUEE_BORDER: Color = Color::argb(AGIO.highlight_selection);
const TIMELINE_BG: Color = Color::argb(0xFF101010);
const TIMELINE_BORDER: Color = Color::argb(AGIO.border);
const TIMELINE_TRACK_BG: Color = Color::argb(0xFF171717);
const TIMELINE_TRACK_FILL: Color = Color::argb(AGIO.highlight_selection);
const TIMELINE_BTN_ACTIVE: Color = Color::argb(0x553B82F6);
const TIMELINE_BTN_IDLE: Color = Color::argb(0xFF171717);
const TIMELINE_TEXT: Color = Color::argb(0xFFD5D5D5);
const GRAPH_TEXT_HIDE_ZOOM: f32 = 0.58;
const WIRE_ENDPOINT_RADIUS_PX: i32 = 2;
const PARAM_BIND_TARGET_RADIUS_PX: i32 = 3;
const PARAM_WIRE_EXIT_TAIL_PX: i32 = 18;
const PARAM_WIRE_ENTRY_TAIL_PX: i32 = 18;

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

        let overlays_key = overlays_layer_key(project, state, panel_width, height);
        if self.cached_overlays_key != Some(overlays_key) {
            self.cached_overlays_key = Some(overlays_key);
            self.frame.dirty.overlays = true;
            self.rebuild_overlays_layer(project, state, panel_width, height);
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

    fn rebuild_overlays_layer(
        &mut self,
        project: &GuiProject,
        state: &PreviewState,
        panel_width: usize,
        panel_height: usize,
    ) {
        let before = self.layer_capacity(ActiveLayer::Overlays);
        self.set_active_layer(ActiveLayer::Overlays);
        self.clear_active_layer();
        self.push_param_links(project, state);
        self.push_wire_drag(project, state);
        self.push_param_dropdown(project, state);
        self.push_right_marquee(state);
        self.push_link_cut(state);
        self.push_menu(state);
        self.push_timeline(state, panel_width, panel_height);
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
            let Some((default_to_x, default_to_y)) = input_pin_center(target) else {
                continue;
            };
            let (default_to_x, default_to_y) =
                graph_point_to_panel(default_to_x, default_to_y, state);
            for source_id in target.inputs() {
                let Some(source) = project.node(*source_id) else {
                    continue;
                };
                let Some((from_x, from_y)) = output_pin_center(source) else {
                    continue;
                };
                let (from_x, from_y) = graph_point_to_panel(from_x, from_y, state);
                let link_kind = project.link_resource_kind(*source_id, target.id());
                if link_kind == Some(ResourceKind::Signal) {
                    continue;
                }
                let (to_x, to_y) = (default_to_x, default_to_y);
                let color = if edge_intersects_cut_line(state, from_x, from_y, to_x, to_y) {
                    CUT_EDGE_COLOR
                } else {
                    EDGE_COLOR
                };
                self.push_straight_wire_with_round_caps(from_x, from_y, to_x, to_y, color);
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
            let bind_hover = state
                .hover_param_target
                .map(|target| target.node_id == node.id() && target.param_index == index)
                .unwrap_or(false);
            if bind_hover {
                self.push_rect(
                    Rect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h),
                    PARAM_BIND_HOVER,
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
            let bound_color = if row.bound {
                PARAM_EDGE_COLOR
            } else {
                NODE_TEXT
            };
            self.push_graph_text_in_rect(label_rect, 0, fitted_label, bound_color, state);
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
            self.push_value_editor_text(value_rect, value_text, active_edit, bound_color, state);
            if row.dropdown {
                let arrow_y = value_rect.y + value_rect.h / 2;
                let arrow_x = value_rect.x + value_rect.w - 8;
                self.push_line(arrow_x - 3, arrow_y - 1, arrow_x, arrow_y + 2, bound_color);
                self.push_line(arrow_x, arrow_y + 2, arrow_x + 3, arrow_y - 1, bound_color);
            }
            self.push_border(
                value_rect,
                if editing {
                    PARAM_VALUE_ACTIVE
                } else if row.bound {
                    PARAM_EDGE_COLOR
                } else {
                    PARAM_VALUE_BORDER
                },
            );
        }
        self.label_scratch = label_scratch;
        self.fitted_label_scratch = fitted_label_scratch;
    }

    fn push_param_dropdown(&mut self, project: &GuiProject, state: &PreviewState) {
        let Some(dropdown) = state.param_dropdown else {
            return;
        };
        let Some(node) = project.node(dropdown.node_id) else {
            return;
        };
        let Some(options) =
            project.node_param_dropdown_options(dropdown.node_id, dropdown.param_index)
        else {
            return;
        };
        if options.is_empty() {
            return;
        }
        let Some(list_world) = node_param_dropdown_rect(node, dropdown.param_index, options.len())
        else {
            return;
        };
        let list_panel = graph_rect_to_panel(list_world, state);
        self.push_rect(list_panel, PARAM_DROPDOWN_BG);
        self.push_border(list_panel, PARAM_VALUE_BORDER);
        let selected = project
            .node_param_dropdown_selected_index(dropdown.node_id, dropdown.param_index)
            .unwrap_or(0);
        for (index, option) in options.iter().enumerate() {
            let row_world = Rect::new(
                list_world.x,
                list_world.y + index as i32 * super::project::NODE_PARAM_DROPDOWN_ROW_HEIGHT,
                list_world.w,
                super::project::NODE_PARAM_DROPDOWN_ROW_HEIGHT,
            );
            let row_panel = graph_rect_to_panel(row_world, state);
            if index == selected {
                self.push_rect(row_panel, PARAM_DROPDOWN_SELECTED);
            }
            if state.hover_dropdown_item == Some(index) {
                self.push_rect(row_panel, PARAM_DROPDOWN_HOVER);
            }
            self.push_graph_text_in_rect(row_panel, 4, option.label, NODE_TEXT, state);
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
            rect.y + 6,
            "Create Node",
            MENU_TEXT,
        );
        let search_rect = state.menu.search_rect();
        let search_text = if state.menu.is_category_picker() {
            "Choose a category"
        } else if state.menu.query.is_empty() {
            "Search in category..."
        } else {
            state.menu.query.as_str()
        };
        self.push_rect(search_rect, MENU_SEARCH_BG);
        self.push_border(search_rect, MENU_BORDER);
        self.push_text(search_rect.x + 6, search_rect.y + 7, search_text, MENU_TEXT);
        let entries = state.menu.visible_entries();
        if entries.is_empty() {
            self.push_text(
                rect.x + MENU_INNER_PADDING + 6,
                search_rect.y + search_rect.h + MENU_BLOCK_GAP + 6,
                "No matching nodes",
                MENU_CATEGORY_TEXT,
            );
            return;
        }
        let mut menu_label_scratch = std::mem::take(&mut self.label_scratch);
        for (entry_index, entry) in entries.into_iter().enumerate() {
            let Some(item) = state.menu.entry_rect(entry_index) else {
                continue;
            };
            if state.menu.selected == entry_index || state.hover_menu_item == Some(entry_index) {
                if !matches!(entry, AddNodeMenuEntry::Category(_)) {
                    self.push_rect(item, MENU_SELECTED);
                }
            }
            let (text, color) = match entry {
                AddNodeMenuEntry::Category(category) => {
                    let chip = category_chip_rect(
                        item,
                        self.text_renderer.measure_text_width(category.label(), 1.0),
                    );
                    self.push_rect(chip, category_menu_color(category));
                    if state.menu.selected == entry_index
                        || state.hover_menu_item == Some(entry_index)
                    {
                        self.push_border(
                            Rect::new(chip.x - 1, chip.y - 1, chip.w + 2, chip.h + 2),
                            MENU_SELECTED,
                        );
                    }
                    self.push_border(chip, MENU_CATEGORY_CHIP_BORDER);
                    menu_label_scratch.clear();
                    menu_label_scratch.push_str(category.label());
                    self.push_text(
                        chip.x + 8,
                        chip.y + 2,
                        menu_label_scratch.as_str(),
                        MENU_CATEGORY_CHIP_TEXT,
                    );
                    (menu_label_scratch.as_str(), MENU_CATEGORY_TEXT)
                }
                AddNodeMenuEntry::Back => ("< Categories", MENU_CATEGORY_TEXT),
                AddNodeMenuEntry::Option(option_index) => {
                    let option = ADD_NODE_OPTIONS[option_index];
                    if state.menu.query.is_empty() {
                        (option.label(), MENU_TEXT)
                    } else {
                        menu_label_scratch.clear();
                        menu_label_scratch.push_str(option.category.label());
                        menu_label_scratch.push_str(" / ");
                        menu_label_scratch.push_str(option.label());
                        (menu_label_scratch.as_str(), MENU_TEXT)
                    }
                }
            };
            if !matches!(entry, AddNodeMenuEntry::Category(_)) {
                self.push_text(item.x + 6, item.y + 6, text, color);
            }
        }
        self.label_scratch = menu_label_scratch;
    }

    fn push_timeline(&mut self, state: &PreviewState, panel_width: usize, panel_height: usize) {
        if panel_width == 0 || panel_height == 0 {
            return;
        }
        let timeline = timeline_rect(panel_width, panel_height);
        let play_btn = play_button_rect(timeline);
        let pause_btn = pause_button_rect(timeline);
        let track = track_rect(timeline);
        self.push_rect(timeline, TIMELINE_BG);
        self.push_border(timeline, TIMELINE_BORDER);

        self.push_rect(
            play_btn,
            if !state.paused {
                TIMELINE_BTN_ACTIVE
            } else {
                TIMELINE_BTN_IDLE
            },
        );
        self.push_border(play_btn, TIMELINE_BORDER);
        let tri_x = play_btn.x + 8;
        let tri_y = play_btn.y + 5;
        self.push_line(tri_x, tri_y, tri_x, tri_y + play_btn.h - 10, TIMELINE_TEXT);
        self.push_line(
            tri_x,
            tri_y,
            tri_x + play_btn.w - 10,
            play_btn.y + play_btn.h / 2,
            TIMELINE_TEXT,
        );
        self.push_line(
            tri_x + play_btn.w - 10,
            play_btn.y + play_btn.h / 2,
            tri_x,
            tri_y + play_btn.h - 10,
            TIMELINE_TEXT,
        );

        self.push_rect(
            pause_btn,
            if state.paused {
                TIMELINE_BTN_ACTIVE
            } else {
                TIMELINE_BTN_IDLE
            },
        );
        self.push_border(pause_btn, TIMELINE_BORDER);
        let bar_h = (pause_btn.h - 10).max(4);
        self.push_rect(
            Rect::new(pause_btn.x + 7, pause_btn.y + 5, 3, bar_h),
            TIMELINE_TEXT,
        );
        self.push_rect(
            Rect::new(pause_btn.x + pause_btn.w - 10, pause_btn.y + 5, 3, bar_h),
            TIMELINE_TEXT,
        );

        self.push_rect(track, TIMELINE_TRACK_BG);
        self.push_border(track, TIMELINE_BORDER);
        let thumb_x = track_x_for_frame(track, state.frame_index);
        let fill_w = (thumb_x - track.x + 1).max(1).min(track.w);
        self.push_rect(
            Rect::new(track.x, track.y, fill_w, track.h),
            TIMELINE_TRACK_FILL,
        );
        self.push_rect(
            Rect::new(thumb_x - 1, track.y - 3, 3, track.h + 6),
            TIMELINE_TEXT,
        );

        let mut label = std::mem::take(&mut self.label_scratch);
        label.clear();
        let _ = write!(&mut label, "{}", TIMELINE_START_FRAME);
        self.push_text(track.x, timeline.y + 4, label.as_str(), TIMELINE_TEXT);
        label.clear();
        let _ = write!(&mut label, "{}", TIMELINE_END_FRAME);
        self.push_text(
            track.x + track.w - 22,
            timeline.y + 4,
            label.as_str(),
            TIMELINE_TEXT,
        );
        label.clear();
        label.push_str("Frame ");
        let _ = write!(&mut label, "{}", state.frame_index);
        self.push_text(
            track.x,
            timeline.y + timeline.h - 16,
            label.as_str(),
            TIMELINE_TEXT,
        );
        self.label_scratch = label;
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
        let (x1, y1) = if wire_drag_source_kind(project, wire) == Some(ResourceKind::Signal) {
            if let Some(target) = state.hover_param_target {
                if let Some(target_node) = project.node(target.node_id) {
                    if let Some(row) = node_param_row_rect(target_node, target.param_index) {
                        graph_point_to_panel(row.x + row.w - 4, row.y + row.h / 2, state)
                    } else {
                        (wire.cursor_x, wire.cursor_y)
                    }
                } else {
                    (wire.cursor_x, wire.cursor_y)
                }
            } else {
                (wire.cursor_x, wire.cursor_y)
            }
        } else if let Some(target_id) = state.hover_input_pin {
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
        if wire_drag_source_kind(project, wire) == Some(ResourceKind::Signal) {
            if state.hover_param_target.is_some() {
                self.push_signal_wire_right_exit_entry(x0, y0, x1, y1, PARAM_EDGE_COLOR);
                self.push_param_target_marker(x1, y1, PARAM_EDGE_COLOR);
            } else {
                self.push_signal_wire_right_exit(x0, y0, x1, y1, PARAM_EDGE_COLOR);
            }
        } else {
            self.push_straight_wire_with_round_caps(x0, y0, x1, y1, PIN_HOVER);
        }
    }

    fn push_param_links(&mut self, project: &GuiProject, state: &PreviewState) {
        if project.edge_count() == 0 {
            return;
        }
        let obstacles = collect_panel_node_obstacles(project, state);
        for target in project.nodes() {
            for param_index in 0..target.param_count() {
                let Some(source_id) = project.signal_source_for_param(target.id(), param_index)
                else {
                    continue;
                };
                let Some(source) = project.node(source_id) else {
                    continue;
                };
                let Some((from_x, from_y)) = output_pin_center(source) else {
                    continue;
                };
                let Some(row) = node_param_row_rect(target, param_index) else {
                    continue;
                };
                let (gx, gy) = (row.x + row.w - 4, row.y + row.h / 2);
                let (from_x, from_y) = graph_point_to_panel(from_x, from_y, state);
                let (to_x, to_y) = graph_point_to_panel(gx, gy, state);
                let exit_x = from_x.saturating_add(PARAM_WIRE_EXIT_TAIL_PX);
                let entry_x = to_x.saturating_add(PARAM_WIRE_ENTRY_TAIL_PX);
                let route =
                    wire_route::route_param_path((exit_x, from_y), (entry_x, to_y), &obstacles);
                let color = if edge_intersects_cut_line(state, from_x, from_y, exit_x, from_y)
                    || path_intersects_cut_line(state, &route)
                    || edge_intersects_cut_line(state, entry_x, to_y, to_x, to_y)
                {
                    CUT_EDGE_COLOR
                } else {
                    PARAM_EDGE_COLOR
                };
                self.push_line(from_x, from_y, exit_x, from_y, color);
                self.push_path_lines(&route, color);
                self.push_line(entry_x, to_y, to_x, to_y, color);
                self.push_param_target_marker(to_x, to_y, color);
            }
        }
    }

    fn push_path_lines(&mut self, points: &[(i32, i32)], color: Color) {
        if points.len() < 2 {
            return;
        }
        for segment in points.windows(2) {
            let (x0, y0) = segment[0];
            let (x1, y1) = segment[1];
            self.push_line(x0, y0, x1, y1, color);
        }
    }

    fn push_signal_wire_right_exit(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        let exit_x = x0.saturating_add(PARAM_WIRE_EXIT_TAIL_PX);
        self.push_line(x0, y0, exit_x, y0, color);
        self.push_rounded_signal_wire(exit_x, y0, x1, y1, color);
    }

    fn push_signal_wire_right_exit_entry(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Color,
    ) {
        let exit_x = x0.saturating_add(PARAM_WIRE_EXIT_TAIL_PX);
        let entry_x = x1.saturating_add(PARAM_WIRE_ENTRY_TAIL_PX);
        self.push_line(x0, y0, exit_x, y0, color);
        self.push_rounded_signal_wire(exit_x, y0, entry_x, y1, color);
        self.push_line(entry_x, y1, x1, y1, color);
    }

    fn push_rounded_signal_wire(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        if dx.abs() < 8 || dy.abs() < 8 {
            self.push_line(x0, y0, x1, y1, color);
            return;
        }
        let step = (dx.abs() / 2).max(24);
        let bend_x = if dx >= 0 { x0 + step } else { x0 - step };
        let r1 = rounded_corner_radius(x0, y0, bend_x, y0, bend_x, y1);
        let r2 = rounded_corner_radius(bend_x, y0, bend_x, y1, x1, y1);
        let sx = bend_x - dx.signum() * r1;
        let sy = y0;
        let cx1 = bend_x;
        let cy1 = y0 + dy.signum() * r1;
        let cx2 = bend_x;
        let cy2 = y1 - dy.signum() * r2;
        let ex = bend_x + (x1 - bend_x).signum() * r2;
        let ey = y1;
        self.push_line(x0, y0, sx, sy, color);
        self.push_line(sx, sy, cx1, cy1, color);
        self.push_line(cx1, cy1, cx2, cy2, color);
        self.push_line(cx2, cy2, ex, ey, color);
        self.push_line(ex, ey, x1, y1, color);
    }

    fn push_straight_wire_with_round_caps(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Color,
    ) {
        if x0 == x1 && y0 == y1 {
            self.push_round_endpoint(x0, y0, color);
            return;
        }
        self.push_line(x0, y0, x1, y1, color);
        self.push_round_endpoint(x0, y0, color);
        self.push_round_endpoint(x1, y1, color);
    }

    fn push_round_endpoint(&mut self, cx: i32, cy: i32, color: Color) {
        for dy in -WIRE_ENDPOINT_RADIUS_PX..=WIRE_ENDPOINT_RADIUS_PX {
            let yy = cy + dy;
            let radius_sq = WIRE_ENDPOINT_RADIUS_PX * WIRE_ENDPOINT_RADIUS_PX;
            let span_sq = radius_sq - (dy * dy);
            let span = (span_sq as f32).sqrt().floor() as i32;
            self.push_line(cx - span, yy, cx + span, yy, color);
        }
    }

    fn push_param_target_marker(&mut self, cx: i32, cy: i32, color: Color) {
        for dy in -PARAM_BIND_TARGET_RADIUS_PX..=PARAM_BIND_TARGET_RADIUS_PX {
            let yy = cy + dy;
            let radius_sq = PARAM_BIND_TARGET_RADIUS_PX * PARAM_BIND_TARGET_RADIUS_PX;
            let span_sq = radius_sq - (dy * dy);
            let span = (span_sq as f32).sqrt().floor() as i32;
            self.push_line(cx - span, yy, cx + span, yy, color);
        }
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
        color: Color,
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
        self.text_renderer
            .push_text_scaled(out, text_x, text_y, text, color, state.zoom);
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

fn collect_panel_node_obstacles(
    project: &GuiProject,
    state: &PreviewState,
) -> Vec<wire_route::NodeObstacle> {
    let mut out = Vec::new();
    for node in project.nodes() {
        out.push(wire_route::NodeObstacle {
            rect: node_rect(node, state),
        });
    }
    out
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
    hash = hash_opt_param_target(hash, state.hover_param_target);
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
    hash = hash_u64(hash, project.render_signature());
    hash = hash_u64(hash, project.ui_signature());
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

fn overlays_layer_key(
    project: &GuiProject,
    state: &PreviewState,
    panel_width: usize,
    panel_height: usize,
) -> u64 {
    let mut hash = hash_start();
    hash = hash_u64(hash, project.render_signature());
    hash = hash_u64(hash, project.ui_signature());
    hash = hash_f32(hash, state.pan_x);
    hash = hash_f32(hash, state.pan_y);
    hash = hash_f32(hash, state.zoom);
    hash = hash_opt_u32(hash, state.hover_input_pin);
    hash = hash_opt_param_target(hash, state.hover_param_target);
    hash = hash_opt_dropdown(hash, state.param_dropdown);
    hash = hash_opt_wire(hash, state.wire_drag);
    hash = hash_opt_cut_line(hash, state.link_cut);
    hash = hash_opt_marquee(hash, state.right_marquee);
    if let Some(index) = state.hover_dropdown_item {
        hash = hash_u64(hash, index as u64);
    }
    hash = hash_u64(hash, state.menu.open as u64);
    hash = hash_i32(hash, state.menu.x);
    hash = hash_i32(hash, state.menu.y);
    hash = hash_u64(hash, state.menu.selected as u64);
    if let Some(category) = state.menu.active_category {
        for byte in category.label().as_bytes() {
            hash = hash_u64(hash, *byte as u64);
        }
    } else {
        hash = hash_u64(hash, u64::MAX - 5);
    }
    for byte in state.menu.query.as_bytes() {
        hash = hash_u64(hash, *byte as u64);
    }
    if let Some(index) = state.hover_menu_item {
        hash = hash_u64(hash, index as u64);
    }
    hash = hash_u64(hash, panel_width as u64);
    hash = hash_u64(hash, panel_height as u64);
    hash = hash_u64(hash, state.frame_index as u64);
    hash = hash_u64(hash, state.paused as u64);
    hash = hash_u64(hash, state.timeline_scrub_active as u64);
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

fn hash_opt_param_target(seed: u64, value: Option<super::state::HoverParamTarget>) -> u64 {
    let Some(target) = value else {
        return hash_u64(seed, u64::MAX - 4);
    };
    let hash = hash_u64(seed, target.node_id as u64);
    hash_u64(hash, target.param_index as u64)
}

fn hash_opt_dropdown(seed: u64, value: Option<super::state::ParamDropdownState>) -> u64 {
    let Some(dropdown) = value else {
        return hash_u64(seed, u64::MAX - 6);
    };
    let hash = hash_u64(seed, dropdown.node_id as u64);
    hash_u64(hash, dropdown.param_index as u64)
}

fn wire_drag_source_kind(
    project: &GuiProject,
    wire: super::state::WireDragState,
) -> Option<ResourceKind> {
    let source = project.node(wire.source_node_id)?;
    source.kind().output_resource_kind()
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
        ProjectNodeKind::TexCircle => Color::argb(AGIO.node_header_tex_circle),
        ProjectNodeKind::BufSphere => Color::argb(AGIO.node_header_buf_sphere),
        ProjectNodeKind::BufCircleNurbs => Color::argb(AGIO.node_header_buf_circle_nurbs),
        ProjectNodeKind::BufNoise => Color::argb(AGIO.node_header_buf_noise),
        ProjectNodeKind::TexTransform2D => Color::argb(AGIO.node_header_tex_transform_2d),
        ProjectNodeKind::TexFeedback => Color::argb(AGIO.node_header_tex_feedback),
        ProjectNodeKind::SceneEntity => Color::argb(AGIO.node_header_scene_entity),
        ProjectNodeKind::SceneBuild => Color::argb(AGIO.node_header_scene_build),
        ProjectNodeKind::RenderCamera => Color::argb(AGIO.node_header_render_camera),
        ProjectNodeKind::RenderScenePass => Color::argb(AGIO.node_header_render_scene_pass),
        ProjectNodeKind::CtlLfo => Color::argb(AGIO.node_header_ctl_lfo),
        ProjectNodeKind::IoWindowOut => Color::argb(AGIO.node_header_io_window_out),
    }
}

fn category_menu_color(category: AddNodeCategory) -> Color {
    match category {
        AddNodeCategory::Texture => Color::argb(AGIO.node_header_tex_solid),
        AddNodeCategory::Buffer => Color::argb(AGIO.node_header_buf_sphere),
        AddNodeCategory::Scene => Color::argb(AGIO.node_header_scene_entity),
        AddNodeCategory::Render => Color::argb(AGIO.node_header_render_scene_pass),
        AddNodeCategory::Control => Color::argb(AGIO.node_header_ctl_lfo),
        AddNodeCategory::Io => Color::argb(AGIO.node_header_io_window_out),
    }
}

fn category_chip_rect(item: Rect, text_width: i32) -> Rect {
    let chip_w = (text_width + 26).clamp(58, item.w);
    let chip_h = (item.h - 2).max(16);
    Rect::new(item.x + 6, item.y + ((item.h - chip_h) / 2), chip_w, chip_h)
}

fn rounded_corner_radius(ax: i32, ay: i32, bx: i32, by: i32, cx: i32, cy: i32) -> i32 {
    let in_len = (bx - ax).abs() + (by - ay).abs();
    let out_len = (cx - bx).abs() + (cy - by).abs();
    if in_len < 2 || out_len < 2 {
        return 0;
    }
    (in_len.min(out_len) / 2).clamp(2, 12)
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

fn path_intersects_cut_line(state: &PreviewState, points: &[(i32, i32)]) -> bool {
    if points.len() < 2 {
        return false;
    }
    for segment in points.windows(2) {
        if edge_intersects_cut_line(
            state,
            segment[0].0,
            segment[0].1,
            segment[1].0,
            segment[1].1,
        ) {
            return true;
        }
    }
    false
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

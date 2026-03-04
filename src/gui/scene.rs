//! Retained-style scene assembly for the GPU GUI renderer.
//!
//! The builder partitions GUI geometry into retained layers and marks only
//! changed layers dirty each update (`static_panel`, `edges`, `nodes`,
//! `param_wires`, `overlays`). Rendering stays on GPU and unchanged layers are
//! reused.

mod layers;
mod layout;
mod menus;
mod signal_scope;
mod style;
mod timeline_helpers;
mod timeline_layer;
pub(super) mod wire_route;
mod wires;

use std::fmt::Write as _;
use std::{collections::HashMap, collections::HashSet, sync::Arc, time::Instant};

use super::geometry::Rect;
use super::project::{
    collapsed_param_entry_pin_center, input_pin_center, node_expand_toggle_rect,
    node_param_dropdown_rect, node_param_row_rect, node_param_value_rect, output_pin_center,
    pin_rect, GuiProject, ProjectNode, ResourceKind, SignalEvalPath, SignalEvalStack,
    SignalSampleMemo, NODE_WIDTH,
};
use super::state::{
    AddNodeMenuEntry, ExportMenuItem, MainMenuItem, PreviewState, ADD_NODE_OPTIONS, MENU_BLOCK_GAP,
    MENU_INNER_PADDING,
};
use super::text::GuiTextRenderer;
use super::timeline::editor_panel_height;
use layers::{active_scene_layer_mut, ActiveLayer};
use layout::*;
use menus::{
    FittedLabelCacheBucketKey, FITTED_LABEL_CACHE_MAX_BUCKETS,
    FITTED_LABEL_CACHE_MAX_ENTRIES_PER_BUCKET,
};
use signal_scope::{
    signal_scope_range, signal_scope_y, SignalScopeCacheEntry, SignalScopeRecomputeConfig,
    SIGNAL_SCOPE_MAX_SAMPLES,
};
use style::*;
#[cfg(test)]
use timeline_helpers::timeline_beat_indicator_on;
use wires::{
    bridged_segment_points_into, cluster_bridge_ranges_into, next_staggered_tail_cells,
    path_intersects_cut_line, segment_crossings, smooth_param_wire_path_with_end_caps,
    BridgeSegmentSpatialHash, DrawnWireSegment, PARAM_BIND_TARGET_RADIUS_PX,
    PARAM_WIRE_ENTRY_TAIL_PX, PARAM_WIRE_EXIT_TAIL_PX, WIRE_ENDPOINT_RADIUS_PX,
};

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
    pub(crate) space: CoordSpace,
}

/// Line segment primitive.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ColoredLine {
    pub(crate) x0: i32,
    pub(crate) y0: i32,
    pub(crate) x1: i32,
    pub(crate) y1: i32,
    pub(crate) color: Color,
    pub(crate) space: CoordSpace,
}

/// Coordinate space used by one GUI primitive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CoordSpace {
    #[default]
    Screen,
    Graph,
}

/// One frame of GPU scene primitives.
#[derive(Debug, Default)]
pub(crate) struct SceneFrame {
    pub(crate) clear: Option<Color>,
    pub(crate) export_preview_rect: Option<Rect>,
    pub(crate) static_panel: SceneLayer,
    pub(crate) edges: SceneLayer,
    pub(crate) nodes: SceneLayer,
    pub(crate) param_wires: SceneLayer,
    pub(crate) overlays: SceneLayer,
    pub(crate) timeline: SceneLayer,
    pub(crate) dirty: SceneLayerDirty,
    pub(crate) ui_alloc_bytes: u64,
    pub(crate) bridge_intersection_tests: u64,
    pub(crate) signal_scope_samples: u64,
    pub(crate) signal_scope_eval_ms: f64,
    pub(crate) nodes_ms: f64,
    pub(crate) edges_ms: f64,
    pub(crate) overlays_ms: f64,
    pub(crate) camera_pan_x: f32,
    pub(crate) camera_pan_y: f32,
    pub(crate) camera_zoom: f32,
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
    pub(crate) param_wires: bool,
    pub(crate) overlays: bool,
    pub(crate) timeline: bool,
}

impl SceneLayerDirty {
    /// Return true when any retained layer needs a GPU buffer update.
    pub(crate) fn any(self) -> bool {
        self.static_panel
            || self.edges
            || self.nodes
            || self.param_wires
            || self.overlays
            || self.timeline
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct EdgeRouteCacheKey {
    source_id: u32,
    target_id: u32,
    obstacle_epoch: u64,
    start_tail_cells: i32,
    end_tail_cells: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ParamRouteCacheKey {
    source_id: u32,
    target_id: u32,
    param_index: usize,
    obstacle_epoch: u64,
    start_tail_cells: i32,
    end_tail_cells: i32,
}

/// Stateful cache and scratch storage for wire-route planning.
#[derive(Default)]
struct WireRouteService {
    edge_cache_epoch: Option<u64>,
    edge_cache: HashMap<EdgeRouteCacheKey, Arc<[(i32, i32)]>>,
    edge_obstacle_map: wire_route::RouteObstacleMap,
    edge_occupied: wire_route::RouteOccupiedEdges,
    param_cache_epoch: Option<u64>,
    param_cache: HashMap<ParamRouteCacheKey, Arc<[(i32, i32)]>>,
    param_obstacle_map: wire_route::RouteObstacleMap,
    edge_live_route_keys_scratch: HashSet<EdgeRouteCacheKey>,
    edge_tail_slots_scratch: HashMap<((i32, i32), wire_route::RouteDirection), i32>,
    edge_route_panel_scratch: Vec<(i32, i32)>,
    param_live_route_keys_scratch: HashSet<ParamRouteCacheKey>,
    param_tail_slots_scratch: HashMap<((i32, i32), wire_route::RouteDirection), i32>,
    param_route_panel_scratch: Vec<(i32, i32)>,
}

impl WireRouteService {
    fn invalidate_epochs(&mut self) {
        self.edge_cache_epoch = None;
        self.param_cache_epoch = None;
    }
}

/// Stateful scene builder that reuses allocation capacity across frames.
#[derive(Default)]
pub(crate) struct SceneBuilder {
    frame: SceneFrame,
    active_layer: ActiveLayer,
    active_space: CoordSpace,
    cached_static_key: Option<(usize, usize, usize)>,
    cached_nodes_epoch: Option<u64>,
    cached_edges_epoch: Option<u64>,
    cached_param_wires_epoch: Option<u64>,
    cached_param_wires_overlay_epoch: Option<u64>,
    cached_overlays_epoch: Option<u64>,
    cached_timeline_epoch: Option<u64>,
    wire_routes: WireRouteService,
    text_renderer: GuiTextRenderer,
    label_scratch: String,
    fitted_label_scratch: String,
    fitted_label_cache: HashMap<FittedLabelCacheBucketKey, HashMap<String, String>>,
    signal_eval_stack: SignalEvalStack,
    signal_sample_memo: SignalSampleMemo,
    signal_scope_cache: HashMap<u32, SignalScopeCacheEntry>,
    live_signal_scope_nodes: HashSet<u32>,
    signal_scope_line_scratch: Vec<(i32, i32, i32, i32)>,
    selected_nodes_lookup_scratch: HashSet<u32>,
    edge_drawn_segments_scratch: Vec<DrawnWireSegment>,
    edge_drawn_segment_hash_scratch: BridgeSegmentSpatialHash,
    param_drawn_segments_scratch: Vec<DrawnWireSegment>,
    param_drawn_segment_hash_scratch: BridgeSegmentSpatialHash,
    bridge_new_segments_scratch: Vec<DrawnWireSegment>,
    bridge_candidate_indices_scratch: Vec<usize>,
    bridge_crossings_scratch: Vec<f32>,
    bridge_clusters_scratch: Vec<(f32, f32)>,
    bridge_points_scratch: Vec<(i32, i32)>,
    frame_alloc_bytes: u64,
    was_dragging: bool,
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
        timeline_fps: u32,
    ) -> &SceneFrame {
        self.frame.clear = Some(PREVIEW_BG);
        self.frame.export_preview_rect = state
            .export_menu
            .open
            .then(|| state.export_menu.preview_viewport_rect());
        self.frame.dirty = SceneLayerDirty::default();
        self.frame_alloc_bytes = 0;
        self.frame.bridge_intersection_tests = 0;
        self.frame.signal_scope_samples = 0;
        self.frame.signal_scope_eval_ms = 0.0;
        self.frame.nodes_ms = 0.0;
        self.frame.edges_ms = 0.0;
        self.frame.overlays_ms = 0.0;
        self.signal_sample_memo.clear();
        self.frame.camera_pan_x = state.pan_x;
        self.frame.camera_pan_y = state.pan_y;
        self.frame.camera_zoom = state.zoom.max(0.001);
        let drag_just_released = self.was_dragging && state.drag.is_none();
        self.was_dragging = state.drag.is_some();
        if drag_just_released {
            // Force one post-drop recompute pass so cached wire routes refresh even
            // when epoch invalidation was suppressed during drag freeze.
            self.cached_edges_epoch = None;
            self.cached_param_wires_epoch = None;
            self.cached_param_wires_overlay_epoch = None;
            self.cached_overlays_epoch = None;
            self.wire_routes.invalidate_epochs();
        }

        self.rebuild_static_if_needed(width, height, panel_width);

        let nodes_epoch = state.invalidation.nodes;
        if self.cached_nodes_epoch != Some(nodes_epoch) {
            self.cached_nodes_epoch = Some(nodes_epoch);
            self.frame.dirty.nodes = true;
            let start = Instant::now();
            self.rebuild_nodes_layer(project, state, timeline_fps);
            self.frame.nodes_ms = start.elapsed().as_secs_f64() * 1000.0;
        }

        let edges_epoch = state.invalidation.wires;
        let freeze_edges_for_drag = state.drag.is_some() && self.cached_edges_epoch.is_some();
        if !freeze_edges_for_drag && self.cached_edges_epoch != Some(edges_epoch) {
            self.cached_edges_epoch = Some(edges_epoch);
            self.frame.dirty.edges = true;
            let start = Instant::now();
            self.rebuild_edges_layer(project, state);
            self.frame.edges_ms = start.elapsed().as_secs_f64() * 1000.0;
        }

        let param_wires_epoch = state.invalidation.wires;
        let param_wires_overlay_epoch = state.link_cut.map(|_| state.invalidation.overlays);
        let freeze_param_wires_for_drag =
            state.drag.is_some() && self.cached_param_wires_epoch.is_some();
        if !freeze_param_wires_for_drag
            && (self.cached_param_wires_epoch != Some(param_wires_epoch)
                || self.cached_param_wires_overlay_epoch != param_wires_overlay_epoch)
        {
            self.cached_param_wires_epoch = Some(param_wires_epoch);
            self.cached_param_wires_overlay_epoch = param_wires_overlay_epoch;
            self.frame.dirty.param_wires = true;
            let start = Instant::now();
            self.rebuild_param_wires_layer(project, state);
            self.frame.overlays_ms += start.elapsed().as_secs_f64() * 1000.0;
        }

        let overlays_epoch = state.invalidation.overlays;
        if self.cached_overlays_epoch != Some(overlays_epoch) {
            self.cached_overlays_epoch = Some(overlays_epoch);
            self.frame.dirty.overlays = true;
            let start = Instant::now();
            self.rebuild_overlays_layer(project, state, panel_width, height);
            self.frame.overlays_ms += start.elapsed().as_secs_f64() * 1000.0;
        }

        let timeline_epoch = state.invalidation.timeline;
        if self.cached_timeline_epoch != Some(timeline_epoch) {
            self.cached_timeline_epoch = Some(timeline_epoch);
            self.frame.dirty.timeline = true;
            self.rebuild_timeline_layer(state, width, height, timeline_fps);
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
        self.set_active_space(CoordSpace::Screen);
        self.clear_active_layer();
        let editor_h = editor_panel_height(height) as i32;
        if editor_h > 0 {
            self.push_rect(Rect::new(0, 0, panel_width as i32, editor_h), PANEL_BG);
            let x = panel_width as i32 - 1;
            self.push_line(x, 0, x, editor_h.saturating_sub(1), BORDER_COLOR);
        }
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::StaticPanel));
    }

    fn rebuild_nodes_layer(
        &mut self,
        project: &GuiProject,
        state: &PreviewState,
        timeline_fps: u32,
    ) {
        let before = self.layer_capacity(ActiveLayer::Nodes);
        self.set_active_layer(ActiveLayer::Nodes);
        self.set_active_space(CoordSpace::Screen);
        self.clear_active_layer();
        self.live_signal_scope_nodes.clear();
        self.push_header(project);
        self.set_active_space(CoordSpace::Graph);
        self.push_nodes(
            project,
            state,
            timeline_fps,
            project.invalidation().tex_eval,
        );
        self.signal_scope_cache
            .retain(|node_id, _| self.live_signal_scope_nodes.contains(node_id));
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::Nodes));
    }

    fn rebuild_edges_layer(&mut self, project: &GuiProject, state: &PreviewState) {
        let before = self.layer_capacity(ActiveLayer::Edges);
        self.set_active_layer(ActiveLayer::Edges);
        self.set_active_space(CoordSpace::Graph);
        self.clear_active_layer();
        self.push_edges(project, state);
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::Edges));
    }

    fn rebuild_param_wires_layer(&mut self, project: &GuiProject, state: &PreviewState) {
        let before = self.layer_capacity(ActiveLayer::ParamWires);
        self.set_active_layer(ActiveLayer::ParamWires);
        self.set_active_space(CoordSpace::Graph);
        self.clear_active_layer();
        self.push_param_links(project, state);
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::ParamWires));
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
        self.set_active_space(CoordSpace::Graph);
        self.clear_active_layer();
        self.push_param_dropdown(project, state);
        self.set_active_space(CoordSpace::Screen);
        self.push_wire_drag(project, state);
        self.push_right_marquee(state);
        self.push_link_cut(state);
        self.push_menu(state);
        self.push_main_menu(state);
        self.push_export_menu(state);
        self.push_help_modal(state, panel_width, panel_height);
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::Overlays));
    }

    fn rebuild_timeline_layer(
        &mut self,
        state: &PreviewState,
        viewport_width: usize,
        height: usize,
        timeline_fps: u32,
    ) {
        let before = self.layer_capacity(ActiveLayer::Timeline);
        self.set_active_layer(ActiveLayer::Timeline);
        self.set_active_space(CoordSpace::Screen);
        self.clear_active_layer();
        timeline_layer::push_timeline(self, state, viewport_width, height, timeline_fps);
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::Timeline));
    }

    fn set_active_layer(&mut self, layer: ActiveLayer) {
        self.active_layer = layer;
    }

    fn set_active_space(&mut self, space: CoordSpace) {
        self.active_space = space;
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
        self.wire_routes.edge_occupied = wire_route::RouteOccupiedEdges::default();
        if project.edge_count() == 0 {
            return;
        }
        let obstacle_epoch = edge_route_obstacle_epoch(project);
        if self.wire_routes.edge_cache_epoch != Some(obstacle_epoch) {
            self.wire_routes.edge_cache_epoch = Some(obstacle_epoch);
            self.wire_routes.edge_cache.clear();
            let obstacles = collect_graph_node_obstacles(project);
            self.wire_routes.edge_obstacle_map =
                wire_route::RouteObstacleMap::from_obstacles(obstacles.as_slice());
        }
        let active_epoch = self.wire_routes.edge_cache_epoch.unwrap_or(obstacle_epoch);
        let mut live_route_keys =
            std::mem::take(&mut self.wire_routes.edge_live_route_keys_scratch);
        let mut drawn_segments = std::mem::take(&mut self.edge_drawn_segments_scratch);
        let mut drawn_segment_hash = std::mem::take(&mut self.edge_drawn_segment_hash_scratch);
        live_route_keys.clear();
        drawn_segments.clear();
        drawn_segment_hash.clear();
        let mut occupied_edges = wire_route::RouteOccupiedEdges::default();
        let mut tail_slots = std::mem::take(&mut self.wire_routes.edge_tail_slots_scratch);
        tail_slots.clear();
        let mut route_panel = std::mem::take(&mut self.wire_routes.edge_route_panel_scratch);
        route_panel.clear();
        for target in project.nodes() {
            let Some((default_to_x_graph, default_to_y_graph)) = input_pin_center(target) else {
                continue;
            };
            let (default_to_x, default_to_y) =
                graph_point_to_panel(default_to_x_graph, default_to_y_graph, state);
            for source_id in target.inputs() {
                let Some(source) = project.node(*source_id) else {
                    continue;
                };
                let Some((from_x_graph, from_y_graph)) = output_pin_center(source) else {
                    continue;
                };
                let (from_x, from_y) = graph_point_to_panel(from_x_graph, from_y_graph, state);
                let link_kind = project.link_resource_kind(*source_id, target.id());
                if link_kind == Some(ResourceKind::Signal) {
                    continue;
                }
                let (to_x, to_y) = (default_to_x, default_to_y);
                let insert_hover = state.drag.is_some()
                    && state
                        .hover_insert_link
                        .map(|link| link.source_id == *source_id && link.target_id == target.id())
                        .unwrap_or(false);
                let start_endpoint = wire_route::RouteEndpoint {
                    point: (from_x_graph, from_y_graph),
                    corridor_dir: wire_route::RouteDirection::East,
                };
                let end_endpoint = wire_route::RouteEndpoint {
                    point: (default_to_x_graph, default_to_y_graph),
                    corridor_dir: wire_route::RouteDirection::West,
                };
                let start_tail_cells = next_staggered_tail_cells(&mut tail_slots, start_endpoint);
                let end_tail_cells = next_staggered_tail_cells(&mut tail_slots, end_endpoint);
                let route_key = EdgeRouteCacheKey {
                    source_id: *source_id,
                    target_id: target.id(),
                    obstacle_epoch: active_epoch,
                    start_tail_cells,
                    end_tail_cells,
                };
                live_route_keys.insert(route_key);
                if !self.wire_routes.edge_cache.contains_key(&route_key) {
                    let route =
                        wire_route::route_wire_path_with_tail_cells_avoiding_overlaps_with_map(
                            start_endpoint,
                            end_endpoint,
                            &self.wire_routes.edge_obstacle_map,
                            &occupied_edges,
                            start_tail_cells,
                            end_tail_cells,
                        );
                    self.wire_routes
                        .edge_cache
                        .insert(route_key, Arc::from(route));
                }
                let Some(route_graph) = self.wire_routes.edge_cache.get(&route_key).cloned() else {
                    continue;
                };
                route_panel.clear();
                route_panel.extend(
                    route_graph
                        .iter()
                        .copied()
                        .map(|(x, y)| graph_point_to_panel(x, y, state)),
                );
                let color = if insert_hover {
                    EDGE_INSERT_HOVER
                } else if path_intersects_cut_line(state, route_panel.as_slice()) {
                    CUT_EDGE_COLOR
                } else {
                    EDGE_COLOR
                };
                self.push_path_lines_with_bridges(
                    route_panel.as_slice(),
                    color,
                    &mut drawn_segments,
                    &mut drawn_segment_hash,
                    state.zoom,
                );
                occupied_edges.record_path_non_tail(route_graph.as_ref());
                self.push_round_endpoint(from_x, from_y, color);
                self.push_round_endpoint(to_x, to_y, color);
            }
        }
        self.wire_routes.edge_occupied = occupied_edges;
        self.wire_routes
            .edge_cache
            .retain(|key, _| key.obstacle_epoch == active_epoch && live_route_keys.contains(key));
        route_panel.clear();
        tail_slots.clear();
        drawn_segments.clear();
        live_route_keys.clear();
        self.wire_routes.edge_route_panel_scratch = route_panel;
        self.wire_routes.edge_tail_slots_scratch = tail_slots;
        self.edge_drawn_segments_scratch = drawn_segments;
        self.edge_drawn_segment_hash_scratch = drawn_segment_hash;
        self.wire_routes.edge_live_route_keys_scratch = live_route_keys;
    }

    fn push_nodes(
        &mut self,
        project: &GuiProject,
        state: &PreviewState,
        timeline_fps: u32,
        tex_eval_epoch: u64,
    ) {
        let mut selected_nodes_lookup = std::mem::take(&mut self.selected_nodes_lookup_scratch);
        selected_nodes_lookup.clear();
        selected_nodes_lookup.extend(state.selected_nodes.iter().copied());
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
            } else if selected_nodes_lookup.contains(&node.id()) {
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
            if node.kind().shows_signal_preview() {
                self.push_signal_scope(project, node, state, timeline_fps, tex_eval_epoch);
            }
            if node.expanded() {
                self.push_node_params(node, state);
            }
            self.push_pins(node, state);
        }
        selected_nodes_lookup.clear();
        self.selected_nodes_lookup_scratch = selected_nodes_lookup;
    }

    fn push_signal_scope(
        &mut self,
        project: &GuiProject,
        node: &ProjectNode,
        state: &PreviewState,
        timeline_fps: u32,
        tex_eval_epoch: u64,
    ) {
        if !node.kind().shows_signal_preview() {
            return;
        }
        self.live_signal_scope_nodes.insert(node.id());
        let rect = node_rect(node, state);
        let mut scope_h = if node.expanded() {
            ((26.0 * state.zoom).round() as i32).clamp(14, 44)
        } else {
            ((18.0 * state.zoom).round() as i32).clamp(10, 30)
        };
        let pad_x = ((6.0 * state.zoom).round() as i32).clamp(4, 12);
        let pad_y = ((5.0 * state.zoom).round() as i32).clamp(3, 8);
        let mut scope_top_min = rect.y + pad_y;
        if node.expanded() && node.param_count() > 0 {
            if let Some(last_row) = node_param_row_rect(node, node.param_count() - 1) {
                let last_row = graph_rect_to_panel(last_row, state);
                let row_gap = ((4.0 * state.zoom).round() as i32).clamp(2, 8);
                scope_top_min = (last_row.y + last_row.h + row_gap).max(scope_top_min);
            }
        }
        let scope_bottom = rect.y + rect.h - pad_y;
        let max_scope_h = scope_bottom - scope_top_min;
        if max_scope_h < 8 {
            return;
        }
        scope_h = scope_h.min(max_scope_h);
        let scope_y = (scope_bottom - scope_h).max(scope_top_min);
        let scope = Rect::new(
            rect.x + pad_x,
            scope_y,
            (rect.w - (pad_x * 2)).max(12),
            scope_h,
        );
        self.push_rect(scope, NODE_SIGNAL_SCOPE_BG);
        self.push_border(scope, NODE_SIGNAL_SCOPE_BORDER);

        let inner = Rect::new(scope.x + 2, scope.y + 2, scope.w - 4, scope.h - 4);
        if inner.w < 8 || inner.h < 4 {
            return;
        }

        let window_secs = if node.expanded() { 2.0 } else { 1.2 };
        let time_now = state.frame_index as f32 / timeline_fps.max(1) as f32;
        let samples = (inner.w.max(16) as usize).min(SIGNAL_SCOPE_MAX_SAMPLES);
        let eval_start = Instant::now();
        let mut signal_scope_line_scratch = std::mem::take(&mut self.signal_scope_line_scratch);
        let (value_min, value_max) = {
            signal_scope_line_scratch.clear();
            let values = self.sample_signal_scope_values(
                project,
                node.id(),
                time_now,
                window_secs,
                samples,
                tex_eval_epoch,
            );
            let (value_min, value_max) = signal_scope_range(values);
            for step in 0..samples.saturating_sub(1) {
                let t0 = step as f32 / samples.saturating_sub(1).max(1) as f32;
                let t1 = (step + 1) as f32 / samples.saturating_sub(1).max(1) as f32;
                let v0 = values[step];
                let v1 = values[step + 1];
                let x0 = inner.x + ((inner.w - 1) as f32 * t0).round() as i32;
                let x1 = inner.x + ((inner.w - 1) as f32 * t1).round() as i32;
                let y0 = signal_scope_y(v0, value_min, value_max, inner);
                let y1 = signal_scope_y(v1, value_min, value_max, inner);
                signal_scope_line_scratch.push((x0, y0, x1, y1));
            }
            (value_min, value_max)
        };
        let eval_ms = eval_start.elapsed().as_secs_f64() * 1000.0;
        let y_zero = signal_scope_y(0.0, value_min, value_max, inner);
        let y_one = signal_scope_y(1.0, value_min, value_max, inner);
        self.push_line(
            inner.x,
            y_zero,
            inner.x + inner.w - 1,
            y_zero,
            NODE_SIGNAL_SCOPE_GUIDE_ZERO,
        );
        self.push_line(
            inner.x,
            y_one,
            inner.x + inner.w - 1,
            y_one,
            NODE_SIGNAL_SCOPE_GUIDE_ONE,
        );
        for (x0, y0, x1, y1) in signal_scope_line_scratch.iter().copied() {
            self.push_line(x0, y0, x1, y1, NODE_SIGNAL_SCOPE_WAVE);
        }
        signal_scope_line_scratch.clear();
        self.signal_scope_line_scratch = signal_scope_line_scratch;
        self.frame.signal_scope_eval_ms += eval_ms;
    }

    fn sample_signal_scope_values(
        &mut self,
        project: &GuiProject,
        node_id: u32,
        time_now: f32,
        window_secs: f32,
        sample_count: usize,
        tex_eval_epoch: u64,
    ) -> &[f32] {
        let step_secs = if sample_count > 1 {
            window_secs / (sample_count.saturating_sub(1) as f32)
        } else {
            window_secs
        };
        let step_secs = step_secs.max(1e-5);
        let window_secs_bits = window_secs.to_bits();
        let target_start = time_now - window_secs;
        let cache_compatible = self
            .signal_scope_cache
            .get(&node_id)
            .map(|entry| {
                entry.sample_count == sample_count
                    && entry.window_secs_bits == window_secs_bits
                    && entry.tex_eval_epoch == tex_eval_epoch
                    && entry.values.len() == sample_count
                    && (entry.step_secs - step_secs).abs() <= f32::EPSILON
                    && entry.start_time.is_finite()
                    && target_start >= entry.start_time
            })
            .unwrap_or(false);
        if !cache_compatible {
            self.recompute_signal_scope_values(
                project,
                node_id,
                SignalScopeRecomputeConfig {
                    start_time: target_start,
                    sample_count,
                    step_secs,
                    window_secs_bits,
                    tex_eval_epoch,
                },
            );
            return self
                .signal_scope_cache
                .get(&node_id)
                .map(|cached| cached.values.as_slice())
                .unwrap_or(&[]);
        }

        let cached_start = self
            .signal_scope_cache
            .get(&node_id)
            .map(|entry| entry.start_time)
            .unwrap_or(target_start);
        let delta_start = target_start - cached_start;
        let shift = (delta_start / step_secs).floor().max(0.0) as usize;
        if shift >= sample_count {
            self.recompute_signal_scope_values(
                project,
                node_id,
                SignalScopeRecomputeConfig {
                    start_time: target_start,
                    sample_count,
                    step_secs,
                    window_secs_bits,
                    tex_eval_epoch,
                },
            );
            return self
                .signal_scope_cache
                .get(&node_id)
                .map(|cached| cached.values.as_slice())
                .unwrap_or(&[]);
        }
        if shift > 0 {
            let shift_applied = self.try_shift_signal_scope_values(
                project,
                node_id,
                sample_count,
                step_secs,
                shift,
            );
            if !shift_applied {
                self.recompute_signal_scope_values(
                    project,
                    node_id,
                    SignalScopeRecomputeConfig {
                        start_time: target_start,
                        sample_count,
                        step_secs,
                        window_secs_bits,
                        tex_eval_epoch,
                    },
                );
            }
        }
        self.signal_scope_cache
            .get(&node_id)
            .map(|cached| cached.values.as_slice())
            .unwrap_or(&[])
    }

    /// Shift one existing signal-scope cache window and append freshly sampled tail values.
    ///
    /// Returns `true` when the shift path applied successfully. Returns `false`
    /// when the expected cache entry is missing so callers can safely fall back
    /// to full recompute without panicking.
    fn try_shift_signal_scope_values(
        &mut self,
        project: &GuiProject,
        node_id: u32,
        sample_count: usize,
        step_secs: f32,
        shift: usize,
    ) -> bool {
        let new_start_index = sample_count.saturating_sub(shift);
        let Some(start_time) = self.signal_scope_cache.get_mut(&node_id).map(|entry| {
            entry.values.rotate_left(shift);
            entry.start_time += step_secs * shift as f32;
            entry.start_time
        }) else {
            return false;
        };
        for index in new_start_index..sample_count {
            let sample_t = start_time + step_secs * index as f32;
            let value = self.sample_scope_value(project, node_id, sample_t.max(0.0));
            self.frame.signal_scope_samples = self.frame.signal_scope_samples.saturating_add(1);
            let Some(entry) = self.signal_scope_cache.get_mut(&node_id) else {
                return false;
            };
            entry.values[index] = value;
        }
        true
    }

    fn recompute_signal_scope_values(
        &mut self,
        project: &GuiProject,
        node_id: u32,
        config: SignalScopeRecomputeConfig,
    ) {
        let SignalScopeRecomputeConfig {
            start_time,
            sample_count,
            step_secs,
            window_secs_bits,
            tex_eval_epoch,
        } = config;
        let mut values = Vec::with_capacity(sample_count);
        for index in 0..sample_count {
            let sample_t = start_time + step_secs * index as f32;
            values.push(self.sample_scope_value(project, node_id, sample_t.max(0.0)));
            self.frame.signal_scope_samples = self.frame.signal_scope_samples.saturating_add(1);
        }
        self.signal_scope_cache.insert(
            node_id,
            SignalScopeCacheEntry {
                sample_count,
                window_secs_bits,
                tex_eval_epoch,
                start_time,
                step_secs,
                values,
            },
        );
    }

    fn sample_scope_value(&mut self, project: &GuiProject, node_id: u32, time_secs: f32) -> f32 {
        self.signal_eval_stack.clear_nodes();
        let value = project
            .sample_signal_node_with_memo(
                node_id,
                time_secs,
                &mut self.signal_eval_stack,
                &mut self.signal_sample_memo,
            )
            .unwrap_or(0.5);
        if value.is_finite() {
            value
        } else {
            0.5
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
            let soft_hover = state
                .hover_param
                .map(|target| target.node_id == node.id() && target.param_index == index)
                .unwrap_or(false);
            if bind_hover {
                self.push_rect(
                    Rect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h),
                    PARAM_BIND_HOVER,
                );
            } else if soft_hover {
                self.push_rect(
                    Rect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h),
                    PARAM_SOFT_HOVER,
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
            self.push_rect(
                value_rect,
                if row.action_button {
                    if soft_hover {
                        PARAM_ACTION_BG_HOVER
                    } else {
                        PARAM_ACTION_BG
                    }
                } else {
                    PARAM_VALUE_BG
                },
            );
            let alt_hover = state
                .hover_alt_param
                .map(|target| target.node_id == node.id() && target.param_index == index)
                .unwrap_or(false);
            let editing = state
                .param_edit
                .as_ref()
                .map(|edit| edit.node_id == node.id() && edit.param_index == index)
                .unwrap_or(false);
            if row.action_button {
                self.push_graph_text_in_rect(value_rect, 4, row.value_text, NODE_TEXT, state);
            } else {
                if alt_hover {
                    self.push_rect(value_rect, PARAM_VALUE_ALT_HOVER);
                }
                if soft_hover && !alt_hover && !editing {
                    self.push_rect(value_rect, PARAM_VALUE_SOFT_HOVER);
                }
                let active_edit = state
                    .param_edit
                    .as_ref()
                    .filter(|edit| edit.node_id == node.id() && edit.param_index == index);
                let value_text = active_edit
                    .map(|edit| edit.buffer.as_str())
                    .unwrap_or(row.value_text);
                self.push_value_editor_text(
                    value_rect,
                    value_text,
                    active_edit,
                    bound_color,
                    state,
                );
                if row.dropdown {
                    let arrow_y = value_rect.y + value_rect.h / 2;
                    let arrow_x = value_rect.x + value_rect.w - 8;
                    self.push_line(arrow_x - 3, arrow_y - 1, arrow_x, arrow_y + 2, bound_color);
                    self.push_line(arrow_x, arrow_y + 2, arrow_x + 3, arrow_y - 1, bound_color);
                }
            }
            self.push_border(
                value_rect,
                if row.action_button {
                    if soft_hover {
                        PARAM_VALUE_ACTIVE
                    } else {
                        PARAM_VALUE_BORDER
                    }
                } else if editing || alt_hover {
                    PARAM_VALUE_ACTIVE
                } else if soft_hover {
                    PARAM_VALUE_SOFT_BORDER
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
        let search_text = if state.menu.query.is_empty() {
            if state.menu.is_category_picker() {
                "Search categories..."
            } else {
                "Search nodes..."
            }
        } else {
            state.menu.query.as_str()
        };
        self.push_rect(search_rect, MENU_SEARCH_BG);
        self.push_border(search_rect, MENU_BORDER);
        self.push_text(search_rect.x + 6, search_rect.y + 7, search_text, MENU_TEXT);
        let entry_count = state.menu.visible_entry_count();
        if entry_count == 0 {
            self.push_text(
                rect.x + MENU_INNER_PADDING + 6,
                search_rect.y + search_rect.h + MENU_BLOCK_GAP + 6,
                "No matching nodes",
                MENU_CATEGORY_TEXT,
            );
            return;
        }
        let mut menu_label_scratch = std::mem::take(&mut self.label_scratch);
        for entry_index in 0..entry_count {
            let Some(entry) = state.menu.visible_entry(entry_index) else {
                continue;
            };
            let Some(item) = state.menu.entry_rect(entry_index) else {
                continue;
            };
            if (state.menu.selected == entry_index || state.hover_menu_item == Some(entry_index))
                && !matches!(entry, AddNodeMenuEntry::Category(_))
            {
                self.push_rect(item, MENU_SELECTED);
            }
            let (text, color) = match entry {
                AddNodeMenuEntry::Category(category) => {
                    let chip = category_chip_rect(item);
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

    fn push_main_menu(&mut self, state: &PreviewState) {
        if !state.main_menu.open {
            return;
        }
        let rect = state.main_menu.rect();
        self.push_rect(rect, MENU_BG);
        self.push_border(rect, MENU_BORDER);
        self.push_text(
            rect.x + MENU_INNER_PADDING + 6,
            rect.y + 6,
            "Main Menu",
            MENU_TEXT,
        );
        for (entry_index, item) in state.main_menu.items().iter().copied().enumerate() {
            let Some(row) = state.main_menu.entry_rect(entry_index) else {
                continue;
            };
            if state.main_menu.selected == entry_index
                || state.hover_main_menu_item == Some(entry_index)
            {
                self.push_rect(row, MENU_SELECTED);
            }
            let label = if item == MainMenuItem::Export && state.export_menu.open {
                "Export >"
            } else {
                item.label()
            };
            self.push_text(row.x + 6, row.y + 6, label, MENU_TEXT);
        }
    }

    fn push_export_menu(&mut self, state: &PreviewState) {
        if !state.export_menu.open {
            return;
        }
        let rect = state.export_menu.rect();
        self.push_rect(rect, MENU_BG);
        self.push_border(rect, MENU_BORDER);
        self.push_text(
            rect.x + MENU_INNER_PADDING + 6,
            rect.y + 6,
            "Export H.264",
            MENU_TEXT,
        );
        let close_rect = state.export_menu.close_button_rect();
        if state.hover_export_menu_close {
            self.push_rect(close_rect, MENU_SELECTED);
        }
        self.push_border(close_rect, MENU_BORDER);
        self.push_line(
            close_rect.x + 3,
            close_rect.y + 3,
            close_rect.x + close_rect.w - 4,
            close_rect.y + close_rect.h - 4,
            MENU_TEXT,
        );
        self.push_line(
            close_rect.x + close_rect.w - 4,
            close_rect.y + 3,
            close_rect.x + 3,
            close_rect.y + close_rect.h - 4,
            MENU_TEXT,
        );
        let mut menu_label_scratch = std::mem::take(&mut self.label_scratch);
        for (entry_index, item) in state.export_menu.items().iter().copied().enumerate() {
            let Some(row) = state.export_menu.entry_rect(entry_index) else {
                continue;
            };
            if state.export_menu.selected == entry_index
                || state.hover_export_menu_item == Some(entry_index)
            {
                self.push_rect(row, MENU_SELECTED);
            }
            menu_label_scratch.clear();
            match item {
                ExportMenuItem::Directory => {
                    menu_label_scratch.push_str("Directory: ");
                    menu_label_scratch.push_str(state.export_menu.directory.as_str());
                }
                ExportMenuItem::FileName => {
                    menu_label_scratch.push_str("File Name: ");
                    menu_label_scratch.push_str(state.export_menu.file_name.as_str());
                }
                ExportMenuItem::BeatsPerBar => {
                    menu_label_scratch.push_str("Beats / Bar: ");
                    menu_label_scratch.push_str(state.export_menu.beats_per_bar.as_str());
                }
                ExportMenuItem::Codec => {
                    menu_label_scratch.push_str("Video: H.264 (OpenH264)");
                }
                ExportMenuItem::StartStop => {
                    if state.export_menu.exporting {
                        menu_label_scratch.push_str("Stop Export");
                    } else {
                        menu_label_scratch.push_str("Start Export");
                    }
                }
                ExportMenuItem::Preview => {
                    let _ = write!(
                        &mut menu_label_scratch,
                        "Preview: {}/{} frames",
                        state.export_menu.preview_frame, state.export_menu.preview_total
                    );
                }
            }
            self.push_text(row.x + 6, row.y + 6, menu_label_scratch.as_str(), MENU_TEXT);
        }
        let preview = state.export_menu.preview_viewport_rect();
        let preview_label_y = (preview.y - 14).max(rect.y + 8);
        self.push_text(
            preview.x,
            preview_label_y,
            "Export Preview",
            MENU_CATEGORY_TEXT,
        );
        self.push_rect(preview, PARAM_VALUE_BG);
        self.push_border(preview, MENU_BORDER);
        if !state.export_menu.status.is_empty() {
            self.push_text(
                rect.x + MENU_INNER_PADDING + 6,
                rect.y + rect.h - 16,
                state.export_menu.status.as_str(),
                MENU_CATEGORY_TEXT,
            );
        }
        self.label_scratch = menu_label_scratch;
    }

    fn push_help_modal(&mut self, state: &PreviewState, panel_width: usize, panel_height: usize) {
        let Some(help) = state.help_modal.as_ref() else {
            return;
        };
        let editor_h = editor_panel_height(panel_height) as i32;
        if panel_width == 0 || editor_h <= 0 {
            return;
        }
        let panel_rect = Rect::new(0, 0, panel_width as i32, editor_h);
        self.push_rect(panel_rect, HELP_BACKDROP);

        let max_modal_w = (panel_width as i32 - 32).max(280);
        let modal_w = max_modal_w.clamp(280, 560);
        let title_h = 18;
        let line_h = 14;
        let footer_h = 16;
        let pad = 10;
        let min_modal_h = 112;
        let desired_h = min_modal_h + (help.lines.len() as i32 * line_h);
        let max_modal_h = (editor_h - 28).max(min_modal_h);
        let modal_h = desired_h.min(max_modal_h);
        let modal_x = ((panel_width as i32 - modal_w) / 2).max(8);
        let modal_y = ((editor_h - modal_h) / 2).max(8);
        let modal = Rect::new(modal_x, modal_y, modal_w, modal_h);
        self.push_rect(modal, HELP_PANEL_BG);
        self.push_border(modal, MENU_BORDER);

        self.push_text(
            modal.x + pad,
            modal.y + pad,
            help.title.as_str(),
            HELP_TITLE,
        );
        let hint = "F1/click to close";
        self.push_text(
            modal.x + modal.w - self.text_renderer.measure_text_width(hint, 1.0) - pad,
            modal.y + pad,
            hint,
            HELP_HINT,
        );

        let body_y = modal.y + pad + title_h;
        let body_h = modal.h - title_h - footer_h - (pad * 2);
        let visible_lines = (body_h / line_h).max(0) as usize;
        let mut y = body_y;
        for line in help.lines.iter().take(visible_lines) {
            self.push_text(modal.x + pad, y, line.as_str(), HELP_TEXT);
            y += line_h;
        }
        if help.lines.len() > visible_lines && visible_lines > 0 {
            self.push_text(
                modal.x + pad,
                modal.y + modal.h - pad - footer_h,
                "...",
                HELP_HINT,
            );
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
            .push(ColoredRect {
                rect,
                color,
                space: self.active_space,
            });
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
                space: self.active_space,
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
        if let Some((cx, cy)) = collapsed_param_entry_pin_center(node) {
            let (cx, cy) = graph_point_to_panel(cx, cy, state);
            let color = if state
                .hover_param_target
                .map(|target| target.node_id == node.id())
                .unwrap_or(false)
            {
                PIN_HOVER
            } else {
                PARAM_EDGE_COLOR
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
                    } else if let Some((pin_x, pin_y)) =
                        collapsed_param_entry_pin_center(target_node)
                    {
                        graph_point_to_panel(pin_x, pin_y, state)
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
        let obstacle_epoch =
            param_route_obstacle_epoch(project, state, self.wire_routes.param_cache_epoch);
        if self.wire_routes.param_cache_epoch != Some(obstacle_epoch) {
            self.wire_routes.param_cache_epoch = Some(obstacle_epoch);
            self.wire_routes.param_cache.clear();
            let obstacles = collect_graph_node_obstacles(project);
            self.wire_routes.param_obstacle_map =
                wire_route::RouteObstacleMap::from_obstacles(&obstacles);
        }
        let active_epoch = self.wire_routes.param_cache_epoch.unwrap_or(obstacle_epoch);
        let mut live_route_keys =
            std::mem::take(&mut self.wire_routes.param_live_route_keys_scratch);
        let mut drawn_segments = std::mem::take(&mut self.param_drawn_segments_scratch);
        let mut drawn_segment_hash = std::mem::take(&mut self.param_drawn_segment_hash_scratch);
        live_route_keys.clear();
        drawn_segments.clear();
        drawn_segment_hash.clear();
        let mut param_occupied_edges = wire_route::RouteOccupiedEdges::default();
        let mut tail_slots = std::mem::take(&mut self.wire_routes.param_tail_slots_scratch);
        tail_slots.clear();
        let mut route_panel = std::mem::take(&mut self.wire_routes.param_route_panel_scratch);
        route_panel.clear();
        for target in project.nodes() {
            for param_index in 0..target.param_count() {
                let Some((source_id, _resource_kind)) =
                    project.param_link_source_for_param(target.id(), param_index)
                else {
                    continue;
                };
                let Some(source) = project.node(source_id) else {
                    continue;
                };
                let Some((from_x, from_y)) = output_pin_center(source) else {
                    continue;
                };
                let (to_x_graph, to_y_graph) =
                    if let Some(row) = node_param_row_rect(target, param_index) {
                        (row.x + row.w - 4, row.y + row.h / 2)
                    } else if let Some((pin_x, pin_y)) = collapsed_param_entry_pin_center(target) {
                        (pin_x, pin_y)
                    } else {
                        continue;
                    };
                let (to_x, to_y) = graph_point_to_panel(to_x_graph, to_y_graph, state);
                let start_endpoint = wire_route::RouteEndpoint {
                    point: (from_x, from_y),
                    corridor_dir: wire_route::RouteDirection::East,
                };
                let end_endpoint = wire_route::RouteEndpoint {
                    point: (to_x_graph, to_y_graph),
                    corridor_dir: wire_route::RouteDirection::East,
                };
                let start_tail_cells = next_staggered_tail_cells(&mut tail_slots, start_endpoint);
                let end_tail_cells = next_staggered_tail_cells(&mut tail_slots, end_endpoint);
                let route_key = ParamRouteCacheKey {
                    source_id,
                    target_id: target.id(),
                    param_index,
                    obstacle_epoch: active_epoch,
                    start_tail_cells,
                    end_tail_cells,
                };
                live_route_keys.insert(route_key);
                if !self.wire_routes.param_cache.contains_key(&route_key) {
                    let route =
                        wire_route::route_wire_path_with_tail_cells_avoiding_overlaps_with_dual_map(
                            start_endpoint,
                            end_endpoint,
                            &self.wire_routes.param_obstacle_map,
                            &self.wire_routes.edge_occupied,
                            &param_occupied_edges,
                            start_tail_cells,
                            end_tail_cells,
                        );
                    self.wire_routes
                        .param_cache
                        .insert(route_key, Arc::from(route));
                }
                let Some(route) = self.wire_routes.param_cache.get(&route_key).cloned() else {
                    continue;
                };
                map_graph_path_to_panel_into(route.as_ref(), state, &mut route_panel);
                let color = if path_intersects_cut_line(state, route_panel.as_slice()) {
                    CUT_EDGE_COLOR
                } else {
                    PARAM_EDGE_COLOR
                };
                self.push_path_lines_with_bridges(
                    route_panel.as_slice(),
                    color,
                    &mut drawn_segments,
                    &mut drawn_segment_hash,
                    state.zoom,
                );
                param_occupied_edges.record_path_non_tail(route.as_ref());
                self.push_param_target_marker(to_x, to_y, color);
            }
        }
        self.wire_routes
            .param_cache
            .retain(|key, _| key.obstacle_epoch == active_epoch && live_route_keys.contains(key));
        route_panel.clear();
        tail_slots.clear();
        drawn_segments.clear();
        live_route_keys.clear();
        self.wire_routes.param_route_panel_scratch = route_panel;
        self.wire_routes.param_tail_slots_scratch = tail_slots;
        self.param_drawn_segments_scratch = drawn_segments;
        self.param_drawn_segment_hash_scratch = drawn_segment_hash;
        self.wire_routes.param_live_route_keys_scratch = live_route_keys;
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

    fn push_path_lines_with_bridges(
        &mut self,
        points: &[(i32, i32)],
        color: Color,
        drawn_segments: &mut Vec<DrawnWireSegment>,
        drawn_segment_hash: &mut BridgeSegmentSpatialHash,
        zoom: f32,
    ) {
        if points.len() < 2 {
            return;
        }
        let bridge_scale = wire_layout_scale(zoom);
        let mut new_segments = std::mem::take(&mut self.bridge_new_segments_scratch);
        let mut candidate_indices = std::mem::take(&mut self.bridge_candidate_indices_scratch);
        let mut crossings = std::mem::take(&mut self.bridge_crossings_scratch);
        let mut bridge_clusters = std::mem::take(&mut self.bridge_clusters_scratch);
        let mut bridge_points = std::mem::take(&mut self.bridge_points_scratch);
        new_segments.clear();
        candidate_indices.clear();
        crossings.clear();
        bridge_clusters.clear();
        bridge_points.clear();
        let total_segments = points.len().saturating_sub(1);
        let mut bridge_intersection_tests = 0u64;
        for (segment_index, pair) in points.windows(2).enumerate() {
            let segment = DrawnWireSegment {
                from: pair[0],
                to: pair[1],
            };
            if segment.from == segment.to {
                continue;
            }
            let dx = (segment.to.0 - segment.from.0) as f32;
            let dy = (segment.to.1 - segment.from.1) as f32;
            let segment_len = (dx * dx + dy * dy).sqrt();
            if segment_len <= 0.0 {
                continue;
            }
            crossings.clear();
            drawn_segment_hash.collect_candidates(segment, &mut candidate_indices);
            bridge_intersection_tests =
                bridge_intersection_tests.saturating_add(candidate_indices.len() as u64);
            segment_crossings(
                segment,
                drawn_segments,
                candidate_indices.as_slice(),
                total_segments,
                segment_index,
                bridge_scale,
                &mut crossings,
            );
            crossings.sort_by(|a, b| a.total_cmp(b));
            bridge_clusters.clear();
            cluster_bridge_ranges_into(
                crossings.as_slice(),
                segment_len,
                bridge_scale,
                &mut bridge_clusters,
            );
            bridge_points.clear();
            bridged_segment_points_into(
                segment,
                bridge_clusters.as_slice(),
                bridge_scale,
                &mut bridge_points,
            );
            self.push_path_lines(bridge_points.as_slice(), color);
            new_segments.push(segment);
        }
        for segment in new_segments.iter().copied() {
            let index = drawn_segments.len();
            drawn_segments.push(segment);
            drawn_segment_hash.insert_segment(segment, index);
        }
        new_segments.clear();
        candidate_indices.clear();
        crossings.clear();
        bridge_clusters.clear();
        bridge_points.clear();
        self.bridge_new_segments_scratch = new_segments;
        self.bridge_candidate_indices_scratch = candidate_indices;
        self.bridge_crossings_scratch = crossings;
        self.bridge_clusters_scratch = bridge_clusters;
        self.bridge_points_scratch = bridge_points;
        self.frame.bridge_intersection_tests = self
            .frame
            .bridge_intersection_tests
            .saturating_add(bridge_intersection_tests);
    }

    fn push_signal_wire_right_exit(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        let exit_x = x0.saturating_add(PARAM_WIRE_EXIT_TAIL_PX);
        let route = [(exit_x, y0), (exit_x, y1)];
        let smooth = smooth_param_wire_path_with_end_caps((x0, y0), route.as_slice(), (x1, y1));
        self.push_path_lines(smooth.as_slice(), color);
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
        let route = [(exit_x, y0), (entry_x, y0), (entry_x, y1)];
        let smooth = smooth_param_wire_path_with_end_caps((x0, y0), route.as_slice(), (x1, y1));
        self.push_path_lines(smooth.as_slice(), color);
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
        let start = out.len();
        self.text_renderer.push_text(out, x, y, text, color);
        for rect in &mut out[start..] {
            rect.space = self.active_space;
        }
    }

    fn push_graph_text(&mut self, x: i32, y: i32, text: &str, color: Color, state: &PreviewState) {
        if state.zoom < GRAPH_TEXT_HIDE_ZOOM {
            return;
        }
        let out = &mut active_scene_layer_mut(&mut self.frame, self.active_layer).rects;
        let start = out.len();
        self.text_renderer
            .push_text_scaled(out, x, y, text, color, state.zoom);
        for rect in &mut out[start..] {
            rect.space = self.active_space;
        }
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
        let start = out.len();
        self.text_renderer
            .push_text_scaled(out, x, y, text, color, state.zoom);
        for rect in &mut out[start..] {
            rect.space = self.active_space;
        }
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
        let start = out.len();
        self.text_renderer
            .push_text_scaled(out, text_x, text_y, text, color, state.zoom);
        for rect in &mut out[start..] {
            rect.space = self.active_space;
        }
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
        &mut self,
        text: &'a str,
        max_width: i32,
        state: &PreviewState,
        out: &'a mut String,
    ) -> &'a str {
        if max_width <= 0 || text.is_empty() {
            return "";
        }
        if let Some(cached) = self.lookup_fitted_label(text, max_width, state.zoom) {
            if cached == text {
                return text;
            }
            out.clear();
            out.push_str(cached);
            return out.as_str();
        }
        let scale = state.zoom;
        let full_w = self.text_renderer.measure_text_width(text, scale);
        if full_w <= max_width {
            self.store_fitted_label(text, max_width, scale, text);
            return text;
        }
        let ellipsis = "...";
        let ellipsis_w = self.text_renderer.measure_text_width(ellipsis, scale);
        if ellipsis_w > max_width {
            self.store_fitted_label(text, max_width, scale, "");
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
        self.store_fitted_label(text, max_width, scale, out.as_str());
        out.as_str()
    }

    /// Return cached fitted label text for one width/zoom bucket.
    fn lookup_fitted_label(&self, text: &str, max_width: i32, zoom: f32) -> Option<&str> {
        let bucket = self.fitted_label_cache.get(&FittedLabelCacheBucketKey {
            max_width,
            zoom_bits: zoom.to_bits(),
        })?;
        bucket.get(text).map(String::as_str)
    }

    /// Store one fitted label result in a bounded cache.
    fn store_fitted_label(&mut self, text: &str, max_width: i32, zoom: f32, fitted: &str) {
        let key = FittedLabelCacheBucketKey {
            max_width,
            zoom_bits: zoom.to_bits(),
        };
        if !self.fitted_label_cache.contains_key(&key)
            && self.fitted_label_cache.len() >= FITTED_LABEL_CACHE_MAX_BUCKETS
        {
            self.fitted_label_cache.clear();
        }
        let bucket = self.fitted_label_cache.entry(key).or_default();
        if !bucket.contains_key(text) && bucket.len() >= FITTED_LABEL_CACHE_MAX_ENTRIES_PER_BUCKET {
            bucket.clear();
        }
        bucket.insert(text.to_owned(), fitted.to_owned());
    }

    fn layer_capacity(&self, layer: ActiveLayer) -> (usize, usize) {
        let data = match layer {
            ActiveLayer::StaticPanel => &self.frame.static_panel,
            ActiveLayer::Edges => &self.frame.edges,
            ActiveLayer::Nodes => &self.frame.nodes,
            ActiveLayer::ParamWires => &self.frame.param_wires,
            ActiveLayer::Overlays => &self.frame.overlays,
            ActiveLayer::Timeline => &self.frame.timeline,
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

fn collect_graph_node_obstacles(project: &GuiProject) -> Vec<wire_route::NodeObstacle> {
    let mut out = Vec::new();
    for node in project.nodes() {
        out.push(wire_route::NodeObstacle {
            rect: Rect::new(node.x(), node.y(), NODE_WIDTH, node.card_height()),
        });
    }
    out
}

/// Return obstacle epoch used to invalidate cached parameter-wire routes.
///
/// The obstacle field is defined by node layout in graph space, not wire hover
/// or transient overlay states. While a node drag is active we intentionally
/// freeze this epoch so expensive route recomputation happens once on drop.
fn param_route_obstacle_epoch(
    project: &GuiProject,
    state: &PreviewState,
    cached_epoch: Option<u64>,
) -> u64 {
    let layout_epoch = project.invalidation().nodes;
    if state.drag.is_some() {
        return cached_epoch.unwrap_or(layout_epoch);
    }
    layout_epoch
}

/// Return obstacle epoch used to invalidate cached primary-edge routes.
///
/// Primary routes depend on node obstacle layout only; pan/zoom remapping is
/// applied after graph-space routing and does not invalidate this cache.
fn edge_route_obstacle_epoch(project: &GuiProject) -> u64 {
    project.invalidation().nodes
}

fn wire_drag_source_kind(
    project: &GuiProject,
    wire: super::state::WireDragState,
) -> Option<ResourceKind> {
    let source = project.node(wire.source_node_id)?;
    source.kind().output_resource_kind()
}

#[cfg(test)]
mod tests {
    use super::wires::{
        build_smoothed_param_wire, smooth_param_wire_path, PARAM_WIRE_ENDPOINT_STRAIGHT_PX,
    };
    use super::{
        signal_scope_range, signal_scope_y, timeline_beat_indicator_on, Rect, SceneBuilder,
        SIGNAL_SCOPE_MAX_SAMPLES,
    };
    use crate::gui::project::{
        collapsed_param_entry_pin_center, output_pin_center, GuiProject, ProjectNodeKind,
        NODE_GRID_PITCH,
    };
    use crate::gui::scene::wire_route::{
        self, RouteDirection, RouteEndpoint, RouteObstacleMap, DEFAULT_ENDPOINT_TAIL_CELLS,
    };
    use crate::gui::state::{DragState, PreviewState};
    use crate::runtime_config::V2Config;
    use std::collections::HashSet;

    #[test]
    fn param_wire_smoothing_does_not_backtrack_near_short_corner_segments() {
        let points = [(0, 0), (4, 0), (4, 2), (10, 2)];
        let smooth = smooth_param_wire_path(&points);
        assert_eq!(smooth.first().copied(), Some((0, 0)));
        assert_eq!(smooth.last().copied(), Some((10, 2)));
        for segment in smooth.windows(2) {
            assert!(
                segment[0].0 == segment[1].0 || segment[0].1 == segment[1].1,
                "segment is not axis aligned: {:?}",
                segment
            );
        }
    }

    #[test]
    fn param_wire_smoothing_skips_rounding_for_straight_path_points() {
        let points = [(0, 0), (8, 0), (16, 0)];
        let smooth = smooth_param_wire_path(&points);
        assert_eq!(smooth, points);
    }

    #[test]
    fn param_wire_smoothing_preserves_straight_pin_tails() {
        let route = [(30, 0), (30, 32), (70, 32)];
        let start = (0, 0);
        let end = (40, 40);
        let smooth = build_smoothed_param_wire(start, route.as_slice(), end);
        assert_eq!(smooth.first().copied(), Some(start));
        assert_eq!(smooth.last().copied(), Some(end));

        let mut start_tail_max_x = start.0;
        for point in smooth.iter().copied() {
            if point.1 != start.1 {
                break;
            }
            start_tail_max_x = start_tail_max_x.max(point.0);
        }
        assert!(
            start_tail_max_x - start.0 >= PARAM_WIRE_ENDPOINT_STRAIGHT_PX,
            "start tail too short: {}",
            start_tail_max_x - start.0
        );

        let mut end_tail_max_x = end.0;
        for point in smooth.iter().rev().copied() {
            if point.1 != end.1 {
                break;
            }
            end_tail_max_x = end_tail_max_x.max(point.0);
        }
        assert!(
            end_tail_max_x - end.0 >= PARAM_WIRE_ENDPOINT_STRAIGHT_PX,
            "end tail too short: {}",
            end_tail_max_x - end.0
        );
    }

    #[test]
    fn signal_scope_range_always_includes_zero_and_one_guides() {
        let (min_v, max_v) = signal_scope_range(&[-0.4, 1.8, 0.3]);
        assert!(min_v <= 0.0);
        assert!(max_v >= 1.0);
    }

    #[test]
    fn signal_scope_y_maps_bounds_to_inner_extents() {
        let inner = Rect::new(10, 20, 100, 40);
        let top = signal_scope_y(2.0, -1.0, 2.0, inner);
        let bottom = signal_scope_y(-1.0, -1.0, 2.0, inner);
        assert_eq!(top, inner.y);
        assert_eq!(bottom, inner.y + inner.h - 1);
    }

    #[test]
    fn signal_scope_sampling_reuses_cached_samples_between_ticks() {
        let mut project = GuiProject::new_empty(640, 480);
        let _lfo = project.add_node(ProjectNodeKind::CtlLfo, 80, 80, 640, 480);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.invalidation.invalidate_nodes();
        let mut scene = SceneBuilder::default();

        let frame = scene.build(&project, &state, 640, 480, 640, 60);
        let initial_samples = frame.signal_scope_samples;
        assert!(
            initial_samples > 0,
            "initial scope build should evaluate sample points"
        );

        state.frame_index = 1;
        state.invalidation.invalidate_nodes();
        let frame = scene.build(&project, &state, 640, 480, 640, 60);
        assert!(
            frame.signal_scope_samples < initial_samples,
            "incremental update should evaluate fewer samples than full rebuild"
        );
    }

    #[test]
    fn signal_scope_shift_handles_missing_cache_entry_without_panic() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 80, 80, 640, 480);
        let mut scene = SceneBuilder::default();
        let window_secs = 1.0;
        let sample_count: usize = 16;
        let step_secs = window_secs / sample_count.saturating_sub(1) as f32;
        let _ = scene.sample_signal_scope_values(&project, lfo, 1.0, window_secs, sample_count, 0);
        scene.signal_scope_cache.remove(&lfo);

        let shifted =
            scene.try_shift_signal_scope_values(&project, lfo, sample_count, step_secs, 1);
        assert!(
            !shifted,
            "missing cache entry should return false instead of panicking"
        );
    }

    #[test]
    fn signal_scope_sampling_caps_per_node_sample_count() {
        let mut project = GuiProject::new_empty(640, 480);
        let _lfo = project.add_node(ProjectNodeKind::CtlLfo, 80, 80, 640, 480);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.zoom = 4.0;
        state.invalidation.invalidate_nodes();
        let mut scene = SceneBuilder::default();

        let frame = scene.build(&project, &state, 640, 480, 640, 60);
        assert!(
            frame.signal_scope_samples <= SIGNAL_SCOPE_MAX_SAMPLES as u64,
            "scope samples should be capped to avoid high zoom recompute spikes"
        );
    }

    #[test]
    fn timeline_beat_indicator_blinks_on_each_beat() {
        assert!(timeline_beat_indicator_on(0, 60, 120.0));
        assert!(timeline_beat_indicator_on(5, 60, 120.0));
        assert!(!timeline_beat_indicator_on(6, 60, 120.0));
        assert!(timeline_beat_indicator_on(30, 60, 120.0));
    }

    #[test]
    fn timeline_beat_indicator_disables_on_invalid_timing() {
        assert!(!timeline_beat_indicator_on(10, 0, 120.0));
        assert!(!timeline_beat_indicator_on(10, 60, 0.0));
        assert!(!timeline_beat_indicator_on(10, 60, f32::NAN));
    }

    #[test]
    fn edge_layer_freezes_during_node_drag_and_refreshes_on_release() {
        let mut project = GuiProject::new_empty(640, 480);
        let source = project.add_node(ProjectNodeKind::TexSolid, 40, 40, 640, 480);
        let target = project.add_node(ProjectNodeKind::IoWindowOut, 280, 40, 640, 480);
        assert!(project.connect_image_link(source, target));

        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        let mut scene = SceneBuilder::default();
        let frame = scene.build(&project, &state, 640, 480, 640, 60);
        assert!(frame.dirty.edges, "initial build should populate edges");
        let frozen_lines_before = frame.edges.lines.len();

        assert!(project.move_node(source, 140, 40, 640, 480));
        state.invalidation.invalidate_wires();
        state.drag = Some(DragState {
            node_id: source,
            offset_x: 0,
            offset_y: 0,
            origin_x: 40,
            origin_y: 40,
        });

        let frame = scene.build(&project, &state, 640, 480, 640, 60);
        assert!(
            !frame.dirty.edges,
            "edges should remain frozen while node drag is active"
        );
        assert_eq!(frame.edges.lines.len(), frozen_lines_before);

        state.drag = None;
        let frame = scene.build(&project, &state, 640, 480, 640, 60);
        assert!(
            frame.dirty.edges,
            "edges should rebuild once drag is released"
        );
    }

    #[test]
    fn param_routes_freeze_during_node_drag_and_refresh_on_release() {
        let mut project = GuiProject::new_empty(640, 480);
        let source = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 640, 480);
        let target = project.add_node(ProjectNodeKind::TexCircle, 280, 40, 640, 480);
        assert!(project.connect_signal_link_to_param(source, target, 0));

        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        let mut scene = SceneBuilder::default();

        state.invalidation.invalidate_overlays();
        let _ = scene.build(&project, &state, 640, 480, 640, 60);
        assert_eq!(scene.wire_routes.param_cache.len(), 1);
        let initial_epoch = scene
            .wire_routes
            .param_cache_epoch
            .expect("param route epoch should be initialized");
        let initial_route = scene
            .wire_routes
            .param_cache
            .values()
            .next()
            .expect("route should exist")
            .to_vec();

        assert!(project.move_node(source, 140, 40, 640, 480));
        state.invalidation.invalidate_overlays();
        state.invalidation.invalidate_wires();
        state.drag = Some(DragState {
            node_id: source,
            offset_x: 0,
            offset_y: 0,
            origin_x: 40,
            origin_y: 40,
        });

        let _ = scene.build(&project, &state, 640, 480, 640, 60);
        assert_eq!(scene.wire_routes.param_cache_epoch, Some(initial_epoch));
        let drag_route = scene
            .wire_routes
            .param_cache
            .values()
            .next()
            .expect("route should stay cached during drag")
            .to_vec();
        assert_eq!(drag_route, initial_route);

        state.drag = None;
        state.invalidation.invalidate_overlays();
        let _ = scene.build(&project, &state, 640, 480, 640, 60);
        let release_epoch = scene
            .wire_routes
            .param_cache_epoch
            .expect("param route epoch should remain initialized");
        assert_ne!(release_epoch, initial_epoch);
        let release_route = scene
            .wire_routes
            .param_cache
            .values()
            .next()
            .expect("route should be recomputed on release")
            .to_vec();
        assert_ne!(release_route, initial_route);
    }

    #[test]
    fn param_routes_refresh_on_drag_release_without_overlay_epoch_bump() {
        let mut project = GuiProject::new_empty(640, 480);
        let source = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 640, 480);
        let target = project.add_node(ProjectNodeKind::TexCircle, 280, 40, 640, 480);
        assert!(project.connect_signal_link_to_param(source, target, 0));

        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        let mut scene = SceneBuilder::default();

        state.invalidation.invalidate_overlays();
        let _ = scene.build(&project, &state, 640, 480, 640, 60);
        let initial_route = scene
            .wire_routes
            .param_cache
            .values()
            .next()
            .expect("route should exist")
            .to_vec();

        assert!(project.move_node(source, 140, 40, 640, 480));
        state.invalidation.invalidate_overlays();
        state.drag = Some(DragState {
            node_id: source,
            offset_x: 0,
            offset_y: 0,
            origin_x: 40,
            origin_y: 40,
        });
        let _ = scene.build(&project, &state, 640, 480, 640, 60);
        let drag_route = scene
            .wire_routes
            .param_cache
            .values()
            .next()
            .expect("route should stay cached during drag")
            .to_vec();
        assert_eq!(drag_route, initial_route);

        // Drop/release without explicitly bumping overlay invalidation.
        state.drag = None;
        let frame = scene.build(&project, &state, 640, 480, 640, 60);
        assert!(
            frame.dirty.param_wires,
            "drop should force one parameter-wire refresh so parameter routes update"
        );
        let release_route = scene
            .wire_routes
            .param_cache
            .values()
            .next()
            .expect("route should be recomputed on release")
            .to_vec();
        assert_ne!(release_route, initial_route);
    }

    #[test]
    fn edge_routes_reuse_cache_across_pan_rebuilds() {
        let mut project = GuiProject::new_empty(640, 480);
        let source = project.add_node(ProjectNodeKind::TexSolid, 40, 40, 640, 480);
        let target = project.add_node(ProjectNodeKind::IoWindowOut, 280, 40, 640, 480);
        assert!(project.connect_image_link(source, target));

        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        let mut scene = SceneBuilder::default();
        let frame = scene.build(&project, &state, 640, 480, 640, 60);
        assert!(frame.dirty.edges, "initial build should populate edges");
        assert_eq!(scene.wire_routes.edge_cache.len(), 1);
        let initial_epoch = scene
            .wire_routes
            .edge_cache_epoch
            .expect("edge route epoch should be initialized");
        let initial_route = scene
            .wire_routes
            .edge_cache
            .values()
            .next()
            .expect("cached edge route should exist")
            .to_vec();

        state.pan_x += 40.0;
        state.pan_y += 16.0;
        state.invalidation.invalidate_wires();
        let frame = scene.build(&project, &state, 640, 480, 640, 60);
        assert!(
            frame.dirty.edges,
            "pan should rebuild panel-space edge layer"
        );
        assert_eq!(scene.wire_routes.edge_cache_epoch, Some(initial_epoch));
        let pan_route = scene
            .wire_routes
            .edge_cache
            .values()
            .next()
            .expect("cached edge route should remain populated")
            .to_vec();
        assert_eq!(pan_route, initial_route);
    }

    fn normalize_segment(a: (i32, i32), b: (i32, i32)) -> ((i32, i32), (i32, i32)) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    fn collect_non_tail_segments(points: &[(i32, i32)]) -> HashSet<((i32, i32), (i32, i32))> {
        let mut out = HashSet::new();
        if points.len() < 2 {
            return out;
        }
        let last_segment = points.len().saturating_sub(2);
        for (segment_index, pair) in points.windows(2).enumerate() {
            let is_endpoint_segment = segment_index == 0 || segment_index == last_segment;
            let is_horizontal = pair[0].1 == pair[1].1;
            if is_endpoint_segment && is_horizontal {
                continue;
            }
            let dx = pair[1].0 - pair[0].0;
            let dy = pair[1].1 - pair[0].1;
            let steps = (dx.abs().max(dy.abs()) / NODE_GRID_PITCH).max(1);
            let step_x = dx.signum() * NODE_GRID_PITCH;
            let step_y = dy.signum() * NODE_GRID_PITCH;
            let mut current = pair[0];
            for _ in 0..steps {
                let next = (current.0 + step_x, current.1 + step_y);
                out.insert(normalize_segment(current, next));
                current = next;
            }
        }
        out
    }

    fn non_tail_overlap_count(a: &[(i32, i32)], b: &[(i32, i32)]) -> usize {
        let a_segments = collect_non_tail_segments(a);
        let b_segments = collect_non_tail_segments(b);
        a_segments.intersection(&b_segments).count()
    }

    #[test]
    fn param_routes_avoid_primary_route_diagonal_segments() {
        let mut project = GuiProject::new_empty(900, 700);
        let source = project.add_node(ProjectNodeKind::CtlLfo, 40, 300, 900, 700);
        let target = project.add_node(ProjectNodeKind::TexCircle, 320, 120, 900, 700);
        assert!(project.connect_signal_link_to_param(source, target, 0));

        let source_node = project.node(source).expect("source node should exist");
        let target_node = project.node(target).expect("target node should exist");
        let start = RouteEndpoint {
            point: output_pin_center(source_node).expect("source output pin"),
            corridor_dir: RouteDirection::East,
        };
        let end = RouteEndpoint {
            point: collapsed_param_entry_pin_center(target_node).expect("target param entry pin"),
            corridor_dir: RouteDirection::East,
        };
        let obstacles = super::collect_graph_node_obstacles(&project);
        let obstacle_map = RouteObstacleMap::from_obstacles(obstacles.as_slice());
        let no_occupied = wire_route::RouteOccupiedEdges::default();
        let baseline_route = wire_route::route_wire_path_with_tail_cells_avoiding_overlaps_with_map(
            start,
            end,
            &obstacle_map,
            &no_occupied,
            DEFAULT_ENDPOINT_TAIL_CELLS,
            DEFAULT_ENDPOINT_TAIL_CELLS,
        );
        assert!(
            !collect_non_tail_segments(baseline_route.as_slice()).is_empty(),
            "baseline route should include non-tail segments"
        );

        let state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        let mut scene = SceneBuilder::default();
        scene
            .wire_routes
            .edge_occupied
            .record_path_non_tail(baseline_route.as_slice());
        scene.push_param_links(&project, &state);
        let rendered_param = scene
            .wire_routes
            .param_cache
            .values()
            .next()
            .expect("param route should be cached")
            .to_vec();
        assert_eq!(
            non_tail_overlap_count(baseline_route.as_slice(), rendered_param.as_slice()),
            0,
            "parameter route should avoid occupied non-tail segments"
        );
        assert_ne!(
            rendered_param, baseline_route,
            "shared occupancy should force an alternate parameter route"
        );
    }
}

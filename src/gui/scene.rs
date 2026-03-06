//! Retained-style scene assembly for the GPU GUI renderer.
//!
//! The builder partitions GUI geometry into retained layers and marks only
//! changed layers dirty each update (`static_panel`, `edges`, `nodes`,
//! `signal_scopes`, `param_wires`, `overlays`). Rendering stays on GPU and
//! unchanged layers are reused.

mod edge_layer;
mod layers;
mod layout;
mod menus;
mod node_params_layer;
mod overlays_layer;
mod param_links_layer;
mod route_context;
mod signal_scope;
mod style;
mod timeline_helpers;
mod timeline_layer;
pub(super) mod wire_route;
mod wires;

use std::{collections::HashMap, collections::HashSet, sync::Arc, time::Instant};

use super::geometry::Rect;
use super::project::{
    collapsed_param_entry_pin_center, input_pin_center, node_expand_toggle_rect,
    node_param_dropdown_rect, node_param_row_rect, node_param_value_rect, output_pin_center,
    pin_rect, GuiProject, ProjectNode, ResourceKind, SignalEvalPath, SignalEvalStack,
    SignalSampleMemo,
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
use route_context::{
    collect_graph_node_obstacles, edge_route_obstacle_epoch, param_route_obstacle_epoch,
    wire_drag_source_kind,
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

const SCENE_TEXT_PREWARM_ASCII: &str =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 .,;:+-*/=%()[]{}<>!?@#\"'_|\\/";

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
    pub(crate) signal_scopes: SceneLayer,
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
    pub(crate) signal_scopes: bool,
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
            || self.signal_scopes
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

#[derive(Clone, Copy, Debug)]
struct SignalScopeLayout {
    scope: Rect,
    inner: Rect,
    window_secs: f32,
    sample_count: usize,
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
    cached_signal_scopes_epoch: Option<u64>,
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
    text_glyphs_prewarmed: bool,
    frame_alloc_bytes: u64,
    was_dragging: bool,
}

/// Frame-invariant dimensions and timing inputs for scene construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SceneBuildRequest {
    width: usize,
    height: usize,
    panel_width: usize,
    timeline_fps: u32,
}

impl SceneBuildRequest {
    /// Construct one scene-build request for a single redraw pass.
    pub(crate) const fn new(
        width: usize,
        height: usize,
        panel_width: usize,
        timeline_fps: u32,
    ) -> Self {
        Self {
            width,
            height,
            panel_width,
            timeline_fps,
        }
    }
}

impl SceneBuilder {
    /// Build one frame of editor scene geometry.
    pub(crate) fn build(
        &mut self,
        project: &GuiProject,
        state: &PreviewState,
        request: SceneBuildRequest,
    ) -> &SceneFrame {
        if !self.text_glyphs_prewarmed {
            self.text_renderer
                .prewarm_ascii_glyphs(SCENE_TEXT_PREWARM_ASCII, 1.0);
            self.text_glyphs_prewarmed = true;
        }
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

        self.rebuild_static_if_needed(request.width, request.height, request.panel_width);

        let nodes_epoch = state.invalidation.nodes;
        if self.cached_nodes_epoch != Some(nodes_epoch) {
            self.cached_nodes_epoch = Some(nodes_epoch);
            self.frame.dirty.nodes = true;
            let start = Instant::now();
            self.rebuild_nodes_layer(project, state);
            self.frame.nodes_ms = start.elapsed().as_secs_f64() * 1000.0;
        }

        let signal_scopes_epoch = state.invalidation.signal_scopes;
        if self.cached_signal_scopes_epoch != Some(signal_scopes_epoch) {
            self.cached_signal_scopes_epoch = Some(signal_scopes_epoch);
            self.frame.dirty.signal_scopes = true;
            self.rebuild_signal_scopes_layer(
                project,
                state,
                request.timeline_fps,
                project.invalidation().tex_eval,
            );
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
            self.rebuild_overlays_layer(project, state, request.panel_width, request.height);
            self.frame.overlays_ms += start.elapsed().as_secs_f64() * 1000.0;
        }

        let timeline_epoch = state.invalidation.timeline;
        if self.cached_timeline_epoch != Some(timeline_epoch) {
            self.cached_timeline_epoch = Some(timeline_epoch);
            self.frame.dirty.timeline = true;
            self.rebuild_timeline_layer(state, request.width, request.height, request.timeline_fps);
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

    fn rebuild_nodes_layer(&mut self, project: &GuiProject, state: &PreviewState) {
        let before = self.layer_capacity(ActiveLayer::Nodes);
        self.set_active_layer(ActiveLayer::Nodes);
        self.set_active_space(CoordSpace::Screen);
        self.clear_active_layer();
        self.push_header(project);
        self.set_active_space(CoordSpace::Graph);
        self.push_nodes(project, state);
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::Nodes));
    }

    fn rebuild_signal_scopes_layer(
        &mut self,
        project: &GuiProject,
        state: &PreviewState,
        timeline_fps: u32,
        tex_eval_epoch: u64,
    ) {
        let before = self.layer_capacity(ActiveLayer::SignalScopes);
        self.set_active_layer(ActiveLayer::SignalScopes);
        self.set_active_space(CoordSpace::Graph);
        self.clear_active_layer();
        self.live_signal_scope_nodes.clear();
        self.push_signal_scopes(project, state, timeline_fps, tex_eval_epoch);
        self.signal_scope_cache
            .retain(|node_id, _| self.live_signal_scope_nodes.contains(node_id));
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::SignalScopes));
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

    fn push_nodes(&mut self, project: &GuiProject, state: &PreviewState) {
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
                self.push_signal_scope_chrome(node, state);
            }
            if node.expanded() {
                self.push_node_params(node, state);
            }
            self.push_pins(node, state);
        }
        selected_nodes_lookup.clear();
        self.selected_nodes_lookup_scratch = selected_nodes_lookup;
    }

    fn push_signal_scopes(
        &mut self,
        project: &GuiProject,
        state: &PreviewState,
        timeline_fps: u32,
        tex_eval_epoch: u64,
    ) {
        for node in project.nodes() {
            self.push_signal_scope(project, node, state, timeline_fps, tex_eval_epoch);
        }
    }

    fn push_signal_scope_chrome(&mut self, node: &ProjectNode, state: &PreviewState) {
        let Some(layout) = Self::signal_scope_layout(node, state) else {
            return;
        };
        self.push_rect(layout.scope, NODE_SIGNAL_SCOPE_BG);
        self.push_border(layout.scope, NODE_SIGNAL_SCOPE_BORDER);
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
        let Some(layout) = Self::signal_scope_layout(node, state) else {
            return;
        };
        let time_now = state.frame_index as f32 / timeline_fps.max(1) as f32;
        let eval_start = Instant::now();
        let mut signal_scope_line_scratch = std::mem::take(&mut self.signal_scope_line_scratch);
        let (value_min, value_max) = {
            signal_scope_line_scratch.clear();
            let values = self.sample_signal_scope_values(
                project,
                node.id(),
                time_now,
                layout.window_secs,
                layout.sample_count,
                tex_eval_epoch,
            );
            let (value_min, value_max) = signal_scope_range(values);
            for step in 0..layout.sample_count.saturating_sub(1) {
                let t0 = step as f32 / layout.sample_count.saturating_sub(1).max(1) as f32;
                let t1 = (step + 1) as f32 / layout.sample_count.saturating_sub(1).max(1) as f32;
                let v0 = values[step];
                let v1 = values[step + 1];
                let x0 = layout.inner.x + ((layout.inner.w - 1) as f32 * t0).round() as i32;
                let x1 = layout.inner.x + ((layout.inner.w - 1) as f32 * t1).round() as i32;
                let y0 = signal_scope_y(v0, value_min, value_max, layout.inner);
                let y1 = signal_scope_y(v1, value_min, value_max, layout.inner);
                signal_scope_line_scratch.push((x0, y0, x1, y1));
            }
            (value_min, value_max)
        };
        let eval_ms = eval_start.elapsed().as_secs_f64() * 1000.0;
        let y_zero = signal_scope_y(0.0, value_min, value_max, layout.inner);
        let y_one = signal_scope_y(1.0, value_min, value_max, layout.inner);
        self.push_line(
            layout.inner.x,
            y_zero,
            layout.inner.x + layout.inner.w - 1,
            y_zero,
            NODE_SIGNAL_SCOPE_GUIDE_ZERO,
        );
        self.push_line(
            layout.inner.x,
            y_one,
            layout.inner.x + layout.inner.w - 1,
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

    fn signal_scope_layout(node: &ProjectNode, state: &PreviewState) -> Option<SignalScopeLayout> {
        if !node.kind().shows_signal_preview() {
            return None;
        }
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
            return None;
        }
        scope_h = scope_h.min(max_scope_h);
        let scope_y = (scope_bottom - scope_h).max(scope_top_min);
        let scope = Rect::new(
            rect.x + pad_x,
            scope_y,
            (rect.w - (pad_x * 2)).max(12),
            scope_h,
        );
        let inner = Rect::new(scope.x + 2, scope.y + 2, scope.w - 4, scope.h - 4);
        if inner.w < 8 || inner.h < 4 {
            return None;
        }
        Some(SignalScopeLayout {
            scope,
            inner,
            window_secs: if node.expanded() { 2.0 } else { 1.2 },
            sample_count: (inner.w.max(16) as usize).min(SIGNAL_SCOPE_MAX_SAMPLES),
        })
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
            ActiveLayer::SignalScopes => &self.frame.signal_scopes,
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

#[cfg(test)]
mod tests {
    use super::wires::{
        build_smoothed_param_wire, smooth_param_wire_path, PARAM_WIRE_ENDPOINT_STRAIGHT_PX,
    };
    use super::{
        signal_scope_range, signal_scope_y, timeline_beat_indicator_on, Rect, SceneBuildRequest,
        SceneBuilder, SIGNAL_SCOPE_MAX_SAMPLES,
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

    fn default_scene_build_request() -> SceneBuildRequest {
        SceneBuildRequest::new(640, 480, 640, 60)
    }

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
        let mut scene = SceneBuilder::default();

        let frame = scene.build(&project, &state, default_scene_build_request());
        let initial_samples = frame.signal_scope_samples;
        assert!(
            initial_samples > 0,
            "initial scope build should evaluate sample points"
        );

        state.frame_index = 1;
        state.invalidation.invalidate_signal_scopes();
        let frame = scene.build(&project, &state, default_scene_build_request());
        assert!(
            !frame.dirty.nodes,
            "timeline-only scope tick should not rebuild the node layer"
        );
        assert!(
            frame.dirty.signal_scopes,
            "timeline-only scope tick should rebuild the signal scope layer"
        );
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
        let mut scene = SceneBuilder::default();

        let frame = scene.build(&project, &state, default_scene_build_request());
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
        let frame = scene.build(&project, &state, default_scene_build_request());
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

        let frame = scene.build(&project, &state, default_scene_build_request());
        assert!(
            !frame.dirty.edges,
            "edges should remain frozen while node drag is active"
        );
        assert_eq!(frame.edges.lines.len(), frozen_lines_before);

        state.drag = None;
        let frame = scene.build(&project, &state, default_scene_build_request());
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
        let _ = scene.build(&project, &state, default_scene_build_request());
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

        let _ = scene.build(&project, &state, default_scene_build_request());
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
        let _ = scene.build(&project, &state, default_scene_build_request());
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
        let _ = scene.build(&project, &state, default_scene_build_request());
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
        let _ = scene.build(&project, &state, default_scene_build_request());
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
        let frame = scene.build(&project, &state, default_scene_build_request());
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
        let frame = scene.build(&project, &state, default_scene_build_request());
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
        let frame = scene.build(&project, &state, default_scene_build_request());
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

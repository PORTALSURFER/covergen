//! GUI project scaffolding and editable node model.
//!
//! The GUI currently starts with an empty project and supports adding and
//! moving nodes directly on the graph canvas.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Width of one graph node card in the editor canvas.
pub(crate) const NODE_WIDTH: i32 = 128;
/// Height of one graph node card in the editor canvas.
pub(crate) const NODE_HEIGHT: i32 = 44;
/// Width/height of node header expand/collapse toggle in graph-space pixels.
pub(crate) const NODE_TOGGLE_SIZE: i32 = 8;
/// Top-left inset from node origin to toggle origin in graph-space pixels.
pub(crate) const NODE_TOGGLE_MARGIN: i32 = 3;
/// Diameter of one node pin in editor pixels.
pub(crate) const NODE_PIN_SIZE: i32 = 8;
/// Height of one expanded parameter row in node cards.
pub(crate) const NODE_PARAM_ROW_HEIGHT: i32 = 16;
/// Horizontal padding for expanded parameter row content.
pub(crate) const NODE_PARAM_ROW_PAD_X: i32 = 4;
/// Horizontal padding from parameter row right edge to value input box.
pub(crate) const NODE_PARAM_VALUE_BOX_RIGHT_PAD: i32 = 6;
/// Width of one parameter value input box in graph-space pixels.
pub(crate) const NODE_PARAM_VALUE_BOX_WIDTH: i32 = 52;
const NODE_PIN_HALF: i32 = NODE_PIN_SIZE / 2;
const NODE_PARAM_FOOTER_PAD: i32 = 8;
const HIT_BIN_SIZE: i32 = 128;

/// Resource kinds currently carried by GUI graph ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResourceKind {
    /// GPU 2D texture resource.
    Texture2D,
    /// CPU-side scalar signal resource.
    Signal,
}

/// Execution kinds currently represented by GUI nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionKind {
    /// Node executes through a render pass.
    Render,
    /// Node executes in control domain.
    Control,
    /// Node is a runtime IO boundary.
    Io,
}

/// Minimal set of node kinds exposed by the Add Node menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectNodeKind {
    /// `tex.solid` source node (currently visualized as a circle placeholder).
    TexSolid,
    /// `tex.transform_2d` render node for texture-space color/alpha mutation.
    TexTransform2D,
    /// `ctl.lfo` signal generator node.
    CtlLfo,
    /// `io.window_out` sink node.
    IoWindowOut,
}

impl ProjectNodeKind {
    /// Return stable registry id used by UI labels and serialization.
    pub(crate) const fn stable_id(self) -> &'static str {
        match self {
            Self::TexSolid => "tex.solid",
            Self::TexTransform2D => "tex.transform_2d",
            Self::CtlLfo => "ctl.lfo",
            Self::IoWindowOut => "io.window_out",
        }
    }

    /// Return execution kind for this node.
    pub(crate) const fn execution_kind(self) -> ExecutionKind {
        match self {
            Self::TexSolid => ExecutionKind::Render,
            Self::TexTransform2D => ExecutionKind::Render,
            Self::CtlLfo => ExecutionKind::Control,
            Self::IoWindowOut => ExecutionKind::Io,
        }
    }

    /// Return short display label used by node and menu UI.
    pub(crate) const fn label(self) -> &'static str {
        self.stable_id()
    }

    /// Return true when this node kind accepts texture input.
    pub(crate) const fn accepts_texture_input(self) -> bool {
        matches!(self, Self::TexTransform2D | Self::IoWindowOut)
    }

    /// Return true when this node kind can bind scalar signal parameters.
    pub(crate) const fn accepts_signal_bindings(self) -> bool {
        matches!(self, Self::TexSolid | Self::TexTransform2D | Self::CtlLfo)
    }

    /// Return true when this node kind has a texture output pin.
    pub(crate) const fn produces_texture_output(self) -> bool {
        matches!(self, Self::TexSolid | Self::TexTransform2D)
    }

    /// Return true when this node kind has a scalar signal output pin.
    pub(crate) const fn produces_signal_output(self) -> bool {
        matches!(self, Self::CtlLfo)
    }

    /// Return true when this node kind has any input pin.
    pub(crate) const fn has_input_pin(self) -> bool {
        self.accepts_texture_input() || self.accepts_signal_bindings()
    }

    /// Return true when this node kind has any output pin.
    pub(crate) const fn has_output_pin(self) -> bool {
        self.produces_texture_output() || self.produces_signal_output()
    }

    /// Return output resource kind when this node publishes one.
    pub(crate) const fn output_resource_kind(self) -> Option<ResourceKind> {
        if self.produces_texture_output() {
            return Some(ResourceKind::Texture2D);
        }
        if self.produces_signal_output() {
            return Some(ResourceKind::Signal);
        }
        None
    }
}

/// Editable node-parameter state with optional signal binding.
#[derive(Clone, Debug)]
pub(crate) struct NodeParamSlot {
    key: &'static str,
    label: &'static str,
    value: f32,
    value_text: String,
    min: f32,
    max: f32,
    step: f32,
    signal_source: Option<u32>,
}

/// Read-only parameter view for rendering node UI.
#[derive(Clone, Debug)]
pub(crate) struct NodeParamView<'a> {
    pub(crate) label: &'a str,
    pub(crate) value_text: &'a str,
    pub(crate) bound: bool,
    pub(crate) selected: bool,
}

/// Zero-allocation iterator over one node's parameter rows.
///
/// This keeps UI traversal allocation-free by borrowing slots directly instead
/// of materializing an intermediate vector every frame.
pub(crate) struct NodeParamIter<'a> {
    params: std::slice::Iter<'a, NodeParamSlot>,
    selected_index: usize,
    index: usize,
}

impl<'a> Iterator for NodeParamIter<'a> {
    type Item = NodeParamView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let slot = self.params.next()?;
        let selected = self.index == self.selected_index;
        self.index += 1;
        Some(NodeParamView {
            label: slot.label,
            value_text: slot.value_text.as_str(),
            bound: slot.signal_source.is_some(),
            selected,
        })
    }
}

/// One user-editable graph node instance in a GUI project.
#[derive(Clone, Debug)]
pub(crate) struct ProjectNode {
    id: u32,
    kind: ProjectNodeKind,
    x: i32,
    y: i32,
    texture_input: Option<u32>,
    inputs: Vec<u32>,
    params: Vec<NodeParamSlot>,
    selected_param: usize,
    expanded: bool,
}

/// Axis-aligned bounds of all graph nodes in world-space coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GraphBounds {
    pub(crate) min_x: i32,
    pub(crate) min_y: i32,
    pub(crate) max_x: i32,
    pub(crate) max_y: i32,
}

impl ProjectNode {
    /// Return stable node id.
    pub(crate) const fn id(&self) -> u32 {
        self.id
    }

    /// Return node kind.
    pub(crate) const fn kind(&self) -> ProjectNodeKind {
        self.kind
    }

    /// Return top-left x-position in panel space.
    pub(crate) const fn x(&self) -> i32 {
        self.x
    }

    /// Return top-left y-position in panel space.
    pub(crate) const fn y(&self) -> i32 {
        self.y
    }

    /// Return input node ids.
    pub(crate) fn inputs(&self) -> &[u32] {
        &self.inputs
    }

    /// Return true when node card is expanded.
    pub(crate) const fn expanded(&self) -> bool {
        self.expanded
    }

    /// Return true when this node supports expand/collapse parameter UI.
    pub(crate) fn supports_expand_toggle(&self) -> bool {
        !self.params.is_empty()
    }

    /// Return node card height in world-space canvas pixels.
    pub(crate) fn card_height(&self) -> i32 {
        if !self.expanded || self.params.is_empty() {
            return NODE_HEIGHT;
        }
        NODE_HEIGHT + (self.params.len() as i32 * NODE_PARAM_ROW_HEIGHT) + NODE_PARAM_FOOTER_PAD
    }

    /// Return number of editable parameters for this node.
    pub(crate) fn param_count(&self) -> usize {
        self.params.len()
    }

    /// Return allocation-free iterator of parameter rows for rendering.
    pub(crate) fn param_views(&self) -> NodeParamIter<'_> {
        NodeParamIter {
            params: self.params.iter(),
            selected_index: self.selected_param.min(self.params.len().saturating_sub(1)),
            index: 0,
        }
    }

    /// Return read-only parameter row data for one index.
    pub(crate) fn param_view(&self, param_index: usize) -> Option<NodeParamView<'_>> {
        let slot = self.params.get(param_index)?;
        let selected = param_index == self.selected_param.min(self.params.len().saturating_sub(1));
        Some(NodeParamView {
            label: slot.label,
            value_text: slot.value_text.as_str(),
            bound: slot.signal_source.is_some(),
            selected,
        })
    }
}

/// In-memory GUI project model.
#[derive(Clone, Debug)]
pub(crate) struct GuiProject {
    /// Project display name.
    pub(crate) name: String,
    /// Preview canvas width.
    pub(crate) preview_width: u32,
    /// Preview canvas height.
    pub(crate) preview_height: u32,
    nodes: Vec<ProjectNode>,
    next_node_id: u32,
    edge_count: usize,
    hit_test_cache: RefCell<HitTestCache>,
    hit_test_dirty: Cell<bool>,
    hit_test_scan_count: Cell<u64>,
}

/// Cached spatial/index structures for fast graph hit-testing.
#[derive(Clone, Debug, Default)]
struct HitTestCache {
    node_index_by_id: HashMap<u32, usize>,
    node_bins: HashMap<i64, Vec<u32>>,
    output_pin_bins: HashMap<i64, Vec<u32>>,
    input_pin_bins: HashMap<i64, Vec<u32>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinHitKind {
    Output,
    Input,
}

impl GuiProject {
    /// Create a fresh empty project sized for the active preview canvas.
    pub(crate) fn new_empty(preview_width: u32, preview_height: u32) -> Self {
        Self {
            name: next_project_name(),
            preview_width,
            preview_height,
            nodes: Vec::new(),
            next_node_id: 1,
            edge_count: 0,
            hit_test_cache: RefCell::new(HitTestCache::default()),
            hit_test_dirty: Cell::new(false),
            hit_test_scan_count: Cell::new(0),
        }
    }

    /// Return immutable node slice for rendering.
    pub(crate) fn nodes(&self) -> &[ProjectNode] {
        &self.nodes
    }

    /// Return current node count.
    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Return and reset accumulated hit-test scan count since last call.
    pub(crate) fn take_hit_test_scan_count(&self) -> u64 {
        let count = self.hit_test_scan_count.get();
        self.hit_test_scan_count.set(0);
        count
    }

    /// Return total input-edge count currently stored in this project.
    pub(crate) fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Return immutable node by id.
    pub(crate) fn node(&self, node_id: u32) -> Option<&ProjectNode> {
        let index = self.node_index(node_id)?;
        self.nodes.get(index)
    }

    /// Return mutable node by id.
    fn node_mut(&mut self, node_id: u32) -> Option<&mut ProjectNode> {
        let index = self.node_index(node_id)?;
        self.nodes.get_mut(index)
    }

    fn node_index(&self, node_id: u32) -> Option<usize> {
        self.ensure_hit_test_cache();
        self.hit_test_cache
            .borrow()
            .node_index_by_id
            .get(&node_id)
            .copied()
    }

    fn invalidate_hit_test_cache(&self) {
        self.hit_test_dirty.set(true);
    }

    fn ensure_hit_test_cache(&self) {
        if !self.hit_test_dirty.get() {
            return;
        }
        let mut cache = HitTestCache::default();
        for (index, node) in self.nodes.iter().enumerate() {
            cache.node_index_by_id.insert(node.id(), index);
            cache_node_rect_bins(
                &mut cache.node_bins,
                node.id(),
                node.x(),
                node.y(),
                node.card_height(),
            );
            if let Some((x, y)) = output_pin_center(node) {
                cache_pin_bin(&mut cache.output_pin_bins, node.id(), x, y);
            }
            if let Some((x, y)) = input_pin_center(node) {
                cache_pin_bin(&mut cache.input_pin_bins, node.id(), x, y);
            }
        }
        *self.hit_test_cache.borrow_mut() = cache;
        self.hit_test_dirty.set(false);
    }

    /// Add one node at canvas position and return created id.
    pub(crate) fn add_node(
        &mut self,
        kind: ProjectNodeKind,
        x: i32,
        y: i32,
        panel_width: usize,
        panel_height: usize,
    ) -> u32 {
        let params = default_params_for_kind(kind);
        let card_h = node_card_height_for_param_count(false, params.len());
        let (x, y) = clamp_node_position(x, y, panel_width, panel_height, card_h);
        let node_id = self.next_node_id;
        self.next_node_id = self.next_node_id.saturating_add(1);
        self.nodes.push(ProjectNode {
            id: node_id,
            kind,
            x,
            y,
            texture_input: None,
            inputs: Vec::new(),
            params,
            selected_param: 0,
            expanded: false,
        });
        self.invalidate_hit_test_cache();
        node_id
    }

    /// Move one node in graph space.
    ///
    /// Returns `true` when the node position changed.
    pub(crate) fn move_node(
        &mut self,
        node_id: u32,
        x: i32,
        y: i32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let Some(index) = self.node_index(node_id) else {
            return false;
        };
        let changed = {
            let node = &mut self.nodes[index];
            let (x, y) = clamp_node_position(x, y, panel_width, panel_height, node.card_height());
            if node.x == x && node.y == y {
                false
            } else {
                node.x = x;
                node.y = y;
                true
            }
        };
        if changed {
            self.invalidate_hit_test_cache();
        }
        changed
    }

    /// Return top-most node id at the given panel-space position.
    pub(crate) fn node_at(&self, x: i32, y: i32) -> Option<u32> {
        self.ensure_hit_test_cache();
        let key = hit_bin_key_for_point(x, y);
        let cache = self.hit_test_cache.borrow();
        let candidates = cache.node_bins.get(&key)?;
        for node_id in candidates.iter().rev() {
            self.bump_hit_test_scan_count(1);
            let Some(index) = cache.node_index_by_id.get(node_id).copied() else {
                continue;
            };
            let Some(node) = self.nodes.get(index) else {
                continue;
            };
            if x >= node.x()
                && x < node.x() + NODE_WIDTH
                && y >= node.y()
                && y < node.y() + node.card_height()
            {
                return Some(*node_id);
            }
        }
        None
    }

    /// Return world-space graph bounds for all current nodes.
    pub(crate) fn graph_bounds(&self) -> Option<GraphBounds> {
        let first = self.nodes.first()?;
        let mut min_x = first.x();
        let mut min_y = first.y();
        let mut max_x = first.x() + NODE_WIDTH;
        let mut max_y = first.y() + first.card_height();
        for node in self.nodes.iter().skip(1) {
            min_x = min_x.min(node.x());
            min_y = min_y.min(node.y());
            max_x = max_x.max(node.x() + NODE_WIDTH);
            max_y = max_y.max(node.y() + node.card_height());
        }
        Some(GraphBounds {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    /// Return the node id whose output pin is hit by the cursor.
    pub(crate) fn output_pin_at(&self, x: i32, y: i32, radius_px: i32) -> Option<u32> {
        self.pin_at(x, y, radius_px, None, output_pin_center, PinHitKind::Output)
    }

    /// Return the node id whose input pin is hit by the cursor.
    pub(crate) fn input_pin_at(
        &self,
        x: i32,
        y: i32,
        radius_px: i32,
        disallow_source: Option<u32>,
    ) -> Option<u32> {
        self.pin_at(
            x,
            y,
            radius_px,
            disallow_source,
            input_pin_center,
            PinHitKind::Input,
        )
    }

    fn pin_at(
        &self,
        x: i32,
        y: i32,
        radius_px: i32,
        disallow_source: Option<u32>,
        center_for_node: fn(&ProjectNode) -> Option<(i32, i32)>,
        pin_kind: PinHitKind,
    ) -> Option<u32> {
        self.ensure_hit_test_cache();
        let radius_sq = radius_px.saturating_mul(radius_px);
        let min_x = x.saturating_sub(radius_px);
        let max_x = x.saturating_add(radius_px);
        let min_y = y.saturating_sub(radius_px);
        let max_y = y.saturating_add(radius_px);
        let mut seen = Vec::new();
        let mut hit = None;
        let mut hit_z = 0_usize;

        let cache = self.hit_test_cache.borrow();
        let bins = match pin_kind {
            PinHitKind::Output => &cache.output_pin_bins,
            PinHitKind::Input => &cache.input_pin_bins,
        };
        for by in hit_bin_coord(min_y)..=hit_bin_coord(max_y) {
            for bx in hit_bin_coord(min_x)..=hit_bin_coord(max_x) {
                let key = hit_bin_key(bx, by);
                let Some(candidates) = bins.get(&key) else {
                    continue;
                };
                for node_id in candidates.iter().rev() {
                    self.bump_hit_test_scan_count(1);
                    if Some(*node_id) == disallow_source || seen.contains(node_id) {
                        continue;
                    }
                    seen.push(*node_id);
                    let Some(index) = cache.node_index_by_id.get(node_id).copied() else {
                        continue;
                    };
                    let Some(node) = self.nodes.get(index) else {
                        continue;
                    };
                    let Some((px, py)) = center_for_node(node) else {
                        continue;
                    };
                    if distance_sq(x, y, px, py) <= radius_sq && (hit.is_none() || index >= hit_z) {
                        hit = Some(*node_id);
                        hit_z = index;
                    }
                }
            }
        }
        hit
    }

    /// Connect one source node output pin to one target node input pin.
    ///
    /// Texture links replace the target texture input. Signal links bind to the
    /// target's currently selected parameter slot.
    ///
    /// Returns `true` when graph wiring changed.
    pub(crate) fn connect_image_link(&mut self, source_id: u32, target_id: u32) -> bool {
        if source_id == target_id {
            return false;
        }
        if self.depends_on(source_id, target_id) {
            // Reject links that would introduce a cycle.
            return false;
        }
        let Some(source) = self.node(source_id) else {
            return false;
        };
        let Some(target) = self.node(target_id) else {
            return false;
        };
        let Some(source_kind) = source.kind().output_resource_kind() else {
            return false;
        };
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let changed = match source_kind {
            ResourceKind::Texture2D => {
                if !target.kind.accepts_texture_input() {
                    return false;
                }
                if target.texture_input == Some(source_id) {
                    false
                } else {
                    target.texture_input = Some(source_id);
                    true
                }
            }
            ResourceKind::Signal => {
                if !target.kind.accepts_signal_bindings() || target.params.is_empty() {
                    return false;
                }
                let param_index = target
                    .selected_param
                    .min(target.params.len().saturating_sub(1));
                let slot = &mut target.params[param_index];
                if slot.signal_source == Some(source_id) {
                    false
                } else {
                    slot.signal_source = Some(source_id);
                    true
                }
            }
        };
        if !changed {
            return false;
        }
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Disconnect one explicit source -> target link.
    ///
    /// Removes both texture-input and signal-parameter bindings that match the
    /// source/target pair.
    pub(crate) fn disconnect_link(&mut self, source_id: u32, target_id: u32) -> bool {
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let mut changed = false;
        if target.texture_input == Some(source_id) {
            target.texture_input = None;
            changed = true;
        }
        for slot in &mut target.params {
            if slot.signal_source == Some(source_id) {
                slot.signal_source = None;
                changed = true;
            }
        }
        if !changed {
            return false;
        }
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Delete all nodes in `node_ids` and remove any links that referenced them.
    ///
    /// Returns `true` when at least one node was removed.
    pub(crate) fn delete_nodes(&mut self, node_ids: &[u32]) -> bool {
        if node_ids.is_empty() {
            return false;
        }
        let mut removed_ids = node_ids.to_vec();
        removed_ids.sort_unstable();
        removed_ids.dedup();
        let before_len = self.nodes.len();
        self.nodes
            .retain(|node| !contains_sorted_id(removed_ids.as_slice(), node.id()));
        let removed_any = self.nodes.len() != before_len;
        let mut links_changed = false;
        for node in &mut self.nodes {
            links_changed |= clear_deleted_links(node, removed_ids.as_slice());
        }
        if !removed_any && !links_changed {
            return false;
        }
        if removed_any {
            self.invalidate_hit_test_cache();
        }
        self.recount_edges();
        true
    }

    /// Return source node id wired into the first `io.window_out` node, if any.
    pub(crate) fn window_out_input_node_id(&self) -> Option<u32> {
        let output = self
            .nodes
            .iter()
            .find(|node| matches!(node.kind, ProjectNodeKind::IoWindowOut))?;
        output.inputs.first().copied()
    }

    /// Return first input source node id for one node.
    pub(crate) fn input_source_node_id(&self, node_id: u32) -> Option<u32> {
        self.node(node_id)?.texture_input
    }

    /// Toggle one node expanded/collapsed state.
    ///
    /// Returns `true` when expanded state changed.
    pub(crate) fn toggle_node_expanded(
        &mut self,
        node_id: u32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let Some(index) = self.node_index(node_id) else {
            return false;
        };
        {
            let node = &mut self.nodes[index];
            if node.params.is_empty() {
                return false;
            }
            node.expanded = !node.expanded;
            let card_h = node.card_height();
            let (x, y) = clamp_node_position(node.x, node.y, panel_width, panel_height, card_h);
            node.x = x;
            node.y = y;
        }
        self.invalidate_hit_test_cache();
        true
    }

    /// Advance selected parameter row for one node.
    pub(crate) fn select_next_param(&mut self, node_id: u32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let max = node.params.len().saturating_sub(1);
        let next = (node.selected_param + 1).min(max);
        if next == node.selected_param {
            return false;
        }
        node.selected_param = next;
        true
    }

    /// Move selected parameter row up for one node.
    pub(crate) fn select_prev_param(&mut self, node_id: u32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() || node.selected_param == 0 {
            return false;
        }
        node.selected_param -= 1;
        true
    }

    /// Select one parameter row by index for one node.
    pub(crate) fn select_param(&mut self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let next = param_index.min(node.params.len().saturating_sub(1));
        if node.selected_param == next {
            return false;
        }
        node.selected_param = next;
        true
    }

    /// Adjust selected parameter value by one step.
    pub(crate) fn adjust_selected_param(&mut self, node_id: u32, direction: f32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let index = node.selected_param.min(node.params.len().saturating_sub(1));
        let slot = &mut node.params[index];
        let next = (slot.value + slot.step * direction).clamp(slot.min, slot.max);
        if (next - slot.value).abs() < 1e-6 {
            return false;
        }
        slot.value = next;
        slot.value_text = format_param_value_text(next);
        true
    }

    /// Return raw parameter value at one index for one node.
    pub(crate) fn node_param_raw_value(&self, node_id: u32, param_index: usize) -> Option<f32> {
        let node = self.node(node_id)?;
        if node.params.is_empty() {
            return None;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        node.params.get(index).map(|slot| slot.value)
    }

    /// Set one parameter value at one index after clamping to slot limits.
    pub(crate) fn set_param_value(&mut self, node_id: u32, param_index: usize, value: f32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        let slot = &mut node.params[index];
        let clamped = value.clamp(slot.min, slot.max);
        if (slot.value - clamped).abs() < 1e-6 {
            return false;
        }
        slot.value = clamped;
        slot.value_text = format_param_value_text(clamped);
        true
    }

    /// Return expanded parameter row index hit by one graph-space point.
    pub(crate) fn param_row_at(&self, node_id: u32, x: i32, y: i32) -> Option<usize> {
        let node = self.node(node_id)?;
        if !node.expanded() {
            return None;
        }
        for index in 0..node.params.len() {
            let Some(rect) = node_param_row_rect(node, index) else {
                continue;
            };
            if rect.contains(x, y) {
                return Some(index);
            }
        }
        None
    }

    /// Return true when graph-space point falls inside one value input box.
    pub(crate) fn param_value_box_contains(
        &self,
        node_id: u32,
        param_index: usize,
        x: i32,
        y: i32,
    ) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        let Some(rect) = node_param_value_rect(node, param_index) else {
            return false;
        };
        rect.contains(x, y)
    }

    /// Return cached formatted parameter text at one index for one node.
    pub(crate) fn node_param_raw_text(&self, node_id: u32, param_index: usize) -> Option<&str> {
        let node = self.node(node_id)?;
        if node.params.is_empty() {
            return None;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        node.params.get(index).map(|slot| slot.value_text.as_str())
    }

    /// Return true when a node is currently expanded.
    pub(crate) fn node_expanded(&self, node_id: u32) -> bool {
        self.node(node_id)
            .map(ProjectNode::expanded)
            .unwrap_or(false)
    }

    /// Return effective parameter value, resolving optional signal binding.
    pub(crate) fn node_param_value(
        &self,
        node_id: u32,
        key: &'static str,
        time_secs: f32,
        eval_stack: &mut Vec<u32>,
    ) -> Option<f32> {
        let node = self.node(node_id)?;
        let slot = node.params.iter().find(|slot| slot.key == key)?;
        let mut value = slot.value;
        if let Some(source_id) = slot.signal_source {
            if let Some(signal) = self.sample_signal_node(source_id, time_secs, eval_stack) {
                value = signal;
            }
        }
        Some(value.clamp(slot.min, slot.max))
    }

    /// Evaluate one scalar signal node output.
    pub(crate) fn sample_signal_node(
        &self,
        node_id: u32,
        time_secs: f32,
        eval_stack: &mut Vec<u32>,
    ) -> Option<f32> {
        if eval_stack.contains(&node_id) {
            return None;
        }
        let node = self.node(node_id)?;
        if !node.kind.produces_signal_output() {
            return None;
        }
        eval_stack.push(node_id);
        let rate = self
            .node_param_value(node_id, "rate_hz", time_secs, eval_stack)
            .unwrap_or(0.4);
        let amplitude = self
            .node_param_value(node_id, "amplitude", time_secs, eval_stack)
            .unwrap_or(0.5);
        let phase = self
            .node_param_value(node_id, "phase", time_secs, eval_stack)
            .unwrap_or(0.0);
        let bias = self
            .node_param_value(node_id, "bias", time_secs, eval_stack)
            .unwrap_or(0.5);
        let v = (time_secs * rate * std::f32::consts::TAU + phase * std::f32::consts::TAU).sin()
            * amplitude
            + bias;
        eval_stack.pop();
        Some(v)
    }

    /// Return stable signature for render-affecting graph state.
    ///
    /// This signature intentionally excludes UI-only fields such as expanded
    /// state and selected parameter row so preview caches only invalidate when
    /// output content can change.
    pub(crate) fn render_signature(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for node in &self.nodes {
            hash = fnv1a_u64(hash, node.id as u64);
            for byte in node.kind.stable_id().as_bytes() {
                hash = fnv1a_u64(hash, *byte as u64);
            }
            if let Some(texture_input) = node.texture_input {
                hash = fnv1a_u64(hash, texture_input as u64);
            }
            hash = fnv1a_u64(hash, 0xff);
            for slot in &node.params {
                for byte in slot.key.as_bytes() {
                    hash = fnv1a_u64(hash, *byte as u64);
                }
                hash = fnv1a_u64(hash, slot.value.to_bits() as u64);
                if let Some(source) = slot.signal_source {
                    hash = fnv1a_u64(hash, source as u64);
                }
            }
            hash = fnv1a_u64(hash, 0xfe);
        }
        hash
    }

    /// Return stable signature for UI-only node-editor state.
    ///
    /// This can be used by UI caches that should react to node-card expansion,
    /// row selection, or node position updates without affecting render caches.
    pub(crate) fn ui_signature(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for node in &self.nodes {
            hash = fnv1a_u64(hash, node.id as u64);
            hash = fnv1a_u64(hash, node.x as i64 as u64);
            hash = fnv1a_u64(hash, node.y as i64 as u64);
            hash = fnv1a_u64(hash, node.expanded as u64);
            hash = fnv1a_u64(hash, node.selected_param as u64);
            hash = fnv1a_u64(hash, 0xfd);
        }
        hash
    }

    /// Return stable signature for both render and UI graph state.
    ///
    /// Prefer [`Self::render_signature`] for TOP/render invalidation.
    pub(crate) fn graph_signature(&self) -> u64 {
        fnv1a_u64(self.render_signature(), self.ui_signature())
    }

    /// Return true when at least one parameter has a live signal binding.
    pub(crate) fn has_signal_bindings(&self) -> bool {
        self.nodes
            .iter()
            .any(|node| node.params.iter().any(|slot| slot.signal_source.is_some()))
    }

    fn depends_on(&self, start_node_id: u32, target_node_id: u32) -> bool {
        let mut stack = vec![start_node_id];
        let mut visited = Vec::new();
        while let Some(node_id) = stack.pop() {
            if node_id == target_node_id {
                return true;
            }
            if visited.contains(&node_id) {
                continue;
            }
            visited.push(node_id);
            if let Some(node) = self.node(node_id) {
                stack.extend(node.inputs.iter().copied());
            }
        }
        false
    }

    fn recount_edges(&mut self) {
        self.edge_count = self.nodes.iter().map(|node| node.inputs.len()).sum();
    }

    fn bump_hit_test_scan_count(&self, delta: u64) {
        let next = self.hit_test_scan_count.get().saturating_add(delta);
        self.hit_test_scan_count.set(next);
    }
}

/// Return panel-space center of a node output pin.
pub(crate) fn output_pin_center(node: &ProjectNode) -> Option<(i32, i32)> {
    if !node.kind().has_output_pin() {
        return None;
    }
    let x = node.x() + NODE_WIDTH - 1;
    let y = node.y() + (node.card_height() / 2);
    Some((x, y))
}

/// Return panel-space center of a node input pin.
pub(crate) fn input_pin_center(node: &ProjectNode) -> Option<(i32, i32)> {
    if !node.kind().has_input_pin() {
        return None;
    }
    let x = node.x();
    let y = node.y() + (node.card_height() / 2);
    Some((x, y))
}

/// Return one pin rectangle centered around a pin position.
pub(crate) fn pin_rect(cx: i32, cy: i32) -> super::geometry::Rect {
    super::geometry::Rect::new(
        cx - NODE_PIN_HALF,
        cy - NODE_PIN_HALF,
        NODE_PIN_SIZE,
        NODE_PIN_SIZE,
    )
}

/// Return node header expand/collapse toggle rectangle in graph-space coordinates.
pub(crate) fn node_expand_toggle_rect(node: &ProjectNode) -> Option<super::geometry::Rect> {
    if !node.supports_expand_toggle() {
        return None;
    }
    Some(super::geometry::Rect::new(
        node.x() + NODE_TOGGLE_MARGIN,
        node.y() + NODE_TOGGLE_MARGIN,
        NODE_TOGGLE_SIZE,
        NODE_TOGGLE_SIZE,
    ))
}

/// Return one parameter row rectangle in graph-space coordinates.
pub(crate) fn node_param_row_rect(
    node: &ProjectNode,
    param_index: usize,
) -> Option<super::geometry::Rect> {
    if !node.expanded() || param_index >= node.params.len() {
        return None;
    }
    let row_y = node.y() + NODE_HEIGHT + param_index as i32 * NODE_PARAM_ROW_HEIGHT;
    Some(super::geometry::Rect::new(
        node.x() + NODE_PARAM_ROW_PAD_X,
        row_y,
        NODE_WIDTH - NODE_PARAM_ROW_PAD_X * 2,
        NODE_PARAM_ROW_HEIGHT,
    ))
}

/// Return one parameter value input box rectangle in graph-space coordinates.
pub(crate) fn node_param_value_rect(
    node: &ProjectNode,
    param_index: usize,
) -> Option<super::geometry::Rect> {
    let row = node_param_row_rect(node, param_index)?;
    let width = NODE_PARAM_VALUE_BOX_WIDTH
        .min(row.w.saturating_sub(8))
        .max(20);
    let x = row.x + row.w - width - NODE_PARAM_VALUE_BOX_RIGHT_PAD;
    Some(super::geometry::Rect::new(
        x,
        row.y + 1,
        width,
        row.h.saturating_sub(2),
    ))
}

fn clamp_node_position(
    x: i32,
    y: i32,
    _panel_width: usize,
    _panel_height: usize,
    _node_height: i32,
) -> (i32, i32) {
    // Keep the call sites stable for now, but stop clamping node coordinates:
    // the graph canvas is intentionally unbounded.
    (x, y)
}

fn node_card_height_for_param_count(expanded: bool, param_count: usize) -> i32 {
    if !expanded || param_count == 0 {
        return NODE_HEIGHT;
    }
    NODE_HEIGHT + (param_count as i32 * NODE_PARAM_ROW_HEIGHT) + NODE_PARAM_FOOTER_PAD
}

fn default_params_for_kind(kind: ProjectNodeKind) -> Vec<NodeParamSlot> {
    match kind {
        ProjectNodeKind::TexSolid => vec![
            param("center_x", "center_x", 0.5, 0.0, 1.0, 0.01),
            param("center_y", "center_y", 0.5, 0.0, 1.0, 0.01),
            param("radius", "radius", 0.24, 0.02, 0.5, 0.005),
            param("feather", "feather", 0.06, 0.0, 0.25, 0.005),
            param("color_r", "color_r", 0.9, 0.0, 1.0, 0.01),
            param("color_g", "color_g", 0.9, 0.0, 1.0, 0.01),
            param("color_b", "color_b", 0.9, 0.0, 1.0, 0.01),
            param("alpha", "alpha", 1.0, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::TexTransform2D => vec![
            param("brightness", "brightness", 1.08, 0.0, 2.0, 0.02),
            param("gain_r", "gain_r", 0.45, 0.0, 2.0, 0.02),
            param("gain_g", "gain_g", 0.8, 0.0, 2.0, 0.02),
            param("gain_b", "gain_b", 1.0, 0.0, 2.0, 0.02),
            param("alpha_mul", "alpha_mul", 0.8, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::CtlLfo => vec![
            param("rate_hz", "rate_hz", 0.4, 0.0, 8.0, 0.05),
            param("amplitude", "amplitude", 0.5, 0.0, 1.0, 0.02),
            param("phase", "phase", 0.0, -1.0, 1.0, 0.02),
            param("bias", "bias", 0.5, -1.0, 1.0, 0.02),
        ],
        ProjectNodeKind::IoWindowOut => Vec::new(),
    }
}

fn param(
    key: &'static str,
    label: &'static str,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
) -> NodeParamSlot {
    NodeParamSlot {
        key,
        label,
        value,
        value_text: format_param_value_text(value),
        min,
        max,
        step,
        signal_source: None,
    }
}

fn format_param_value_text(value: f32) -> String {
    format!("{value:.3}")
}

fn rebuild_node_inputs(node: &mut ProjectNode) {
    node.inputs.clear();
    if let Some(texture_source) = node.texture_input {
        node.inputs.push(texture_source);
    }
    for slot in &node.params {
        let Some(signal_source) = slot.signal_source else {
            continue;
        };
        if !node.inputs.contains(&signal_source) {
            node.inputs.push(signal_source);
        }
    }
}

fn clear_deleted_links(node: &mut ProjectNode, removed_ids: &[u32]) -> bool {
    let mut changed = false;
    if let Some(source) = node.texture_input {
        if contains_sorted_id(removed_ids, source) {
            node.texture_input = None;
            changed = true;
        }
    }
    for slot in &mut node.params {
        if let Some(source) = slot.signal_source {
            if contains_sorted_id(removed_ids, source) {
                slot.signal_source = None;
                changed = true;
            }
        }
    }
    if changed {
        rebuild_node_inputs(node);
    }
    changed
}

fn contains_sorted_id(ids: &[u32], id: u32) -> bool {
    ids.binary_search(&id).is_ok()
}

fn distance_sq(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    let dx = ax - bx;
    let dy = ay - by;
    dx.saturating_mul(dx) + dy.saturating_mul(dy)
}

fn cache_node_rect_bins(
    bins: &mut HashMap<i64, Vec<u32>>,
    node_id: u32,
    x: i32,
    y: i32,
    card_height: i32,
) {
    if card_height <= 0 {
        return;
    }
    let max_x = x.saturating_add(NODE_WIDTH.saturating_sub(1));
    let max_y = y.saturating_add(card_height.saturating_sub(1));
    for by in hit_bin_coord(y)..=hit_bin_coord(max_y) {
        for bx in hit_bin_coord(x)..=hit_bin_coord(max_x) {
            bins.entry(hit_bin_key(bx, by)).or_default().push(node_id);
        }
    }
}

fn cache_pin_bin(bins: &mut HashMap<i64, Vec<u32>>, node_id: u32, x: i32, y: i32) {
    bins.entry(hit_bin_key_for_point(x, y))
        .or_default()
        .push(node_id);
}

fn hit_bin_coord(value: i32) -> i32 {
    value.div_euclid(HIT_BIN_SIZE)
}

fn hit_bin_key_for_point(x: i32, y: i32) -> i64 {
    hit_bin_key(hit_bin_coord(x), hit_bin_coord(y))
}

fn hit_bin_key(x: i32, y: i32) -> i64 {
    ((x as i64) << 32) | ((y as u32) as i64)
}

fn fnv1a_u64(hash: u64, data: u64) -> u64 {
    (hash ^ data).wrapping_mul(0x100000001b3)
}

fn next_project_name() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("Untitled-{}", now)
}

#[cfg(test)]
mod tests {
    use super::{
        input_pin_center, node_expand_toggle_rect, node_param_value_rect, output_pin_center,
        GraphBounds, GuiProject, ProjectNodeKind, NODE_HEIGHT,
    };

    #[test]
    fn empty_project_has_no_nodes() {
        let project = GuiProject::new_empty(640, 480);
        assert_eq!(project.node_count(), 0);
    }

    #[test]
    fn add_node_assigns_incrementing_ids() {
        let mut project = GuiProject::new_empty(640, 480);
        let a = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let b = project.add_node(ProjectNodeKind::IoWindowOut, 120, 120, 420, 480);
        assert_eq!(a, 1);
        assert_eq!(b, 2);
    }

    #[test]
    fn node_hit_test_uses_topmost_order() {
        let mut project = GuiProject::new_empty(640, 480);
        let a = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let b = project.add_node(ProjectNodeKind::IoWindowOut, 80, 80, 420, 480);
        assert_eq!(project.node_at(90, 90), Some(b));
        assert_ne!(project.node_at(90, 90), Some(a));
    }

    #[test]
    fn node_hit_test_updates_after_move_without_full_scan_state_drift() {
        let mut project = GuiProject::new_empty(640, 480);
        let node = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        assert_eq!(project.node_at(90, 90), Some(node));
        assert!(project.move_node(node, 260, 220, 420, 480));
        assert_eq!(project.node_at(90, 90), None);
        assert_eq!(project.node_at(270, 230), Some(node));
    }

    #[test]
    fn expanded_node_hit_bounds_update_after_toggle() {
        let mut project = GuiProject::new_empty(640, 480);
        let node = project.add_node(ProjectNodeKind::TexSolid, 60, 60, 420, 480);
        let base_miss_y = 60 + NODE_HEIGHT + 4;
        assert_eq!(project.node_at(72, base_miss_y), None);
        assert!(project.toggle_node_expanded(node, 420, 480));
        assert_eq!(project.node_at(72, base_miss_y), Some(node));
    }

    #[test]
    fn pin_hit_tests_work_through_spatial_bins() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 240, 80, 420, 480);
        let solid_node = project.node(solid).expect("solid node");
        let out_node = project.node(out).expect("output node");
        let (ox, oy) = output_pin_center(solid_node).expect("solid output");
        let (ix, iy) = input_pin_center(out_node).expect("output input");
        assert_eq!(project.output_pin_at(ox, oy, 10), Some(solid));
        assert_eq!(project.input_pin_at(ix, iy, 10, None), Some(out));
        assert_eq!(project.input_pin_at(ix, iy, 10, Some(out)), None);
    }

    #[test]
    fn connect_image_link_wires_solid_to_window_out() {
        let mut project = GuiProject::new_empty(640, 480);
        let top = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(top, out));
        assert_eq!(project.edge_count(), 1);
        let source_id = project
            .window_out_input_node_id()
            .expect("window-out input must exist");
        let source = project.node(source_id).expect("source node must exist");
        assert_eq!(source.kind(), ProjectNodeKind::TexSolid);
        assert!(!project.connect_image_link(top, out));
    }

    #[test]
    fn transform_node_supports_in_and_out_links() {
        let mut project = GuiProject::new_empty(640, 480);
        let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 160, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
        assert!(project.connect_image_link(source, xform));
        assert!(project.connect_image_link(xform, out));
        assert_eq!(project.edge_count(), 2);
        let source_id = project
            .window_out_input_node_id()
            .expect("window-out input must exist");
        let source = project.node(source_id).expect("source node must exist");
        assert_eq!(source.kind(), ProjectNodeKind::TexTransform2D);
    }

    #[test]
    fn connect_image_link_rejects_cycle() {
        let mut project = GuiProject::new_empty(640, 480);
        let a = project.add_node(ProjectNodeKind::TexTransform2D, 20, 40, 420, 480);
        let b = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
        assert!(project.connect_image_link(a, b));
        assert!(!project.connect_image_link(b, a));
    }

    #[test]
    fn disconnect_link_removes_texture_and_signal_bindings() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 160, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 40, 420, 480);
        assert!(project.connect_image_link(solid, out));
        assert!(project.toggle_node_expanded(solid, 420, 480));
        assert!(project.select_next_param(solid));
        assert!(project.connect_image_link(lfo, solid));
        assert!(project.edge_count() >= 2);
        assert!(project.disconnect_link(lfo, solid));
        assert!(project.disconnect_link(solid, out));
        assert_eq!(project.edge_count(), 0);
    }

    #[test]
    fn delete_nodes_removes_nodes_and_clears_referenced_links() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 160, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 40, 420, 480);
        assert!(project.connect_image_link(solid, out));
        assert!(project.toggle_node_expanded(solid, 420, 480));
        assert!(project.select_next_param(solid));
        assert!(project.connect_image_link(lfo, solid));
        assert!(project.edge_count() >= 2);
        assert!(project.delete_nodes(&[solid]));
        assert!(project.node(solid).is_none());
        assert_eq!(project.edge_count(), 0);
        assert!(project.window_out_input_node_id().is_none());
    }

    #[test]
    fn set_param_value_clamps_to_slot_range() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        assert!(project.set_param_value(solid, 0, 10.0));
        let value = project
            .node_param_raw_value(solid, 0)
            .expect("param value should exist");
        assert_eq!(value, 1.0);
        let value_text = project
            .node_param_raw_text(solid, 0)
            .expect("param text should exist");
        assert_eq!(value_text, "1.000");
    }

    #[test]
    fn cached_param_text_updates_when_value_changes() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let initial = project
            .node_param_raw_text(solid, 0)
            .expect("param text should exist")
            .to_string();
        assert_eq!(initial, "0.500");

        assert!(project.set_param_value(solid, 0, 0.25));
        let after_set = project
            .node_param_raw_text(solid, 0)
            .expect("param text should exist");
        assert_eq!(after_set, "0.250");

        assert!(project.adjust_selected_param(solid, 1.0));
        let after_adjust = project
            .node_param_raw_text(solid, 0)
            .expect("param text should exist");
        assert_eq!(after_adjust, "0.260");
    }

    #[test]
    fn render_signature_ignores_expand_and_param_selection_state() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 180, 40, 420, 480);
        assert!(project.connect_image_link(solid, out));
        let base = project.render_signature();

        assert!(project.toggle_node_expanded(solid, 420, 480));
        assert_eq!(project.render_signature(), base);

        assert!(project.select_next_param(solid));
        assert_eq!(project.render_signature(), base);
    }

    #[test]
    fn ui_signature_changes_for_expand_or_param_selection() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let base = project.ui_signature();

        assert!(project.toggle_node_expanded(solid, 420, 480));
        let after_expand = project.ui_signature();
        assert_ne!(after_expand, base);

        assert!(project.select_next_param(solid));
        assert_ne!(project.ui_signature(), after_expand);
    }

    #[test]
    fn render_signature_changes_when_render_param_changes() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let base = project.render_signature();

        assert!(project.set_param_value(solid, 0, 0.2));
        assert_ne!(project.render_signature(), base);
    }

    #[test]
    fn param_row_hit_returns_index_for_expanded_node() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
        assert!(project.toggle_node_expanded(solid, 420, 480));
        let node = project.node(solid).expect("node should exist");
        let row = super::node_param_row_rect(node, 2).expect("row rect");
        let hit = project.param_row_at(solid, row.x + 2, row.y + 2);
        assert_eq!(hit, Some(2));
        let value_rect = node_param_value_rect(node, 2).expect("value rect");
        let value_hit =
            project.param_value_box_contains(solid, 2, value_rect.x + 2, value_rect.y + 2);
        assert!(value_hit);
    }

    #[test]
    fn expand_toggle_rect_exists_for_param_nodes_only() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        let solid_node = project.node(solid).expect("solid node");
        let out_node = project.node(out).expect("out node");
        let solid_rect = node_expand_toggle_rect(solid_node).expect("solid toggle");
        assert_eq!(solid_rect.x, solid_node.x() + super::NODE_TOGGLE_MARGIN);
        assert_eq!(solid_rect.y, solid_node.y() + super::NODE_TOGGLE_MARGIN);
        assert!(node_expand_toggle_rect(out_node).is_none());
    }

    #[test]
    fn pin_centers_follow_node_kind_capabilities() {
        let mut project = GuiProject::new_empty(640, 480);
        let top = project.add_node(ProjectNodeKind::TexSolid, 60, 70, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 70, 420, 480);
        let top_node = project.node(top).expect("top node must exist");
        let out_node = project.node(out).expect("output node must exist");
        assert!(output_pin_center(top_node).is_some());
        assert!(input_pin_center(top_node).is_some());
        assert!(output_pin_center(out_node).is_none());
        assert!(input_pin_center(out_node).is_some());
    }

    #[test]
    fn graph_bounds_span_all_nodes() {
        let mut project = GuiProject::new_empty(640, 480);
        project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
        project.add_node(ProjectNodeKind::IoWindowOut, 200, 160, 420, 480);
        assert_eq!(
            project.graph_bounds(),
            Some(GraphBounds {
                min_x: 40,
                min_y: 80,
                max_x: 328,
                max_y: 204,
            })
        );
    }
}

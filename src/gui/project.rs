//! GUI project scaffolding and editable node model.
//!
//! The GUI currently starts with an empty project and supports adding and
//! moving nodes directly on the graph canvas.

use std::time::{SystemTime, UNIX_EPOCH};

/// Width of one graph node card in the editor canvas.
pub(crate) const NODE_WIDTH: i32 = 128;
/// Height of one graph node card in the editor canvas.
pub(crate) const NODE_HEIGHT: i32 = 44;
/// Diameter of one node pin in editor pixels.
pub(crate) const NODE_PIN_SIZE: i32 = 8;
/// Height of one expanded parameter row in node cards.
pub(crate) const NODE_PARAM_ROW_HEIGHT: i32 = 16;
const NODE_PIN_HALF: i32 = NODE_PIN_SIZE / 2;
const NODE_PARAM_FOOTER_PAD: i32 = 8;

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
    min: f32,
    max: f32,
    step: f32,
    signal_source: Option<u32>,
}

/// Read-only parameter view for rendering node UI.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeParamView {
    pub(crate) label: &'static str,
    pub(crate) value: f32,
    pub(crate) bound: bool,
    pub(crate) selected: bool,
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

    /// Return node card height in world-space canvas pixels.
    pub(crate) fn card_height(&self) -> i32 {
        if !self.expanded || self.params.is_empty() {
            return NODE_HEIGHT;
        }
        NODE_HEIGHT + (self.params.len() as i32 * NODE_PARAM_ROW_HEIGHT) + NODE_PARAM_FOOTER_PAD
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

    /// Return total input-edge count currently stored in this project.
    pub(crate) fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Return immutable node by id.
    pub(crate) fn node(&self, node_id: u32) -> Option<&ProjectNode> {
        self.nodes.iter().find(|node| node.id == node_id)
    }

    /// Return mutable node by id.
    fn node_mut(&mut self, node_id: u32) -> Option<&mut ProjectNode> {
        self.nodes.iter_mut().find(|node| node.id == node_id)
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
        node_id
    }

    /// Move one node while keeping it in canvas bounds.
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
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == node_id) {
            let (x, y) = clamp_node_position(x, y, panel_width, panel_height, node.card_height());
            if node.x == x && node.y == y {
                return false;
            }
            node.x = x;
            node.y = y;
            return true;
        }
        false
    }

    /// Return top-most node id at the given panel-space position.
    pub(crate) fn node_at(&self, x: i32, y: i32) -> Option<u32> {
        self.nodes
            .iter()
            .rev()
            .find(|node| x >= node.x && x < node.x + NODE_WIDTH && y >= node.y && y < node.y + node.card_height())
            .map(|node| node.id)
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
        let radius_sq = radius_px.saturating_mul(radius_px);
        self.nodes
            .iter()
            .rev()
            .find(|node| {
                let Some((px, py)) = output_pin_center(node) else {
                    return false;
                };
                distance_sq(x, y, px, py) <= radius_sq
            })
            .map(ProjectNode::id)
    }

    /// Return the node id whose input pin is hit by the cursor.
    pub(crate) fn input_pin_at(
        &self,
        x: i32,
        y: i32,
        radius_px: i32,
        disallow_source: Option<u32>,
    ) -> Option<u32> {
        let radius_sq = radius_px.saturating_mul(radius_px);
        self.nodes
            .iter()
            .rev()
            .filter(|node| Some(node.id()) != disallow_source)
            .find(|node| {
                let Some((px, py)) = input_pin_center(node) else {
                    return false;
                };
                distance_sq(x, y, px, py) <= radius_sq
            })
            .map(ProjectNode::id)
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
                let param_index = target.selected_param.min(target.params.len().saturating_sub(1));
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
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        node.expanded = !node.expanded;
        let card_h = node.card_height();
        let (x, y) = clamp_node_position(node.x, node.y, panel_width, panel_height, card_h);
        node.x = x;
        node.y = y;
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
        true
    }

    /// Return rendered parameter rows for one node.
    pub(crate) fn node_param_views(&self, node_id: u32) -> Option<Vec<NodeParamView>> {
        let node = self.node(node_id)?;
        if node.params.is_empty() {
            return Some(Vec::new());
        }
        let selected = node.selected_param.min(node.params.len().saturating_sub(1));
        let mut out = Vec::with_capacity(node.params.len());
        for (idx, slot) in node.params.iter().enumerate() {
            out.push(NodeParamView {
                label: slot.label,
                value: slot.value,
                bound: slot.signal_source.is_some(),
                selected: idx == selected,
            });
        }
        Some(out)
    }

    /// Return true when a node is currently expanded.
    pub(crate) fn node_expanded(&self, node_id: u32) -> bool {
        self.node(node_id).map(ProjectNode::expanded).unwrap_or(false)
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

    /// Return stable signature for graph topology + node kinds.
    ///
    /// This is used by preview caches to detect wiring or node-kind updates.
    pub(crate) fn graph_signature(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for node in &self.nodes {
            hash = fnv1a_u64(hash, node.id as u64);
            for byte in node.kind.stable_id().as_bytes() {
                hash = fnv1a_u64(hash, *byte as u64);
            }
            hash = fnv1a_u64(hash, node.expanded as u64);
            hash = fnv1a_u64(hash, node.selected_param as u64);
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

fn clamp_node_position(
    x: i32,
    y: i32,
    panel_width: usize,
    panel_height: usize,
    node_height: i32,
) -> (i32, i32) {
    let max_x = (panel_width as i32 - NODE_WIDTH - 6).max(6);
    let max_y = (panel_height as i32 - node_height - 6).max(6);
    (x.clamp(6, max_x), y.clamp(40, max_y))
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
        min,
        max,
        step,
        signal_source: None,
    }
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

fn distance_sq(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    let dx = ax - bx;
    let dy = ay - by;
    dx.saturating_mul(dx) + dy.saturating_mul(dy)
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
    use super::{input_pin_center, output_pin_center, GraphBounds, GuiProject, ProjectNodeKind};

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

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
const NODE_PIN_HALF: i32 = NODE_PIN_SIZE / 2;

/// Resource kinds currently carried by GUI graph ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResourceKind {
    /// GPU 2D texture resource.
    Texture2D,
}

/// Execution kinds currently represented by GUI nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionKind {
    /// Node executes through a render pass.
    Render,
    /// Node is a runtime IO boundary.
    Io,
}

/// Minimal set of node kinds exposed by the Add Node menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectNodeKind {
    /// `tex.solid` source node (currently visualized as a circle placeholder).
    TexSolid,
    /// `io.window_out` sink node.
    IoWindowOut,
}

impl ProjectNodeKind {
    /// Return stable registry id used by UI labels and serialization.
    pub(crate) const fn stable_id(self) -> &'static str {
        match self {
            Self::TexSolid => "tex.solid",
            Self::IoWindowOut => "io.window_out",
        }
    }

    /// Return execution kind for this node.
    pub(crate) const fn execution_kind(self) -> ExecutionKind {
        match self {
            Self::TexSolid => ExecutionKind::Render,
            Self::IoWindowOut => ExecutionKind::Io,
        }
    }

    /// Return short display label used by node and menu UI.
    pub(crate) const fn label(self) -> &'static str {
        self.stable_id()
    }

    /// Return true when this node kind has an input pin.
    pub(crate) const fn accepts_image_input(self) -> bool {
        matches!(self, Self::IoWindowOut)
    }

    /// Return required input resource kind when this node consumes one.
    pub(crate) const fn input_resource_kind(self) -> Option<ResourceKind> {
        if self.accepts_image_input() {
            return Some(ResourceKind::Texture2D);
        }
        None
    }

    /// Return true when this node kind has an output pin.
    pub(crate) const fn produces_image_output(self) -> bool {
        matches!(self, Self::TexSolid)
    }

    /// Return output resource kind when this node publishes one.
    pub(crate) const fn output_resource_kind(self) -> Option<ResourceKind> {
        if self.produces_image_output() {
            return Some(ResourceKind::Texture2D);
        }
        None
    }
}

/// One user-editable graph node instance in a GUI project.
#[derive(Clone, Debug)]
pub(crate) struct ProjectNode {
    id: u32,
    kind: ProjectNodeKind,
    x: i32,
    y: i32,
    inputs: Vec<u32>,
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
        let (x, y) = clamp_node_position(x, y, panel_width, panel_height);
        let node_id = self.next_node_id;
        self.next_node_id = self.next_node_id.saturating_add(1);
        self.nodes.push(ProjectNode {
            id: node_id,
            kind,
            x,
            y,
            inputs: Vec::new(),
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
        let (x, y) = clamp_node_position(x, y, panel_width, panel_height);
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == node_id) {
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
            .find(|node| {
                x >= node.x && x < node.x + NODE_WIDTH && y >= node.y && y < node.y + NODE_HEIGHT
            })
            .map(|node| node.id)
    }

    /// Return world-space graph bounds for all current nodes.
    pub(crate) fn graph_bounds(&self) -> Option<GraphBounds> {
        let first = self.nodes.first()?;
        let mut min_x = first.x();
        let mut min_y = first.y();
        let mut max_x = first.x() + NODE_WIDTH;
        let mut max_y = first.y() + NODE_HEIGHT;
        for node in self.nodes.iter().skip(1) {
            min_x = min_x.min(node.x());
            min_y = min_y.min(node.y());
            max_x = max_x.max(node.x() + NODE_WIDTH);
            max_y = max_y.max(node.y() + NODE_HEIGHT);
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
    /// The target uses one image input slot; connecting replaces its prior input.
    /// Returns `true` when graph wiring changed.
    pub(crate) fn connect_image_link(&mut self, source_id: u32, target_id: u32) -> bool {
        if source_id == target_id {
            return false;
        }
        let Some(source) = self.node(source_id) else {
            return false;
        };
        if source.kind().output_resource_kind() != Some(ResourceKind::Texture2D) {
            return false;
        }
        let Some(target) = self.node(target_id) else {
            return false;
        };
        if target.kind().input_resource_kind() != Some(ResourceKind::Texture2D) {
            return false;
        }
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        if target.inputs.as_slice() == [source_id] {
            return false;
        }
        target.inputs.clear();
        target.inputs.push(source_id);
        self.recount_edges();
        true
    }

    /// Return the source kind wired into the first `io.window_out` node, if any.
    pub(crate) fn output_source_kind(&self) -> Option<ProjectNodeKind> {
        let output = self
            .nodes
            .iter()
            .find(|node| matches!(node.kind, ProjectNodeKind::IoWindowOut))?;
        let source_id = *output.inputs.first()?;
        self.node(source_id).map(ProjectNode::kind)
    }

    fn recount_edges(&mut self) {
        self.edge_count = self.nodes.iter().map(|node| node.inputs.len()).sum();
    }
}

/// Return panel-space center of a node output pin.
pub(crate) fn output_pin_center(node: &ProjectNode) -> Option<(i32, i32)> {
    if !node.kind().produces_image_output() {
        return None;
    }
    let x = node.x() + NODE_WIDTH - 1;
    let y = node.y() + (NODE_HEIGHT / 2);
    Some((x, y))
}

/// Return panel-space center of a node input pin.
pub(crate) fn input_pin_center(node: &ProjectNode) -> Option<(i32, i32)> {
    if !node.kind().accepts_image_input() {
        return None;
    }
    let x = node.x();
    let y = node.y() + (NODE_HEIGHT / 2);
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

fn clamp_node_position(x: i32, y: i32, panel_width: usize, panel_height: usize) -> (i32, i32) {
    let max_x = (panel_width as i32 - NODE_WIDTH - 6).max(6);
    let max_y = (panel_height as i32 - NODE_HEIGHT - 6).max(6);
    (x.clamp(6, max_x), y.clamp(40, max_y))
}

fn distance_sq(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    let dx = ax - bx;
    let dy = ay - by;
    dx.saturating_mul(dx) + dy.saturating_mul(dy)
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
    fn connect_image_link_wires_top_to_output() {
        let mut project = GuiProject::new_empty(640, 480);
        let top = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(top, out));
        assert_eq!(project.edge_count(), 1);
        assert_eq!(project.output_source_kind(), Some(ProjectNodeKind::TexSolid));
        assert!(!project.connect_image_link(top, out));
    }

    #[test]
    fn pin_centers_follow_node_kind_capabilities() {
        let mut project = GuiProject::new_empty(640, 480);
        let top = project.add_node(ProjectNodeKind::TexSolid, 60, 70, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 70, 420, 480);
        let top_node = project.node(top).expect("top node must exist");
        let out_node = project.node(out).expect("output node must exist");
        assert!(output_pin_center(top_node).is_some());
        assert!(input_pin_center(top_node).is_none());
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

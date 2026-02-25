//! GUI project scaffolding and editable node model.
//!
//! The GUI currently starts with an empty project and supports adding and
//! moving nodes directly on the graph canvas.

use std::time::{SystemTime, UNIX_EPOCH};

/// Width of one graph node card in the editor canvas.
pub(crate) const NODE_WIDTH: i32 = 128;
/// Height of one graph node card in the editor canvas.
pub(crate) const NODE_HEIGHT: i32 = 44;

/// Minimal set of node kinds exposed by the Add Node menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectNodeKind {
    /// Basic TOP source/processor placeholder node.
    TopBasic,
    /// Final output node.
    Output,
}

impl ProjectNodeKind {
    /// Return true when this node kind belongs to TOP-like operators.
    pub(crate) const fn is_top_like(self) -> bool {
        matches!(self, Self::TopBasic)
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
}

fn clamp_node_position(x: i32, y: i32, panel_width: usize, panel_height: usize) -> (i32, i32) {
    let max_x = (panel_width as i32 - NODE_WIDTH - 6).max(6);
    let max_y = (panel_height as i32 - NODE_HEIGHT - 6).max(6);
    (x.clamp(6, max_x), y.clamp(40, max_y))
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
    use super::{GuiProject, ProjectNodeKind};

    #[test]
    fn empty_project_has_no_nodes() {
        let project = GuiProject::new_empty(640, 480);
        assert_eq!(project.node_count(), 0);
    }

    #[test]
    fn add_node_assigns_incrementing_ids() {
        let mut project = GuiProject::new_empty(640, 480);
        let a = project.add_node(ProjectNodeKind::TopBasic, 80, 80, 420, 480);
        let b = project.add_node(ProjectNodeKind::Output, 120, 120, 420, 480);
        assert_eq!(a, 1);
        assert_eq!(b, 2);
    }

    #[test]
    fn node_hit_test_uses_topmost_order() {
        let mut project = GuiProject::new_empty(640, 480);
        let a = project.add_node(ProjectNodeKind::TopBasic, 80, 80, 420, 480);
        let b = project.add_node(ProjectNodeKind::Output, 80, 80, 420, 480);
        assert_eq!(project.node_at(90, 90), Some(b));
        assert_ne!(project.node_at(90, 90), Some(a));
    }
}

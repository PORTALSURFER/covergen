//! GUI project scaffolding for TouchDesigner-style authoring.
//!
//! The runtime currently boots with an empty project each launch. This module
//! provides stable project data types so node-editing and persistence features
//! can be added incrementally without changing GUI wiring.

use std::time::{SystemTime, UNIX_EPOCH};

/// High-level operator family used by the node editor lanes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectNodeFamily {
    GenFx,
    Chop,
    Sop,
    Top,
    Output,
}

/// Canonical lane ordering used by the node editor and future project tooling.
pub(crate) const ALL_NODE_FAMILIES: [ProjectNodeFamily; 5] = [
    ProjectNodeFamily::GenFx,
    ProjectNodeFamily::Chop,
    ProjectNodeFamily::Sop,
    ProjectNodeFamily::Top,
    ProjectNodeFamily::Output,
];

/// One project node record used by the GUI editor model.
#[derive(Clone, Debug)]
pub(crate) struct ProjectNode {
    pub(crate) id: u32,
    pub(crate) label: String,
    pub(crate) family: ProjectNodeFamily,
    pub(crate) inputs: Vec<u32>,
}

/// In-memory GUI project model.
#[derive(Clone, Debug)]
pub(crate) struct GuiProject {
    pub(crate) name: String,
    pub(crate) preview_width: u32,
    pub(crate) preview_height: u32,
    pub(crate) nodes: Vec<ProjectNode>,
}

impl GuiProject {
    /// Create a fresh empty project sized for the active preview canvas.
    pub(crate) fn new_empty(preview_width: u32, preview_height: u32) -> Self {
        Self {
            name: next_project_name(),
            preview_width,
            preview_height,
            nodes: Vec::new(),
        }
    }

    /// Return the current node count for GUI status and summaries.
    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }
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
    use super::GuiProject;

    #[test]
    fn empty_project_has_no_nodes() {
        let project = GuiProject::new_empty(640, 480);
        assert_eq!(project.node_count(), 0);
        assert_eq!(project.preview_width, 640);
        assert_eq!(project.preview_height, 480);
    }
}

//! Scene-level routing context helpers shared by edge/param wire builders.

use crate::gui::geometry::Rect;
use crate::gui::project::{GuiProject, ResourceKind, NODE_WIDTH};
use crate::gui::scene::wire_route;
use crate::gui::state::{PreviewState, WireDragState};

/// Collect graph-space node obstacle rectangles used by route search.
pub(super) fn collect_graph_node_obstacles(project: &GuiProject) -> Vec<wire_route::NodeObstacle> {
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
pub(super) fn param_route_obstacle_epoch(
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
pub(super) fn edge_route_obstacle_epoch(project: &GuiProject) -> u64 {
    project.invalidation().nodes
}

/// Return output resource type for the active wire-drag source node.
pub(super) fn wire_drag_source_kind(
    project: &GuiProject,
    wire: WireDragState,
) -> Option<ResourceKind> {
    let source = project.node(wire.source_node_id)?;
    source.kind().output_resource_kind()
}

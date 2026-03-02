//! Node drag, wire-insert hover, and overlap-snap behavior.

use crate::gui::scene::wire_route;
use crate::gui::state::DragState;

use super::{
    graph_point_to_panel, input_pin_center, inside_panel, node_expand_toggle_rect,
    output_pin_center, GuiProject, HoverInsertLink, InputSnapshot, InteractionPanelContext,
    PreviewState, NODE_OVERLAP_SNAP_GAP_PX, NODE_WIDTH,
};

/// Handle drag start/update/release interactions for nodes.
pub(super) fn handle_drag_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    ctx: InteractionPanelContext,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    if input.left_clicked {
        changed |= begin_drag_if_node_hit(input, project, ctx, state);
    }
    let dragged_node_ids = state
        .drag
        .map(|drag| drag_selection_node_ids(state, drag.node_id))
        .unwrap_or_default();
    let is_group_drag = dragged_node_ids.len() > 1;
    if !input.left_down {
        if let Some(drag) = state.drag {
            if !is_group_drag {
                if let Some(link) =
                    resolve_insert_link_on_release(input, project, ctx, state, drag.node_id)
                {
                    changed |= project.insert_node_on_primary_link(
                        drag.node_id,
                        link.source_id,
                        link.target_id,
                    );
                }
                changed |= snap_dragged_node_out_of_overlap(project, drag, ctx);
            }
        }
        changed |= state.drag.is_some();
        state.drag = None;
        changed |= state.hover_insert_link.take().is_some();
        return changed;
    }
    let Some(drag) = state.drag else {
        changed |= state.hover_insert_link.take().is_some();
        return changed;
    };
    let Some((mx, my)) = input.mouse_pos else {
        changed |= state.hover_insert_link.take().is_some();
        return changed;
    };
    if !inside_panel(mx, my, ctx.panel_width, ctx.panel_height) {
        changed |= state.hover_insert_link.take().is_some();
        return changed;
    }
    let (graph_x, graph_y) = super::screen_to_graph(mx, my, state);
    if is_group_drag {
        changed |= move_drag_selection_by_anchor_delta(
            project,
            drag,
            dragged_node_ids.as_slice(),
            graph_x,
            graph_y,
            ctx,
        );
        changed |= state.hover_insert_link.take().is_some();
    } else {
        let node_x = graph_x - drag.offset_x;
        let node_y = graph_y - drag.offset_y;
        changed |= project.move_node(
            drag.node_id,
            node_x,
            node_y,
            ctx.panel_width,
            ctx.panel_height,
        );
        let next_insert_hover =
            hover_insert_link_at_cursor(project, ctx, state, mx, my, drag.node_id);
        if state.hover_insert_link != next_insert_hover {
            state.hover_insert_link = next_insert_hover;
            changed = true;
        }
    }
    changed
}

/// Return drag group ids for one anchor node.
pub(super) fn drag_selection_node_ids(state: &PreviewState, anchor_node_id: u32) -> Vec<u32> {
    if state.selected_nodes.len() > 1 && state.selected_nodes.contains(&anchor_node_id) {
        return state.selected_nodes.clone();
    }
    vec![anchor_node_id]
}

/// Move selected drag nodes by the anchor node cursor delta.
pub(super) fn move_drag_selection_by_anchor_delta(
    project: &mut GuiProject,
    drag: DragState,
    dragged_node_ids: &[u32],
    graph_x: i32,
    graph_y: i32,
    ctx: InteractionPanelContext,
) -> bool {
    let Some(anchor_before) = project.node(drag.node_id) else {
        return false;
    };
    let anchor_before_x = anchor_before.x();
    let anchor_before_y = anchor_before.y();
    let desired_x = graph_x - drag.offset_x;
    let desired_y = graph_y - drag.offset_y;
    let mut changed = project.move_node(
        drag.node_id,
        desired_x,
        desired_y,
        ctx.panel_width,
        ctx.panel_height,
    );
    let Some(anchor_after) = project.node(drag.node_id) else {
        return changed;
    };
    let dx = anchor_after.x() - anchor_before_x;
    let dy = anchor_after.y() - anchor_before_y;
    if dx == 0 && dy == 0 {
        return changed;
    }
    changed |= project.move_nodes_by_delta_excluding(
        dragged_node_ids,
        Some(drag.node_id),
        dx,
        dy,
        ctx.panel_width,
        ctx.panel_height,
    );
    changed
}

/// Begin a node drag if the pointer clicked a node card.
pub(super) fn begin_drag_if_node_hit(
    input: &InputSnapshot,
    project: &mut GuiProject,
    ctx: InteractionPanelContext,
    state: &mut PreviewState,
) -> bool {
    let Some((mx, my)) = input.mouse_pos else {
        return false;
    };
    if !inside_panel(mx, my, ctx.panel_width, ctx.panel_height) {
        return false;
    }
    let (graph_x, graph_y) = super::screen_to_graph(mx, my, state);
    let Some(node_id) = project.node_at(graph_x, graph_y) else {
        let changed = state.drag.is_some();
        state.drag = None;
        state.hover_insert_link = None;
        return changed;
    };
    let Some((node_x, node_y, toggle_rect)) = project
        .node(node_id)
        .map(|node| (node.x(), node.y(), node_expand_toggle_rect(node)))
    else {
        let changed = state.drag.is_some();
        state.drag = None;
        return changed;
    };
    if let Some(toggle_rect) = toggle_rect {
        if toggle_rect.contains(graph_x, graph_y) {
            state.drag = None;
            state.active_node = Some(node_id);
            state.param_edit = None;
            state.param_dropdown = None;
            state.hover_dropdown_item = None;
            return project.toggle_node_expanded(node_id, ctx.panel_width, ctx.panel_height);
        }
    }
    if state.drag.map(|drag| drag.node_id) == Some(node_id) {
        return false;
    }
    state.drag = Some(DragState {
        node_id,
        offset_x: graph_x - node_x,
        offset_y: graph_y - node_y,
        origin_x: node_x,
        origin_y: node_y,
    });
    state.active_node = Some(node_id);
    state.hover_insert_link = None;
    state.param_dropdown = None;
    state.hover_dropdown_item = None;
    true
}

/// Snap one dragged node beside an overlapped node using drag origin side.
pub(super) fn snap_dragged_node_out_of_overlap(
    project: &mut GuiProject,
    drag: DragState,
    ctx: InteractionPanelContext,
) -> bool {
    let Some(dragged) = project.node(drag.node_id) else {
        return false;
    };
    let dragged_rect = (dragged.x(), dragged.y(), NODE_WIDTH, dragged.card_height());
    let dragged_center = (
        dragged_rect.0 + dragged_rect.2 / 2,
        dragged_rect.1 + dragged_rect.3 / 2,
    );
    let mut best_target: Option<(u32, i64)> = None;
    for node in project.nodes() {
        if node.id() == drag.node_id {
            continue;
        }
        let target_rect = (node.x(), node.y(), NODE_WIDTH, node.card_height());
        if !rects_overlap_strict(dragged_rect, target_rect) {
            continue;
        }
        let target_center = (
            target_rect.0 + target_rect.2 / 2,
            target_rect.1 + target_rect.3 / 2,
        );
        let dx = (dragged_center.0 - target_center.0) as i64;
        let dy = (dragged_center.1 - target_center.1) as i64;
        let dist_sq = dx * dx + dy * dy;
        if best_target
            .as_ref()
            .map(|(_, best_dist)| dist_sq < *best_dist)
            .unwrap_or(true)
        {
            best_target = Some((node.id(), dist_sq));
        }
    }
    let Some((target_id, _)) = best_target else {
        return false;
    };
    let Some(target) = project.node(target_id) else {
        return false;
    };
    let target_rect = (target.x(), target.y(), NODE_WIDTH, target.card_height());
    let target_center = (
        target_rect.0 + target_rect.2 / 2,
        target_rect.1 + target_rect.3 / 2,
    );
    let origin_center = (
        drag.origin_x + dragged_rect.2 / 2,
        drag.origin_y + dragged_rect.3 / 2,
    );
    let dx = origin_center.0 - target_center.0;
    let dy = origin_center.1 - target_center.1;
    let (next_x, next_y) = if dx.abs() >= dy.abs() {
        if dx <= 0 {
            (
                target_rect.0 - dragged_rect.2 - NODE_OVERLAP_SNAP_GAP_PX,
                dragged_rect.1,
            )
        } else {
            (
                target_rect.0 + target_rect.2 + NODE_OVERLAP_SNAP_GAP_PX,
                dragged_rect.1,
            )
        }
    } else if dy <= 0 {
        (
            dragged_rect.0,
            target_rect.1 - dragged_rect.3 - NODE_OVERLAP_SNAP_GAP_PX,
        )
    } else {
        (
            dragged_rect.0,
            target_rect.1 + target_rect.3 + NODE_OVERLAP_SNAP_GAP_PX,
        )
    };
    project.move_node(
        drag.node_id,
        next_x,
        next_y,
        ctx.panel_width,
        ctx.panel_height,
    )
}

/// Return true when two rectangles overlap with positive area.
pub(super) fn rects_overlap_strict(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    let ax1 = ax + aw;
    let ay1 = ay + ah;
    let bx1 = bx + bw;
    let by1 = by + bh;
    ax < bx1 && ax1 > bx && ay < by1 && ay1 > by
}

/// Resolve one hovered wire insertion candidate at cursor position.
pub(super) fn hover_insert_link_at_cursor(
    project: &GuiProject,
    ctx: InteractionPanelContext,
    state: &PreviewState,
    cursor_x: i32,
    cursor_y: i32,
    dragged_node_id: u32,
) -> Option<HoverInsertLink> {
    let mut best: Option<(HoverInsertLink, f32)> = None;
    let obstacles = collect_hover_obstacles(project, dragged_node_id);
    let route_map = wire_route::RouteObstacleMap::from_obstacles(obstacles.as_slice());
    let query = HoverInsertQuery {
        cursor_x,
        cursor_y,
        threshold_sq: (super::INSERT_WIRE_HOVER_RADIUS_PX * super::INSERT_WIRE_HOVER_RADIUS_PX)
            as f32,
        dragged_node_id,
    };
    let (view_x0, view_y0, view_x1, view_y1) = super::marquee::panel_graph_rect(ctx, state);
    let target_ids = project.node_ids_overlapping_graph_rect(view_x0, view_y0, view_x1, view_y1);
    for target_id in target_ids.iter().copied() {
        consider_hover_insert_candidate(project, state, query, &route_map, target_id, &mut best);
    }
    if best.is_none() && target_ids.len() < project.node_count() {
        for target in project.nodes() {
            consider_hover_insert_candidate(
                project,
                state,
                query,
                &route_map,
                target.id(),
                &mut best,
            );
        }
    }
    best.map(|(link, _)| link)
}

fn collect_hover_obstacles(
    project: &GuiProject,
    excluded_node_id: u32,
) -> Vec<wire_route::NodeObstacle> {
    let mut out = Vec::new();
    for node in project.nodes() {
        if node.id() == excluded_node_id {
            continue;
        }
        out.push(wire_route::NodeObstacle {
            rect: crate::gui::geometry::Rect::new(
                node.x(),
                node.y(),
                NODE_WIDTH,
                node.card_height(),
            ),
        });
    }
    out
}

#[derive(Clone, Copy, Debug)]
struct HoverInsertQuery {
    cursor_x: i32,
    cursor_y: i32,
    threshold_sq: f32,
    dragged_node_id: u32,
}

fn consider_hover_insert_candidate(
    project: &GuiProject,
    state: &PreviewState,
    query: HoverInsertQuery,
    route_map: &wire_route::RouteObstacleMap,
    target_id: u32,
    best: &mut Option<(HoverInsertLink, f32)>,
) {
    let Some(target) = project.node(target_id) else {
        return;
    };
    let Some(source_id) = project.input_source_node_id(target_id) else {
        return;
    };
    if !can_insert_dragged_node_on_link(project, query.dragged_node_id, source_id, target_id) {
        return;
    }
    let Some(source) = project.node(source_id) else {
        return;
    };
    let Some((from_x, from_y)) = output_pin_center(source) else {
        return;
    };
    let Some((to_x, to_y)) = input_pin_center(target) else {
        return;
    };
    let route_graph = wire_route::route_wire_path_with_tails_with_map(
        wire_route::RouteEndpoint {
            point: (from_x, from_y),
            corridor_dir: wire_route::RouteDirection::East,
        },
        wire_route::RouteEndpoint {
            point: (to_x, to_y),
            corridor_dir: wire_route::RouteDirection::West,
        },
        route_map,
    );
    let dist_sq = route_graph
        .windows(2)
        .map(|segment| {
            let (ax, ay) = graph_point_to_panel(segment[0].0, segment[0].1, state);
            let (bx, by) = graph_point_to_panel(segment[1].0, segment[1].1, state);
            point_to_segment_distance_sq(
                query.cursor_x as f32,
                query.cursor_y as f32,
                ax as f32,
                ay as f32,
                bx as f32,
                by as f32,
            )
        })
        .fold(f32::MAX, f32::min);
    if !dist_sq.is_finite() {
        return;
    }
    if dist_sq > query.threshold_sq {
        return;
    }
    let candidate = HoverInsertLink {
        source_id,
        target_id,
    };
    if best
        .as_ref()
        .map(|(_, best_sq)| dist_sq < *best_sq)
        .unwrap_or(true)
    {
        *best = Some((candidate, dist_sq));
    }
}

/// Return true when `dragged_node_id` can be inserted on `source -> target`.
pub(super) fn can_insert_dragged_node_on_link(
    project: &GuiProject,
    dragged_node_id: u32,
    source_id: u32,
    target_id: u32,
) -> bool {
    if dragged_node_id == source_id || dragged_node_id == target_id || source_id == target_id {
        return false;
    }
    let Some(dragged) = project.node(dragged_node_id) else {
        return false;
    };
    let Some(source) = project.node(source_id) else {
        return false;
    };
    let Some(target) = project.node(target_id) else {
        return false;
    };
    if target.inputs().first().copied() != Some(source_id) {
        return false;
    }
    let Some(source_out_kind) = source.kind().output_resource_kind() else {
        return false;
    };
    let Some(dragged_in_kind) = dragged.kind().input_resource_kind() else {
        return false;
    };
    let Some(dragged_out_kind) = dragged.kind().output_resource_kind() else {
        return false;
    };
    let Some(target_in_kind) = target.kind().input_resource_kind() else {
        return false;
    };
    source_out_kind == dragged_in_kind && dragged_out_kind == target_in_kind
}

/// Resolve insertion candidate on drag release from hover cache or hit-test.
pub(super) fn resolve_insert_link_on_release(
    input: &InputSnapshot,
    project: &GuiProject,
    ctx: InteractionPanelContext,
    state: &PreviewState,
    dragged_node_id: u32,
) -> Option<HoverInsertLink> {
    if let Some(link) = state.hover_insert_link {
        return Some(link);
    }
    let (mx, my) = input.mouse_pos?;
    if !inside_panel(mx, my, ctx.panel_width, ctx.panel_height) {
        return None;
    }
    hover_insert_link_at_cursor(project, ctx, state, mx, my, dragged_node_id)
}

/// Return squared distance from point `p` to line segment `ab`.
pub(super) fn point_to_segment_distance_sq(
    px: f32,
    py: f32,
    ax: f32,
    ay: f32,
    bx: f32,
    by: f32,
) -> f32 {
    let abx = bx - ax;
    let aby = by - ay;
    let apx = px - ax;
    let apy = py - ay;
    let ab_len_sq = abx * abx + aby * aby;
    if ab_len_sq <= f32::EPSILON {
        return apx * apx + apy * apy;
    }
    let t = ((apx * abx + apy * aby) / ab_len_sq).clamp(0.0, 1.0);
    let cx = ax + abx * t;
    let cy = ay + aby * t;
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy
}

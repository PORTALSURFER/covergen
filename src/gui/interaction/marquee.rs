//! Right-button selection and marquee helpers.

use super::{
    editor_panel_height, inside_panel, screen_to_graph, GuiProject, InputSnapshot,
    InteractionPanelContext, PreviewState, RightMarqueeState,
};

/// Handle right-click selection and marquee drag behavior.
#[allow(unused_assignments)]
pub(super) fn handle_right_selection(
    input: &InputSnapshot,
    project: &mut GuiProject,
    ctx: InteractionPanelContext,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    if input.right_clicked
        && !input.alt_down
        && !state.menu.open
        && !state.main_menu.open
        && !state.export_menu.open
    {
        let Some((mx, my)) = input.mouse_pos else {
            return false;
        };
        if !inside_panel(mx, my, ctx.panel_width, ctx.panel_height) {
            return false;
        }
        let (graph_x, graph_y) = screen_to_graph(mx, my, state);
        if let Some(node_id) = project.node_at(graph_x, graph_y) {
            if let Some(param_index) = project.param_row_at(node_id, graph_x, graph_y) {
                if project.param_value_box_contains(node_id, param_index, graph_x, graph_y)
                    && project
                        .param_link_source_for_param(node_id, param_index)
                        .is_some()
                {
                    changed |= project.disconnect_param_link_from_param(node_id, param_index);
                    changed |= set_single_selection(state, node_id);
                    state.active_node = Some(node_id);
                    state.right_marquee = None;
                    state.drag = None;
                    state.wire_drag = None;
                    state.hover_param_target = None;
                    state.param_edit = None;
                    state.param_dropdown = None;
                    state.hover_dropdown_item = None;
                    return true;
                }
            }
            changed |= set_single_selection(state, node_id);
            state.active_node = Some(node_id);
            state.right_marquee = None;
            state.drag = None;
            state.wire_drag = None;
            state.hover_param_target = None;
            state.param_edit = None;
            state.param_dropdown = None;
            state.hover_dropdown_item = None;
            return true;
        }
        state.right_marquee = Some(RightMarqueeState {
            start_x: mx,
            start_y: my,
            cursor_x: mx,
            cursor_y: my,
        });
        state.drag = None;
        state.wire_drag = None;
        state.hover_param_target = None;
        state.param_edit = None;
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return true;
    }
    let Some(mut marquee) = state.right_marquee else {
        return changed;
    };
    if let Some((mx, my)) = input.mouse_pos {
        if marquee.cursor_x != mx || marquee.cursor_y != my {
            marquee.cursor_x = mx;
            marquee.cursor_y = my;
            changed = true;
        }
    }
    let moved = marquee_moved(marquee);
    if moved {
        let selected = collect_marquee_nodes(project, state, marquee);
        changed |= set_multi_selection(state, selected);
    }
    if !input.right_down {
        if !moved {
            changed |= clear_selection(state);
        }
        state.right_marquee = None;
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return true;
    }
    state.right_marquee = Some(marquee);
    changed
}

/// Return whether marquee movement exceeds click dead-zone threshold.
pub(super) fn marquee_moved(marquee: RightMarqueeState) -> bool {
    (marquee.cursor_x - marquee.start_x).abs() > 4 || (marquee.cursor_y - marquee.start_y).abs() > 4
}

/// Collect node ids intersecting the current marquee rectangle.
pub(super) fn collect_marquee_nodes(
    project: &GuiProject,
    state: &PreviewState,
    marquee: RightMarqueeState,
) -> Vec<u32> {
    let rect = screen_rect_to_graph_rect(
        marquee.start_x,
        marquee.start_y,
        marquee.cursor_x,
        marquee.cursor_y,
        state,
    );
    project.node_ids_overlapping_graph_rect(rect.0, rect.1, rect.2, rect.3)
}

/// Convert one screen-space drag rectangle into graph coordinates.
pub(super) fn screen_rect_to_graph_rect(
    sx0: i32,
    sy0: i32,
    sx1: i32,
    sy1: i32,
    state: &PreviewState,
) -> (i32, i32, i32, i32) {
    let (gx0, gy0) = screen_to_graph(sx0, sy0, state);
    let (gx1, gy1) = screen_to_graph(sx1, sy1, state);
    (gx0.min(gx1), gy0.min(gy1), gx0.max(gx1), gy0.max(gy1))
}

/// Convert current editor panel bounds to one graph-space rectangle.
pub(super) fn panel_graph_rect(
    ctx: InteractionPanelContext,
    state: &PreviewState,
) -> (i32, i32, i32, i32) {
    let max_x = ctx.panel_width.saturating_sub(1) as i32;
    let max_y = editor_panel_height(ctx.panel_height).saturating_sub(1) as i32;
    screen_rect_to_graph_rect(0, 0, max_x, max_y, state)
}

/// Return true when two axis-aligned rectangles overlap.
#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn rects_overlap(
    ax0: i32,
    ay0: i32,
    ax1: i32,
    ay1: i32,
    bx0: i32,
    by0: i32,
    bx1: i32,
    by1: i32,
) -> bool {
    ax0 <= bx1 && ax1 >= bx0 && ay0 <= by1 && ay1 >= by0
}

/// Replace selection with one node id.
pub(super) fn set_single_selection(state: &mut PreviewState, node_id: u32) -> bool {
    if state.selected_nodes.len() == 1 && state.selected_nodes[0] == node_id {
        return false;
    }
    state.selected_nodes.clear();
    state.selected_nodes.push(node_id);
    true
}

/// Replace selection with sorted unique node ids.
pub(super) fn set_multi_selection(state: &mut PreviewState, mut nodes: Vec<u32>) -> bool {
    nodes.sort_unstable();
    nodes.dedup();
    if state.selected_nodes == nodes {
        return false;
    }
    state.selected_nodes = nodes;
    state.active_node = state.selected_nodes.first().copied();
    true
}

/// Clear active multi-selection and active node.
pub(super) fn clear_selection(state: &mut PreviewState) -> bool {
    if state.selected_nodes.is_empty() && state.active_node.is_none() {
        return false;
    }
    state.selected_nodes.clear();
    state.active_node = None;
    true
}

//! Wire dragging, linking, and drop-target resolution.

use super::{
    collapsed_param_entry_pin_center, inside_panel, node_param_row_rect, pin_hit_radius_world,
    screen_to_graph, GuiProject, HoverParamTarget, InputSnapshot, InteractionPanelContext,
    PreviewState, ResourceKind, WireDragState,
};

/// Handle wire drag updates and release linking.
pub(super) fn handle_wire_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    ctx: InteractionPanelContext,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    if input.left_clicked {
        changed |= begin_wire_drag_if_pin_hit(input, project, ctx, state);
    }
    let Some(mut wire) = state.wire_drag else {
        return changed;
    };
    if let Some((mx, my)) = input.mouse_pos {
        wire.cursor_x = mx;
        wire.cursor_y = my;
    }
    if !input.left_down {
        match wire_drag_source_kind(project, wire) {
            Some(ResourceKind::Signal) => {
                if let Some(target) = state.hover_param_target {
                    let _ = project.connect_signal_link_to_param(
                        wire.source_node_id,
                        target.node_id,
                        target.param_index,
                    );
                }
            }
            Some(ResourceKind::Texture2D) => {
                if let Some(target) =
                    resolve_texture_param_target_on_release(input, project, ctx, state, wire)
                {
                    let _ = project.connect_texture_link_to_param(
                        wire.source_node_id,
                        target.node_id,
                        target.param_index,
                    );
                } else if let Some(target_id) = state.hover_input_pin {
                    let _ = project.connect_image_link(wire.source_node_id, target_id);
                }
            }
            _ => {
                if let Some(target_id) = state.hover_input_pin {
                    let _ = project.connect_image_link(wire.source_node_id, target_id);
                }
            }
        }
        state.wire_drag = None;
        state.hover_param_target = None;
        return true;
    }
    changed |= state.wire_drag.map(|drag| drag.cursor_x) != Some(wire.cursor_x);
    changed |= state.wire_drag.map(|drag| drag.cursor_y) != Some(wire.cursor_y);
    state.wire_drag = Some(wire);
    changed
}

/// Resolve texture-parameter drop target on release.
pub(super) fn resolve_texture_param_target_on_release(
    input: &InputSnapshot,
    project: &GuiProject,
    ctx: InteractionPanelContext,
    state: &PreviewState,
    wire: WireDragState,
) -> Option<HoverParamTarget> {
    if input.mouse_pos.is_none() {
        return state.hover_param_target;
    }
    let (mx, my) = input.mouse_pos.unwrap_or((wire.cursor_x, wire.cursor_y));
    if !inside_panel(mx, my, ctx.panel_width, ctx.panel_height) {
        return None;
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
    if let Some(target) = state.hover_param_target {
        let node = project.node(target.node_id)?;
        if let Some(row) = node_param_row_rect(node, target.param_index) {
            if row.contains(graph_x, graph_y)
                && project.param_accepts_texture_link(target.node_id, target.param_index)
            {
                return Some(target);
            }
        } else if let Some((pin_x, pin_y)) = collapsed_param_entry_pin_center(node) {
            let pin_radius = pin_hit_radius_world(state);
            if distance_sq(graph_x, graph_y, pin_x, pin_y) <= pin_radius.saturating_mul(pin_radius)
                && project.param_accepts_texture_link(target.node_id, target.param_index)
            {
                return Some(target);
            }
        }
    }
    let node_id = project.node_at(graph_x, graph_y)?;
    let pin_radius = pin_hit_radius_world(state);
    if project.input_pin_at(graph_x, graph_y, pin_radius, Some(wire.source_node_id))
        == Some(node_id)
    {
        return None;
    }
    let param_index = project.param_row_at(node_id, graph_x, graph_y)?;
    if !project.param_accepts_texture_link(node_id, param_index) {
        return None;
    }
    Some(HoverParamTarget {
        node_id,
        param_index,
    })
}

fn distance_sq(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    let dx = ax - bx;
    let dy = ay - by;
    dx.saturating_mul(dx) + dy.saturating_mul(dy)
}

/// Begin wire drag when the pointer pressed an output pin.
pub(super) fn begin_wire_drag_if_pin_hit(
    input: &InputSnapshot,
    project: &GuiProject,
    ctx: InteractionPanelContext,
    state: &mut PreviewState,
) -> bool {
    let Some((mx, my)) = input.mouse_pos else {
        return false;
    };
    if !inside_panel(mx, my, ctx.panel_width, ctx.panel_height) {
        return false;
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
    let pin_radius = pin_hit_radius_world(state);
    let Some(source_node_id) = project.output_pin_at(graph_x, graph_y, pin_radius) else {
        return false;
    };
    state.drag = None;
    state.hover_insert_link = None;
    state.active_node = Some(source_node_id);
    state.hover_param_target = None;
    state.param_dropdown = None;
    state.hover_dropdown_item = None;
    state.wire_drag = Some(WireDragState {
        source_node_id,
        cursor_x: mx,
        cursor_y: my,
    });
    true
}

/// Return output resource kind of current drag source node.
pub(super) fn wire_drag_source_kind(
    project: &GuiProject,
    wire: WireDragState,
) -> Option<ResourceKind> {
    let source = project.node(wire.source_node_id)?;
    source.kind().output_resource_kind()
}

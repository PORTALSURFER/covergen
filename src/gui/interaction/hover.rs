//! Hover-state updates and bind-drag auto-collapse behavior.

use super::{
    inside_panel, pin_hit_radius_world, screen_to_graph, GuiProject, HoverParamTarget,
    InputSnapshot, InteractionPanelContext, PreviewState, ResourceKind,
};

/// Collapse all nodes auto-expanded during wire-binding drag.
pub(super) fn collapse_auto_expanded_binding_nodes(
    project: &mut GuiProject,
    ctx: InteractionPanelContext,
    state: &mut PreviewState,
) -> bool {
    if state.auto_expanded_binding_nodes.is_empty() {
        return false;
    }
    let mut changed = false;
    for node_id in state.auto_expanded_binding_nodes.drain(..) {
        changed |= project.collapse_node(node_id, ctx.panel_width, ctx.panel_height);
    }
    changed
}

/// Collapse auto-expanded bind nodes except one optional kept id.
pub(super) fn collapse_auto_expanded_binding_nodes_except(
    project: &mut GuiProject,
    ctx: InteractionPanelContext,
    state: &mut PreviewState,
    keep_node_id: Option<u32>,
) -> bool {
    if state.auto_expanded_binding_nodes.is_empty() {
        return false;
    }
    let mut changed = false;
    let mut kept = Vec::with_capacity(state.auto_expanded_binding_nodes.len());
    for node_id in state.auto_expanded_binding_nodes.drain(..) {
        if Some(node_id) == keep_node_id {
            kept.push(node_id);
            continue;
        }
        changed |= project.collapse_node(node_id, ctx.panel_width, ctx.panel_height);
    }
    state.auto_expanded_binding_nodes = kept;
    changed
}

/// Update all hover targets for one input frame.
pub(super) fn update_hover_state(
    input: &InputSnapshot,
    project: &mut GuiProject,
    ctx: InteractionPanelContext,
    state: &mut PreviewState,
) -> bool {
    let prev_hover_node = state.hover_node;
    let prev_hover_output = state.hover_output_pin;
    let prev_hover_input = state.hover_input_pin;
    let prev_hover_param_target = state.hover_param_target;
    let prev_hover_dropdown_item = state.hover_dropdown_item;
    let prev_hover_item = state.hover_menu_item;
    let prev_hover_main_item = state.hover_main_menu_item;
    let prev_hover_export_item = state.hover_export_menu_item;
    let prev_hover_export_close = state.hover_export_menu_close;
    state.hover_node = None;
    state.hover_output_pin = None;
    state.hover_input_pin = None;
    state.hover_param_target = None;
    state.hover_dropdown_item = None;
    state.hover_menu_item = None;
    state.hover_main_menu_item = None;
    state.hover_export_menu_item = None;
    state.hover_export_menu_close = false;
    let param_bind_drag_kind = state
        .wire_drag
        .and_then(|wire| super::wire::wire_drag_source_kind(project, wire))
        .filter(|kind| matches!(kind, ResourceKind::Signal | ResourceKind::Texture2D));

    let Some((mx, my)) = input.mouse_pos else {
        let mut changed = prev_hover_node.is_some()
            || prev_hover_output.is_some()
            || prev_hover_input.is_some()
            || prev_hover_param_target.is_some()
            || prev_hover_dropdown_item.is_some()
            || prev_hover_item.is_some()
            || prev_hover_main_item.is_some()
            || prev_hover_export_item.is_some()
            || prev_hover_export_close;
        if param_bind_drag_kind.is_some() {
            changed |= collapse_auto_expanded_binding_nodes_except(project, ctx, state, None);
        }
        return changed;
    };
    if !inside_panel(mx, my, ctx.panel_width, ctx.panel_height) {
        let mut changed = prev_hover_node.is_some()
            || prev_hover_output.is_some()
            || prev_hover_input.is_some()
            || prev_hover_param_target.is_some()
            || prev_hover_dropdown_item.is_some()
            || prev_hover_item.is_some()
            || prev_hover_main_item.is_some()
            || prev_hover_export_item.is_some()
            || prev_hover_export_close;
        if param_bind_drag_kind.is_some() {
            changed |= collapse_auto_expanded_binding_nodes_except(project, ctx, state, None);
        }
        return changed;
    }
    if state.menu.open {
        state.hover_menu_item = state.menu.item_at(mx, my);
        let mut changed = state.hover_menu_item != prev_hover_item
            || prev_hover_node.is_some()
            || prev_hover_output.is_some()
            || prev_hover_input.is_some()
            || prev_hover_param_target.is_some()
            || prev_hover_dropdown_item.is_some()
            || prev_hover_main_item.is_some()
            || prev_hover_export_item.is_some()
            || prev_hover_export_close;
        if param_bind_drag_kind.is_some() {
            changed |= collapse_auto_expanded_binding_nodes_except(project, ctx, state, None);
        }
        return changed;
    }
    if state.main_menu.open || state.export_menu.open {
        if state.main_menu.open {
            state.hover_main_menu_item = state.main_menu.item_at(mx, my);
        }
        if state.export_menu.open && state.export_menu_drag.is_none() {
            state.hover_export_menu_item = state.export_menu.item_at(mx, my);
            state.hover_export_menu_close = state.export_menu.close_button_rect().contains(mx, my);
        }
        let mut changed = state.hover_main_menu_item != prev_hover_main_item
            || state.hover_export_menu_item != prev_hover_export_item
            || state.hover_export_menu_close != prev_hover_export_close
            || prev_hover_node.is_some()
            || prev_hover_output.is_some()
            || prev_hover_input.is_some()
            || prev_hover_param_target.is_some()
            || prev_hover_dropdown_item.is_some()
            || prev_hover_item.is_some();
        if param_bind_drag_kind.is_some() {
            changed |= collapse_auto_expanded_binding_nodes_except(project, ctx, state, None);
        }
        return changed;
    }
    if state.param_dropdown.is_some() {
        state.hover_dropdown_item =
            super::param_edit::dropdown_option_at_cursor(project, state, mx, my);
        return state.hover_dropdown_item != prev_hover_dropdown_item
            || prev_hover_node.is_some()
            || prev_hover_output.is_some()
            || prev_hover_input.is_some()
            || prev_hover_param_target.is_some()
            || prev_hover_dropdown_item.is_some()
            || prev_hover_item.is_some()
            || prev_hover_main_item.is_some()
            || prev_hover_export_item.is_some()
            || prev_hover_export_close;
    }
    let mut param_bind_hover_changed = false;
    if state.wire_drag.is_some() {
        if let Some(bind_kind) = param_bind_drag_kind {
            let mut changed = false;
            let (graph_x, graph_y) = screen_to_graph(mx, my, state);
            let mut keep_auto_expanded_node = None;
            state.hover_node = project.node_at(graph_x, graph_y);
            if let Some(node_id) = state.hover_node {
                if state.auto_expanded_binding_nodes.contains(&node_id) {
                    keep_auto_expanded_node = Some(node_id);
                }
                if let Some(param_index) = project.param_row_at(node_id, graph_x, graph_y) {
                    let accepts_param = match bind_kind {
                        ResourceKind::Signal => {
                            project.param_accepts_signal_link(node_id, param_index)
                        }
                        ResourceKind::Texture2D => {
                            project.param_accepts_texture_link(node_id, param_index)
                        }
                        _ => false,
                    };
                    if accepts_param {
                        state.hover_param_target = Some(HoverParamTarget {
                            node_id,
                            param_index,
                        });
                    }
                }
            }
            changed |= collapse_auto_expanded_binding_nodes_except(
                project,
                ctx,
                state,
                keep_auto_expanded_node,
            );
            if bind_kind == ResourceKind::Signal || state.hover_param_target.is_some() {
                return changed
                    || state.hover_node != prev_hover_node
                    || prev_hover_output.is_some()
                    || prev_hover_input.is_some()
                    || state.hover_param_target != prev_hover_param_target
                    || prev_hover_dropdown_item.is_some()
                    || prev_hover_item.is_some()
                    || prev_hover_main_item.is_some()
                    || prev_hover_export_item.is_some()
                    || prev_hover_export_close;
            }
            param_bind_hover_changed = changed;
        }
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
    let pin_radius = pin_hit_radius_world(state);
    let disallow_source = state.wire_drag.map(|wire| wire.source_node_id);
    state.hover_output_pin = project.output_pin_at(graph_x, graph_y, pin_radius);
    state.hover_input_pin = project.input_pin_at(graph_x, graph_y, pin_radius, disallow_source);
    if state.hover_output_pin.is_some() || state.hover_input_pin.is_some() {
        return param_bind_hover_changed
            || state.hover_output_pin != prev_hover_output
            || state.hover_input_pin != prev_hover_input
            || prev_hover_node.is_some()
            || prev_hover_dropdown_item.is_some()
            || prev_hover_item.is_some()
            || prev_hover_param_target.is_some()
            || prev_hover_main_item.is_some()
            || prev_hover_export_item.is_some()
            || prev_hover_export_close;
    }
    state.hover_node = project.node_at(graph_x, graph_y);
    if state.hover_node.is_some() {
        state.active_node = state.hover_node;
    }
    param_bind_hover_changed
        || state.hover_node != prev_hover_node
        || prev_hover_output.is_some()
        || prev_hover_input.is_some()
        || prev_hover_dropdown_item.is_some()
        || prev_hover_item.is_some()
        || prev_hover_param_target.is_some()
        || prev_hover_main_item.is_some()
        || prev_hover_export_item.is_some()
        || prev_hover_export_close
}

//! Overlay toggle and parameter-edit interaction phase.

use super::*;

/// Apply overlay toggles plus parameter edit state transitions.
pub(super) fn apply_overlay_and_param_phase(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_ctx: InteractionPanelContext,
    state: &mut PreviewState,
    changed: &mut bool,
) -> Option<bool> {
    let add_menu_changed =
        handle_add_menu_toggle(input, panel_ctx.panel_width, panel_ctx.panel_height, state);
    *changed |= add_menu_changed;
    if add_menu_changed {
        state.invalidation.invalidate_overlays();
    }
    let main_menu_changed =
        handle_main_menu_toggle(input, panel_ctx.panel_width, panel_ctx.panel_height, state);
    *changed |= main_menu_changed;
    if main_menu_changed {
        state.invalidation.invalidate_overlays();
    }
    let hover_before = HoverInvalidationSnapshot::capture(state);
    let hover_changed = update_hover_state(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
    *changed |= hover_changed;
    if hover_changed {
        let hover_after = HoverInvalidationSnapshot::capture(state);
        if hover_before.nodes_changed(hover_after) {
            state.invalidation.invalidate_nodes();
        }
        if hover_before.overlays_changed(hover_after, state.wire_drag.is_some()) {
            state.invalidation.invalidate_overlays();
        }
    }
    let node_toggle_changed = handle_node_open_toggle(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
    *changed |= node_toggle_changed;
    if node_toggle_changed {
        state.invalidation.invalidate_overlays();
    }
    let (param_changed, param_click_consumed) = handle_param_edit_input(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
    *changed |= param_changed;
    if param_changed {
        state.invalidation.invalidate_nodes();
        state.invalidation.invalidate_overlays();
    }
    if param_click_consumed {
        state.drag = None;
        state.wire_drag = None;
        clear_param_hover_state(state);
        state.param_scrub = None;
        let _ = collapse_auto_expanded_binding_nodes_with_panel(project, panel_ctx, state);
        return Some(true);
    }
    if state.param_edit.is_some() {
        cancel_node_interaction_modes(state);
        *changed |= collapse_auto_expanded_binding_nodes_with_panel(project, panel_ctx, state);
        return Some(*changed);
    }
    None
}

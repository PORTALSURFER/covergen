//! Menu and graph-edit interaction phase.

use super::*;

/// Apply menu-specific and graph-edit interactions after modal/navigation phases.
pub(super) fn apply_menu_or_graph_phase(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_ctx: InteractionPanelContext,
    state: &mut PreviewState,
    changed: &mut bool,
) {
    if state.export_menu.open || state.main_menu.open {
        let menu_changed = handle_main_export_menu_input(
            input,
            panel_ctx.panel_width,
            panel_ctx.panel_height,
            state,
        );
        *changed |= menu_changed;
        if menu_changed {
            state.invalidation.invalidate_overlays();
            state.invalidation.invalidate_timeline();
        }
        return;
    }
    if state.menu.open {
        let menu_changed = handle_add_menu_input(
            input,
            project,
            panel_ctx.panel_width,
            panel_ctx.panel_height,
            state,
        );
        *changed |= menu_changed;
        if menu_changed {
            state.invalidation.invalidate_overlays();
        }
        return;
    }

    let delete_changed = handle_delete_selected_nodes(input, project, state);
    *changed |= delete_changed;
    if delete_changed {
        state.invalidation.invalidate_overlays();
    }
    let param_shortcut_changed = handle_parameter_shortcuts(input, project, state);
    *changed |= param_shortcut_changed;
    if param_shortcut_changed {
        state.invalidation.invalidate_nodes();
    }
    let wire_changed = handle_wire_input(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
    *changed |= wire_changed;
    if wire_changed {
        invalidate_graph_layers(state);
    }
    if state.wire_drag.is_none() {
        let drag_changed = handle_drag_input(
            input,
            project,
            panel_ctx.panel_width,
            panel_ctx.panel_height,
            state,
        );
        *changed |= drag_changed;
        if drag_changed {
            invalidate_graph_layers(state);
        }
        return;
    }
    state.drag = None;
}

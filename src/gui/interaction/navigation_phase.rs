//! Pan/zoom and navigation-phase input handling.

use super::*;

/// Run viewport-navigation phases (pan/zoom, scrubbing, link cut, marquee selection).
pub(super) fn apply_navigation_phase(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_ctx: InteractionPanelContext,
    state: &mut PreviewState,
    changed: &mut bool,
) -> InteractionPhaseControl {
    let zoom_before = state.zoom.to_bits();
    *changed |= handle_pan_zoom_and_focus(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
    if zoom_before != state.zoom.to_bits() {
        invalidate_graph_layers(state);
    }
    if state.pan_drag.is_some() {
        cancel_node_interaction_modes(state);
        let _ = collapse_auto_expanded_binding_nodes_with_panel(project, panel_ctx, state);
        return InteractionPhaseControl::Finish(true);
    }

    let scrub_code_before = state.debug_scrub_code;
    let (param_scrub_changed, param_scrub_active) = handle_alt_param_drag(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
    if state.debug_scrub_code != scrub_code_before {
        *changed = true;
        state.invalidation.invalidate_overlays();
    }
    *changed |= param_scrub_changed;
    if param_scrub_changed {
        state.invalidation.invalidate_nodes();
        state.invalidation.invalidate_overlays();
    }
    if param_scrub_active {
        state.drag = None;
        state.wire_drag = None;
        state.link_cut = None;
        state.hover_param_target = None;
        state.hover_param = None;
        clear_param_edit_state(state);
        clear_timeline_edit_state(state);
        let _ = collapse_auto_expanded_binding_nodes_with_panel(project, panel_ctx, state);
        return InteractionPhaseControl::Finish(true);
    }

    let cut_changed = handle_link_cut(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
    *changed |= cut_changed;
    if cut_changed {
        state.invalidation.invalidate_wires();
        state.invalidation.invalidate_overlays();
    }
    if state.link_cut.is_some() {
        cancel_node_interaction_modes(state);
        let _ = collapse_auto_expanded_binding_nodes_with_panel(project, panel_ctx, state);
        return InteractionPhaseControl::Finish(true);
    }

    let right_sel_changed = handle_right_selection(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
    *changed |= right_sel_changed;
    if right_sel_changed {
        state.invalidation.invalidate_nodes();
        state.invalidation.invalidate_overlays();
    }
    if state.right_marquee.is_some() {
        cancel_node_interaction_modes(state);
        let _ = collapse_auto_expanded_binding_nodes_with_panel(project, panel_ctx, state);
        return InteractionPhaseControl::Finish(true);
    }
    InteractionPhaseControl::Continue
}

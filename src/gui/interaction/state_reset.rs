//! Interaction state-reset helpers shared across phase handlers.

use crate::gui::state::{AddNodeMenuState, MainMenuState, PreviewState};

/// Clear drag/cut/pan/transient pointer interaction modes.
pub(super) fn clear_pointer_interactions(state: &mut PreviewState) {
    state.drag = None;
    state.wire_drag = None;
    state.link_cut = None;
    state.pan_drag = None;
    state.export_menu_drag = None;
    state.right_marquee = None;
}

/// Clear parameter hover targets and highlighted parameter UI rows.
pub(super) fn clear_param_hover_state(state: &mut PreviewState) {
    state.hover_param_target = None;
    state.hover_param = None;
    state.hover_alt_param = None;
}

/// Clear active in-place parameter and dropdown editors.
pub(super) fn clear_param_edit_state(state: &mut PreviewState) {
    state.param_edit = None;
    state.param_scrub = None;
    state.param_dropdown = None;
    state.hover_dropdown_item = None;
}

/// Clear active timeline text-edit widgets.
pub(super) fn clear_timeline_edit_state(state: &mut PreviewState) {
    state.timeline_bpm_edit = None;
    state.timeline_bar_edit = None;
}

/// Cancel drag/wire interaction modes plus parameter-hover/dropdown state.
pub(super) fn cancel_node_interaction_modes(state: &mut PreviewState) {
    state.drag = None;
    state.wire_drag = None;
    clear_param_hover_state(state);
    state.param_dropdown = None;
    state.param_scrub = None;
}

/// Close the add-node and main menu overlays.
pub(super) fn close_primary_menus(state: &mut PreviewState) {
    state.menu = AddNodeMenuState::closed();
    state.main_menu = MainMenuState::closed();
}

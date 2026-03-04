//! Pre-frame initialization and modal phase orchestration.

use super::*;
use crate::gui::help::{build_global_help_modal, build_node_help_modal, build_param_help_modal};

/// Apply pre-phase state updates shared by all interactions for one frame.
pub(super) fn begin_interaction_frame(
    config: &V2Config,
    input: &InputSnapshot,
    project: &mut GuiProject,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    if state.drag.is_none() && state.hover_insert_link.take().is_some() {
        changed = true;
    }
    if input.toggle_pause {
        state.paused = !state.paused;
        state.invalidation.invalidate_timeline();
        changed = true;
    }
    if input.new_project || state.request_new_project {
        state.request_new_project = false;
        *project = GuiProject::new_empty(config.width, config.height);
        state.frame_index = 0;
        state.timeline_accum_secs = 0.0;
        state.timeline_scrub_active = false;
        state.timeline_volume_drag_active = false;
        clear_pointer_interactions(state);
        clear_param_edit_state(state);
        clear_timeline_edit_state(state);
        state.selected_nodes.clear();
        state.pan_x = 0.0;
        state.pan_y = 0.0;
        state.zoom = 1.0;
        close_primary_menus(state);
        state.active_node = None;
        state.hover_node = None;
        state.hover_output_pin = None;
        state.hover_input_pin = None;
        clear_param_hover_state(state);
        state.hover_insert_link = None;
        state.auto_expanded_binding_nodes.clear();
        state.hover_menu_item = None;
        state.hover_main_menu_item = None;
        state.hover_export_menu_item = None;
        state.hover_export_menu_close = false;
        state.pending_app_action = None;
        state.help_modal = None;
        state.invalidation.invalidate_all();
        changed = true;
    }
    changed
}

/// Run modal interaction phases that can consume the frame early.
pub(super) fn apply_help_and_timeline_phase(
    input: &InputSnapshot,
    project: &mut GuiProject,
    viewport_width: usize,
    panel_ctx: InteractionPanelContext,
    timeline_fps: u32,
    state: &mut PreviewState,
    changed: &mut bool,
) -> InteractionPhaseControl {
    let (help_changed, help_consumed) = handle_help_input(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
    *changed |= help_changed;
    if help_consumed {
        return InteractionPhaseControl::Finish(*changed);
    }

    let (timeline_changed, timeline_consumed) = handle_timeline_input(
        input,
        viewport_width,
        panel_ctx.panel_height,
        timeline_fps,
        state,
    );
    *changed |= timeline_changed;
    if timeline_changed {
        invalidate_timeline_and_signal_previews(project, state);
    }
    if timeline_consumed {
        clear_pointer_interactions(state);
        clear_param_hover_state(state);
        clear_param_edit_state(state);
        close_primary_menus(state);
        let _ = collapse_auto_expanded_binding_nodes_with_panel(project, panel_ctx, state);
        return InteractionPhaseControl::Finish(*changed);
    }
    if state.timeline_bpm_edit.is_some() || state.timeline_bar_edit.is_some() {
        cancel_node_interaction_modes(state);
        *changed |= collapse_auto_expanded_binding_nodes_with_panel(project, panel_ctx, state);
        return InteractionPhaseControl::Finish(*changed);
    }
    InteractionPhaseControl::Continue
}

/// Finalize one interaction frame and persist edge-trigger input state.
pub(super) fn finish_interaction_frame(
    input: &InputSnapshot,
    state: &mut PreviewState,
    changed: bool,
) -> bool {
    state.prev_left_down = input.left_down;
    changed
}

pub(super) fn handle_help_input(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> (bool, bool) {
    let close_requested = input.open_help || input.left_clicked || input.right_clicked;
    if state.help_modal.is_some() {
        if close_requested {
            state.help_modal = None;
            state.invalidation.invalidate_overlays();
            return (true, true);
        }
        return (false, true);
    }
    if !input.open_help {
        return (false, false);
    }
    let modal = match resolve_help_target(input, project, panel_width, panel_height, state) {
        Some(HelpTarget::Param {
            node_id,
            param_index,
        }) => build_param_help_modal(project, node_id, param_index)
            .or_else(|| build_node_help_modal(project, node_id))
            .unwrap_or_else(build_global_help_modal),
        Some(HelpTarget::Node(node_id)) => {
            build_node_help_modal(project, node_id).unwrap_or_else(build_global_help_modal)
        }
        None => build_global_help_modal(),
    };
    state.help_modal = Some(modal);
    close_primary_menus(state);
    clear_pointer_interactions(state);
    clear_param_hover_state(state);
    clear_param_edit_state(state);
    clear_timeline_edit_state(state);
    state.invalidation.invalidate_overlays();
    (true, true)
}

fn resolve_help_target(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
) -> Option<HelpTarget> {
    if let Some((mx, my)) = input.mouse_pos {
        if inside_panel(mx, my, panel_width, panel_height) {
            let (graph_x, graph_y) = screen_to_graph(mx, my, state);
            if let Some(node_id) = project.node_at(graph_x, graph_y) {
                if let Some(param_index) = project.param_row_at(node_id, graph_x, graph_y) {
                    return Some(HelpTarget::Param {
                        node_id,
                        param_index,
                    });
                }
                return Some(HelpTarget::Node(node_id));
            }
        }
    }
    if let Some(target) = state.hover_param_target {
        return Some(HelpTarget::Param {
            node_id: target.node_id,
            param_index: target.param_index,
        });
    }
    state
        .hover_node
        .or(state.hover_input_pin)
        .or(state.hover_output_pin)
        .or(state.active_node)
        .map(HelpTarget::Node)
}

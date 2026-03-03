//! GUI input handling and graph-editor interaction logic.

mod drag;
mod hover;
mod marquee;
mod param_edit;
mod wire;

#[cfg(test)]
use self::drag::point_to_segment_distance_sq;
#[cfg(test)]
use self::marquee::{marquee_moved, rects_overlap};
#[cfg(test)]
use self::param_edit::{
    backspace_param_text, can_append_param_char, insert_param_char, move_param_cursor_left,
    move_param_cursor_right,
};

use crate::runtime_config::V2Config;
use std::time::Duration;

use super::geometry::Rect;
use super::help::{build_global_help_modal, build_node_help_modal, build_param_help_modal};
use super::project::{
    collapsed_param_entry_pin_center, input_pin_center, node_expand_toggle_rect,
    node_param_dropdown_rect, node_param_row_rect, output_pin_center, GraphBounds, GuiProject,
    ResourceKind, NODE_PARAM_DROPDOWN_ROW_HEIGHT, NODE_WIDTH,
};
use super::state::{
    AddNodeMenuEntry, AddNodeMenuState, ExportMenuItem, HoverInsertLink, HoverParamTarget,
    InputSnapshot, LinkCutState, MainMenuItem, MainMenuState, PanDragState, ParamDropdownState,
    ParamEditState, PendingAppAction, PopupDragState, PreviewState, RightMarqueeState,
    TimelineBpmEditState, WireDragState, ADD_NODE_OPTIONS, MAIN_MENU_WIDTH,
};
use super::timeline::{
    editor_panel_height, frame_from_track_x, next_looped_frame, pause_button_rect,
    play_button_rect, timeline_control_layout, timeline_rect, track_rect,
};

const PIN_HIT_RADIUS_PX: i32 = 10;
const MIN_ZOOM: f32 = 0.35;
const MAX_ZOOM: f32 = 2.75;
const ZOOM_SENSITIVITY: f32 = 1.12;
const FOCUS_MARGIN_PX: f32 = 28.0;
const PARAM_SCRUB_PX_PER_STEP: f32 = 12.0;
#[cfg(test)]
const PARAM_WIRE_EXIT_TAIL_PX: i32 = 18;
#[cfg(test)]
const PARAM_WIRE_ENTRY_TAIL_PX: i32 = 18;
const INSERT_WIRE_HOVER_RADIUS_PX: i32 = 10;
const NODE_OVERLAP_SNAP_GAP_PX: i32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CutLink {
    source_id: u32,
    target_id: u32,
    param_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HelpTarget {
    Node(u32),
    Param { node_id: u32, param_index: usize },
}

fn invalidate_graph_layers(state: &mut PreviewState) {
    state.invalidation.invalidate_nodes();
    state.invalidation.invalidate_wires();
    state.invalidation.invalidate_overlays();
}

fn invalidate_timeline_and_signal_previews(project: &GuiProject, state: &mut PreviewState) {
    state.invalidation.invalidate_timeline();
    if project.has_signal_preview_nodes() {
        state.invalidation.invalidate_nodes();
    }
}

/// Clear drag/cut/pan/transient pointer interaction modes.
fn clear_pointer_interactions(state: &mut PreviewState) {
    state.drag = None;
    state.wire_drag = None;
    state.link_cut = None;
    state.pan_drag = None;
    state.export_menu_drag = None;
    state.right_marquee = None;
}

/// Clear parameter hover targets and highlighted parameter UI rows.
fn clear_param_hover_state(state: &mut PreviewState) {
    state.hover_param_target = None;
    state.hover_param = None;
    state.hover_alt_param = None;
}

/// Clear active in-place parameter and dropdown editors.
fn clear_param_edit_state(state: &mut PreviewState) {
    state.param_edit = None;
    state.param_scrub = None;
    state.param_dropdown = None;
    state.hover_dropdown_item = None;
}

/// Clear active timeline text-edit widgets.
fn clear_timeline_edit_state(state: &mut PreviewState) {
    state.timeline_bpm_edit = None;
    state.timeline_bar_edit = None;
}

/// Cancel drag/wire interaction modes plus parameter-hover/dropdown state.
fn cancel_node_interaction_modes(state: &mut PreviewState) {
    state.drag = None;
    state.wire_drag = None;
    clear_param_hover_state(state);
    state.param_dropdown = None;
    state.param_scrub = None;
}

/// Close the add-node and main menu overlays.
fn close_primary_menus(state: &mut PreviewState) {
    state.menu = AddNodeMenuState::closed();
    state.main_menu = MainMenuState::closed();
}

/// Shared panel-size context for interaction submodules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InteractionPanelContext {
    panel_width: usize,
    panel_height: usize,
}

impl InteractionPanelContext {
    const fn new(panel_width: usize, panel_height: usize) -> Self {
        Self {
            panel_width,
            panel_height,
        }
    }
}

/// Apply one frame of input actions to project/editor state.
///
/// Returns `true` when this frame changed visual/editor state and should be redrawn.
pub(crate) fn apply_preview_actions(
    config: &V2Config,
    input: InputSnapshot,
    project: &mut GuiProject,
    viewport_width: usize,
    panel_width: usize,
    panel_height: usize,
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

    let (help_changed, help_consumed) =
        handle_help_input(&input, project, panel_width, panel_height, state);
    changed |= help_changed;
    if help_consumed {
        state.prev_left_down = input.left_down;
        return changed;
    }

    let (timeline_changed, timeline_consumed) = handle_timeline_input(
        &input,
        viewport_width,
        panel_height,
        config.animation.fps,
        state,
    );
    changed |= timeline_changed;
    if timeline_changed {
        invalidate_timeline_and_signal_previews(project, state);
    }
    if timeline_consumed {
        clear_pointer_interactions(state);
        clear_param_hover_state(state);
        clear_param_edit_state(state);
        close_primary_menus(state);
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return changed;
    }
    if state.timeline_bpm_edit.is_some() || state.timeline_bar_edit.is_some() {
        cancel_node_interaction_modes(state);
        changed |= collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return changed;
    }

    let zoom_before = state.zoom.to_bits();
    changed |= handle_pan_zoom_and_focus(&input, project, panel_width, panel_height, state);
    if zoom_before != state.zoom.to_bits() {
        invalidate_graph_layers(state);
    }
    if state.pan_drag.is_some() {
        cancel_node_interaction_modes(state);
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return true;
    }

    let (param_scrub_changed, param_scrub_active) =
        handle_alt_param_drag(&input, project, panel_width, panel_height, state);
    changed |= param_scrub_changed;
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
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return true;
    }

    let cut_changed = handle_link_cut(&input, project, panel_width, panel_height, state);
    changed |= cut_changed;
    if cut_changed {
        state.invalidation.invalidate_wires();
        state.invalidation.invalidate_overlays();
    }
    if state.link_cut.is_some() {
        cancel_node_interaction_modes(state);
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return true;
    }

    let right_sel_changed =
        handle_right_selection(&input, project, panel_width, panel_height, state);
    changed |= right_sel_changed;
    if right_sel_changed {
        state.invalidation.invalidate_nodes();
        state.invalidation.invalidate_overlays();
    }
    if state.right_marquee.is_some() {
        cancel_node_interaction_modes(state);
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return true;
    }

    let add_menu_changed = handle_add_menu_toggle(&input, panel_width, panel_height, state);
    changed |= add_menu_changed;
    if add_menu_changed {
        state.invalidation.invalidate_overlays();
    }
    let main_menu_changed = handle_main_menu_toggle(&input, panel_width, panel_height, state);
    changed |= main_menu_changed;
    if main_menu_changed {
        state.invalidation.invalidate_overlays();
    }
    let hover_changed = update_hover_state(&input, project, panel_width, panel_height, state);
    changed |= hover_changed;
    if hover_changed {
        invalidate_graph_layers(state);
    }
    let node_toggle_changed =
        handle_node_open_toggle(&input, project, panel_width, panel_height, state);
    changed |= node_toggle_changed;
    if node_toggle_changed {
        state.invalidation.invalidate_overlays();
    }
    let (param_changed, param_click_consumed) =
        handle_param_edit_input(&input, project, panel_width, panel_height, state);
    changed |= param_changed;
    if param_changed {
        state.invalidation.invalidate_nodes();
        state.invalidation.invalidate_overlays();
    }
    if param_click_consumed {
        cancel_node_interaction_modes(state);
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return true;
    }
    if state.param_edit.is_some() {
        cancel_node_interaction_modes(state);
        changed |= collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return changed;
    }
    if state.export_menu.open || state.main_menu.open {
        let menu_changed = handle_main_export_menu_input(&input, panel_width, panel_height, state);
        changed |= menu_changed;
        if menu_changed {
            state.invalidation.invalidate_overlays();
            state.invalidation.invalidate_timeline();
        }
    } else if state.menu.open {
        let menu_changed = handle_add_menu_input(&input, project, panel_width, panel_height, state);
        changed |= menu_changed;
        if menu_changed {
            state.invalidation.invalidate_overlays();
        }
    } else {
        let delete_changed = handle_delete_selected_nodes(&input, project, state);
        changed |= delete_changed;
        if delete_changed {
            state.invalidation.invalidate_overlays();
        }
        let param_shortcut_changed = handle_parameter_shortcuts(&input, project, state);
        changed |= param_shortcut_changed;
        if param_shortcut_changed {
            state.invalidation.invalidate_nodes();
        }
        let wire_changed = handle_wire_input(&input, project, panel_width, panel_height, state);
        changed |= wire_changed;
        if wire_changed {
            invalidate_graph_layers(state);
        }
        if state.wire_drag.is_none() {
            let drag_changed = handle_drag_input(&input, project, panel_width, panel_height, state);
            changed |= drag_changed;
            if drag_changed {
                invalidate_graph_layers(state);
            }
        } else {
            state.drag = None;
        }
    }
    if state.wire_drag.is_none() {
        changed |= collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
    }
    state.prev_left_down = input.left_down;
    changed
}

fn handle_help_input(
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

/// Advance timeline frame counter at the configured playback frame rate.
///
/// Returns `true` when at least one timeline tick advanced this frame.
pub(crate) fn step_timeline_if_running(
    state: &mut PreviewState,
    frame_delta: Duration,
    timeline_fps: u32,
    timeline_total_frames: u32,
) -> bool {
    let mut advanced = false;
    if !state.paused {
        let tick_secs = 1.0 / timeline_fps.max(1) as f32;
        state.timeline_accum_secs += frame_delta.as_secs_f32();
        while state.timeline_accum_secs >= tick_secs {
            state.timeline_accum_secs -= tick_secs;
            state.frame_index = next_looped_frame(state.frame_index, timeline_total_frames);
            advanced = true;
        }
    }
    advanced
}

fn handle_timeline_input(
    input: &InputSnapshot,
    viewport_width: usize,
    panel_height: usize,
    timeline_fps: u32,
    state: &mut PreviewState,
) -> (bool, bool) {
    let mut changed = apply_timeline_bpm_text_edits(input, state);
    changed |= apply_timeline_bar_text_edits(input, state);
    let mut consumed = false;
    let timeline = timeline_rect(viewport_width, panel_height);
    let play = play_button_rect(timeline);
    let pause = pause_button_rect(timeline);
    let track = track_rect(timeline);
    let controls = timeline_control_layout(timeline);
    let total_frames = state.export_menu.timeline_total_frames(timeline_fps);
    let mouse_pos = input.mouse_pos;
    if !input.left_down && (state.timeline_scrub_active || state.timeline_volume_drag_active) {
        state.timeline_scrub_active = false;
        state.timeline_volume_drag_active = false;
        return (changed, true);
    }
    if let Some((mx, my)) = mouse_pos {
        if input.left_clicked && controls.bpm_value.contains(mx, my) {
            changed |= start_timeline_bpm_edit(state);
            return (changed, true);
        }
        if input.left_clicked
            && controls.bar_value.contains(mx, my)
            && !state.export_menu.bar_length_overridden_by_audio()
        {
            changed |= start_timeline_bar_edit(state);
            return (changed, true);
        }
        if input.left_clicked
            && (state.timeline_bpm_edit.is_some() || state.timeline_bar_edit.is_some())
        {
            changed |= finish_timeline_bpm_edit(state);
            changed |= finish_timeline_bar_edit(state);
            consumed = true;
        }
        if input.left_clicked && play.contains(mx, my) {
            state.paused = false;
            state.timeline_scrub_active = false;
            state.timeline_volume_drag_active = false;
            return (true, true);
        }
        if input.left_clicked && pause.contains(mx, my) {
            state.paused = true;
            state.timeline_scrub_active = false;
            state.timeline_volume_drag_active = false;
            return (true, true);
        }
        if input.left_clicked && controls.bpm_down.contains(mx, my) {
            changed |= adjust_timeline_bpm(state, -1.0);
            return (changed, true);
        }
        if input.left_clicked && controls.bpm_up.contains(mx, my) {
            changed |= adjust_timeline_bpm(state, 1.0);
            return (changed, true);
        }
        if controls.bpm_value.contains(mx, my) && input.wheel_lines_y.abs() > f32::EPSILON {
            if state.timeline_bpm_edit.is_some() {
                changed |= finish_timeline_bpm_edit(state);
            }
            if state.timeline_bar_edit.is_some() {
                changed |= finish_timeline_bar_edit(state);
            }
            changed |= adjust_timeline_bpm(state, input.wheel_lines_y.signum());
            return (changed, true);
        }
        if input.left_clicked && controls.volume_slider.contains(mx, my) {
            state.timeline_volume_drag_active = true;
            changed |= set_timeline_volume_from_slider_x(state, controls.volume_slider, mx);
            consumed = true;
        } else if state.timeline_volume_drag_active && input.left_down {
            changed |= set_timeline_volume_from_slider_x(state, controls.volume_slider, mx);
            consumed = true;
        }
        if input.left_clicked && track.contains(mx, my) {
            state.timeline_scrub_active = true;
            state.timeline_volume_drag_active = false;
            consumed = true;
            changed |= scrub_frame_from_timeline(track, mx, total_frames, state);
        } else if state.timeline_scrub_active && input.left_down {
            consumed = true;
            changed |= scrub_frame_from_timeline(track, mx, total_frames, state);
        }
        if input.left_clicked && timeline.contains(mx, my) {
            consumed = true;
        }
    } else if state.timeline_scrub_active || state.timeline_volume_drag_active {
        consumed = true;
    }
    (changed, consumed)
}

fn start_timeline_bpm_edit(state: &mut PreviewState) -> bool {
    let Some(active) = state.timeline_bpm_edit.as_mut() else {
        let cursor = state.export_menu.bpm.len();
        state.timeline_bpm_edit = Some(TimelineBpmEditState {
            buffer: state.export_menu.bpm.clone(),
            cursor,
            anchor: 0,
        });
        return true;
    };
    let end = active.buffer.len();
    if active.cursor == end && active.anchor == end {
        return false;
    }
    active.cursor = end;
    active.anchor = end;
    true
}

fn finish_timeline_bpm_edit(state: &mut PreviewState) -> bool {
    let Some(edit) = state.timeline_bpm_edit.take() else {
        return false;
    };
    let _ = commit_timeline_bpm_buffer(state, edit.buffer.as_str());
    true
}

fn start_timeline_bar_edit(state: &mut PreviewState) -> bool {
    if state.export_menu.bar_length_overridden_by_audio() {
        state.timeline_bar_edit = None;
        return false;
    }
    let Some(active) = state.timeline_bar_edit.as_mut() else {
        let cursor = state.export_menu.bar_length.len();
        state.timeline_bar_edit = Some(TimelineBpmEditState {
            buffer: state.export_menu.bar_length.clone(),
            cursor,
            anchor: 0,
        });
        return true;
    };
    let end = active.buffer.len();
    if active.cursor == end && active.anchor == end {
        return false;
    }
    active.cursor = end;
    active.anchor = end;
    true
}

fn finish_timeline_bar_edit(state: &mut PreviewState) -> bool {
    let Some(edit) = state.timeline_bar_edit.take() else {
        return false;
    };
    let _ = commit_timeline_bar_length_buffer(state, edit.buffer.as_str());
    true
}

fn apply_timeline_bpm_text_edits(input: &InputSnapshot, state: &mut PreviewState) -> bool {
    let Some(edit) = state.timeline_bpm_edit.take() else {
        return false;
    };
    let mut draft = timeline_bpm_edit_to_param(edit);
    let mut changed = false;
    if input.param_cancel {
        return true;
    }
    if input.param_select_all {
        changed |= param_edit::select_all_param_text(&mut draft);
    }
    if input.param_dec {
        changed |= param_edit::move_param_cursor_left(&mut draft, input.shift_down);
    }
    if input.param_inc {
        changed |= param_edit::move_param_cursor_right(&mut draft, input.shift_down);
    }
    if input.param_backspace {
        changed |= param_edit::backspace_param_text(&mut draft);
    }
    if input.param_delete {
        changed |= param_edit::delete_param_text(&mut draft);
    }
    if !input.typed_text.is_empty() {
        for ch in input.typed_text.chars() {
            if param_edit::insert_param_char(&mut draft, ch) {
                changed = true;
            }
        }
    }
    if input.param_commit && commit_timeline_bpm_buffer(state, draft.buffer.as_str()) {
        return true;
    }
    state.timeline_bpm_edit = Some(timeline_bpm_edit_from_param(draft));
    changed
}

fn apply_timeline_bar_text_edits(input: &InputSnapshot, state: &mut PreviewState) -> bool {
    let Some(edit) = state.timeline_bar_edit.take() else {
        return false;
    };
    if state.export_menu.bar_length_overridden_by_audio() {
        return true;
    }
    let mut draft = timeline_bpm_edit_to_param(edit);
    let mut changed = false;
    if input.param_cancel {
        return true;
    }
    if input.param_select_all {
        changed |= param_edit::select_all_param_text(&mut draft);
    }
    if input.param_dec {
        changed |= param_edit::move_param_cursor_left(&mut draft, input.shift_down);
    }
    if input.param_inc {
        changed |= param_edit::move_param_cursor_right(&mut draft, input.shift_down);
    }
    if input.param_backspace {
        changed |= param_edit::backspace_param_text(&mut draft);
    }
    if input.param_delete {
        changed |= param_edit::delete_param_text(&mut draft);
    }
    if !input.typed_text.is_empty() {
        for ch in input.typed_text.chars() {
            if param_edit::insert_param_char(&mut draft, ch) {
                changed = true;
            }
        }
    }
    if input.param_commit && commit_timeline_bar_length_buffer(state, draft.buffer.as_str()) {
        return true;
    }
    state.timeline_bar_edit = Some(timeline_bpm_edit_from_param(draft));
    changed
}

fn timeline_bpm_edit_to_param(edit: TimelineBpmEditState) -> ParamEditState {
    ParamEditState {
        node_id: 0,
        param_index: 0,
        buffer: edit.buffer,
        cursor: edit.cursor,
        anchor: edit.anchor,
    }
}

fn timeline_bpm_edit_from_param(edit: ParamEditState) -> TimelineBpmEditState {
    TimelineBpmEditState {
        buffer: edit.buffer,
        cursor: edit.cursor,
        anchor: edit.anchor,
    }
}

fn commit_timeline_bpm_buffer(state: &mut PreviewState, buffer: &str) -> bool {
    let Ok(value) = buffer.trim().parse::<f32>() else {
        return false;
    };
    state.export_menu.bpm = format_timeline_bpm(value.clamp(1.0, 400.0));
    true
}

fn commit_timeline_bar_length_buffer(state: &mut PreviewState, buffer: &str) -> bool {
    if state.export_menu.bar_length_overridden_by_audio() {
        return false;
    }
    let Ok(value) = buffer.trim().parse::<f32>() else {
        return false;
    };
    state.export_menu.bar_length = format_timeline_bar_length(value.clamp(0.01, 10_000.0));
    true
}

fn format_timeline_bpm(value: f32) -> String {
    if (value - value.round()).abs() < 0.001 {
        format!("{}", value.round() as u32)
    } else {
        format!("{value:.2}")
    }
}

fn format_timeline_bar_length(value: f32) -> String {
    if (value - value.round()).abs() < 0.001 {
        format!("{}", value.round() as u32)
    } else {
        format!("{value:.2}")
    }
}

fn adjust_timeline_bpm(state: &mut PreviewState, delta: f32) -> bool {
    let current = state.export_menu.parsed_bpm();
    let next = (current + delta).clamp(1.0, 400.0);
    if (next - current).abs() < f32::EPSILON {
        return false;
    }
    state.export_menu.bpm = format_timeline_bpm(next);
    true
}

fn set_timeline_volume_from_slider_x(state: &mut PreviewState, slider: Rect, mouse_x: i32) -> bool {
    if slider.w <= 1 {
        return false;
    }
    let clamped_x = mouse_x.clamp(slider.x, slider.x + slider.w - 1);
    let t = (clamped_x - slider.x) as f32 / (slider.w - 1) as f32;
    let next = (t * 2.0).clamp(0.0, 2.0);
    let current = state.export_menu.parsed_audio_volume();
    if (next - current).abs() < 0.000_5 {
        return false;
    }
    state.export_menu.audio_volume = format!("{next:.2}");
    true
}

fn scrub_frame_from_timeline(
    track: Rect,
    mouse_x: i32,
    total_frames: u32,
    state: &mut PreviewState,
) -> bool {
    let frame = frame_from_track_x(track, mouse_x, total_frames);
    if frame == state.frame_index {
        return false;
    }
    state.frame_index = frame;
    state.timeline_accum_secs = 0.0;
    true
}

#[cfg_attr(not(test), allow(dead_code))]
fn handle_param_wheel_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> (bool, bool) {
    let _ = (input, project, panel_width, panel_height, state);
    (false, false)
}

fn handle_alt_param_drag(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> (bool, bool) {
    if state.menu.open
        || state.main_menu.open
        || state.export_menu.open
        || state.param_edit.is_some()
        || state.timeline_bpm_edit.is_some()
        || state.timeline_bar_edit.is_some()
        || state.param_dropdown.is_some()
    {
        let ended = state.param_scrub.take().is_some();
        return (ended, ended);
    }

    if let Some(mut scrub) = state.param_scrub {
        if !input.left_down || !input.alt_down {
            state.param_scrub = None;
            return (true, true);
        }
        let Some((mx, my)) = input.mouse_pos else {
            state.param_scrub = Some(scrub);
            return (false, true);
        };
        if !inside_panel(mx, my, panel_width, panel_height) {
            state.param_scrub = Some(scrub);
            return (false, true);
        }
        let mut changed = false;
        // Vertical scrub: dragging up increases, dragging down decreases.
        let dy = scrub.last_mouse_y - my;
        scrub.last_mouse_y = my;
        scrub.pixel_remainder += dy as f32;
        let step_delta = (scrub.pixel_remainder / PARAM_SCRUB_PX_PER_STEP).trunc();
        if step_delta.abs() >= 1.0 {
            scrub.pixel_remainder -= step_delta * PARAM_SCRUB_PX_PER_STEP;
            changed |= project.select_param(scrub.node_id, scrub.param_index);
            changed |= project.adjust_param(scrub.node_id, scrub.param_index, step_delta);
            state.active_node = Some(scrub.node_id);
            state.hover_alt_param = Some(HoverParamTarget {
                node_id: scrub.node_id,
                param_index: scrub.param_index,
            });
        }
        state.param_scrub = Some(scrub);
        return (changed, true);
    }

    if !input.alt_down || !input.left_clicked {
        return (false, false);
    }
    let Some(target) = scrubbable_param_at_cursor(input, project, panel_width, panel_height, state)
    else {
        return (false, false);
    };
    let Some((_mx, my)) = input.mouse_pos else {
        return (false, false);
    };
    state.param_scrub = Some(super::state::ParamScrubState {
        node_id: target.node_id,
        param_index: target.param_index,
        last_mouse_y: my,
        pixel_remainder: 0.0,
    });
    state.active_node = Some(target.node_id);
    state.hover_alt_param = Some(target);
    state.link_cut = None;
    state.hover_dropdown_item = None;
    state.param_edit = None;
    (
        project.select_param(target.node_id, target.param_index),
        true,
    )
}

fn scrubbable_param_at_cursor(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
) -> Option<HoverParamTarget> {
    let (mx, my) = input.mouse_pos?;
    if !inside_panel(mx, my, panel_width, panel_height) {
        return None;
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
    let node_id = project.node_at(graph_x, graph_y)?;
    let param_index = project.param_row_at(node_id, graph_x, graph_y)?;
    if !project.param_value_box_contains(node_id, param_index, graph_x, graph_y) {
        return None;
    }
    if !project.param_supports_text_edit(node_id, param_index) {
        return None;
    }
    Some(HoverParamTarget {
        node_id,
        param_index,
    })
}

fn handle_pan_zoom_and_focus(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    if state.menu.open || state.main_menu.open || state.export_menu.open {
        return false;
    }
    let mut changed = false;
    if input.focus_all {
        changed |= focus_all_nodes(project, panel_width, panel_height, state);
    }
    if let Some((mx, my)) = input.mouse_pos {
        if inside_panel(mx, my, panel_width, panel_height) && input.wheel_lines_y.abs() > 0.0 {
            changed |= apply_zoom(mx, my, input.wheel_lines_y, state);
        }
    }
    if input.middle_clicked {
        if let Some((mx, my)) = input.mouse_pos {
            if inside_panel(mx, my, panel_width, panel_height) {
                state.pan_drag = Some(PanDragState {
                    last_x: mx,
                    last_y: my,
                });
                state.drag = None;
                state.wire_drag = None;
                clear_param_hover_state(state);
                state.param_scrub = None;
            }
        }
    }
    let Some(mut pan_drag) = state.pan_drag else {
        return changed;
    };
    if !input.middle_down {
        state.pan_drag = None;
        return true;
    }
    let Some((mx, my)) = input.mouse_pos else {
        state.pan_drag = Some(pan_drag);
        return changed;
    };
    let dx = mx - pan_drag.last_x;
    let dy = my - pan_drag.last_y;
    pan_drag.last_x = mx;
    pan_drag.last_y = my;
    state.pan_drag = Some(pan_drag);
    if dx == 0 && dy == 0 {
        return changed;
    }
    state.pan_x += dx as f32;
    state.pan_y += dy as f32;
    true
}

fn handle_add_menu_toggle(
    input: &InputSnapshot,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    if state.export_menu.open {
        return false;
    }
    if !input.toggle_add_menu {
        return false;
    }
    if state.menu.open {
        state.menu = AddNodeMenuState::closed();
        state.main_menu = super::state::MainMenuState::closed();
        state.wire_drag = None;
        clear_param_hover_state(state);
        clear_param_edit_state(state);
        clear_timeline_edit_state(state);
        return true;
    }
    let (x, y) = input
        .mouse_pos
        .unwrap_or((panel_width as i32 / 2, panel_height as i32 / 3));
    state.menu = AddNodeMenuState::open_at(x, y, panel_width, editor_panel_height(panel_height));
    state.main_menu = MainMenuState::closed();
    state.drag = None;
    state.wire_drag = None;
    clear_param_hover_state(state);
    clear_param_edit_state(state);
    clear_timeline_edit_state(state);
    true
}

fn handle_main_menu_toggle(
    input: &InputSnapshot,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    if !input.toggle_main_menu {
        return false;
    }
    if state.main_menu.open {
        return close_main_menu(state);
    }
    let (x, y) = input
        .mouse_pos
        .unwrap_or((panel_width as i32 / 4, panel_height as i32 / 4));
    state.main_menu = MainMenuState::open_at(x, y, panel_width, editor_panel_height(panel_height));
    state.menu = AddNodeMenuState::closed();
    clear_param_edit_state(state);
    clear_timeline_edit_state(state);
    state.drag = None;
    state.wire_drag = None;
    clear_param_hover_state(state);
    true
}

fn handle_main_export_menu_input(
    input: &InputSnapshot,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    let (drag_changed, drag_consumed) =
        handle_export_menu_drag(input, panel_width, panel_height, state);
    changed |= drag_changed;
    if drag_consumed {
        return changed;
    }
    if let Some(hovered) = state.hover_export_menu_item {
        if state.export_menu.open {
            changed |= state.export_menu.select_index(hovered);
        }
    }
    if let Some(hovered) = state.hover_main_menu_item {
        if state.main_menu.open {
            changed |= state.main_menu.select_index(hovered);
        }
    }
    if input.param_cancel {
        return close_main_menu(state) || changed;
    }

    if state.export_menu.open {
        if input.menu_up {
            changed |= state.export_menu.select_prev();
        }
        if input.menu_down {
            changed |= state.export_menu.select_next();
        }
        changed |= apply_export_menu_text_input(input, state);
        if input.menu_accept && activate_export_menu_selection(state) {
            return true;
        }
    } else if state.main_menu.open {
        if input.menu_up {
            changed |= state.main_menu.select_prev();
        }
        if input.menu_down {
            changed |= state.main_menu.select_next();
        }
        if input.menu_accept
            && activate_main_menu_selection(input, panel_width, panel_height, state)
        {
            return true;
        }
    }

    if !input.left_clicked {
        return changed;
    }
    let Some((mx, my)) = input.mouse_pos else {
        return close_main_menu(state) || changed;
    };

    if state.export_menu.open {
        if state.export_menu.close_button_rect().contains(mx, my) {
            return close_export_menu(state) || changed;
        }
        if let Some(index) = state.export_menu.item_at(mx, my) {
            let _ = state.export_menu.select_index(index);
            return activate_export_menu_selection(state) || changed;
        }
    }
    if state.main_menu.open {
        if let Some(index) = state.main_menu.item_at(mx, my) {
            let _ = state.main_menu.select_index(index);
            return activate_main_menu_selection(input, panel_width, panel_height, state)
                || changed;
        }
    }
    let inside_main = state.main_menu.open && state.main_menu.rect().contains(mx, my);
    if state.main_menu.open && !inside_main {
        return close_main_menu(state) || changed;
    }
    changed
}

fn handle_export_menu_drag(
    input: &InputSnapshot,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> (bool, bool) {
    if !state.export_menu.open {
        return (state.export_menu_drag.take().is_some(), false);
    }
    if let Some(drag) = state.export_menu_drag {
        if !input.left_down {
            state.export_menu_drag = None;
            return (false, false);
        }
        let Some((mx, my)) = input.mouse_pos else {
            return (false, true);
        };
        let changed = state.export_menu.move_to(
            mx - drag.offset_x,
            my - drag.offset_y,
            panel_width,
            editor_panel_height(panel_height),
        );
        return (changed, true);
    }
    if !input.left_clicked {
        return (false, false);
    }
    let Some((mx, my)) = input.mouse_pos else {
        return (false, false);
    };
    if !state.export_menu.title_bar_rect().contains(mx, my) {
        return (false, false);
    }
    if state.export_menu.close_button_rect().contains(mx, my) {
        return (false, false);
    }
    state.export_menu_drag = Some(PopupDragState {
        offset_x: mx - state.export_menu.x,
        offset_y: my - state.export_menu.y,
    });
    state.hover_export_menu_item = None;
    state.hover_export_menu_close = false;
    (true, true)
}

fn apply_export_menu_text_input(input: &InputSnapshot, state: &mut PreviewState) -> bool {
    let selected = state.export_menu.selected_item();
    let target = match selected {
        ExportMenuItem::Directory => Some(&mut state.export_menu.directory),
        ExportMenuItem::FileName => Some(&mut state.export_menu.file_name),
        ExportMenuItem::BeatsPerBar => Some(&mut state.export_menu.beats_per_bar),
        _ => None,
    };
    let Some(target) = target else {
        return false;
    };
    let mut changed = false;
    if input.param_backspace && !target.is_empty() {
        target.pop();
        changed = true;
    }
    if !input.typed_text.is_empty() {
        target.push_str(input.typed_text.as_str());
        changed = true;
    }
    if changed {
        target.truncate(240);
    }
    changed
}

fn activate_main_menu_selection(
    input: &InputSnapshot,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let selected = state.main_menu.selected_item();
    match selected {
        MainMenuItem::New => {
            state.request_new_project = true;
            close_main_menu(state)
        }
        MainMenuItem::Save => {
            state.pending_app_action = Some(PendingAppAction::SaveProject);
            close_main_menu(state)
        }
        MainMenuItem::Load => {
            state.pending_app_action = Some(PendingAppAction::LoadProject);
            close_main_menu(state)
        }
        MainMenuItem::Export => {
            let export_x = state.main_menu.x + MAIN_MENU_WIDTH + 8;
            let export_y = state
                .main_menu
                .entry_rect(state.main_menu.selected)
                .map(|rect| rect.y)
                .unwrap_or(state.main_menu.y);
            let opened = super::state::ExportMenuState::open_at(
                export_x,
                export_y,
                panel_width,
                editor_panel_height(panel_height),
            );
            state.export_menu.open = true;
            state.export_menu.x = opened.x;
            state.export_menu.y = opened.y;
            if input.mouse_pos.is_none() {
                state.export_menu.selected = 0;
            }
            close_main_menu(state)
        }
        MainMenuItem::Exit => {
            state.pending_app_action = Some(PendingAppAction::Exit);
            close_main_menu(state)
        }
    }
}

fn activate_export_menu_selection(state: &mut PreviewState) -> bool {
    match state.export_menu.selected_item() {
        ExportMenuItem::Directory
        | ExportMenuItem::FileName
        | ExportMenuItem::BeatsPerBar
        | ExportMenuItem::Codec
        | ExportMenuItem::Preview => false,
        ExportMenuItem::StartStop => {
            state.pending_app_action = Some(if state.export_menu.exporting {
                PendingAppAction::StopExport
            } else {
                PendingAppAction::StartExport
            });
            true
        }
    }
}

fn close_main_menu(state: &mut PreviewState) -> bool {
    let changed = state.main_menu.open || state.hover_main_menu_item.is_some();
    state.main_menu = MainMenuState::closed();
    state.hover_main_menu_item = None;
    changed
}

fn close_export_menu(state: &mut PreviewState) -> bool {
    let changed = state.export_menu.open
        || state.export_menu_drag.is_some()
        || state.hover_export_menu_item.is_some()
        || state.hover_export_menu_close;
    state.export_menu.open = false;
    state.export_menu_drag = None;
    state.hover_export_menu_item = None;
    state.hover_export_menu_close = false;
    changed
}

fn handle_node_open_toggle(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    if !input.toggle_node_open || state.menu.open || state.main_menu.open || state.export_menu.open
    {
        return false;
    }
    let target = state
        .hover_node
        .or(state.active_node)
        .or(state.hover_input_pin)
        .or(state.hover_output_pin);
    let Some(node_id) = target else {
        return false;
    };
    let was_expanded = project.node_expanded(node_id);
    let changed = if was_expanded {
        project.collapse_node(node_id, panel_width, panel_height)
    } else {
        project.expand_node(node_id, panel_width, panel_height)
    };
    if !changed {
        return false;
    }
    let now_expanded = !was_expanded;
    let bind_drag = state
        .wire_drag
        .and_then(|wire| wire_drag_source_kind(project, wire))
        .filter(|kind| matches!(kind, ResourceKind::Signal | ResourceKind::Texture2D))
        .is_some();
    if bind_drag {
        if !was_expanded && now_expanded {
            if !state.auto_expanded_binding_nodes.contains(&node_id) {
                state.auto_expanded_binding_nodes.push(node_id);
            }
        } else if was_expanded && !now_expanded {
            state
                .auto_expanded_binding_nodes
                .retain(|tracked| *tracked != node_id);
        }
    }
    true
}

fn handle_parameter_shortcuts(
    input: &InputSnapshot,
    project: &mut GuiProject,
    state: &mut PreviewState,
) -> bool {
    if state.param_edit.is_some()
        || state.timeline_bpm_edit.is_some()
        || state.timeline_bar_edit.is_some()
        || state.param_dropdown.is_some()
    {
        return false;
    }
    let target = state.hover_node.or(state.active_node);
    let Some(node_id) = target else {
        return false;
    };
    if !project.node_expanded(node_id) {
        return false;
    }
    state.active_node = Some(node_id);
    let mut changed = false;
    if input.menu_up {
        changed |= project.select_prev_param(node_id);
    }
    if input.menu_down {
        changed |= project.select_next_param(node_id);
    }
    if input.param_dec {
        changed |= project.adjust_selected_param(node_id, -1.0);
    }
    if input.param_inc {
        changed |= project.adjust_selected_param(node_id, 1.0);
    }
    changed
}

fn handle_delete_selected_nodes(
    input: &InputSnapshot,
    project: &mut GuiProject,
    state: &mut PreviewState,
) -> bool {
    if !input.param_delete || state.selected_nodes.is_empty() {
        return false;
    }
    if !project.delete_nodes(state.selected_nodes.as_slice()) {
        return false;
    }
    state.selected_nodes.clear();
    state.active_node = None;
    state.hover_node = None;
    state.hover_output_pin = None;
    state.hover_input_pin = None;
    clear_param_hover_state(state);
    clear_pointer_interactions(state);
    clear_param_edit_state(state);
    clear_timeline_edit_state(state);
    true
}

#[allow(unused_assignments)]
fn handle_right_selection(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    marquee::handle_right_selection(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

/// Convert current editor panel bounds to one graph-space rectangle.
fn panel_graph_rect(
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
) -> (i32, i32, i32, i32) {
    marquee::panel_graph_rect(
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

fn handle_param_edit_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> (bool, bool) {
    param_edit::handle_param_edit_input(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

fn handle_link_cut(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    if input.alt_down
        && input.left_clicked
        && state.param_scrub.is_none()
        && !state.menu.open
        && !state.main_menu.open
        && !state.export_menu.open
    {
        if let Some((mx, my)) = input.mouse_pos {
            if inside_panel(mx, my, panel_width, panel_height) {
                state.link_cut = Some(LinkCutState {
                    start_x: mx,
                    start_y: my,
                    cursor_x: mx,
                    cursor_y: my,
                });
                state.drag = None;
                state.wire_drag = None;
                clear_param_hover_state(state);
                clear_param_edit_state(state);
                clear_timeline_edit_state(state);
                return true;
            }
        }
    }
    let Some(mut cut) = state.link_cut else {
        return false;
    };
    if let Some((mx, my)) = input.mouse_pos {
        if cut.cursor_x != mx || cut.cursor_y != my {
            cut.cursor_x = mx;
            cut.cursor_y = my;
            changed = true;
        }
    }
    if !input.left_down {
        let cut_links = collect_cut_links(project, panel_width, panel_height, state, cut);
        for link in cut_links {
            if let Some(param_index) = link.param_index {
                let _ = project.disconnect_param_link_from_param(link.target_id, param_index);
            } else {
                let _ = project.disconnect_link(link.source_id, link.target_id);
            }
        }
        state.link_cut = None;
        return true;
    }
    state.link_cut = Some(cut);
    changed
}

fn collect_cut_links(
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
    cut: LinkCutState,
) -> Vec<CutLink> {
    let mut links = Vec::new();
    let obstacles = collect_graph_node_obstacles(project);
    let route_map =
        super::scene::wire_route::RouteObstacleMap::from_obstacles(obstacles.as_slice());
    let (view_x0, view_y0, view_x1, view_y1) = panel_graph_rect(panel_width, panel_height, state);
    let target_ids = project.node_ids_overlapping_graph_rect(view_x0, view_y0, view_x1, view_y1);
    for target_id in target_ids.iter().copied() {
        collect_cut_links_for_target(project, state, cut, &route_map, target_id, &mut links);
    }
    let cut_outside_panel = !inside_panel(cut.start_x, cut.start_y, panel_width, panel_height)
        || !inside_panel(cut.cursor_x, cut.cursor_y, panel_width, panel_height);
    if (links.is_empty() || cut_outside_panel) && target_ids.len() < project.node_count() {
        for target in project.nodes() {
            collect_cut_links_for_target(project, state, cut, &route_map, target.id(), &mut links);
        }
    }
    links.sort_unstable();
    links.dedup();
    links
}

fn collect_cut_links_for_target(
    project: &GuiProject,
    state: &PreviewState,
    cut: LinkCutState,
    route_map: &super::scene::wire_route::RouteObstacleMap,
    target_id: u32,
    links: &mut Vec<CutLink>,
) {
    let Some(target) = project.node(target_id) else {
        return;
    };
    if let Some(texture_source_id) = project.input_source_node_id(target_id) {
        let Some((to_x, to_y)) = input_pin_center(target) else {
            return;
        };
        let Some(source) = project.node(texture_source_id) else {
            return;
        };
        let Some((from_x, from_y)) = output_pin_center(source) else {
            return;
        };
        let route_graph = super::scene::wire_route::route_wire_path_with_tails_with_map(
            super::scene::wire_route::RouteEndpoint {
                point: (from_x, from_y),
                corridor_dir: super::scene::wire_route::RouteDirection::East,
            },
            super::scene::wire_route::RouteEndpoint {
                point: (to_x, to_y),
                corridor_dir: super::scene::wire_route::RouteDirection::West,
            },
            route_map,
        );
        let route_panel = map_graph_path_to_panel(route_graph.as_slice(), state);
        if cut_intersects_path(cut, route_panel.as_slice()) {
            links.push(CutLink {
                source_id: texture_source_id,
                target_id,
                param_index: None,
            });
        }
    }
    for param_index in 0..target.param_count() {
        let Some((source_id, _resource_kind)) =
            project.param_link_source_for_param(target_id, param_index)
        else {
            continue;
        };
        let Some(source) = project.node(source_id) else {
            continue;
        };
        let Some((from_x, from_y)) = output_pin_center(source) else {
            continue;
        };
        let (to_x, to_y) = if let Some(row) = node_param_row_rect(target, param_index) {
            (row.x + row.w - 4, row.y + row.h / 2)
        } else if let Some((pin_x, pin_y)) = collapsed_param_entry_pin_center(target) {
            (pin_x, pin_y)
        } else {
            continue;
        };
        let route_graph = super::scene::wire_route::route_wire_path_with_tails_with_map(
            super::scene::wire_route::RouteEndpoint {
                point: (from_x, from_y),
                corridor_dir: super::scene::wire_route::RouteDirection::East,
            },
            super::scene::wire_route::RouteEndpoint {
                point: (to_x, to_y),
                corridor_dir: super::scene::wire_route::RouteDirection::East,
            },
            route_map,
        );
        let route_panel = map_graph_path_to_panel(route_graph.as_slice(), state);
        if cut_intersects_path(cut, route_panel.as_slice()) {
            links.push(CutLink {
                source_id,
                target_id,
                param_index: Some(param_index),
            });
        }
    }
}

fn cut_intersects_path(cut: LinkCutState, path: &[(i32, i32)]) -> bool {
    if path.len() < 2 {
        return false;
    }
    for segment in path.windows(2) {
        if segments_intersect(
            cut.start_x,
            cut.start_y,
            cut.cursor_x,
            cut.cursor_y,
            segment[0].0,
            segment[0].1,
            segment[1].0,
            segment[1].1,
        ) {
            return true;
        }
    }
    false
}

fn collect_graph_node_obstacles(
    project: &GuiProject,
) -> Vec<super::scene::wire_route::NodeObstacle> {
    let mut out = Vec::new();
    for node in project.nodes() {
        out.push(super::scene::wire_route::NodeObstacle {
            rect: Rect::new(node.x(), node.y(), NODE_WIDTH, node.card_height()),
        });
    }
    out
}

fn handle_add_menu_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    if let Some(hovered) = state.hover_menu_item {
        changed |= state.menu.select_index(hovered);
    }
    let query_typed = if input.toggle_add_menu {
        ""
    } else {
        input.typed_text.as_str()
    };
    changed |= state
        .menu
        .apply_query_input(query_typed, input.param_backspace);
    if input.param_cancel {
        if state.menu.close_category() {
            return true;
        }
        state.menu = AddNodeMenuState::closed();
        return true;
    }
    if input.menu_up {
        changed |= state.menu.select_prev();
    }
    if input.menu_down {
        changed |= state.menu.select_next();
    }
    changed |= state.menu.clamp_selection();
    if input.menu_accept {
        if activate_add_menu_selection(project, panel_width, panel_height, state) {
            return true;
        }
        return changed;
    }
    if !input.left_clicked {
        return changed;
    }
    let Some((mx, my)) = input.mouse_pos else {
        state.menu = AddNodeMenuState::closed();
        return true;
    };
    if let Some(index) = state.menu.item_at(mx, my) {
        let _ = state.menu.select_index(index);
        return activate_add_menu_selection(project, panel_width, panel_height, state);
    } else if !state.menu.rect().contains(mx, my) {
        state.menu = AddNodeMenuState::closed();
        return true;
    }
    changed
}

fn activate_add_menu_selection(
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let Some(entry) = state.menu.selected_entry() else {
        return false;
    };
    match entry {
        AddNodeMenuEntry::Category(category) => {
            let changed = state.menu.open_category(category);
            state.hover_menu_item = None;
            changed
        }
        AddNodeMenuEntry::Back => {
            let changed = state.menu.close_category();
            state.hover_menu_item = None;
            changed
        }
        AddNodeMenuEntry::Option(option_index) => {
            let option = ADD_NODE_OPTIONS[option_index];
            let drop_cursor_x = state.menu.open_cursor_x;
            let drop_cursor_y = state.menu.open_cursor_y;
            let (spawn_x, spawn_y) = screen_to_graph(drop_cursor_x, drop_cursor_y, state);
            let node_id =
                project.add_node(option.kind, spawn_x, spawn_y, panel_width, panel_height);
            if let Some(link) = hover_insert_link_at_cursor(
                project,
                panel_width,
                panel_height,
                state,
                drop_cursor_x,
                drop_cursor_y,
                node_id,
            ) {
                let _ =
                    project.insert_node_on_primary_link(node_id, link.source_id, link.target_id);
            }
            state.menu = AddNodeMenuState::closed();
            state.hover_menu_item = None;
            true
        }
    }
}

fn handle_drag_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    drag::handle_drag_input(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

fn handle_wire_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    wire::handle_wire_input(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

fn inside_panel(x: i32, y: i32, panel_width: usize, panel_height: usize) -> bool {
    let editor_h = editor_panel_height(panel_height) as i32;
    x >= 0 && y >= 0 && x < panel_width as i32 && y < editor_h
}

fn collapse_auto_expanded_binding_nodes(
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    hover::collapse_auto_expanded_binding_nodes(
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

fn update_hover_state(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    hover::update_hover_state(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

fn wire_drag_source_kind(project: &GuiProject, wire: WireDragState) -> Option<ResourceKind> {
    wire::wire_drag_source_kind(project, wire)
}

fn screen_to_graph(x: i32, y: i32, state: &PreviewState) -> (i32, i32) {
    let zoom = state.zoom.max(0.001);
    let gx = ((x as f32 - state.pan_x) / zoom).round() as i32;
    let gy = ((y as f32 - state.pan_y) / zoom).round() as i32;
    (gx, gy)
}

fn graph_point_to_panel(x: i32, y: i32, state: &PreviewState) -> (i32, i32) {
    let sx = (x as f32 * state.zoom + state.pan_x).round() as i32;
    let sy = (y as f32 * state.zoom + state.pan_y).round() as i32;
    (sx, sy)
}

fn map_graph_path_to_panel(points: &[(i32, i32)], state: &PreviewState) -> Vec<(i32, i32)> {
    points
        .iter()
        .copied()
        .map(|(x, y)| graph_point_to_panel(x, y, state))
        .collect()
}

fn graph_rect_to_panel(rect: Rect, state: &PreviewState) -> Rect {
    let x = (rect.x as f32 * state.zoom + state.pan_x).round() as i32;
    let y = (rect.y as f32 * state.zoom + state.pan_y).round() as i32;
    let w = (rect.w as f32 * state.zoom).round().max(1.0) as i32;
    let h = (rect.h as f32 * state.zoom).round().max(1.0) as i32;
    Rect::new(x, y, w, h)
}

#[allow(clippy::too_many_arguments)]
fn segments_intersect(
    ax: i32,
    ay: i32,
    bx: i32,
    by: i32,
    cx: i32,
    cy: i32,
    dx: i32,
    dy: i32,
) -> bool {
    let o1 = orient(ax, ay, bx, by, cx, cy);
    let o2 = orient(ax, ay, bx, by, dx, dy);
    let o3 = orient(cx, cy, dx, dy, ax, ay);
    let o4 = orient(cx, cy, dx, dy, bx, by);
    if o1 == 0 && on_segment(ax, ay, bx, by, cx, cy) {
        return true;
    }
    if o2 == 0 && on_segment(ax, ay, bx, by, dx, dy) {
        return true;
    }
    if o3 == 0 && on_segment(cx, cy, dx, dy, ax, ay) {
        return true;
    }
    if o4 == 0 && on_segment(cx, cy, dx, dy, bx, by) {
        return true;
    }
    (o1 > 0) != (o2 > 0) && (o3 > 0) != (o4 > 0)
}

fn orient(ax: i32, ay: i32, bx: i32, by: i32, cx: i32, cy: i32) -> i64 {
    let abx = (bx - ax) as i64;
    let aby = (by - ay) as i64;
    let acx = (cx - ax) as i64;
    let acy = (cy - ay) as i64;
    abx * acy - aby * acx
}

fn on_segment(ax: i32, ay: i32, bx: i32, by: i32, px: i32, py: i32) -> bool {
    px >= ax.min(bx) && px <= ax.max(bx) && py >= ay.min(by) && py <= ay.max(by)
}

fn pin_hit_radius_world(state: &PreviewState) -> i32 {
    ((PIN_HIT_RADIUS_PX as f32) / state.zoom.max(0.001))
        .round()
        .clamp(1.0, 64.0) as i32
}

fn apply_zoom(mx: i32, my: i32, wheel_lines_y: f32, state: &mut PreviewState) -> bool {
    let old_zoom = state.zoom;
    let zoom_factor = ZOOM_SENSITIVITY.powf(wheel_lines_y);
    let new_zoom = (old_zoom * zoom_factor).clamp(MIN_ZOOM, MAX_ZOOM);
    if (new_zoom - old_zoom).abs() < 1e-4 {
        return false;
    }
    let world_x = (mx as f32 - state.pan_x) / old_zoom.max(0.001);
    let world_y = (my as f32 - state.pan_y) / old_zoom.max(0.001);
    state.zoom = new_zoom;
    state.pan_x = mx as f32 - world_x * new_zoom;
    state.pan_y = my as f32 - world_y * new_zoom;
    true
}

fn focus_all_nodes(
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let Some(bounds) = project.graph_bounds() else {
        return false;
    };
    focus_bounds(bounds, panel_width, panel_height, state)
}

fn focus_bounds(
    bounds: GraphBounds,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let editor_h = editor_panel_height(panel_height) as f32;
    let bounds_w = (bounds.max_x - bounds.min_x).max(1) as f32;
    let bounds_h = (bounds.max_y - bounds.min_y).max(1) as f32;
    let avail_w = (panel_width as f32 - FOCUS_MARGIN_PX * 2.0).max(32.0);
    let avail_h = (editor_h - FOCUS_MARGIN_PX * 2.0).max(32.0);
    let zoom = (avail_w / bounds_w)
        .min(avail_h / bounds_h)
        .clamp(MIN_ZOOM, MAX_ZOOM);
    let center_x = (bounds.min_x + bounds.max_x) as f32 * 0.5;
    let center_y = (bounds.min_y + bounds.max_y) as f32 * 0.5;
    let pan_x = panel_width as f32 * 0.5 - center_x * zoom;
    let pan_y = editor_h * 0.5 - center_y * zoom;
    let changed = (state.zoom - zoom).abs() > 1e-3
        || (state.pan_x - pan_x).abs() > 0.5
        || (state.pan_y - pan_y).abs() > 0.5;
    state.zoom = zoom;
    state.pan_x = pan_x;
    state.pan_y = pan_y;
    changed
}

#[cfg(test)]
mod tests;

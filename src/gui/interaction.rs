//! GUI input handling and graph-editor interaction logic.

use crate::runtime_config::V2Config;
use std::time::Duration;

use super::geometry::Rect;
use super::help::{build_global_help_modal, build_node_help_modal, build_param_help_modal};
use super::project::{
    input_pin_center, node_expand_toggle_rect, node_param_dropdown_rect, node_param_row_rect,
    output_pin_center, GraphBounds, GuiProject, ProjectNode, ResourceKind,
    NODE_PARAM_DROPDOWN_ROW_HEIGHT, NODE_WIDTH,
};
use super::state::{
    AddNodeMenuEntry, AddNodeMenuState, ExportMenuItem, HoverInsertLink, HoverParamTarget,
    InputSnapshot, LinkCutState, MainMenuItem, MainMenuState, PanDragState, ParamDropdownState,
    ParamEditState, PendingAppAction, PreviewState, RightMarqueeState, WireDragState,
    ADD_NODE_OPTIONS, MAIN_MENU_WIDTH,
};
use super::timeline::{
    editor_panel_height, frame_from_track_x, next_looped_frame, pause_button_rect,
    play_button_rect, timeline_rect, track_rect,
};

const PIN_HIT_RADIUS_PX: i32 = 10;
const MIN_ZOOM: f32 = 0.35;
const MAX_ZOOM: f32 = 2.75;
const ZOOM_SENSITIVITY: f32 = 1.12;
const FOCUS_MARGIN_PX: f32 = 28.0;
const PARAM_WIRE_EXIT_TAIL_PX: i32 = 18;
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
        changed = true;
    }
    if input.new_project || state.request_new_project {
        state.request_new_project = false;
        *project = GuiProject::new_empty(config.width, config.height);
        state.frame_index = 0;
        state.timeline_accum_secs = 0.0;
        state.timeline_scrub_active = false;
        state.drag = None;
        state.wire_drag = None;
        state.link_cut = None;
        state.pan_drag = None;
        state.right_marquee = None;
        state.param_edit = None;
        state.param_dropdown = None;
        state.selected_nodes.clear();
        state.pan_x = 0.0;
        state.pan_y = 0.0;
        state.zoom = 1.0;
        state.menu = AddNodeMenuState::closed();
        state.main_menu = MainMenuState::closed();
        state.active_node = None;
        state.hover_node = None;
        state.hover_output_pin = None;
        state.hover_input_pin = None;
        state.hover_param_target = None;
        state.hover_insert_link = None;
        state.hover_dropdown_item = None;
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

    let (timeline_changed, timeline_consumed) =
        handle_timeline_input(&input, viewport_width, panel_height, state);
    changed |= timeline_changed;
    if timeline_consumed {
        state.drag = None;
        state.wire_drag = None;
        state.link_cut = None;
        state.pan_drag = None;
        state.right_marquee = None;
        state.hover_param_target = None;
        state.param_dropdown = None;
        state.param_edit = None;
        state.menu = AddNodeMenuState::closed();
        state.main_menu = MainMenuState::closed();
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return changed;
    }

    let (wheel_param_changed, wheel_consumed) =
        handle_param_wheel_input(&input, project, panel_width, panel_height, state);
    changed |= wheel_param_changed;
    let mut pan_zoom_input = input.clone();
    if wheel_consumed {
        pan_zoom_input.wheel_lines_y = 0.0;
    }
    changed |=
        handle_pan_zoom_and_focus(&pan_zoom_input, project, panel_width, panel_height, state);
    if state.pan_drag.is_some() {
        state.drag = None;
        state.wire_drag = None;
        state.hover_param_target = None;
        state.param_dropdown = None;
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return true;
    }

    changed |= handle_link_cut(&input, project, panel_width, panel_height, state);
    if state.link_cut.is_some() {
        state.drag = None;
        state.wire_drag = None;
        state.hover_param_target = None;
        state.param_dropdown = None;
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return true;
    }

    changed |= handle_right_selection(&input, project, panel_width, panel_height, state);
    if state.right_marquee.is_some() {
        state.drag = None;
        state.wire_drag = None;
        state.hover_param_target = None;
        state.param_dropdown = None;
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return true;
    }

    changed |= handle_add_menu_toggle(&input, panel_width, panel_height, state);
    changed |= handle_main_menu_toggle(&input, panel_width, panel_height, state);
    changed |= update_hover_state(&input, project, panel_width, panel_height, state);
    changed |= handle_node_open_toggle(&input, project, panel_width, panel_height, state);
    let (param_changed, param_click_consumed) =
        handle_param_edit_input(&input, project, panel_width, panel_height, state);
    changed |= param_changed;
    if param_click_consumed {
        state.drag = None;
        state.wire_drag = None;
        state.hover_param_target = None;
        state.param_dropdown = None;
        let _ = collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return true;
    }
    if state.param_edit.is_some() {
        state.drag = None;
        state.wire_drag = None;
        state.hover_param_target = None;
        state.param_dropdown = None;
        changed |= collapse_auto_expanded_binding_nodes(project, panel_width, panel_height, state);
        state.prev_left_down = input.left_down;
        return changed;
    }
    if state.export_menu.open || state.main_menu.open {
        changed |= handle_main_export_menu_input(&input, panel_width, panel_height, state);
    } else if state.menu.open {
        changed |= handle_add_menu_input(&input, project, panel_width, panel_height, state);
    } else {
        changed |= handle_delete_selected_nodes(&input, project, state);
        changed |= handle_parameter_shortcuts(&input, project, state);
        changed |= handle_wire_input(&input, project, panel_width, panel_height, state);
        if state.wire_drag.is_none() {
            changed |= handle_drag_input(&input, project, panel_width, panel_height, state);
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
    state.menu = AddNodeMenuState::closed();
    state.main_menu = MainMenuState::closed();
    state.drag = None;
    state.wire_drag = None;
    state.link_cut = None;
    state.pan_drag = None;
    state.right_marquee = None;
    state.param_edit = None;
    state.param_dropdown = None;
    state.hover_param_target = None;
    state.hover_dropdown_item = None;
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
) -> bool {
    let mut advanced = false;
    if !state.paused {
        let tick_secs = 1.0 / timeline_fps.max(1) as f32;
        state.timeline_accum_secs += frame_delta.as_secs_f32();
        while state.timeline_accum_secs >= tick_secs {
            state.timeline_accum_secs -= tick_secs;
            state.frame_index = next_looped_frame(state.frame_index);
            advanced = true;
        }
    }
    advanced
}

fn handle_timeline_input(
    input: &InputSnapshot,
    viewport_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> (bool, bool) {
    let mut changed = false;
    let mut consumed = false;
    let timeline = timeline_rect(viewport_width, panel_height);
    let play = play_button_rect(timeline);
    let pause = pause_button_rect(timeline);
    let track = track_rect(timeline);
    let mouse_pos = input.mouse_pos;
    if !input.left_down && state.timeline_scrub_active {
        state.timeline_scrub_active = false;
        return (false, true);
    }
    if let Some((mx, my)) = mouse_pos {
        if input.left_clicked && play.contains(mx, my) {
            state.paused = false;
            state.timeline_scrub_active = false;
            return (true, true);
        }
        if input.left_clicked && pause.contains(mx, my) {
            state.paused = true;
            state.timeline_scrub_active = false;
            return (true, true);
        }
        if input.left_clicked && track.contains(mx, my) {
            state.timeline_scrub_active = true;
            consumed = true;
            changed |= scrub_frame_from_timeline(track, mx, state);
        } else if state.timeline_scrub_active && input.left_down {
            consumed = true;
            changed |= scrub_frame_from_timeline(track, mx, state);
        }
        if input.left_clicked && timeline.contains(mx, my) {
            consumed = true;
        }
    } else if state.timeline_scrub_active {
        consumed = true;
    }
    (changed, consumed)
}

fn scrub_frame_from_timeline(track: Rect, mouse_x: i32, state: &mut PreviewState) -> bool {
    let frame = frame_from_track_x(track, mouse_x);
    if frame == state.frame_index {
        return false;
    }
    state.frame_index = frame;
    state.timeline_accum_secs = 0.0;
    true
}

fn handle_param_wheel_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> (bool, bool) {
    if input.wheel_lines_y.abs() <= f32::EPSILON {
        return (false, false);
    }
    if state.param_edit.is_some()
        || state.menu.open
        || state.main_menu.open
        || state.export_menu.open
        || state.param_dropdown.is_some()
    {
        return (false, false);
    }
    let Some((mx, my)) = input.mouse_pos else {
        return (false, false);
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
        return (false, false);
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
    let Some(node_id) = project.node_at(graph_x, graph_y) else {
        return (false, false);
    };
    let Some(param_index) = project.param_row_at(node_id, graph_x, graph_y) else {
        return (false, false);
    };
    if !project.param_value_box_contains(node_id, param_index, graph_x, graph_y) {
        return (false, false);
    }
    let mut changed = project.select_param(node_id, param_index);
    state.active_node = Some(node_id);
    changed |= project.adjust_param(node_id, param_index, input.wheel_lines_y);
    state.hover_dropdown_item = None;
    (changed, true)
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
                state.hover_param_target = None;
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
        state.hover_param_target = None;
        state.param_edit = None;
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return true;
    }
    let (x, y) = input
        .mouse_pos
        .unwrap_or((panel_width as i32 / 2, panel_height as i32 / 3));
    state.menu = AddNodeMenuState::open_at(x, y, panel_width, editor_panel_height(panel_height));
    state.main_menu = MainMenuState::closed();
    state.drag = None;
    state.wire_drag = None;
    state.hover_param_target = None;
    state.param_edit = None;
    state.param_dropdown = None;
    state.hover_dropdown_item = None;
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
    state.param_edit = None;
    state.param_dropdown = None;
    state.hover_dropdown_item = None;
    state.drag = None;
    state.wire_drag = None;
    state.hover_param_target = None;
    true
}

fn handle_main_export_menu_input(
    input: &InputSnapshot,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
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

fn apply_export_menu_text_input(input: &InputSnapshot, state: &mut PreviewState) -> bool {
    let selected = state.export_menu.selected_item();
    let target = match selected {
        ExportMenuItem::Directory => Some(&mut state.export_menu.directory),
        ExportMenuItem::FileName => Some(&mut state.export_menu.file_name),
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
        || state.hover_export_menu_item.is_some()
        || state.hover_export_menu_close;
    state.export_menu.open = false;
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
    if state.param_edit.is_some() || state.param_dropdown.is_some() {
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
    state.hover_param_target = None;
    state.drag = None;
    state.wire_drag = None;
    state.right_marquee = None;
    state.link_cut = None;
    state.param_edit = None;
    state.param_dropdown = None;
    state.hover_dropdown_item = None;
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
        if !inside_panel(mx, my, panel_width, panel_height) {
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

fn marquee_moved(marquee: RightMarqueeState) -> bool {
    (marquee.cursor_x - marquee.start_x).abs() > 4 || (marquee.cursor_y - marquee.start_y).abs() > 4
}

fn collect_marquee_nodes(
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

fn screen_rect_to_graph_rect(
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
fn panel_graph_rect(
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
) -> (i32, i32, i32, i32) {
    let max_x = panel_width.saturating_sub(1) as i32;
    let max_y = editor_panel_height(panel_height).saturating_sub(1) as i32;
    screen_rect_to_graph_rect(0, 0, max_x, max_y, state)
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(test), allow(dead_code))]
fn rects_overlap(
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

fn set_single_selection(state: &mut PreviewState, node_id: u32) -> bool {
    if state.selected_nodes.len() == 1 && state.selected_nodes[0] == node_id {
        return false;
    }
    state.selected_nodes.clear();
    state.selected_nodes.push(node_id);
    true
}

fn set_multi_selection(state: &mut PreviewState, mut nodes: Vec<u32>) -> bool {
    nodes.sort_unstable();
    nodes.dedup();
    if state.selected_nodes == nodes {
        return false;
    }
    state.selected_nodes = nodes;
    state.active_node = state.selected_nodes.first().copied();
    true
}

fn clear_selection(state: &mut PreviewState) -> bool {
    if state.selected_nodes.is_empty() && state.active_node.is_none() {
        return false;
    }
    state.selected_nodes.clear();
    state.active_node = None;
    true
}

fn handle_param_edit_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> (bool, bool) {
    let mut changed = false;
    if state.menu.open || state.main_menu.open || state.export_menu.open {
        return (changed, false);
    }
    changed |= apply_param_text_edits(input, project, state);
    if !input.left_clicked {
        return (changed, false);
    }
    if handle_dropdown_click(input, project, panel_width, panel_height, state) {
        return (true, true);
    }
    let consumed = handle_param_click(input, project, panel_width, panel_height, state);
    (changed, consumed)
}

fn apply_param_text_edits(
    input: &InputSnapshot,
    project: &mut GuiProject,
    state: &mut PreviewState,
) -> bool {
    if let Some(edit) = state.param_edit.as_ref() {
        if !project.node_expanded(edit.node_id) {
            state.param_edit = None;
            return true;
        }
    }
    let Some(edit) = state.param_edit.as_mut() else {
        return false;
    };
    clamp_param_edit_indices(edit);
    let mut changed = false;
    if input.param_cancel {
        state.param_edit = None;
        return true;
    }
    if input.param_select_all {
        changed |= select_all_param_text(edit);
    }
    if input.param_dec {
        changed |= move_param_cursor_left(edit, input.shift_down);
    }
    if input.param_inc {
        changed |= move_param_cursor_right(edit, input.shift_down);
    }
    if input.param_backspace {
        changed |= backspace_param_text(edit);
    }
    if input.param_delete {
        changed |= delete_param_text(edit);
    }
    if !input.typed_text.is_empty() {
        for ch in input.typed_text.chars() {
            if insert_param_char(edit, ch) {
                changed = true;
            }
        }
    }
    if input.param_commit && commit_param_edit(project, edit) {
        state.param_edit = None;
        return true;
    }
    changed
}

fn handle_param_click(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let Some((mx, my)) = input.mouse_pos else {
        let _ = finish_param_edit(project, state);
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return false;
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
        let _ = finish_param_edit(project, state);
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return false;
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
    let Some(node_id) = project.node_at(graph_x, graph_y) else {
        let _ = finish_param_edit(project, state);
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return false;
    };
    let Some(param_index) = project.param_row_at(node_id, graph_x, graph_y) else {
        let _ = finish_param_edit(project, state);
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return false;
    };
    let _ = project.select_param(node_id, param_index);
    state.active_node = Some(node_id);
    if !project.param_value_box_contains(node_id, param_index, graph_x, graph_y) {
        let _ = finish_param_edit(project, state);
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return true;
    }
    if project.param_is_dropdown(node_id, param_index) {
        state.param_edit = None;
        if state
            .param_dropdown
            .map(|dropdown| dropdown.node_id == node_id && dropdown.param_index == param_index)
            .unwrap_or(false)
        {
            state.param_dropdown = None;
            state.hover_dropdown_item = None;
            return true;
        }
        state.param_dropdown = Some(ParamDropdownState {
            node_id,
            param_index,
        });
        state.hover_dropdown_item = None;
        return true;
    }
    state.param_dropdown = None;
    state.hover_dropdown_item = None;
    let same_edit_target = state
        .param_edit
        .as_ref()
        .map(|edit| edit.node_id == node_id && edit.param_index == param_index)
        .unwrap_or(false);
    if same_edit_target {
        if let Some(edit) = state.param_edit.as_mut() {
            let end = edit.buffer.len();
            edit.cursor = end;
            edit.anchor = end;
        }
        return true;
    }
    let _ = finish_param_edit(project, state);
    let _ = start_param_edit(project, state, node_id, param_index);
    true
}

fn handle_dropdown_click(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let Some(dropdown) = state.param_dropdown else {
        return false;
    };
    let Some((mx, my)) = input.mouse_pos else {
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return true;
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return true;
    }
    if let Some(option_index) = dropdown_option_at_cursor(project, state, mx, my) {
        let _ =
            project.set_param_dropdown_index(dropdown.node_id, dropdown.param_index, option_index);
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return true;
    }
    state.param_dropdown = None;
    state.hover_dropdown_item = None;
    true
}

fn dropdown_option_at_cursor(
    project: &GuiProject,
    state: &PreviewState,
    mx: i32,
    my: i32,
) -> Option<usize> {
    let dropdown = state.param_dropdown?;
    let node = project.node(dropdown.node_id)?;
    let options = project.node_param_dropdown_options(dropdown.node_id, dropdown.param_index)?;
    let list_world = node_param_dropdown_rect(node, dropdown.param_index, options.len())?;
    let list_panel = graph_rect_to_panel(list_world, state);
    if !list_panel.contains(mx, my) {
        return None;
    }
    for (index, _) in options.iter().enumerate() {
        let row_world = Rect::new(
            list_world.x,
            list_world.y + index as i32 * NODE_PARAM_DROPDOWN_ROW_HEIGHT,
            list_world.w,
            NODE_PARAM_DROPDOWN_ROW_HEIGHT,
        );
        let row_panel = graph_rect_to_panel(row_world, state);
        if row_panel.contains(mx, my) {
            return Some(index);
        }
    }
    None
}

fn start_param_edit(
    project: &GuiProject,
    state: &mut PreviewState,
    node_id: u32,
    param_index: usize,
) -> bool {
    if !project.param_supports_text_edit(node_id, param_index) {
        return false;
    }
    if state
        .param_edit
        .as_ref()
        .map(|edit| edit.node_id == node_id && edit.param_index == param_index)
        .unwrap_or(false)
    {
        return false;
    }
    let Some(value_text) = project.node_param_raw_text(node_id, param_index) else {
        return false;
    };
    state.param_edit = Some(ParamEditState {
        node_id,
        param_index,
        buffer: value_text.to_owned(),
        cursor: 0,
        anchor: 0,
    });
    if let Some(edit) = state.param_edit.as_mut() {
        let len = edit.buffer.len();
        edit.cursor = len;
        edit.anchor = 0;
    }
    true
}

fn finish_param_edit(project: &mut GuiProject, state: &mut PreviewState) -> bool {
    let Some(mut edit) = state.param_edit.take() else {
        return false;
    };
    let _ = commit_param_edit(project, &mut edit);
    true
}

fn commit_param_edit(project: &mut GuiProject, edit: &mut ParamEditState) -> bool {
    let Ok(value) = edit.buffer.trim().parse::<f32>() else {
        return false;
    };
    let _ = project.set_param_value(edit.node_id, edit.param_index, value);
    true
}

fn can_append_param_char(current: &str, ch: char) -> bool {
    if !(ch.is_ascii_digit() || ch == '-' || ch == '.') {
        return false;
    }
    let mut next = String::with_capacity(current.len() + ch.len_utf8());
    next.push_str(current);
    next.push(ch);
    is_valid_param_buffer(next.as_str())
}

fn is_valid_param_buffer(buffer: &str) -> bool {
    for (index, ch) in buffer.char_indices() {
        if ch.is_ascii_digit() {
            continue;
        }
        if ch == '-' {
            if index == 0 {
                continue;
            }
            return false;
        }
        if ch == '.' {
            if buffer[..index].contains('.') {
                return false;
            }
            continue;
        }
        return false;
    }
    true
}

fn clamp_param_edit_indices(edit: &mut ParamEditState) {
    let len = edit.buffer.len();
    edit.cursor = edit.cursor.min(len);
    edit.anchor = edit.anchor.min(len);
}

fn has_param_selection(edit: &ParamEditState) -> bool {
    edit.cursor != edit.anchor
}

fn param_selection_bounds(edit: &ParamEditState) -> (usize, usize) {
    (edit.cursor.min(edit.anchor), edit.cursor.max(edit.anchor))
}

fn collapse_param_selection(edit: &mut ParamEditState, at: usize) {
    let clamped = at.min(edit.buffer.len());
    edit.cursor = clamped;
    edit.anchor = clamped;
}

fn select_all_param_text(edit: &mut ParamEditState) -> bool {
    let len = edit.buffer.len();
    if len == 0 {
        return false;
    }
    if edit.anchor == 0 && edit.cursor == len {
        return false;
    }
    edit.anchor = 0;
    edit.cursor = len;
    true
}

fn delete_param_selection(edit: &mut ParamEditState) -> bool {
    if !has_param_selection(edit) {
        return false;
    }
    let (start, end) = param_selection_bounds(edit);
    edit.buffer.replace_range(start..end, "");
    collapse_param_selection(edit, start);
    true
}

fn backspace_param_text(edit: &mut ParamEditState) -> bool {
    if delete_param_selection(edit) {
        return true;
    }
    if edit.cursor == 0 {
        return false;
    }
    let start = prev_char_boundary(&edit.buffer, edit.cursor);
    edit.buffer.replace_range(start..edit.cursor, "");
    collapse_param_selection(edit, start);
    true
}

fn delete_param_text(edit: &mut ParamEditState) -> bool {
    if delete_param_selection(edit) {
        return true;
    }
    if edit.cursor >= edit.buffer.len() {
        return false;
    }
    let end = next_char_boundary(&edit.buffer, edit.cursor);
    edit.buffer.replace_range(edit.cursor..end, "");
    collapse_param_selection(edit, edit.cursor);
    true
}

fn insert_param_char(edit: &mut ParamEditState, ch: char) -> bool {
    if !(ch.is_ascii_digit() || ch == '-' || ch == '.') {
        return false;
    }
    let candidate = ch.to_string();
    let mut next = edit.buffer.clone();
    if has_param_selection(edit) {
        let (start, end) = param_selection_bounds(edit);
        next.replace_range(start..end, candidate.as_str());
        if !is_valid_param_buffer(next.as_str()) {
            return false;
        }
        edit.buffer = next;
        let next_cursor = start + candidate.len();
        collapse_param_selection(edit, next_cursor);
        return true;
    }
    if edit.cursor == edit.buffer.len() && !can_append_param_char(edit.buffer.as_str(), ch) {
        return false;
    }
    next.insert(edit.cursor, ch);
    if !is_valid_param_buffer(next.as_str()) {
        return false;
    }
    edit.buffer = next;
    collapse_param_selection(edit, edit.cursor + ch.len_utf8());
    true
}

fn move_param_cursor_left(edit: &mut ParamEditState, extend_selection: bool) -> bool {
    if edit.cursor == 0 && (!has_param_selection(edit) || extend_selection) {
        return false;
    }
    if !extend_selection && has_param_selection(edit) {
        let (start, _) = param_selection_bounds(edit);
        collapse_param_selection(edit, start);
        return true;
    }
    let next = prev_char_boundary(&edit.buffer, edit.cursor);
    if next == edit.cursor {
        return false;
    }
    edit.cursor = next;
    if !extend_selection {
        edit.anchor = edit.cursor;
    }
    true
}

fn move_param_cursor_right(edit: &mut ParamEditState, extend_selection: bool) -> bool {
    if edit.cursor >= edit.buffer.len() && (!has_param_selection(edit) || extend_selection) {
        return false;
    }
    if !extend_selection && has_param_selection(edit) {
        let (_, end) = param_selection_bounds(edit);
        collapse_param_selection(edit, end);
        return true;
    }
    let next = next_char_boundary(&edit.buffer, edit.cursor);
    if next == edit.cursor {
        return false;
    }
    edit.cursor = next;
    if !extend_selection {
        edit.anchor = edit.cursor;
    }
    true
}

fn prev_char_boundary(text: &str, index: usize) -> usize {
    if index == 0 {
        return 0;
    }
    let clamped = index.min(text.len());
    text[..clamped]
        .char_indices()
        .next_back()
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn next_char_boundary(text: &str, index: usize) -> usize {
    let clamped = index.min(text.len());
    if clamped >= text.len() {
        return text.len();
    }
    text[clamped..]
        .chars()
        .next()
        .map(|ch| clamped + ch.len_utf8())
        .unwrap_or(text.len())
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
                state.hover_param_target = None;
                state.param_edit = None;
                state.param_dropdown = None;
                state.hover_dropdown_item = None;
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
    let obstacles = collect_panel_node_obstacles(project, state);
    let (view_x0, view_y0, view_x1, view_y1) = panel_graph_rect(panel_width, panel_height, state);
    let target_ids = project.node_ids_overlapping_graph_rect(view_x0, view_y0, view_x1, view_y1);
    for target_id in target_ids.iter().copied() {
        collect_cut_links_for_target(
            project,
            state,
            cut,
            obstacles.as_slice(),
            target_id,
            &mut links,
        );
    }
    let cut_outside_panel = !inside_panel(cut.start_x, cut.start_y, panel_width, panel_height)
        || !inside_panel(cut.cursor_x, cut.cursor_y, panel_width, panel_height);
    if (links.is_empty() || cut_outside_panel) && target_ids.len() < project.node_count() {
        for target in project.nodes() {
            collect_cut_links_for_target(
                project,
                state,
                cut,
                obstacles.as_slice(),
                target.id(),
                &mut links,
            );
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
    obstacles: &[super::scene::wire_route::NodeObstacle],
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
        let (to_x, to_y) = graph_point_to_panel(to_x, to_y, state);
        let (from_x, from_y) = graph_point_to_panel(from_x, from_y, state);
        if segments_intersect(
            cut.start_x,
            cut.start_y,
            cut.cursor_x,
            cut.cursor_y,
            from_x,
            from_y,
            to_x,
            to_y,
        ) {
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
        let Some(row) = node_param_row_rect(target, param_index) else {
            continue;
        };
        let to_x = row.x + row.w - 4;
        let to_y = row.y + row.h / 2;
        let (from_x, from_y) = graph_point_to_panel(from_x, from_y, state);
        let (to_x, to_y) = graph_point_to_panel(to_x, to_y, state);
        let exit_x = from_x.saturating_add(PARAM_WIRE_EXIT_TAIL_PX);
        let entry_x = to_x.saturating_add(PARAM_WIRE_ENTRY_TAIL_PX);
        let route = super::scene::wire_route::route_param_path(
            (exit_x, from_y),
            (entry_x, to_y),
            obstacles,
        );
        if segments_intersect(
            cut.start_x,
            cut.start_y,
            cut.cursor_x,
            cut.cursor_y,
            from_x,
            from_y,
            exit_x,
            from_y,
        ) || cut_intersects_path(cut, route.as_slice())
            || segments_intersect(
                cut.start_x,
                cut.start_y,
                cut.cursor_x,
                cut.cursor_y,
                entry_x,
                to_y,
                to_x,
                to_y,
            )
        {
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

fn node_rect(node: &ProjectNode, state: &PreviewState) -> Rect {
    graph_rect_to_panel(
        Rect::new(node.x(), node.y(), NODE_WIDTH, node.card_height()),
        state,
    )
}

fn collect_panel_node_obstacles(
    project: &GuiProject,
    state: &PreviewState,
) -> Vec<super::scene::wire_route::NodeObstacle> {
    let mut out = Vec::new();
    for node in project.nodes() {
        out.push(super::scene::wire_route::NodeObstacle {
            rect: node_rect(node, state),
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
    changed |= state
        .menu
        .apply_query_input(input.typed_text.as_str(), input.param_backspace);
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
            let (spawn_x, spawn_y) =
                screen_to_graph(state.menu.open_cursor_x, state.menu.open_cursor_y, state);
            project.add_node(option.kind, spawn_x, spawn_y, panel_width, panel_height);
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
    let mut changed = false;
    if input.left_clicked {
        changed |= begin_drag_if_node_hit(input, project, panel_width, panel_height, state);
    }
    let dragged_node_ids = state
        .drag
        .map(|drag| drag_selection_node_ids(state, drag.node_id))
        .unwrap_or_default();
    let is_group_drag = dragged_node_ids.len() > 1;
    if !input.left_down {
        if let Some(drag) = state.drag {
            if !is_group_drag {
                if let Some(link) = resolve_insert_link_on_release(
                    input,
                    project,
                    panel_width,
                    panel_height,
                    state,
                    drag.node_id,
                ) {
                    changed |= project.insert_node_on_primary_link(
                        drag.node_id,
                        link.source_id,
                        link.target_id,
                    );
                }
                changed |=
                    snap_dragged_node_out_of_overlap(project, drag, panel_width, panel_height);
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
    if !inside_panel(mx, my, panel_width, panel_height) {
        changed |= state.hover_insert_link.take().is_some();
        return changed;
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
    if is_group_drag {
        changed |= move_drag_selection_by_anchor_delta(
            project,
            drag,
            dragged_node_ids.as_slice(),
            graph_x,
            graph_y,
            panel_width,
            panel_height,
        );
        changed |= state.hover_insert_link.take().is_some();
    } else {
        let node_x = graph_x - drag.offset_x;
        let node_y = graph_y - drag.offset_y;
        changed |= project.move_node(drag.node_id, node_x, node_y, panel_width, panel_height);
        let next_insert_hover = hover_insert_link_at_cursor(
            project,
            panel_width,
            panel_height,
            state,
            mx,
            my,
            drag.node_id,
        );
        if state.hover_insert_link != next_insert_hover {
            state.hover_insert_link = next_insert_hover;
            changed = true;
        }
    }
    changed
}

/// Return drag group ids for one anchor node.
///
/// Multi-node dragging is enabled only when the anchor node is part of the
/// current selection and at least two nodes are selected.
fn drag_selection_node_ids(state: &PreviewState, anchor_node_id: u32) -> Vec<u32> {
    if state.selected_nodes.len() > 1 && state.selected_nodes.contains(&anchor_node_id) {
        return state.selected_nodes.clone();
    }
    vec![anchor_node_id]
}

/// Move selected drag nodes by the anchor node cursor delta.
///
/// The anchor node follows the cursor first; the resolved delta is then applied
/// to the remaining selected nodes to keep group movement coherent.
fn move_drag_selection_by_anchor_delta(
    project: &mut GuiProject,
    drag: super::state::DragState,
    dragged_node_ids: &[u32],
    graph_x: i32,
    graph_y: i32,
    panel_width: usize,
    panel_height: usize,
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
        panel_width,
        panel_height,
    );
    let Some(anchor_after) = project.node(drag.node_id) else {
        return changed;
    };
    let dx = anchor_after.x() - anchor_before_x;
    let dy = anchor_after.y() - anchor_before_y;
    if dx == 0 && dy == 0 {
        return changed;
    }
    for node_id in dragged_node_ids.iter().copied() {
        if node_id == drag.node_id {
            continue;
        }
        let Some(node) = project.node(node_id) else {
            continue;
        };
        let next_x = node.x().saturating_add(dx);
        let next_y = node.y().saturating_add(dy);
        changed |= project.move_node(node_id, next_x, next_y, panel_width, panel_height);
    }
    changed
}

fn handle_wire_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    if input.left_clicked {
        changed |= begin_wire_drag_if_pin_hit(input, project, panel_width, panel_height, state);
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
                if let Some(target) = resolve_texture_param_target_on_release(
                    input,
                    project,
                    panel_width,
                    panel_height,
                    state,
                    wire,
                ) {
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
///
/// This confirms the release cursor against parameter hit-testing so texture
/// binds stay reliable even if hover state did not update on the same frame.
fn resolve_texture_param_target_on_release(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
    wire: WireDragState,
) -> Option<HoverParamTarget> {
    if input.mouse_pos.is_none() {
        return state.hover_param_target;
    }
    let (mx, my) = input.mouse_pos.unwrap_or((wire.cursor_x, wire.cursor_y));
    if !inside_panel(mx, my, panel_width, panel_height) {
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
        }
    }
    let node_id = project.node_at(graph_x, graph_y)?;
    // Input-pin drops are explicit primary-input links.
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

fn begin_wire_drag_if_pin_hit(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let Some((mx, my)) = input.mouse_pos else {
        return false;
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
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

fn begin_drag_if_node_hit(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let Some((mx, my)) = input.mouse_pos else {
        return false;
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
        return false;
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
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
            return project.toggle_node_expanded(node_id, panel_width, panel_height);
        }
    }
    if state.drag.map(|drag| drag.node_id) == Some(node_id) {
        return false;
    }
    state.drag = Some(super::state::DragState {
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
fn snap_dragged_node_out_of_overlap(
    project: &mut GuiProject,
    drag: super::state::DragState,
    panel_width: usize,
    panel_height: usize,
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
    project.move_node(drag.node_id, next_x, next_y, panel_width, panel_height)
}

/// Return true when two rectangles overlap with positive area.
fn rects_overlap_strict(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    let ax1 = ax + aw;
    let ay1 = ay + ah;
    let bx1 = bx + bw;
    let by1 = by + bh;
    ax < bx1 && ax1 > bx && ay < by1 && ay1 > by
}

/// Resolve one hovered wire insertion candidate at cursor position.
fn hover_insert_link_at_cursor(
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
    cursor_x: i32,
    cursor_y: i32,
    dragged_node_id: u32,
) -> Option<HoverInsertLink> {
    let mut best: Option<(HoverInsertLink, f32)> = None;
    let query = HoverInsertQuery {
        cursor_x,
        cursor_y,
        threshold_sq: (INSERT_WIRE_HOVER_RADIUS_PX * INSERT_WIRE_HOVER_RADIUS_PX) as f32,
        dragged_node_id,
    };
    let (view_x0, view_y0, view_x1, view_y1) = panel_graph_rect(panel_width, panel_height, state);
    let target_ids = project.node_ids_overlapping_graph_rect(view_x0, view_y0, view_x1, view_y1);
    for target_id in target_ids.iter().copied() {
        consider_hover_insert_candidate(project, state, query, target_id, &mut best);
    }
    if best.is_none() && target_ids.len() < project.node_count() {
        for target in project.nodes() {
            consider_hover_insert_candidate(project, state, query, target.id(), &mut best);
        }
    }
    best.map(|(link, _)| link)
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
    let (from_x, from_y) = graph_point_to_panel(from_x, from_y, state);
    let (to_x, to_y) = graph_point_to_panel(to_x, to_y, state);
    let dist_sq = point_to_segment_distance_sq(
        query.cursor_x as f32,
        query.cursor_y as f32,
        from_x as f32,
        from_y as f32,
        to_x as f32,
        to_y as f32,
    );
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
fn can_insert_dragged_node_on_link(
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
fn resolve_insert_link_on_release(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
    dragged_node_id: u32,
) -> Option<HoverInsertLink> {
    if let Some(link) = state.hover_insert_link {
        return Some(link);
    }
    let (mx, my) = input.mouse_pos?;
    if !inside_panel(mx, my, panel_width, panel_height) {
        return None;
    }
    hover_insert_link_at_cursor(
        project,
        panel_width,
        panel_height,
        state,
        mx,
        my,
        dragged_node_id,
    )
}

/// Return squared distance from point `p` to line segment `ab`.
fn point_to_segment_distance_sq(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
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
    if state.auto_expanded_binding_nodes.is_empty() {
        return false;
    }
    let mut changed = false;
    for node_id in state.auto_expanded_binding_nodes.drain(..) {
        changed |= project.collapse_node(node_id, panel_width, panel_height);
    }
    changed
}

fn collapse_auto_expanded_binding_nodes_except(
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
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
        changed |= project.collapse_node(node_id, panel_width, panel_height);
    }
    state.auto_expanded_binding_nodes = kept;
    changed
}

fn update_hover_state(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
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
        .and_then(|wire| wire_drag_source_kind(project, wire))
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
            changed |= collapse_auto_expanded_binding_nodes_except(
                project,
                panel_width,
                panel_height,
                state,
                None,
            );
        }
        return changed;
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
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
            changed |= collapse_auto_expanded_binding_nodes_except(
                project,
                panel_width,
                panel_height,
                state,
                None,
            );
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
            changed |= collapse_auto_expanded_binding_nodes_except(
                project,
                panel_width,
                panel_height,
                state,
                None,
            );
        }
        return changed;
    }
    if state.main_menu.open || state.export_menu.open {
        if state.main_menu.open {
            state.hover_main_menu_item = state.main_menu.item_at(mx, my);
        }
        if state.export_menu.open {
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
            changed |= collapse_auto_expanded_binding_nodes_except(
                project,
                panel_width,
                panel_height,
                state,
                None,
            );
        }
        return changed;
    }
    if state.param_dropdown.is_some() {
        state.hover_dropdown_item = dropdown_option_at_cursor(project, state, mx, my);
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
                panel_width,
                panel_height,
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

fn wire_drag_source_kind(project: &GuiProject, wire: WireDragState) -> Option<ResourceKind> {
    let source = project.node(wire.source_node_id)?;
    source.kind().output_resource_kind()
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
mod tests {
    use super::{
        backspace_param_text, can_append_param_char, handle_add_menu_input,
        handle_delete_selected_nodes, handle_drag_input, handle_help_input, handle_link_cut,
        handle_main_export_menu_input, handle_node_open_toggle, handle_param_edit_input,
        handle_param_wheel_input, handle_right_selection, handle_wire_input, insert_param_char,
        marquee_moved, move_param_cursor_left, move_param_cursor_right, rects_overlap,
        segments_intersect, update_hover_state, AddNodeMenuEntry, RightMarqueeState,
    };
    use crate::gui::geometry::Rect;
    use crate::gui::project::{
        input_pin_center, node_param_dropdown_rect, node_param_row_rect, node_param_value_rect,
        output_pin_center, GuiProject, ProjectNodeKind, NODE_PARAM_DROPDOWN_ROW_HEIGHT, NODE_WIDTH,
    };
    use crate::gui::state::{
        AddNodeMenuState, DragState, ExportMenuState, HoverInsertLink, HoverParamTarget,
        InputSnapshot, LinkCutState, ParamEditState, PreviewState, WireDragState,
    };
    use crate::runtime_config::V2Config;

    #[test]
    fn segments_intersect_detects_crossing_lines() {
        assert!(segments_intersect(0, 0, 10, 10, 0, 10, 10, 0));
    }

    #[test]
    fn segments_intersect_detects_non_crossing_lines() {
        assert!(!segments_intersect(0, 0, 10, 0, 0, 5, 10, 5));
    }

    #[test]
    fn can_append_param_char_limits_numeric_input_shape() {
        assert!(can_append_param_char("", '1'));
        assert!(can_append_param_char("", '-'));
        assert!(!can_append_param_char("1", '-'));
        assert!(can_append_param_char("1", '.'));
        assert!(!can_append_param_char("1.2", '.'));
        assert!(!can_append_param_char("", 'a'));
    }

    #[test]
    fn marquee_moved_requires_drag_threshold() {
        assert!(!marquee_moved(RightMarqueeState {
            start_x: 10,
            start_y: 10,
            cursor_x: 13,
            cursor_y: 12,
        }));
        assert!(marquee_moved(RightMarqueeState {
            start_x: 10,
            start_y: 10,
            cursor_x: 18,
            cursor_y: 10,
        }));
    }

    #[test]
    fn rects_overlap_detects_intersection() {
        assert!(rects_overlap(0, 0, 10, 10, 8, 8, 16, 16));
        assert!(!rects_overlap(0, 0, 10, 10, 11, 11, 20, 20));
    }

    #[test]
    fn insert_param_char_replaces_selection() {
        let mut edit = ParamEditState {
            node_id: 7,
            param_index: 0,
            buffer: "1.000".to_string(),
            cursor: 5,
            anchor: 0,
        };
        assert!(insert_param_char(&mut edit, '2'));
        assert_eq!(edit.buffer, "2");
        assert_eq!(edit.cursor, 1);
        assert_eq!(edit.anchor, 1);
    }

    #[test]
    fn backspace_param_text_deletes_selected_range() {
        let mut edit = ParamEditState {
            node_id: 7,
            param_index: 0,
            buffer: "12.34".to_string(),
            cursor: 4,
            anchor: 1,
        };
        assert!(backspace_param_text(&mut edit));
        assert_eq!(edit.buffer, "14");
        assert_eq!(edit.cursor, 1);
        assert_eq!(edit.anchor, 1);
    }

    #[test]
    fn cursor_moves_collapse_selection_when_not_extending() {
        let mut edit = ParamEditState {
            node_id: 7,
            param_index: 0,
            buffer: "12.34".to_string(),
            cursor: 4,
            anchor: 1,
        };
        assert!(move_param_cursor_left(&mut edit, false));
        assert_eq!(edit.cursor, 1);
        assert_eq!(edit.anchor, 1);
        assert!(move_param_cursor_right(&mut edit, false));
        assert_eq!(edit.cursor, 2);
        assert_eq!(edit.anchor, 2);
    }

    #[test]
    fn delete_hotkey_removes_selected_nodes() {
        let mut project = GuiProject::new_empty(640, 480);
        let top = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(top, out));
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.selected_nodes.push(top);
        state.active_node = Some(top);
        let input = InputSnapshot {
            param_delete: true,
            ..InputSnapshot::default()
        };
        assert!(handle_delete_selected_nodes(
            &input,
            &mut project,
            &mut state
        ));
        assert!(project.node(top).is_none());
        assert_eq!(project.edge_count(), 0);
        assert!(state.selected_nodes.is_empty());
        assert!(state.active_node.is_none());
    }

    #[test]
    fn f1_over_node_opens_help_modal() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        let input = InputSnapshot {
            mouse_pos: Some((90, 90)),
            open_help: true,
            ..InputSnapshot::default()
        };
        let (changed, consumed) = handle_help_input(&input, &project, 420, 480, &mut state);
        assert!(changed);
        assert!(consumed);
        let modal = state.help_modal.as_ref().expect("help modal should open");
        assert!(modal.title.starts_with("Node Help:"));
        assert!(modal
            .lines
            .iter()
            .any(|line| line.contains(&format!("#{solid}"))));
    }

    #[test]
    fn help_modal_closes_on_click() {
        let mut project = GuiProject::new_empty(640, 480);
        let _solid = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.help_modal = Some(crate::gui::help::build_global_help_modal());
        let close = InputSnapshot {
            left_clicked: true,
            ..InputSnapshot::default()
        };
        let (changed, consumed) = handle_help_input(&close, &project, 420, 480, &mut state);
        assert!(changed);
        assert!(consumed);
        assert!(state.help_modal.is_none());
    }

    #[test]
    fn export_panel_stays_open_on_outside_click() {
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.export_menu = ExportMenuState::open_at(80, 80, 420, 480);
        let outside_click = InputSnapshot {
            left_clicked: true,
            mouse_pos: Some((12, 12)),
            ..InputSnapshot::default()
        };
        assert!(!handle_main_export_menu_input(
            &outside_click,
            420,
            480,
            &mut state
        ));
        assert!(state.export_menu.open);
    }

    #[test]
    fn export_panel_close_button_closes_panel() {
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.export_menu = ExportMenuState::open_at(80, 80, 420, 480);
        let close = state.export_menu.close_button_rect();
        let click_close = InputSnapshot {
            left_clicked: true,
            mouse_pos: Some((close.x + close.w / 2, close.y + close.h / 2)),
            ..InputSnapshot::default()
        };
        assert!(handle_main_export_menu_input(
            &click_close,
            420,
            480,
            &mut state
        ));
        assert!(!state.export_menu.open);
    }

    #[test]
    fn wheel_over_param_value_box_adjusts_value_and_consumes_zoom() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
        assert!(project.toggle_node_expanded(solid, 420, 480));
        let value_rect = {
            let node = project.node(solid).expect("solid node exists");
            node_param_value_rect(node, 0).expect("value rect exists")
        };
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        let input = InputSnapshot {
            mouse_pos: Some((value_rect.x + 2, value_rect.y + 2)),
            wheel_lines_y: 2.0,
            ..InputSnapshot::default()
        };
        let (changed, consumed) =
            handle_param_wheel_input(&input, &mut project, 420, 480, &mut state);
        assert!(changed);
        assert!(consumed);
        let value = project
            .node_param_raw_value(solid, 0)
            .expect("param value should exist");
        assert!((value - 0.92).abs() < 1e-5);
    }

    #[test]
    fn signal_wire_hover_does_not_auto_expand_node() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
        assert!(!project.node_expanded(solid));
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: lfo,
            cursor_x: 0,
            cursor_y: 0,
        });

        let expand_hover = InputSnapshot {
            mouse_pos: Some((225, 85)),
            ..InputSnapshot::default()
        };
        assert!(update_hover_state(
            &expand_hover,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert!(!project.node_expanded(solid));
        assert!(state.hover_param_target.is_none());
    }

    #[test]
    fn tab_opened_bind_hover_node_collapses_on_exit() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
        assert!(!project.node_expanded(solid));
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: lfo,
            cursor_x: 0,
            cursor_y: 0,
        });

        let hover_node = InputSnapshot {
            mouse_pos: Some((225, 85)),
            ..InputSnapshot::default()
        };
        assert!(update_hover_state(
            &hover_node,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert!(!project.node_expanded(solid));
        let toggle = InputSnapshot {
            toggle_node_open: true,
            ..InputSnapshot::default()
        };
        assert!(handle_node_open_toggle(
            &toggle,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert!(project.node_expanded(solid));

        let hover_away = InputSnapshot {
            mouse_pos: Some((16, 16)),
            ..InputSnapshot::default()
        };
        assert!(update_hover_state(
            &hover_away,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert!(!project.node_expanded(solid));
    }

    #[test]
    fn signal_wire_hover_does_not_collapse_user_expanded_node() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
        assert!(project.toggle_node_expanded(solid, 420, 480));

        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: lfo,
            cursor_x: 0,
            cursor_y: 0,
        });

        let hover_node = InputSnapshot {
            mouse_pos: Some((225, 85)),
            ..InputSnapshot::default()
        };
        let _ = update_hover_state(&hover_node, &mut project, 420, 480, &mut state);
        let hover_away = InputSnapshot {
            mouse_pos: Some((16, 16)),
            ..InputSnapshot::default()
        };
        let _ = update_hover_state(&hover_away, &mut project, 420, 480, &mut state);
        assert!(project.node_expanded(solid));
    }

    #[test]
    fn texture_wire_hover_over_feedback_does_not_auto_expand_node() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
        assert!(!project.node_expanded(feedback));

        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: solid,
            cursor_x: 0,
            cursor_y: 0,
        });
        let hover_node = InputSnapshot {
            mouse_pos: Some((225, 85)),
            ..InputSnapshot::default()
        };
        assert!(update_hover_state(
            &hover_node,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert!(!project.node_expanded(feedback));
        assert!(state.hover_param_target.is_none());
    }

    #[test]
    fn texture_wire_hover_still_targets_input_pin_for_regular_nodes() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 220, 80, 420, 480);
        let (in_x, in_y) = {
            let node = project.node(xform).expect("transform node should exist");
            input_pin_center(node).expect("input pin should exist")
        };

        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: solid,
            cursor_x: 0,
            cursor_y: 0,
        });
        let hover_input = InputSnapshot {
            mouse_pos: Some((in_x, in_y)),
            ..InputSnapshot::default()
        };
        assert!(update_hover_state(
            &hover_input,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert_eq!(state.hover_input_pin, Some(xform));
    }

    #[test]
    fn dropping_signal_wire_binds_hovered_parameter() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
        let circle = project.add_node(ProjectNodeKind::TexCircle, 220, 80, 420, 480);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: lfo,
            cursor_x: 0,
            cursor_y: 0,
        });
        state.hover_param_target = Some(HoverParamTarget {
            node_id: circle,
            param_index: 2,
        });
        let input = InputSnapshot {
            left_down: false,
            ..InputSnapshot::default()
        };
        assert!(handle_wire_input(
            &input,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert!(state.wire_drag.is_none());
        assert_eq!(project.signal_source_for_param(circle, 2), Some(lfo));
    }

    #[test]
    fn dropping_texture_wire_binds_feedback_target_parameter() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: solid,
            cursor_x: 0,
            cursor_y: 0,
        });
        state.hover_param_target = Some(HoverParamTarget {
            node_id: feedback,
            param_index: 0,
        });
        let input = InputSnapshot {
            left_down: false,
            ..InputSnapshot::default()
        };
        assert!(handle_wire_input(
            &input,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert!(state.wire_drag.is_none());
        assert_eq!(project.texture_source_for_param(feedback, 0), Some(solid));
    }

    #[test]
    fn dropping_texture_wire_on_feedback_target_value_box_binds_parameter() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
        assert!(project.toggle_node_expanded(feedback, 420, 480));
        let value_rect = {
            let node = project.node(feedback).expect("feedback node should exist");
            node_param_value_rect(node, 0).expect("feedback target value rect should exist")
        };
        let cursor = (value_rect.x + 2, value_rect.y + 2);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: solid,
            cursor_x: cursor.0,
            cursor_y: cursor.1,
        });

        let hover = InputSnapshot {
            mouse_pos: Some(cursor),
            left_down: true,
            ..InputSnapshot::default()
        };
        assert!(update_hover_state(
            &hover,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert_eq!(
            state.hover_param_target,
            Some(HoverParamTarget {
                node_id: feedback,
                param_index: 0,
            })
        );

        let release = InputSnapshot {
            mouse_pos: Some(cursor),
            left_down: false,
            ..InputSnapshot::default()
        };
        assert!(handle_wire_input(
            &release,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert_eq!(project.texture_source_for_param(feedback, 0), Some(solid));
    }

    #[test]
    fn texture_drop_release_hit_test_binds_feedback_target_without_hover_target() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
        assert!(project.toggle_node_expanded(feedback, 420, 480));
        let value_rect = {
            let node = project.node(feedback).expect("feedback node should exist");
            node_param_value_rect(node, 0).expect("feedback target value rect should exist")
        };
        let cursor = (value_rect.x + 2, value_rect.y + 2);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: solid,
            cursor_x: cursor.0,
            cursor_y: cursor.1,
        });

        let release = InputSnapshot {
            mouse_pos: Some(cursor),
            left_down: false,
            ..InputSnapshot::default()
        };
        assert!(handle_wire_input(
            &release,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert_eq!(project.texture_source_for_param(feedback, 0), Some(solid));
    }

    #[test]
    fn texture_drop_on_collapsed_feedback_card_does_not_create_implicit_binding() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
        assert!(!project.node_expanded(feedback));

        let center = (220 + NODE_WIDTH / 2, 80 + 22);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: solid,
            cursor_x: center.0,
            cursor_y: center.1,
        });
        let release = InputSnapshot {
            mouse_pos: Some(center),
            left_down: false,
            ..InputSnapshot::default()
        };
        assert!(handle_wire_input(
            &release,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert_eq!(project.texture_source_for_param(feedback, 0), None);
        assert_eq!(project.input_source_node_id(feedback), None);
    }

    #[test]
    fn texture_drop_on_feedback_input_pin_keeps_primary_input_wiring() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
        let input_pin = {
            let node = project.node(feedback).expect("feedback node should exist");
            input_pin_center(node).expect("feedback input pin should exist")
        };
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.wire_drag = Some(WireDragState {
            source_node_id: solid,
            cursor_x: input_pin.0,
            cursor_y: input_pin.1,
        });
        state.hover_input_pin = Some(feedback);
        let release = InputSnapshot {
            mouse_pos: Some(input_pin),
            left_down: false,
            ..InputSnapshot::default()
        };
        assert!(handle_wire_input(
            &release,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert_eq!(project.input_source_node_id(feedback), Some(solid));
        assert_eq!(project.texture_source_for_param(feedback, 0), None);
    }

    #[test]
    fn dragging_node_over_wire_highlights_insert_candidate() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 120, 160, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
        assert!(project.connect_image_link(solid, out));
        let (from_x, from_y) = {
            let node = project.node(solid).expect("solid should exist");
            output_pin_center(node).expect("solid output pin")
        };
        let (to_x, to_y) = {
            let node = project.node(out).expect("out should exist");
            input_pin_center(node).expect("out input pin")
        };
        let mid = ((from_x + to_x) / 2, (from_y + to_y) / 2);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.drag = Some(DragState {
            node_id: xform,
            offset_x: 0,
            offset_y: 0,
            origin_x: 120,
            origin_y: 160,
        });
        let drag = InputSnapshot {
            mouse_pos: Some(mid),
            left_down: true,
            ..InputSnapshot::default()
        };
        assert!(handle_drag_input(&drag, &mut project, 420, 480, &mut state));
        assert_eq!(
            state.hover_insert_link,
            Some(HoverInsertLink {
                source_id: solid,
                target_id: out,
            })
        );
    }

    #[test]
    fn dropping_dragged_node_on_wire_inserts_node_between_link() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 120, 160, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
        assert!(project.connect_image_link(solid, out));
        let (from_x, from_y) = {
            let node = project.node(solid).expect("solid should exist");
            output_pin_center(node).expect("solid output pin")
        };
        let (to_x, to_y) = {
            let node = project.node(out).expect("out should exist");
            input_pin_center(node).expect("out input pin")
        };
        let mid = ((from_x + to_x) / 2, (from_y + to_y) / 2);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.drag = Some(DragState {
            node_id: xform,
            offset_x: 0,
            offset_y: 0,
            origin_x: 120,
            origin_y: 160,
        });
        state.hover_insert_link = Some(HoverInsertLink {
            source_id: solid,
            target_id: out,
        });
        let drop = InputSnapshot {
            mouse_pos: Some(mid),
            left_down: false,
            ..InputSnapshot::default()
        };
        assert!(handle_drag_input(&drop, &mut project, 420, 480, &mut state));
        assert!(state.drag.is_none());
        assert!(state.hover_insert_link.is_none());
        assert_eq!(project.input_source_node_id(xform), Some(solid));
        assert_eq!(project.input_source_node_id(out), Some(xform));
    }

    #[test]
    fn dragging_selected_nodes_moves_selection_as_one_group() {
        let mut project = GuiProject::new_empty(640, 480);
        let first = project.add_node(ProjectNodeKind::TexTransform2D, 40, 80, 420, 480);
        let second = project.add_node(ProjectNodeKind::TexSolid, 180, 120, 420, 480);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.selected_nodes = vec![first, second];
        state.drag = Some(DragState {
            node_id: first,
            offset_x: 0,
            offset_y: 0,
            origin_x: 40,
            origin_y: 80,
        });

        let drag = InputSnapshot {
            mouse_pos: Some((90, 130)),
            left_down: true,
            ..InputSnapshot::default()
        };
        assert!(handle_drag_input(&drag, &mut project, 420, 480, &mut state));
        let first_node = project.node(first).expect("first node should exist");
        let second_node = project.node(second).expect("second node should exist");
        assert_eq!(first_node.x(), 90);
        assert_eq!(first_node.y(), 130);
        assert_eq!(second_node.x(), 230);
        assert_eq!(second_node.y(), 170);
    }

    #[test]
    fn dropping_node_on_top_of_other_snaps_to_side_from_drag_origin() {
        let mut project = GuiProject::new_empty(640, 480);
        let dragged = project.add_node(ProjectNodeKind::TexTransform2D, 40, 80, 420, 480);
        let target = project.add_node(ProjectNodeKind::TexSolid, 260, 80, 420, 480);
        // Simulate release while dragged node overlaps target card.
        assert!(project.move_node(dragged, 260, 80, 420, 480));
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.drag = Some(DragState {
            node_id: dragged,
            offset_x: 0,
            offset_y: 0,
            origin_x: 40,
            origin_y: 80,
        });
        let drop = InputSnapshot {
            mouse_pos: Some((300, 100)),
            left_down: false,
            ..InputSnapshot::default()
        };
        assert!(handle_drag_input(&drop, &mut project, 420, 480, &mut state));
        let dragged_node = project.node(dragged).expect("dragged node should exist");
        let target_node = project.node(target).expect("target node should exist");
        assert_eq!(
            dragged_node.x(),
            target_node.x() - NODE_WIDTH - super::NODE_OVERLAP_SNAP_GAP_PX
        );
    }

    #[test]
    fn right_click_on_bound_param_value_unbinds_parameter() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
        let circle = project.add_node(ProjectNodeKind::TexCircle, 220, 80, 420, 480);
        assert!(project.toggle_node_expanded(circle, 420, 480));
        assert!(project.connect_signal_link_to_param(lfo, circle, 2));
        let value_rect = {
            let node = project.node(circle).expect("circle node should exist");
            node_param_value_rect(node, 2).expect("value rect should exist")
        };
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        let input = InputSnapshot {
            mouse_pos: Some((value_rect.x + 2, value_rect.y + 2)),
            right_clicked: true,
            ..InputSnapshot::default()
        };
        assert!(handle_right_selection(
            &input,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert_eq!(project.signal_source_for_param(circle, 2), None);
    }

    #[test]
    fn dropdown_click_selects_correct_option_at_low_zoom() {
        let mut project = GuiProject::new_empty(640, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 220, 80, 420, 480);
        assert!(project.toggle_node_expanded(pass, 420, 480));
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.zoom = 0.5;

        let value_rect = {
            let node = project.node(pass).expect("scene-pass node should exist");
            node_param_value_rect(node, 2).expect("bg_mode value rect should exist")
        };
        let value_panel = super::graph_rect_to_panel(value_rect, &state);
        let open_dropdown = InputSnapshot {
            mouse_pos: Some((value_panel.x + 2, value_panel.y + 2)),
            left_clicked: true,
            ..InputSnapshot::default()
        };
        let (_, consumed_open) =
            handle_param_edit_input(&open_dropdown, &mut project, 420, 480, &mut state);
        assert!(consumed_open);
        assert_eq!(state.param_dropdown.map(|d| d.node_id), Some(pass));

        let second_row_panel = {
            let node = project.node(pass).expect("scene-pass node should exist");
            let options = project
                .node_param_dropdown_options(pass, 2)
                .expect("bg_mode dropdown options");
            let list_world =
                node_param_dropdown_rect(node, 2, options.len()).expect("dropdown rect");
            let second_row_world = Rect::new(
                list_world.x,
                list_world.y + NODE_PARAM_DROPDOWN_ROW_HEIGHT,
                list_world.w,
                NODE_PARAM_DROPDOWN_ROW_HEIGHT,
            );
            super::graph_rect_to_panel(second_row_world, &state)
        };
        let select_second_option = InputSnapshot {
            mouse_pos: Some((second_row_panel.x + 2, second_row_panel.y + 1)),
            left_clicked: true,
            ..InputSnapshot::default()
        };
        let (_, consumed_select) =
            handle_param_edit_input(&select_second_option, &mut project, 420, 480, &mut state);
        assert!(consumed_select);
        assert_eq!(project.node_param_raw_text(pass, 2), Some("alpha_clip"));
    }

    #[test]
    fn alt_cut_unbinds_parameter_link_when_cut_crosses_param_wire() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
        let circle = project.add_node(ProjectNodeKind::TexCircle, 220, 80, 420, 480);
        assert!(project.toggle_node_expanded(circle, 420, 480));
        assert!(project.connect_signal_link_to_param(lfo, circle, 2));

        let (from_x, from_y) = {
            let source = project.node(lfo).expect("lfo node should exist");
            output_pin_center(source).expect("source output pin should exist")
        };
        let (to_x, to_y) = {
            let target = project.node(circle).expect("circle node should exist");
            let row = node_param_row_rect(target, 2).expect("row rect should exist");
            (row.x + row.w - 4, row.y + row.h / 2)
        };
        let cut_x = (from_x + to_x) / 2;
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.link_cut = Some(LinkCutState {
            start_x: cut_x,
            start_y: from_y.min(to_y) - 24,
            cursor_x: cut_x,
            cursor_y: from_y.max(to_y) + 24,
        });
        let input = InputSnapshot {
            left_down: false,
            ..InputSnapshot::default()
        };
        assert!(handle_link_cut(&input, &mut project, 420, 480, &mut state));
        assert_eq!(project.signal_source_for_param(circle, 2), None);
    }

    #[test]
    fn alt_cut_unbinds_parameter_link_when_cut_crosses_routed_param_wire() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
        let _blocker = project.add_node(ProjectNodeKind::TexSolid, 210, 70, 420, 480);
        let circle = project.add_node(ProjectNodeKind::TexCircle, 420, 80, 420, 480);
        assert!(project.toggle_node_expanded(circle, 420, 480));
        assert!(project.connect_signal_link_to_param(lfo, circle, 2));

        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        let (from_x, from_y) = {
            let source = project.node(lfo).expect("lfo node should exist");
            output_pin_center(source).expect("source output pin should exist")
        };
        let (to_x, to_y) = {
            let target = project.node(circle).expect("circle node should exist");
            let row = node_param_row_rect(target, 2).expect("row rect should exist");
            (row.x + row.w - 4, row.y + row.h / 2)
        };
        let obstacles = super::collect_panel_node_obstacles(&project, &state);
        let exit_x = from_x.saturating_add(super::PARAM_WIRE_EXIT_TAIL_PX);
        let entry_x = to_x.saturating_add(super::PARAM_WIRE_ENTRY_TAIL_PX);
        let route = crate::gui::scene::wire_route::route_param_path(
            (exit_x, from_y),
            (entry_x, to_y),
            obstacles.as_slice(),
        );
        let cut = route
            .windows(2)
            .find_map(|segment| {
                let (ax, ay) = segment[0];
                let (bx, by) = segment[1];
                let (start_x, start_y, cursor_x, cursor_y) = if ax == bx {
                    (ax - 24, (ay + by) / 2, ax + 24, (ay + by) / 2)
                } else {
                    ((ax + bx) / 2, ay - 24, (ax + bx) / 2, ay + 24)
                };
                if !segments_intersect(start_x, start_y, cursor_x, cursor_y, ax, ay, bx, by)
                    || segments_intersect(
                        start_x, start_y, cursor_x, cursor_y, from_x, from_y, to_x, to_y,
                    )
                {
                    return None;
                }
                Some(LinkCutState {
                    start_x,
                    start_y,
                    cursor_x,
                    cursor_y,
                })
            })
            .expect("expected routed segment that is distinct from source-to-target straight wire");
        state.link_cut = Some(cut);
        let input = InputSnapshot {
            left_down: false,
            ..InputSnapshot::default()
        };
        assert!(handle_link_cut(&input, &mut project, 420, 480, &mut state));
        assert_eq!(project.signal_source_for_param(circle, 2), None);
    }

    #[test]
    fn add_menu_category_then_secondary_picker_spawns_node() {
        let mut project = GuiProject::new_empty(640, 480);
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.pan_x = 48.0;
        state.pan_y = 30.0;
        state.zoom = 2.0;
        state.menu = AddNodeMenuState::open_at(120, 100, 420, 480);
        let mut control_index = None;
        for index in 0..state.menu.visible_entry_count() {
            let Some(entry) = state.menu.visible_entry(index) else {
                continue;
            };
            if matches!(
                entry,
                AddNodeMenuEntry::Category(category) if category.label() == "Control"
            ) {
                control_index = Some(index);
                break;
            }
        }
        let control_index = control_index.expect("control category should exist");
        state.menu.selected = control_index;
        let open_category = InputSnapshot {
            menu_accept: true,
            ..InputSnapshot::default()
        };
        assert!(handle_add_menu_input(
            &open_category,
            &mut project,
            420,
            480,
            &mut state
        ));
        assert!(state.menu.active_category.is_some());
        let query = InputSnapshot {
            typed_text: "lfo".to_string(),
            ..InputSnapshot::default()
        };
        assert!(handle_add_menu_input(
            &query,
            &mut project,
            420,
            480,
            &mut state
        ));
        state.menu.selected = 1;
        let spawn = InputSnapshot {
            menu_accept: true,
            ..InputSnapshot::default()
        };
        assert!(handle_add_menu_input(
            &spawn,
            &mut project,
            420,
            480,
            &mut state
        ));
        let mut spawned_lfo = None;
        for node in project.nodes() {
            if node.kind() == ProjectNodeKind::CtlLfo {
                spawned_lfo = Some((node.x(), node.y()));
                break;
            }
        }
        assert_eq!(spawned_lfo, Some((36, 35)));
        assert!(!state.menu.open);
    }
}

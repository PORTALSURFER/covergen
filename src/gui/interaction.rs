//! GUI input handling and graph-editor interaction logic.

mod drag;
mod hover;
mod marquee;
mod param_edit;
mod wire;

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
    ParamEditState, PendingAppAction, PopupDragState, PreviewState, RightMarqueeState,
    WireDragState, ADD_NODE_OPTIONS, MAIN_MENU_WIDTH,
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
const PARAM_WIRE_ROUTE_LEAD_PX: i32 = 12;
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
        state.export_menu_drag = None;
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

    let (timeline_changed, timeline_consumed) = handle_timeline_input(
        &input,
        viewport_width,
        panel_height,
        config.animation.fps,
        state,
    );
    changed |= timeline_changed;
    if timeline_consumed {
        state.drag = None;
        state.wire_drag = None;
        state.link_cut = None;
        state.pan_drag = None;
        state.export_menu_drag = None;
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
    state.export_menu_drag = None;
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
    let mut changed = false;
    let mut consumed = false;
    let timeline = timeline_rect(viewport_width, panel_height);
    let play = play_button_rect(timeline);
    let pause = pause_button_rect(timeline);
    let track = track_rect(timeline);
    let total_frames = state.export_menu.timeline_total_frames(timeline_fps);
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
            changed |= scrub_frame_from_timeline(track, mx, total_frames, state);
        } else if state.timeline_scrub_active && input.left_down {
            consumed = true;
            changed |= scrub_frame_from_timeline(track, mx, total_frames, state);
        }
        if input.left_clicked && timeline.contains(mx, my) {
            consumed = true;
        }
    } else if state.timeline_scrub_active {
        consumed = true;
    }
    (changed, consumed)
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
        ExportMenuItem::AudioWav => Some(&mut state.export_menu.audio_wav),
        ExportMenuItem::AudioVolume => Some(&mut state.export_menu.audio_volume),
        ExportMenuItem::Bpm => Some(&mut state.export_menu.bpm),
        ExportMenuItem::Bars => Some(&mut state.export_menu.bars),
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
        | ExportMenuItem::AudioWav
        | ExportMenuItem::AudioVolume
        | ExportMenuItem::Bpm
        | ExportMenuItem::Bars
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
    marquee::handle_right_selection(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

#[allow(dead_code)]
fn marquee_moved(marquee: RightMarqueeState) -> bool {
    marquee::marquee_moved(marquee)
}

#[allow(dead_code)]
fn collect_marquee_nodes(
    project: &GuiProject,
    state: &PreviewState,
    marquee: RightMarqueeState,
) -> Vec<u32> {
    marquee::collect_marquee_nodes(project, state, marquee)
}

#[allow(dead_code)]
fn screen_rect_to_graph_rect(
    sx0: i32,
    sy0: i32,
    sx1: i32,
    sy1: i32,
    state: &PreviewState,
) -> (i32, i32, i32, i32) {
    marquee::screen_rect_to_graph_rect(sx0, sy0, sx1, sy1, state)
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

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
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
    marquee::rects_overlap(ax0, ay0, ax1, ay1, bx0, by0, bx1, by1)
}

#[allow(dead_code)]
fn set_single_selection(state: &mut PreviewState, node_id: u32) -> bool {
    marquee::set_single_selection(state, node_id)
}

#[allow(dead_code)]
fn set_multi_selection(state: &mut PreviewState, nodes: Vec<u32>) -> bool {
    marquee::set_multi_selection(state, nodes)
}

#[allow(dead_code)]
fn clear_selection(state: &mut PreviewState) -> bool {
    marquee::clear_selection(state)
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

#[allow(dead_code)]
fn apply_param_text_edits(
    input: &InputSnapshot,
    project: &mut GuiProject,
    state: &mut PreviewState,
) -> bool {
    param_edit::apply_param_text_edits(input, project, state)
}

#[allow(dead_code)]
fn handle_param_click(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    param_edit::handle_param_click(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

#[allow(dead_code)]
fn handle_dropdown_click(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    param_edit::handle_dropdown_click(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

#[allow(dead_code)]
fn dropdown_option_at_cursor(
    project: &GuiProject,
    state: &PreviewState,
    mx: i32,
    my: i32,
) -> Option<usize> {
    param_edit::dropdown_option_at_cursor(project, state, mx, my)
}

#[allow(dead_code)]
fn start_param_edit(
    project: &GuiProject,
    state: &mut PreviewState,
    node_id: u32,
    param_index: usize,
) -> bool {
    param_edit::start_param_edit(project, state, node_id, param_index)
}

#[allow(dead_code)]
fn finish_param_edit(project: &mut GuiProject, state: &mut PreviewState) -> bool {
    param_edit::finish_param_edit(project, state)
}

#[allow(dead_code)]
fn commit_param_edit(project: &mut GuiProject, edit: &mut ParamEditState) -> bool {
    param_edit::commit_param_edit(project, edit)
}

#[allow(dead_code)]
fn can_append_param_char(current: &str, ch: char) -> bool {
    param_edit::can_append_param_char(current, ch)
}

#[allow(dead_code)]
fn is_valid_param_buffer(buffer: &str) -> bool {
    param_edit::is_valid_param_buffer(buffer)
}

#[allow(dead_code)]
fn clamp_param_edit_indices(edit: &mut ParamEditState) {
    param_edit::clamp_param_edit_indices(edit)
}

#[allow(dead_code)]
fn has_param_selection(edit: &ParamEditState) -> bool {
    param_edit::has_param_selection(edit)
}

#[allow(dead_code)]
fn param_selection_bounds(edit: &ParamEditState) -> (usize, usize) {
    param_edit::param_selection_bounds(edit)
}

#[allow(dead_code)]
fn collapse_param_selection(edit: &mut ParamEditState, at: usize) {
    param_edit::collapse_param_selection(edit, at)
}

#[allow(dead_code)]
fn select_all_param_text(edit: &mut ParamEditState) -> bool {
    param_edit::select_all_param_text(edit)
}

#[allow(dead_code)]
fn delete_param_selection(edit: &mut ParamEditState) -> bool {
    param_edit::delete_param_selection(edit)
}

#[allow(dead_code)]
fn backspace_param_text(edit: &mut ParamEditState) -> bool {
    param_edit::backspace_param_text(edit)
}

#[allow(dead_code)]
fn delete_param_text(edit: &mut ParamEditState) -> bool {
    param_edit::delete_param_text(edit)
}

#[allow(dead_code)]
fn insert_param_char(edit: &mut ParamEditState, ch: char) -> bool {
    param_edit::insert_param_char(edit, ch)
}

#[allow(dead_code)]
fn move_param_cursor_left(edit: &mut ParamEditState, extend_selection: bool) -> bool {
    param_edit::move_param_cursor_left(edit, extend_selection)
}

#[allow(dead_code)]
fn move_param_cursor_right(edit: &mut ParamEditState, extend_selection: bool) -> bool {
    param_edit::move_param_cursor_right(edit, extend_selection)
}

#[allow(dead_code)]
fn prev_char_boundary(text: &str, index: usize) -> usize {
    param_edit::prev_char_boundary(text, index)
}

#[allow(dead_code)]
fn next_char_boundary(text: &str, index: usize) -> usize {
    param_edit::next_char_boundary(text, index)
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
        let route_start_x = exit_x.saturating_add(PARAM_WIRE_ROUTE_LEAD_PX);
        let entry_x = to_x.saturating_add(PARAM_WIRE_ENTRY_TAIL_PX);
        let route_end_x = entry_x.saturating_add(PARAM_WIRE_ROUTE_LEAD_PX);
        let route = super::scene::wire_route::route_param_path(
            (route_start_x, from_y),
            (route_end_x, to_y),
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
        ) || segments_intersect(
            cut.start_x,
            cut.start_y,
            cut.cursor_x,
            cut.cursor_y,
            exit_x,
            from_y,
            route_start_x,
            from_y,
        ) || cut_intersects_path(cut, route.as_slice())
            || segments_intersect(
                cut.start_x,
                cut.start_y,
                cut.cursor_x,
                cut.cursor_y,
                route_end_x,
                to_y,
                entry_x,
                to_y,
            )
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
    drag::handle_drag_input(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

/// Return drag group ids for one anchor node.
///
/// Multi-node dragging is enabled only when the anchor node is part of the
/// current selection and at least two nodes are selected.
#[allow(dead_code)]
fn drag_selection_node_ids(state: &PreviewState, anchor_node_id: u32) -> Vec<u32> {
    drag::drag_selection_node_ids(state, anchor_node_id)
}

/// Move selected drag nodes by the anchor node cursor delta.
///
/// The anchor node follows the cursor first; the resolved delta is then applied
/// to the remaining selected nodes to keep group movement coherent.
#[allow(dead_code)]
fn move_drag_selection_by_anchor_delta(
    project: &mut GuiProject,
    drag: super::state::DragState,
    dragged_node_ids: &[u32],
    graph_x: i32,
    graph_y: i32,
    panel_width: usize,
    panel_height: usize,
) -> bool {
    drag::move_drag_selection_by_anchor_delta(
        project,
        drag,
        dragged_node_ids,
        graph_x,
        graph_y,
        InteractionPanelContext::new(panel_width, panel_height),
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

/// Resolve texture-parameter drop target on release.
///
/// This confirms the release cursor against parameter hit-testing so texture
/// binds stay reliable even if hover state did not update on the same frame.
#[allow(dead_code)]
fn resolve_texture_param_target_on_release(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
    wire: WireDragState,
) -> Option<HoverParamTarget> {
    wire::resolve_texture_param_target_on_release(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
        wire,
    )
}

#[allow(dead_code)]
fn begin_wire_drag_if_pin_hit(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    wire::begin_wire_drag_if_pin_hit(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

#[allow(dead_code)]
fn begin_drag_if_node_hit(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    drag::begin_drag_if_node_hit(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
    )
}

/// Snap one dragged node beside an overlapped node using drag origin side.
#[allow(dead_code)]
fn snap_dragged_node_out_of_overlap(
    project: &mut GuiProject,
    drag: super::state::DragState,
    panel_width: usize,
    panel_height: usize,
) -> bool {
    drag::snap_dragged_node_out_of_overlap(
        project,
        drag,
        InteractionPanelContext::new(panel_width, panel_height),
    )
}

/// Return true when two rectangles overlap with positive area.
#[allow(dead_code)]
fn rects_overlap_strict(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    drag::rects_overlap_strict(a, b)
}

/// Resolve one hovered wire insertion candidate at cursor position.
#[allow(dead_code)]
fn hover_insert_link_at_cursor(
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
    cursor_x: i32,
    cursor_y: i32,
    dragged_node_id: u32,
) -> Option<HoverInsertLink> {
    drag::hover_insert_link_at_cursor(
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
        cursor_x,
        cursor_y,
        dragged_node_id,
    )
}

/// Return true when `dragged_node_id` can be inserted on `source -> target`.
#[allow(dead_code)]
fn can_insert_dragged_node_on_link(
    project: &GuiProject,
    dragged_node_id: u32,
    source_id: u32,
    target_id: u32,
) -> bool {
    drag::can_insert_dragged_node_on_link(project, dragged_node_id, source_id, target_id)
}

/// Resolve insertion candidate on drag release from hover cache or hit-test.
#[allow(dead_code)]
fn resolve_insert_link_on_release(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
    dragged_node_id: u32,
) -> Option<HoverInsertLink> {
    drag::resolve_insert_link_on_release(
        input,
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
        dragged_node_id,
    )
}

/// Return squared distance from point `p` to line segment `ab`.
#[allow(dead_code)]
fn point_to_segment_distance_sq(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    drag::point_to_segment_distance_sq(px, py, ax, ay, bx, by)
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

#[allow(dead_code)]
fn collapse_auto_expanded_binding_nodes_except(
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
    keep_node_id: Option<u32>,
) -> bool {
    hover::collapse_auto_expanded_binding_nodes_except(
        project,
        InteractionPanelContext::new(panel_width, panel_height),
        state,
        keep_node_id,
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
    use crate::gui::timeline::editor_panel_height;
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
        let tex_source = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(tex_source, out));
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.selected_nodes.push(tex_source);
        state.active_node = Some(tex_source);
        let input = InputSnapshot {
            param_delete: true,
            ..InputSnapshot::default()
        };
        assert!(handle_delete_selected_nodes(
            &input,
            &mut project,
            &mut state
        ));
        assert!(project.node(tex_source).is_none());
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
    fn export_panel_title_drag_moves_popup() {
        let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
        state.export_menu = ExportMenuState::open_at(80, 80, 420, 480);
        let initial_x = state.export_menu.x;
        let initial_y = state.export_menu.y;
        let start_drag = InputSnapshot {
            left_clicked: true,
            left_down: true,
            mouse_pos: Some((100, 92)),
            ..InputSnapshot::default()
        };
        assert!(handle_main_export_menu_input(
            &start_drag,
            420,
            480,
            &mut state
        ));
        assert_eq!(
            state.export_menu_drag.map(|drag| drag.offset_x),
            Some(100 - initial_x)
        );
        assert_eq!(
            state.export_menu_drag.map(|drag| drag.offset_y),
            Some(92 - initial_y)
        );

        let drag_move = InputSnapshot {
            left_down: true,
            mouse_pos: Some((180, 160)),
            ..InputSnapshot::default()
        };
        assert!(handle_main_export_menu_input(
            &drag_move, 420, 480, &mut state
        ));
        let expected_max_y =
            (editor_panel_height(480) as i32 - state.export_menu.rect().h - 8).max(8);
        assert_eq!(state.export_menu.x, 8);
        assert_eq!(state.export_menu.y, expected_max_y);

        let release = InputSnapshot {
            left_down: false,
            mouse_pos: Some((180, 160)),
            ..InputSnapshot::default()
        };
        assert!(!handle_main_export_menu_input(
            &release, 420, 480, &mut state
        ));
        assert!(state.export_menu_drag.is_none());
        assert!(state.export_menu.open);
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

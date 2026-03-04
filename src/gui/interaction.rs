//! GUI input handling and graph-editor interaction logic.

mod add_menu_input;
mod drag;
mod hover;
mod marquee;
mod menu_input;
mod param_edit;
mod route_cache;
mod timeline_input;
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

use super::geometry::{
    graph_point_to_panel as geometry_graph_point_to_panel,
    graph_rect_to_panel as geometry_graph_rect_to_panel,
    map_graph_path_to_panel_into as geometry_map_graph_path_to_panel_into,
    screen_point_to_graph as geometry_screen_point_to_graph, segments_intersect, Rect,
};
use super::help::{build_global_help_modal, build_node_help_modal, build_param_help_modal};
use super::project::{
    collapsed_param_entry_pin_center, input_pin_center, node_expand_toggle_rect,
    node_param_dropdown_rect, node_param_row_rect, output_pin_center, GraphBounds, GuiProject,
    ResourceKind, NODE_PARAM_DROPDOWN_ROW_HEIGHT, NODE_WIDTH,
};
#[cfg(test)]
use super::state::AddNodeMenuEntry;
use super::state::{
    AddNodeMenuState, HoverInsertLink, HoverParamTarget, InputSnapshot, LinkCutState,
    MainMenuState, PanDragState, ParamDropdownState, ParamEditState, PendingAppAction,
    PreviewState, RightMarqueeState, WireDragState,
};
use super::timeline::{editor_panel_height, next_looped_frame};

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

/// Hover fields snapshot used to scope retained-layer invalidation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HoverInvalidationSnapshot {
    hover_node: Option<u32>,
    hover_output_pin: Option<u32>,
    hover_input_pin: Option<u32>,
    hover_param: Option<HoverParamTarget>,
    hover_param_target: Option<HoverParamTarget>,
    hover_alt_param: Option<HoverParamTarget>,
    hover_dropdown_item: Option<usize>,
    hover_menu_item: Option<usize>,
    hover_main_menu_item: Option<usize>,
    hover_export_menu_item: Option<usize>,
    hover_export_menu_close: bool,
}

impl HoverInvalidationSnapshot {
    fn capture(state: &PreviewState) -> Self {
        Self {
            hover_node: state.hover_node,
            hover_output_pin: state.hover_output_pin,
            hover_input_pin: state.hover_input_pin,
            hover_param: state.hover_param,
            hover_param_target: state.hover_param_target,
            hover_alt_param: state.hover_alt_param,
            hover_dropdown_item: state.hover_dropdown_item,
            hover_menu_item: state.hover_menu_item,
            hover_main_menu_item: state.hover_main_menu_item,
            hover_export_menu_item: state.hover_export_menu_item,
            hover_export_menu_close: state.hover_export_menu_close,
        }
    }

    fn nodes_changed(self, next: Self) -> bool {
        self.hover_node != next.hover_node
            || self.hover_output_pin != next.hover_output_pin
            || self.hover_input_pin != next.hover_input_pin
            || self.hover_param != next.hover_param
            || self.hover_param_target != next.hover_param_target
            || self.hover_alt_param != next.hover_alt_param
    }

    fn overlays_changed(self, next: Self, wire_drag_active: bool) -> bool {
        self.hover_dropdown_item != next.hover_dropdown_item
            || self.hover_menu_item != next.hover_menu_item
            || self.hover_main_menu_item != next.hover_main_menu_item
            || self.hover_export_menu_item != next.hover_export_menu_item
            || self.hover_export_menu_close != next.hover_export_menu_close
            || (wire_drag_active
                && (self.hover_input_pin != next.hover_input_pin
                    || self.hover_param_target != next.hover_param_target))
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
    let panel_ctx = InteractionPanelContext::new(panel_width, panel_height);
    let mut changed = begin_interaction_frame(config, &input, project, state);

    if let Some(result) = apply_help_and_timeline_phase(
        &input,
        project,
        viewport_width,
        panel_ctx,
        config.animation.fps,
        state,
        &mut changed,
    ) {
        return finish_interaction_frame(&input, state, result);
    }
    if let Some(result) = apply_navigation_phase(&input, project, panel_ctx, state, &mut changed) {
        return finish_interaction_frame(&input, state, result);
    }
    if let Some(result) =
        apply_overlay_and_param_phase(&input, project, panel_ctx, state, &mut changed)
    {
        return finish_interaction_frame(&input, state, result);
    }
    apply_menu_or_graph_phase(&input, project, panel_ctx, state, &mut changed);
    if state.wire_drag.is_none() {
        changed |= collapse_auto_expanded_binding_nodes_with_panel(project, panel_ctx, state);
    }
    finish_interaction_frame(&input, state, changed)
}

/// Apply pre-phase state updates shared by all interactions for one frame.
fn begin_interaction_frame(
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
fn apply_help_and_timeline_phase(
    input: &InputSnapshot,
    project: &mut GuiProject,
    viewport_width: usize,
    panel_ctx: InteractionPanelContext,
    timeline_fps: u32,
    state: &mut PreviewState,
    changed: &mut bool,
) -> Option<bool> {
    let (help_changed, help_consumed) = handle_help_input(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
    *changed |= help_changed;
    if help_consumed {
        return Some(*changed);
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
        return Some(*changed);
    }
    if state.timeline_bpm_edit.is_some() || state.timeline_bar_edit.is_some() {
        cancel_node_interaction_modes(state);
        *changed |= collapse_auto_expanded_binding_nodes_with_panel(project, panel_ctx, state);
        return Some(*changed);
    }
    None
}

/// Run viewport-navigation phases (pan/zoom, scrubbing, link cut, marquee selection).
fn apply_navigation_phase(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_ctx: InteractionPanelContext,
    state: &mut PreviewState,
    changed: &mut bool,
) -> Option<bool> {
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
        return Some(true);
    }

    let (param_scrub_changed, param_scrub_active) = handle_alt_param_drag(
        input,
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    );
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
        return Some(true);
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
        return Some(true);
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
        return Some(true);
    }
    None
}

/// Apply overlay toggles plus parameter edit state transitions.
fn apply_overlay_and_param_phase(
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

/// Apply menu-specific and graph-edit interactions after modal/navigation phases.
fn apply_menu_or_graph_phase(
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

/// Collapse auto-expanded binding nodes using the panel context.
fn collapse_auto_expanded_binding_nodes_with_panel(
    project: &mut GuiProject,
    panel_ctx: InteractionPanelContext,
    state: &mut PreviewState,
) -> bool {
    collapse_auto_expanded_binding_nodes(
        project,
        panel_ctx.panel_width,
        panel_ctx.panel_height,
        state,
    )
}

/// Finalize one interaction frame and persist edge-trigger input state.
fn finish_interaction_frame(
    input: &InputSnapshot,
    state: &mut PreviewState,
    changed: bool,
) -> bool {
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
    if state.paused {
        return false;
    }
    let tick_secs = 1.0_f64 / timeline_fps.max(1) as f64;
    state.timeline_accum_secs += frame_delta.as_secs_f32();
    let accum_secs = state.timeline_accum_secs as f64;
    let ticks = (accum_secs / tick_secs).floor() as u32;
    if ticks > 0 {
        state.timeline_accum_secs = (accum_secs - tick_secs * ticks as f64).max(0.0) as f32;
        state.frame_index = advance_looped_frame(state.frame_index, ticks, timeline_total_frames);
        return true;
    }
    false
}

fn advance_looped_frame(frame: u32, ticks: u32, total_frames: u32) -> u32 {
    if ticks == 1 {
        return next_looped_frame(frame, total_frames);
    }
    let total = total_frames.max(1) as u64;
    ((frame as u64 + ticks as u64) % total) as u32
}

fn handle_timeline_input(
    input: &InputSnapshot,
    viewport_width: usize,
    panel_height: usize,
    timeline_fps: u32,
    state: &mut PreviewState,
) -> (bool, bool) {
    timeline_input::handle_timeline_input(input, viewport_width, panel_height, timeline_fps, state)
}

#[cfg(test)]
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
    add_menu_input::handle_add_menu_toggle(input, panel_width, panel_height, state)
}

fn handle_main_menu_toggle(
    input: &InputSnapshot,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    menu_input::handle_main_menu_toggle(input, panel_width, panel_height, state)
}

fn handle_main_export_menu_input(
    input: &InputSnapshot,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    menu_input::handle_main_export_menu_input(input, panel_width, panel_height, state)
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
    let obstacle_signature = route_cache::obstacle_signature_for_project(project, None);
    let obstacles = collect_graph_node_obstacles(project);
    let route_map =
        super::scene::wire_route::RouteObstacleMap::from_obstacles(obstacles.as_slice());
    let (view_x0, view_y0, view_x1, view_y1) = panel_graph_rect(panel_width, panel_height, state);
    let target_ids = project.node_ids_overlapping_graph_rect(view_x0, view_y0, view_x1, view_y1);
    for target_id in target_ids.iter().copied() {
        collect_cut_links_for_target(
            project,
            state,
            cut,
            &route_map,
            obstacle_signature,
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
                &route_map,
                obstacle_signature,
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
    route_map: &super::scene::wire_route::RouteObstacleMap,
    obstacle_signature: u64,
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
        let route_graph = route_cache::route_with_tails_cached(
            super::scene::wire_route::RouteEndpoint {
                point: (from_x, from_y),
                corridor_dir: super::scene::wire_route::RouteDirection::East,
            },
            super::scene::wire_route::RouteEndpoint {
                point: (to_x, to_y),
                corridor_dir: super::scene::wire_route::RouteDirection::West,
            },
            route_map,
            obstacle_signature,
        );
        let route_panel = map_graph_path_to_panel(route_graph.as_ref(), state);
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
        let route_graph = route_cache::route_with_tails_cached(
            super::scene::wire_route::RouteEndpoint {
                point: (from_x, from_y),
                corridor_dir: super::scene::wire_route::RouteDirection::East,
            },
            super::scene::wire_route::RouteEndpoint {
                point: (to_x, to_y),
                corridor_dir: super::scene::wire_route::RouteDirection::East,
            },
            route_map,
            obstacle_signature,
        );
        let route_panel = map_graph_path_to_panel(route_graph.as_ref(), state);
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
            (cut.start_x, cut.start_y),
            (cut.cursor_x, cut.cursor_y),
            segment[0],
            segment[1],
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
    add_menu_input::handle_add_menu_input(input, project, panel_width, panel_height, state)
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
    geometry_screen_point_to_graph((x, y), state.zoom, state.pan_x, state.pan_y)
}

fn graph_point_to_panel(x: i32, y: i32, state: &PreviewState) -> (i32, i32) {
    geometry_graph_point_to_panel((x, y), state.zoom, state.pan_x, state.pan_y)
}

fn map_graph_path_to_panel(points: &[(i32, i32)], state: &PreviewState) -> Vec<(i32, i32)> {
    let mut panel_points = Vec::with_capacity(points.len());
    geometry_map_graph_path_to_panel_into(
        points,
        state.zoom,
        state.pan_x,
        state.pan_y,
        &mut panel_points,
    );
    panel_points
}

fn graph_rect_to_panel(rect: Rect, state: &PreviewState) -> Rect {
    geometry_graph_rect_to_panel(rect, state.zoom, state.pan_x, state.pan_y)
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

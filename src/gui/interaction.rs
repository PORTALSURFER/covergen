//! GUI input handling and graph-editor interaction logic.

mod add_menu_input;
mod drag;
mod frame_lifecycle;
mod hover;
mod marquee;
mod menu_graph_phase;
mod menu_input;
mod navigation_phase;
mod overlay_param_phase;
mod param_edit;
mod route_cache;
mod state_reset;
mod timeline_input;
mod wire;

#[cfg(test)]
use self::marquee::{marquee_moved, rects_overlap};
#[cfg(test)]
use self::param_edit::{
    backspace_param_text, can_append_param_char, insert_param_char, move_param_cursor_left,
    move_param_cursor_right,
};

#[cfg(test)]
use self::frame_lifecycle::handle_help_input;
use self::frame_lifecycle::{
    apply_help_and_timeline_phase, begin_interaction_frame, finish_interaction_frame,
};
use self::menu_graph_phase::apply_menu_or_graph_phase;
use self::navigation_phase::apply_navigation_phase;
use self::overlay_param_phase::apply_overlay_and_param_phase;
use crate::runtime_config::V2Config;
use std::time::Duration;

use super::geometry::{
    graph_point_to_panel as geometry_graph_point_to_panel,
    graph_rect_to_panel as geometry_graph_rect_to_panel,
    map_graph_path_to_panel_into as geometry_map_graph_path_to_panel_into,
    screen_point_to_graph as geometry_screen_point_to_graph, segments_intersect, Rect,
};
use super::project::{
    collapsed_param_entry_pin_center, input_pin_center, node_expand_toggle_rect,
    node_param_dropdown_rect, node_param_row_rect, output_pin_center, GraphBounds, GuiProject,
    ResourceKind, NODE_PARAM_DROPDOWN_ROW_HEIGHT, NODE_WIDTH,
};
#[cfg(test)]
use super::state::AddNodeMenuEntry;
use super::state::{
    HoverInsertLink, HoverParamTarget, InputSnapshot, LinkCutState, PanDragState,
    ParamDropdownState, ParamEditState, PendingAppAction, PreviewState, RightMarqueeState,
    WireDragState,
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

/// Control flow emitted by one interaction phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InteractionPhaseControl {
    Continue,
    Finish(bool),
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

use self::state_reset::{
    cancel_node_interaction_modes, clear_param_edit_state, clear_param_hover_state,
    clear_pointer_interactions, clear_timeline_edit_state, close_primary_menus,
};

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

/// Cohesive frame-scope inputs for GUI interaction updates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InteractionFrameContext<'a> {
    config: &'a V2Config,
    viewport_width: usize,
    panel_width: usize,
    panel_height: usize,
}

impl<'a> InteractionFrameContext<'a> {
    /// Build one interaction context from immutable frame dimensions/config.
    pub(crate) fn new(
        config: &'a V2Config,
        viewport_width: usize,
        panel_width: usize,
        panel_height: usize,
    ) -> Self {
        Self {
            config,
            viewport_width,
            panel_width,
            panel_height,
        }
    }

    fn panel_context(self) -> InteractionPanelContext {
        InteractionPanelContext::new(self.panel_width, self.panel_height)
    }

    fn timeline_fps(self) -> u32 {
        self.config.animation.fps
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
    context: InteractionFrameContext<'_>,
    input: InputSnapshot,
    project: &mut GuiProject,
    state: &mut PreviewState,
) -> bool {
    let panel_ctx = context.panel_context();
    let mut changed = begin_interaction_frame(context.config, &input, project, state);

    if let InteractionPhaseControl::Finish(result) = apply_help_and_timeline_phase(
        &input,
        project,
        context.viewport_width,
        panel_ctx,
        context.timeline_fps(),
        state,
        &mut changed,
    ) {
        return finish_interaction_frame(&input, state, result);
    }
    if let InteractionPhaseControl::Finish(result) =
        apply_navigation_phase(&input, project, panel_ctx, state, &mut changed)
    {
        return finish_interaction_frame(&input, state, result);
    }
    if let InteractionPhaseControl::Finish(result) =
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

    let can_start_scrub = input.alt_down
        && input.left_down
        && state.link_cut.is_none()
        && state.drag.is_none()
        && state.wire_drag.is_none()
        && state.pan_drag.is_none()
        && state.right_marquee.is_none();
    if !can_start_scrub {
        return (false, false);
    }
    let target = scrubbable_param_at_cursor(input, project, panel_width, panel_height, state)
        .or_else(|| hover_alt_param_scrub_target(project, state))
        .or_else(|| active_param_edit_scrub_target(project, state));
    let Some(target) = target else {
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

/// Return active parameter-edit target when it can be converted into scrub mode.
fn active_param_edit_scrub_target(
    project: &GuiProject,
    state: &PreviewState,
) -> Option<HoverParamTarget> {
    let edit = state.param_edit.as_ref()?;
    if !project.node_expanded(edit.node_id) {
        return None;
    }
    if !project.param_supports_text_edit(edit.node_id, edit.param_index) {
        return None;
    }
    Some(HoverParamTarget {
        node_id: edit.node_id,
        param_index: edit.param_index,
    })
}

/// Return Alt-hover target when it is still a valid scrubbable parameter.
fn hover_alt_param_scrub_target(
    project: &GuiProject,
    state: &PreviewState,
) -> Option<HoverParamTarget> {
    let target = state.hover_alt_param?;
    if !project.node_expanded(target.node_id) {
        return None;
    }
    if !project.param_supports_text_edit(target.node_id, target.param_index) {
        return None;
    }
    Some(target)
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
        && state.param_edit.is_none()
        && state.hover_alt_param.is_none()
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

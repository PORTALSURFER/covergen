//! GUI input handling and graph-editor interaction logic.

use crate::runtime_config::V2Config;
use std::time::Duration;

use super::project::{
    input_pin_center, node_expand_toggle_rect, output_pin_center, GraphBounds, GuiProject,
    NODE_WIDTH,
};
use super::state::{
    menu_height, AddNodeMenuState, InputSnapshot, LinkCutState, PanDragState, ParamEditState,
    PreviewState, RightMarqueeState, WireDragState,
    ADD_NODE_OPTIONS,
};

const PIN_HIT_RADIUS_PX: i32 = 10;
const MIN_ZOOM: f32 = 0.35;
const MAX_ZOOM: f32 = 2.75;
const ZOOM_SENSITIVITY: f32 = 1.12;
const FOCUS_MARGIN_PX: f32 = 28.0;

/// Apply one frame of input actions to project/editor state.
///
/// Returns `true` when this frame changed visual/editor state and should be redrawn.
pub(crate) fn apply_preview_actions(
    config: &V2Config,
    input: InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    if input.toggle_pause {
        state.paused = !state.paused;
        changed = true;
    }
    if input.new_project {
        *project = GuiProject::new_empty(config.width, config.height);
        state.frame_index = 0;
        state.drag = None;
        state.wire_drag = None;
        state.link_cut = None;
        state.pan_drag = None;
        state.right_marquee = None;
        state.param_edit = None;
        state.selected_nodes.clear();
        state.pan_x = 0.0;
        state.pan_y = 0.0;
        state.zoom = 1.0;
        state.menu = AddNodeMenuState::closed();
        state.active_node = None;
        state.hover_node = None;
        state.hover_output_pin = None;
        state.hover_input_pin = None;
        state.hover_menu_item = None;
        changed = true;
    }

    changed |= handle_pan_zoom_and_focus(&input, project, panel_width, panel_height, state);
    if state.pan_drag.is_some() {
        state.drag = None;
        state.wire_drag = None;
        state.prev_left_down = input.left_down;
        return true;
    }

    changed |= handle_link_cut(&input, project, panel_width, panel_height, state);
    if state.link_cut.is_some() {
        state.drag = None;
        state.wire_drag = None;
        state.prev_left_down = input.left_down;
        return true;
    }

    changed |= handle_right_selection(&input, project, panel_width, panel_height, state);
    if state.right_marquee.is_some() {
        state.drag = None;
        state.wire_drag = None;
        state.prev_left_down = input.left_down;
        return true;
    }

    changed |= handle_add_menu_toggle(&input, panel_width, panel_height, state);
    changed |= update_hover_state(&input, project, panel_width, panel_height, state);
    changed |= handle_node_open_toggle(&input, project, panel_width, panel_height, state);
    let (param_changed, param_click_consumed) =
        handle_param_edit_input(&input, project, panel_width, panel_height, state);
    changed |= param_changed;
    if param_click_consumed {
        state.drag = None;
        state.wire_drag = None;
        state.prev_left_down = input.left_down;
        return true;
    }
    if state.param_edit.is_some() {
        state.drag = None;
        state.wire_drag = None;
        state.prev_left_down = input.left_down;
        return changed;
    }
    if state.menu.open {
        changed |= handle_add_menu_input(&input, project, panel_width, panel_height, state);
    } else {
        changed |= handle_parameter_shortcuts(&input, project, state);
        changed |= handle_wire_input(&input, project, panel_width, panel_height, state);
        if state.wire_drag.is_none() {
            changed |= handle_drag_input(&input, project, panel_width, panel_height, state);
        } else {
            state.drag = None;
        }
    }
    state.prev_left_down = input.left_down;
    changed
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
            state.frame_index = state.frame_index.wrapping_add(1);
            advanced = true;
        }
    }
    advanced
}

fn handle_pan_zoom_and_focus(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
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
    if !input.toggle_add_menu {
        return false;
    }
    if state.menu.open {
        state.menu = AddNodeMenuState::closed();
        state.wire_drag = None;
        state.param_edit = None;
        return true;
    }
    let (x, y) = input
        .mouse_pos
        .unwrap_or((panel_width as i32 / 2, panel_height as i32 / 3));
    state.menu = AddNodeMenuState::open_at(x, y, panel_width, panel_height);
    state.drag = None;
    state.wire_drag = None;
    state.param_edit = None;
    true
}

fn handle_node_open_toggle(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    if !input.toggle_node_open || state.menu.open {
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
    project.toggle_node_expanded(node_id, panel_width, panel_height)
}

fn handle_parameter_shortcuts(
    input: &InputSnapshot,
    project: &mut GuiProject,
    state: &mut PreviewState,
) -> bool {
    if state.param_edit.is_some() {
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

fn handle_right_selection(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    if input.right_clicked && !input.alt_down && !state.menu.open {
        let Some((mx, my)) = input.mouse_pos else {
            return false;
        };
        if !inside_panel(mx, my, panel_width, panel_height) {
            return false;
        }
        let (graph_x, graph_y) = screen_to_graph(mx, my, state);
        if let Some(node_id) = project.node_at(graph_x, graph_y) {
            changed |= set_single_selection(state, node_id);
            state.active_node = Some(node_id);
            state.right_marquee = None;
            state.drag = None;
            state.wire_drag = None;
            state.param_edit = None;
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
        state.param_edit = None;
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
    let rect = screen_rect_to_graph_rect(marquee.start_x, marquee.start_y, marquee.cursor_x, marquee.cursor_y, state);
    let mut out = Vec::new();
    for node in project.nodes() {
        let nx0 = node.x();
        let ny0 = node.y();
        let nx1 = nx0 + NODE_WIDTH;
        let ny1 = ny0 + node.card_height();
        if rects_overlap(rect.0, rect.1, rect.2, rect.3, nx0, ny0, nx1, ny1) {
            out.push(node.id());
        }
    }
    out
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
    if state.menu.open {
        return (changed, false);
    }
    changed |= apply_param_text_edits(input, project, state);
    if !input.left_clicked {
        return (changed, false);
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
    if input.param_commit {
        if commit_param_edit(project, edit) {
            state.param_edit = None;
            return true;
        }
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
        return false;
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
        let _ = finish_param_edit(project, state);
        return false;
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
    let Some(node_id) = project.node_at(graph_x, graph_y) else {
        let _ = finish_param_edit(project, state);
        return false;
    };
    let Some(param_index) = project.param_row_at(node_id, graph_x, graph_y) else {
        let _ = finish_param_edit(project, state);
        return false;
    };
    let _ = project.select_param(node_id, param_index);
    state.active_node = Some(node_id);
    if !project.param_value_box_contains(node_id, param_index, graph_x, graph_y) {
        let _ = finish_param_edit(project, state);
        return true;
    }
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

fn start_param_edit(
    project: &GuiProject,
    state: &mut PreviewState,
    node_id: u32,
    param_index: usize,
) -> bool {
    if state
        .param_edit
        .as_ref()
        .map(|edit| edit.node_id == node_id && edit.param_index == param_index)
        .unwrap_or(false)
    {
        return false;
    }
    let Some(value) = project.node_param_raw_value(node_id, param_index) else {
        return false;
    };
    state.param_edit = Some(ParamEditState {
        node_id,
        param_index,
        buffer: format!("{value:.3}"),
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
    if input.alt_down && input.left_clicked && !state.menu.open {
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
                state.param_edit = None;
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
        let cut_links = collect_cut_links(project, state, cut);
        for (source_id, target_id) in cut_links {
            let _ = project.disconnect_link(source_id, target_id);
        }
        state.link_cut = None;
        return true;
    }
    state.link_cut = Some(cut);
    changed
}

fn collect_cut_links(
    project: &GuiProject,
    state: &PreviewState,
    cut: LinkCutState,
) -> Vec<(u32, u32)> {
    let mut links = Vec::new();
    for target in project.nodes() {
        let Some((to_x, to_y)) = input_pin_center(target) else {
            continue;
        };
        let (to_x, to_y) = graph_point_to_panel(to_x, to_y, state);
        for source_id in target.inputs() {
            let Some(source) = project.node(*source_id) else {
                continue;
            };
            let Some((from_x, from_y)) = output_pin_center(source) else {
                continue;
            };
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
                links.push((*source_id, target.id()));
            }
        }
    }
    links.sort_unstable();
    links.dedup();
    links
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
        if state.menu.selected != hovered {
            state.menu.selected = hovered;
            changed = true;
        }
    }
    if input.menu_up {
        let next = state.menu.selected.saturating_sub(1);
        if next != state.menu.selected {
            state.menu.selected = next;
            changed = true;
        }
    }
    if input.menu_down {
        let max_index = ADD_NODE_OPTIONS.len().saturating_sub(1);
        let next = (state.menu.selected + 1).min(max_index);
        if next != state.menu.selected {
            state.menu.selected = next;
            changed = true;
        }
    }
    if input.menu_accept {
        add_menu_selected_node(project, panel_width, panel_height, state);
        return true;
    }
    if !input.left_clicked {
        return changed;
    }
    let Some((mx, my)) = input.mouse_pos else {
        state.menu = AddNodeMenuState::closed();
        return true;
    };
    if let Some(index) = state.menu.item_at(mx, my) {
        state.menu.selected = index;
        add_menu_selected_node(project, panel_width, panel_height, state);
        return true;
    } else if !state.menu.rect().contains(mx, my) {
        state.menu = AddNodeMenuState::closed();
        return true;
    }
    changed
}

fn add_menu_selected_node(
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) {
    let option = ADD_NODE_OPTIONS
        .get(state.menu.selected)
        .copied()
        .unwrap_or(ADD_NODE_OPTIONS[0]);
    let spawn_x = state.menu.x + 8;
    let spawn_y = (state.menu.y + menu_height() + 8).min(panel_height as i32 - 32);
    project.add_node(option.kind, spawn_x, spawn_y, panel_width, panel_height);
    state.menu = AddNodeMenuState::closed();
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
    if !input.left_down {
        changed |= state.drag.is_some();
        state.drag = None;
        return changed;
    }
    let Some(drag) = state.drag else {
        return changed;
    };
    let Some((mx, my)) = input.mouse_pos else {
        return changed;
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
        return changed;
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
    let node_x = graph_x - drag.offset_x;
    let node_y = graph_y - drag.offset_y;
    changed |= project.move_node(drag.node_id, node_x, node_y, panel_width, panel_height);
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
        if let Some(target_id) = state.hover_input_pin {
            let _ = project.connect_image_link(wire.source_node_id, target_id);
        }
        state.wire_drag = None;
        return true;
    }
    changed |= state.wire_drag.map(|drag| drag.cursor_x) != Some(wire.cursor_x);
    changed |= state.wire_drag.map(|drag| drag.cursor_y) != Some(wire.cursor_y);
    state.wire_drag = Some(wire);
    changed
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
    state.active_node = Some(source_node_id);
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
        return changed;
    };
    let Some((node_x, node_y, toggle_rect)) = project.node(node_id).map(|node| {
        (
            node.x(),
            node.y(),
            node_expand_toggle_rect(node),
        )
    }) else {
        let changed = state.drag.is_some();
        state.drag = None;
        return changed;
    };
    if let Some(toggle_rect) = toggle_rect {
        if toggle_rect.contains(graph_x, graph_y) {
            state.drag = None;
            state.active_node = Some(node_id);
            state.param_edit = None;
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
    });
    state.active_node = Some(node_id);
    true
}

fn inside_panel(x: i32, y: i32, panel_width: usize, panel_height: usize) -> bool {
    x >= 0 && y >= 0 && x < panel_width as i32 && y < panel_height as i32
}

fn update_hover_state(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let prev_hover_node = state.hover_node;
    let prev_hover_output = state.hover_output_pin;
    let prev_hover_input = state.hover_input_pin;
    let prev_hover_item = state.hover_menu_item;
    state.hover_node = None;
    state.hover_output_pin = None;
    state.hover_input_pin = None;
    state.hover_menu_item = None;

    let Some((mx, my)) = input.mouse_pos else {
        return prev_hover_node.is_some()
            || prev_hover_output.is_some()
            || prev_hover_input.is_some()
            || prev_hover_item.is_some();
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
        return prev_hover_node.is_some()
            || prev_hover_output.is_some()
            || prev_hover_input.is_some()
            || prev_hover_item.is_some();
    }
    if state.menu.open {
        state.hover_menu_item = state.menu.item_at(mx, my);
        return state.hover_menu_item != prev_hover_item
            || prev_hover_node.is_some()
            || prev_hover_output.is_some()
            || prev_hover_input.is_some();
    }
    let (graph_x, graph_y) = screen_to_graph(mx, my, state);
    let pin_radius = pin_hit_radius_world(state);
    let disallow_source = state.wire_drag.map(|wire| wire.source_node_id);
    state.hover_output_pin = project.output_pin_at(graph_x, graph_y, pin_radius);
    state.hover_input_pin = project.input_pin_at(graph_x, graph_y, pin_radius, disallow_source);
    if state.hover_output_pin.is_some() || state.hover_input_pin.is_some() {
        return state.hover_output_pin != prev_hover_output
            || state.hover_input_pin != prev_hover_input
            || prev_hover_node.is_some()
            || prev_hover_item.is_some();
    }
    state.hover_node = project.node_at(graph_x, graph_y);
    if state.hover_node.is_some() {
        state.active_node = state.hover_node;
    }
    state.hover_node != prev_hover_node
        || prev_hover_output.is_some()
        || prev_hover_input.is_some()
        || prev_hover_item.is_some()
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
    let bounds_w = (bounds.max_x - bounds.min_x).max(1) as f32;
    let bounds_h = (bounds.max_y - bounds.min_y).max(1) as f32;
    let avail_w = (panel_width as f32 - FOCUS_MARGIN_PX * 2.0).max(32.0);
    let avail_h = (panel_height as f32 - FOCUS_MARGIN_PX * 2.0).max(32.0);
    let zoom = (avail_w / bounds_w)
        .min(avail_h / bounds_h)
        .clamp(MIN_ZOOM, MAX_ZOOM);
    let center_x = (bounds.min_x + bounds.max_x) as f32 * 0.5;
    let center_y = (bounds.min_y + bounds.max_y) as f32 * 0.5;
    let pan_x = panel_width as f32 * 0.5 - center_x * zoom;
    let pan_y = panel_height as f32 * 0.5 - center_y * zoom;
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
        backspace_param_text, can_append_param_char, insert_param_char, marquee_moved,
        move_param_cursor_left, move_param_cursor_right, rects_overlap, segments_intersect,
        RightMarqueeState,
    };
    use crate::gui::state::ParamEditState;

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
}

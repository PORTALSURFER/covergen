//! Parameter text-edit and dropdown interaction handlers.

use super::{
    graph_rect_to_panel, inside_panel, node_param_dropdown_rect, screen_to_graph, GuiProject,
    InputSnapshot, InteractionPanelContext, ParamDropdownState, ParamEditState, PendingAppAction,
    PreviewState, Rect, NODE_PARAM_DROPDOWN_ROW_HEIGHT,
};
use crate::gui::project::{ProjectNodeKind, FEEDBACK_HISTORY_PARAM_KEY, FEEDBACK_RESET_PARAM_KEY};

/// Handle parameter text-edit and dropdown interaction for one input frame.
pub(super) fn handle_param_edit_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    ctx: InteractionPanelContext,
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
    if handle_dropdown_click(input, project, ctx, state) {
        return (true, true);
    }
    let consumed = handle_param_click(input, project, ctx, state);
    (changed, consumed)
}

/// Apply keyboard text edits to the active parameter edit buffer.
pub(super) fn apply_param_text_edits(
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

/// Handle one click in parameter rows / value fields.
pub(super) fn handle_param_click(
    input: &InputSnapshot,
    project: &mut GuiProject,
    ctx: InteractionPanelContext,
    state: &mut PreviewState,
) -> bool {
    let Some((mx, my)) = input.mouse_pos else {
        let _ = finish_param_edit(project, state);
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        return false;
    };
    if !inside_panel(mx, my, ctx.panel_width, ctx.panel_height) {
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
    if project.param_is_action_button(node_id, param_index) {
        let _ = finish_param_edit(project, state);
        state.param_dropdown = None;
        state.hover_dropdown_item = None;
        state.pending_app_action = feedback_reset_action(project, node_id, param_index);
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

fn feedback_reset_action(
    project: &GuiProject,
    node_id: u32,
    param_index: usize,
) -> Option<PendingAppAction> {
    let descriptor = project.node_param_descriptor(node_id, param_index)?;
    if descriptor.key != FEEDBACK_RESET_PARAM_KEY {
        return None;
    }
    if project.node(node_id)?.kind() != ProjectNodeKind::TexFeedback {
        return None;
    }
    let accumulation_texture_node_id = project
        .node_param_slot_index(node_id, FEEDBACK_HISTORY_PARAM_KEY)
        .and_then(|slot_index| project.texture_source_for_param(node_id, slot_index));
    Some(PendingAppAction::ResetFeedback {
        feedback_node_id: node_id,
        accumulation_texture_node_id,
    })
}

/// Handle clicks on an open dropdown list.
pub(super) fn handle_dropdown_click(
    input: &InputSnapshot,
    project: &mut GuiProject,
    ctx: InteractionPanelContext,
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
    if !inside_panel(mx, my, ctx.panel_width, ctx.panel_height) {
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

/// Return hovered dropdown option index at panel cursor location.
pub(super) fn dropdown_option_at_cursor(
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

/// Start a text-edit session for one numeric parameter.
pub(super) fn start_param_edit(
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

/// Finish active text-edit, committing when parse succeeds.
pub(super) fn finish_param_edit(project: &mut GuiProject, state: &mut PreviewState) -> bool {
    let Some(mut edit) = state.param_edit.take() else {
        return false;
    };
    let _ = commit_param_edit(project, &mut edit);
    true
}

/// Commit parsed numeric value into project parameter state.
pub(super) fn commit_param_edit(project: &mut GuiProject, edit: &mut ParamEditState) -> bool {
    let Ok(value) = edit.buffer.trim().parse::<f32>() else {
        return false;
    };
    let _ = project.set_param_value(edit.node_id, edit.param_index, value);
    true
}

/// Return whether `ch` can be appended to `current` while keeping valid numeric shape.
pub(super) fn can_append_param_char(current: &str, ch: char) -> bool {
    if !(ch.is_ascii_digit() || ch == '-' || ch == '.') {
        return false;
    }
    let mut next = String::with_capacity(current.len() + ch.len_utf8());
    next.push_str(current);
    next.push(ch);
    is_valid_param_buffer(next.as_str())
}

/// Validate one in-progress numeric edit buffer.
pub(super) fn is_valid_param_buffer(buffer: &str) -> bool {
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

/// Clamp cursor/anchor positions to buffer boundaries.
pub(super) fn clamp_param_edit_indices(edit: &mut ParamEditState) {
    let len = edit.buffer.len();
    edit.cursor = edit.cursor.min(len);
    edit.anchor = edit.anchor.min(len);
}

/// Return whether the edit state has an active selection.
pub(super) fn has_param_selection(edit: &ParamEditState) -> bool {
    edit.cursor != edit.anchor
}

/// Return normalized `(start,end)` bounds for current selection.
pub(super) fn param_selection_bounds(edit: &ParamEditState) -> (usize, usize) {
    (edit.cursor.min(edit.anchor), edit.cursor.max(edit.anchor))
}

/// Collapse selection/cursor to one insertion point.
pub(super) fn collapse_param_selection(edit: &mut ParamEditState, at: usize) {
    let clamped = at.min(edit.buffer.len());
    edit.cursor = clamped;
    edit.anchor = clamped;
}

/// Select all text for current edit buffer.
pub(super) fn select_all_param_text(edit: &mut ParamEditState) -> bool {
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

/// Delete active selection range.
pub(super) fn delete_param_selection(edit: &mut ParamEditState) -> bool {
    if !has_param_selection(edit) {
        return false;
    }
    let (start, end) = param_selection_bounds(edit);
    edit.buffer.replace_range(start..end, "");
    collapse_param_selection(edit, start);
    true
}

/// Backspace one codepoint or delete the active selection.
pub(super) fn backspace_param_text(edit: &mut ParamEditState) -> bool {
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

/// Delete one codepoint at cursor or active selection.
pub(super) fn delete_param_text(edit: &mut ParamEditState) -> bool {
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

/// Insert one validated character into the edit buffer.
pub(super) fn insert_param_char(edit: &mut ParamEditState, ch: char) -> bool {
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

/// Move cursor left (optionally extending selection).
pub(super) fn move_param_cursor_left(edit: &mut ParamEditState, extend_selection: bool) -> bool {
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

/// Move cursor right (optionally extending selection).
pub(super) fn move_param_cursor_right(edit: &mut ParamEditState, extend_selection: bool) -> bool {
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

/// Return previous UTF-8 character boundary at or before `index`.
pub(super) fn prev_char_boundary(text: &str, index: usize) -> usize {
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

/// Return next UTF-8 character boundary at or after `index`.
pub(super) fn next_char_boundary(text: &str, index: usize) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(buffer: &str, cursor: usize, anchor: usize) -> ParamEditState {
        ParamEditState {
            node_id: 1,
            param_index: 0,
            buffer: buffer.to_string(),
            cursor,
            anchor,
        }
    }

    #[test]
    fn insert_param_char_rejects_second_decimal_point() {
        let mut state = edit("1.2", 3, 3);
        assert!(!insert_param_char(&mut state, '.'));
        assert_eq!(state.buffer, "1.2");
        assert_eq!(state.cursor, 3);
    }

    #[test]
    fn insert_param_char_replaces_active_selection() {
        let mut state = edit("12.4", 4, 1);
        assert!(insert_param_char(&mut state, '9'));
        assert_eq!(state.buffer, "19");
        assert_eq!(state.cursor, 2);
        assert_eq!(state.anchor, 2);
    }

    #[test]
    fn delete_and_cursor_helpers_respect_utf8_boundaries() {
        let text = "aéz";
        assert_eq!(next_char_boundary(text, 1), 3);
        assert_eq!(prev_char_boundary(text, 3), 1);

        let mut state = edit("aéz", 3, 3);
        assert!(backspace_param_text(&mut state));
        assert_eq!(state.buffer, "az");
        assert_eq!(state.cursor, 1);
    }
}

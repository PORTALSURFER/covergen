//! Main/export menu input handling.

use crate::gui::state::{
    AddNodeMenuState, ExportMenuItem, InputSnapshot, MainMenuItem, MainMenuState, PendingAppAction,
    PopupDragState, PreviewState, MAIN_MENU_WIDTH,
};
use crate::gui::timeline::editor_panel_height;

use super::{clear_param_edit_state, clear_param_hover_state, clear_timeline_edit_state};

/// Toggle the main menu open/closed state.
pub(super) fn handle_main_menu_toggle(
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

/// Handle active main/export menu keyboard and pointer input.
pub(super) fn handle_main_export_menu_input(
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
            let opened = crate::gui::state::ExportMenuState::open_at(
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

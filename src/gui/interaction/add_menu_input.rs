//! Add-node menu input handling.

use crate::gui::project::GuiProject;
use crate::gui::state::{
    add_node_options, AddNodeMenuEntry, AddNodeMenuState, InputSnapshot, MainMenuState,
    PreviewState,
};
use crate::gui::timeline::editor_panel_height;

use super::drag::hover_insert_link_at_cursor;
use super::{
    clear_param_edit_state, clear_param_hover_state, clear_timeline_edit_state,
    InteractionPanelContext,
};

/// Toggle the add-node menu open/closed state.
pub(super) fn handle_add_menu_toggle(
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
        state.main_menu = MainMenuState::closed();
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

/// Handle active add-node menu keyboard and pointer input.
pub(super) fn handle_add_menu_input(
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
            let option = add_node_options()[option_index];
            let drop_cursor_x = state.menu.open_cursor_x;
            let drop_cursor_y = state.menu.open_cursor_y;
            let (spawn_x, spawn_y) = super::screen_to_graph(drop_cursor_x, drop_cursor_y, state);
            let node_id =
                project.add_node(option.kind, spawn_x, spawn_y, panel_width, panel_height);
            if let Some(link) = hover_insert_link_at_cursor(
                project,
                InteractionPanelContext::new(panel_width, panel_height),
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

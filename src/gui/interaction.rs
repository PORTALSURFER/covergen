//! GUI input handling and graph-editor interaction logic.

use crate::runtime_config::V2Config;

use super::project::GuiProject;
use super::state::{menu_height, AddNodeMenuState, InputSnapshot, PreviewState, ADD_NODE_OPTIONS};

/// Apply one frame of input actions to project/editor state.
pub(crate) fn apply_preview_actions(
    config: &V2Config,
    input: InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) {
    if input.toggle_pause {
        state.paused = !state.paused;
    }
    if input.new_project {
        *project = GuiProject::new_empty(config.width, config.height);
        state.frame_index = 0;
        state.drag = None;
        state.menu = AddNodeMenuState::closed();
        state.hover_node = None;
        state.hover_menu_item = None;
    }

    handle_add_menu_toggle(&input, panel_width, panel_height, state);
    update_hover_state(input, project, panel_width, panel_height, state);
    if state.menu.open {
        handle_add_menu_input(&input, project, panel_width, panel_height, state);
    } else {
        handle_drag_input(&input, project, panel_width, panel_height, state);
    }
    state.prev_left_down = input.left_down;
}

/// Advance timeline frame counter when unpaused.
pub(crate) fn step_timeline_if_running(state: &mut PreviewState) {
    if !state.paused {
        state.frame_index = state.frame_index.wrapping_add(1);
    }
}

fn handle_add_menu_toggle(
    input: &InputSnapshot,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) {
    if !input.toggle_add_menu {
        return;
    }
    if state.menu.open {
        state.menu = AddNodeMenuState::closed();
        return;
    }
    let (x, y) = input
        .mouse_pos
        .unwrap_or((panel_width as i32 / 2, panel_height as i32 / 3));
    state.menu = AddNodeMenuState::open_at(x, y, panel_width, panel_height);
    state.drag = None;
}

fn handle_add_menu_input(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) {
    if let Some(hovered) = state.hover_menu_item {
        state.menu.selected = hovered;
    }
    if input.menu_up {
        state.menu.selected = state.menu.selected.saturating_sub(1);
    }
    if input.menu_down {
        let max_index = ADD_NODE_OPTIONS.len().saturating_sub(1);
        state.menu.selected = (state.menu.selected + 1).min(max_index);
    }
    if input.menu_accept {
        add_menu_selected_node(project, panel_width, panel_height, state);
        return;
    }
    if !input.left_clicked {
        return;
    }
    let Some((mx, my)) = input.mouse_pos else {
        state.menu = AddNodeMenuState::closed();
        return;
    };
    if let Some(index) = state.menu.item_at(mx, my) {
        state.menu.selected = index;
        add_menu_selected_node(project, panel_width, panel_height, state);
    } else if !state.menu.rect().contains(mx, my) {
        state.menu = AddNodeMenuState::closed();
    }
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
) {
    if input.left_clicked {
        begin_drag_if_node_hit(input, project, panel_width, panel_height, state);
    }
    if !input.left_down {
        state.drag = None;
        return;
    }
    let Some(drag) = state.drag else {
        return;
    };
    let Some((mx, my)) = input.mouse_pos else {
        return;
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
        return;
    }
    let node_x = mx - drag.offset_x;
    let node_y = my - drag.offset_y;
    project.move_node(drag.node_id, node_x, node_y, panel_width, panel_height);
}

fn begin_drag_if_node_hit(
    input: &InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) {
    let Some((mx, my)) = input.mouse_pos else {
        return;
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
        return;
    }
    let Some(node_id) = project.node_at(mx, my) else {
        state.drag = None;
        return;
    };
    let Some(node) = project.node(node_id) else {
        state.drag = None;
        return;
    };
    state.drag = Some(super::state::DragState {
        node_id,
        offset_x: mx - node.x(),
        offset_y: my - node.y(),
    });
}

fn inside_panel(x: i32, y: i32, panel_width: usize, panel_height: usize) -> bool {
    x >= 0 && y >= 0 && x < panel_width as i32 && y < panel_height as i32
}

fn update_hover_state(
    input: InputSnapshot,
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) {
    state.hover_node = None;
    state.hover_menu_item = None;

    let Some((mx, my)) = input.mouse_pos else {
        return;
    };
    if !inside_panel(mx, my, panel_width, panel_height) {
        return;
    }
    if state.menu.open {
        state.hover_menu_item = state.menu.item_at(mx, my);
        return;
    }
    state.hover_node = project.node_at(mx, my);
}

//! GUI input handling and graph-editor interaction logic.

use crate::runtime_config::V2Config;
use std::time::Duration;

use super::project::GuiProject;
use super::state::{
    menu_height, AddNodeMenuState, InputSnapshot, PreviewState, WireDragState, ADD_NODE_OPTIONS,
};

const PIN_HIT_RADIUS_PX: i32 = 10;

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
        state.menu = AddNodeMenuState::closed();
        state.hover_node = None;
        state.hover_output_pin = None;
        state.hover_input_pin = None;
        state.hover_menu_item = None;
        changed = true;
    }

    changed |= handle_add_menu_toggle(&input, panel_width, panel_height, state);
    changed |= update_hover_state(input, project, panel_width, panel_height, state);
    if state.menu.open {
        changed |= handle_add_menu_input(&input, project, panel_width, panel_height, state);
    } else {
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
        return true;
    }
    let (x, y) = input
        .mouse_pos
        .unwrap_or((panel_width as i32 / 2, panel_height as i32 / 3));
    state.menu = AddNodeMenuState::open_at(x, y, panel_width, panel_height);
    state.drag = None;
    state.wire_drag = None;
    true
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
    let node_x = mx - drag.offset_x;
    let node_y = my - drag.offset_y;
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
            project.connect_image_link(wire.source_node_id, target_id);
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
    let Some(source_node_id) = project.output_pin_at(mx, my, PIN_HIT_RADIUS_PX) else {
        return false;
    };
    state.drag = None;
    state.wire_drag = Some(WireDragState {
        source_node_id,
        cursor_x: mx,
        cursor_y: my,
    });
    true
}

fn begin_drag_if_node_hit(
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
    let Some(node_id) = project.node_at(mx, my) else {
        let changed = state.drag.is_some();
        state.drag = None;
        return changed;
    };
    let Some(node) = project.node(node_id) else {
        let changed = state.drag.is_some();
        state.drag = None;
        return changed;
    };
    if state.drag.map(|drag| drag.node_id) == Some(node_id) {
        return false;
    }
    state.drag = Some(super::state::DragState {
        node_id,
        offset_x: mx - node.x(),
        offset_y: my - node.y(),
    });
    true
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
    let disallow_source = state.wire_drag.map(|wire| wire.source_node_id);
    state.hover_output_pin = project.output_pin_at(mx, my, PIN_HIT_RADIUS_PX);
    state.hover_input_pin = project.input_pin_at(mx, my, PIN_HIT_RADIUS_PX, disallow_source);
    if state.hover_output_pin.is_some() || state.hover_input_pin.is_some() {
        return state.hover_output_pin != prev_hover_output
            || state.hover_input_pin != prev_hover_input
            || prev_hover_node.is_some()
            || prev_hover_item.is_some();
    }
    state.hover_node = project.node_at(mx, my);
    state.hover_node != prev_hover_node
        || prev_hover_output.is_some()
        || prev_hover_input.is_some()
        || prev_hover_item.is_some()
}

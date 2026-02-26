//! Window-event input capture for one GUI frame.

use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use super::state::InputSnapshot;

/// Accumulates one frame of input edge-triggered events and pointer state.
#[derive(Debug, Default)]
pub(crate) struct InputCollector {
    mouse_pos: Option<(f64, f64)>,
    left_down: bool,
    left_clicked: bool,
    right_down: bool,
    right_clicked: bool,
    left_alt_down: bool,
    right_alt_down: bool,
    left_shift_down: bool,
    right_shift_down: bool,
    left_ctrl_down: bool,
    right_ctrl_down: bool,
    middle_down: bool,
    middle_clicked: bool,
    wheel_lines_y: f32,
    toggle_pause: bool,
    new_project: bool,
    focus_all: bool,
    open_help: bool,
    toggle_node_open: bool,
    toggle_add_menu: bool,
    toggle_main_menu: bool,
    menu_up: bool,
    menu_down: bool,
    param_dec: bool,
    param_inc: bool,
    menu_accept: bool,
    typed_text: String,
    param_backspace: bool,
    param_delete: bool,
    param_select_all: bool,
    param_commit: bool,
    param_cancel: bool,
}

impl InputCollector {
    /// Update collector state from one winit window event.
    pub(crate) fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = Some((position.x, position.y));
            }
            WindowEvent::CursorLeft { .. } => {
                self.mouse_pos = None;
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => self.handle_left_button(*state),
            WindowEvent::MouseInput {
                button: MouseButton::Middle,
                state,
                ..
            } => self.handle_middle_button(*state),
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => self.handle_right_button(*state),
            WindowEvent::MouseWheel { delta, .. } => {
                self.wheel_lines_y += mouse_wheel_lines(*delta);
            }
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(
                event.physical_key,
                event.state,
                event.repeat,
                event.text.as_ref().map(|text| text.as_ref()),
            ),
            _ => {}
        }
    }

    /// Build one immutable input snapshot for this frame and reset edge flags.
    pub(crate) fn snapshot(
        &mut self,
        viewport_width: usize,
        viewport_height: usize,
    ) -> InputSnapshot {
        let mouse_pos = self.normalize_mouse(viewport_width, viewport_height);
        let snapshot = InputSnapshot {
            mouse_pos,
            left_down: self.left_down,
            left_clicked: self.left_clicked,
            right_down: self.right_down,
            right_clicked: self.right_clicked,
            alt_down: self.left_alt_down || self.right_alt_down,
            shift_down: self.left_shift_down || self.right_shift_down,
            middle_down: self.middle_down,
            middle_clicked: self.middle_clicked,
            wheel_lines_y: self.wheel_lines_y,
            toggle_pause: self.toggle_pause,
            new_project: self.new_project,
            focus_all: self.focus_all,
            open_help: self.open_help,
            toggle_node_open: self.toggle_node_open,
            toggle_add_menu: self.toggle_add_menu,
            toggle_main_menu: self.toggle_main_menu,
            menu_up: self.menu_up,
            menu_down: self.menu_down,
            param_dec: self.param_dec,
            param_inc: self.param_inc,
            menu_accept: self.menu_accept,
            typed_text: self.typed_text.clone(),
            param_backspace: self.param_backspace,
            param_delete: self.param_delete,
            param_select_all: self.param_select_all,
            param_commit: self.param_commit,
            param_cancel: self.param_cancel,
        };
        self.left_clicked = false;
        self.right_clicked = false;
        self.middle_clicked = false;
        self.wheel_lines_y = 0.0;
        self.toggle_pause = false;
        self.new_project = false;
        self.focus_all = false;
        self.open_help = false;
        self.toggle_node_open = false;
        self.toggle_add_menu = false;
        self.toggle_main_menu = false;
        self.menu_up = false;
        self.menu_down = false;
        self.param_dec = false;
        self.param_inc = false;
        self.menu_accept = false;
        self.typed_text.clear();
        self.param_backspace = false;
        self.param_delete = false;
        self.param_select_all = false;
        self.param_commit = false;
        self.param_cancel = false;
        snapshot
    }

    fn handle_left_button(&mut self, state: ElementState) {
        if state == ElementState::Pressed && !self.left_down {
            self.left_clicked = true;
        }
        self.left_down = state == ElementState::Pressed;
    }

    fn handle_middle_button(&mut self, state: ElementState) {
        if state == ElementState::Pressed && !self.middle_down {
            self.middle_clicked = true;
        }
        self.middle_down = state == ElementState::Pressed;
    }

    fn handle_right_button(&mut self, state: ElementState) {
        if state == ElementState::Pressed && !self.right_down {
            self.right_clicked = true;
        }
        self.right_down = state == ElementState::Pressed;
    }

    fn handle_key(
        &mut self,
        key: PhysicalKey,
        state: ElementState,
        repeat: bool,
        text: Option<&str>,
    ) {
        let PhysicalKey::Code(code) = key else {
            return;
        };
        match code {
            KeyCode::AltLeft => {
                self.left_alt_down = state == ElementState::Pressed;
                return;
            }
            KeyCode::AltRight => {
                self.right_alt_down = state == ElementState::Pressed;
                return;
            }
            KeyCode::ShiftLeft => {
                self.left_shift_down = state == ElementState::Pressed;
                return;
            }
            KeyCode::ShiftRight => {
                self.right_shift_down = state == ElementState::Pressed;
                return;
            }
            KeyCode::ControlLeft => {
                self.left_ctrl_down = state == ElementState::Pressed;
                return;
            }
            KeyCode::ControlRight => {
                self.right_ctrl_down = state == ElementState::Pressed;
                return;
            }
            _ => {}
        }
        if state != ElementState::Pressed || repeat {
            return;
        }
        match code {
            KeyCode::Backspace => self.param_backspace = true,
            KeyCode::Delete => self.param_delete = true,
            KeyCode::Enter | KeyCode::NumpadEnter => self.param_commit = true,
            KeyCode::Escape => self.param_cancel = true,
            _ => {}
        }
        if let Some(text) = text {
            self.append_text_input(text);
        }
        match code {
            KeyCode::KeyA if self.left_ctrl_down || self.right_ctrl_down => {
                self.param_select_all = true;
            }
            KeyCode::Space => self.toggle_add_menu = true,
            KeyCode::Backquote => self.toggle_main_menu = true,
            KeyCode::Tab => self.toggle_node_open = true,
            KeyCode::KeyP => self.toggle_pause = true,
            KeyCode::KeyF => self.focus_all = true,
            KeyCode::F1 => self.open_help = true,
            KeyCode::ArrowUp => self.menu_up = true,
            KeyCode::ArrowDown => self.menu_down = true,
            KeyCode::ArrowLeft => self.param_dec = true,
            KeyCode::ArrowRight => self.param_inc = true,
            KeyCode::Enter | KeyCode::NumpadEnter => self.menu_accept = true,
            _ => {}
        }
    }

    fn append_text_input(&mut self, text: &str) {
        for ch in text.chars() {
            if ch.is_control() {
                continue;
            }
            self.typed_text.push(ch);
        }
    }

    fn normalize_mouse(&self, viewport_width: usize, viewport_height: usize) -> Option<(i32, i32)> {
        let (x, y) = self.mouse_pos?;
        if viewport_width == 0 || viewport_height == 0 {
            return None;
        }
        let max_x = (viewport_width.saturating_sub(1)) as f64;
        let max_y = (viewport_height.saturating_sub(1)) as f64;
        let nx = x.floor().clamp(0.0, max_x) as i32;
        let ny = y.floor().clamp(0.0, max_y) as i32;
        Some((nx, ny))
    }
}

fn mouse_wheel_lines(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        // Approximate one mouse wheel notch for typical pixel-based events.
        MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 48.0,
    }
}

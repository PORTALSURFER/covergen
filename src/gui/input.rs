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
    middle_down: bool,
    middle_clicked: bool,
    wheel_lines_y: f32,
    toggle_pause: bool,
    new_project: bool,
    focus_all: bool,
    toggle_add_menu: bool,
    menu_up: bool,
    menu_down: bool,
    menu_accept: bool,
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
            WindowEvent::MouseWheel { delta, .. } => {
                self.wheel_lines_y += mouse_wheel_lines(*delta);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_key(event.physical_key, event.state, event.repeat)
            }
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
            middle_down: self.middle_down,
            middle_clicked: self.middle_clicked,
            wheel_lines_y: self.wheel_lines_y,
            toggle_pause: self.toggle_pause,
            new_project: self.new_project,
            focus_all: self.focus_all,
            toggle_add_menu: self.toggle_add_menu,
            menu_up: self.menu_up,
            menu_down: self.menu_down,
            menu_accept: self.menu_accept,
        };
        self.left_clicked = false;
        self.middle_clicked = false;
        self.wheel_lines_y = 0.0;
        self.toggle_pause = false;
        self.new_project = false;
        self.focus_all = false;
        self.toggle_add_menu = false;
        self.menu_up = false;
        self.menu_down = false;
        self.menu_accept = false;
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

    fn handle_key(&mut self, key: PhysicalKey, state: ElementState, repeat: bool) {
        if state != ElementState::Pressed || repeat {
            return;
        }
        let PhysicalKey::Code(code) = key else {
            return;
        };
        match code {
            KeyCode::Space => self.toggle_pause = true,
            KeyCode::KeyF => self.focus_all = true,
            KeyCode::KeyR => self.new_project = true,
            KeyCode::Tab => self.toggle_add_menu = true,
            KeyCode::ArrowUp => self.menu_up = true,
            KeyCode::ArrowDown => self.menu_down = true,
            KeyCode::Enter => self.menu_accept = true,
            _ => {}
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

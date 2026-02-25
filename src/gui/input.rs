//! Window-event input capture for one GUI frame.

use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use super::state::InputSnapshot;

/// Accumulates one frame of input edge-triggered events and pointer state.
#[derive(Debug, Default)]
pub(crate) struct InputCollector {
    mouse_pos: Option<(f64, f64)>,
    left_down: bool,
    left_clicked: bool,
    toggle_pause: bool,
    new_project: bool,
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
            toggle_pause: self.toggle_pause,
            new_project: self.new_project,
            toggle_add_menu: self.toggle_add_menu,
            menu_up: self.menu_up,
            menu_down: self.menu_down,
            menu_accept: self.menu_accept,
        };
        self.left_clicked = false;
        self.toggle_pause = false;
        self.new_project = false;
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

    fn handle_key(&mut self, key: PhysicalKey, state: ElementState, repeat: bool) {
        if state != ElementState::Pressed || repeat {
            return;
        }
        let PhysicalKey::Code(code) = key else {
            return;
        };
        match code {
            KeyCode::Space => self.toggle_pause = true,
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

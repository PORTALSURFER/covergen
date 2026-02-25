//! GUI host window and software panel drawing.

use std::error::Error;

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

use super::draw::{draw_line, draw_text, fill_rect, stroke_rect};
use super::node_editor::NodeEditorLayout;
use super::project::GuiProject;
use super::state::{InputSnapshot, PreviewState, ADD_NODE_OPTIONS};

const PREVIEW_BG: u32 = 0xFF0A0D12;
const DIVIDER_COLOR: u32 = 0xFF2A313A;
const HUD_TEXT_COLOR: u32 = 0xFFE5E7EB;
const MENU_BG: u32 = 0xFF1D2430;
const MENU_SELECTED: u32 = 0xFF334155;
const MENU_BORDER: u32 = 0xFF475569;
const MENU_TEXT: u32 = 0xFFE2E8F0;

/// Host window for split-panel GUI rendering.
pub(crate) struct TopPreviewWindow {
    width: usize,
    height: usize,
    panel_width: usize,
    preview_width: usize,
    preview_height: usize,
    rgb: Vec<u32>,
    window: Window,
    editor: NodeEditorLayout,
}

impl TopPreviewWindow {
    /// Create one split-panel window with left graph editor and right preview.
    pub(crate) fn new(
        preview_width: u32,
        preview_height: u32,
        panel_width: usize,
        target_fps: u32,
    ) -> Result<Self, Box<dyn Error>> {
        let preview_width = usize::try_from(preview_width).map_err(|_| "invalid preview width")?;
        let preview_height =
            usize::try_from(preview_height).map_err(|_| "invalid preview height")?;
        let width = panel_width
            .checked_add(preview_width)
            .ok_or("invalid split-panel width")?;
        let height = preview_height;

        let mut window = Window::new(
            "covergen TD",
            width,
            height,
            WindowOptions {
                resize: true,
                ..WindowOptions::default()
            },
        )?;
        window.set_target_fps(target_fps as usize);

        let editor = NodeEditorLayout::new(panel_width);
        let rgb = vec![
            PREVIEW_BG;
            width
                .checked_mul(height)
                .ok_or("invalid panel dimensions")?
        ];

        Ok(Self {
            width,
            height,
            panel_width,
            preview_width,
            preview_height,
            rgb,
            window,
            editor,
        })
    }

    /// Return left-panel width.
    pub(crate) fn panel_width(&self) -> usize {
        self.panel_width
    }

    /// Return window height.
    pub(crate) fn height(&self) -> usize {
        self.height
    }

    /// Return true while window is open and Escape not held.
    pub(crate) fn is_open(&self) -> bool {
        self.window.is_open() && !self.window.is_key_down(Key::Escape)
    }

    /// Capture one frame of keyboard/mouse input.
    pub(crate) fn capture_input(&self, prev_left_down: bool) -> InputSnapshot {
        let mouse_pos = self.mouse_pos_in_buffer_space();
        let left_down = self.window.get_mouse_down(MouseButton::Left);

        InputSnapshot {
            mouse_pos,
            left_down,
            left_clicked: left_down && !prev_left_down,
            toggle_pause: self.window.is_key_pressed(Key::Space, KeyRepeat::No),
            new_project: self.window.is_key_pressed(Key::R, KeyRepeat::No),
            toggle_add_menu: self.window.is_key_pressed(Key::Tab, KeyRepeat::No),
            menu_up: self.window.is_key_pressed(Key::Up, KeyRepeat::No),
            menu_down: self.window.is_key_pressed(Key::Down, KeyRepeat::No),
            menu_accept: self.window.is_key_pressed(Key::Enter, KeyRepeat::No),
        }
    }

    /// Draw current project/editor frame to window backbuffer.
    pub(crate) fn present(
        &mut self,
        project: &GuiProject,
        state: &PreviewState,
    ) -> Result<(), Box<dyn Error>> {
        self.rgb.fill(PREVIEW_BG);
        self.editor.draw(
            &mut self.rgb,
            self.width,
            self.height,
            project,
            state.hover_node,
            state.drag.map(|drag| drag.node_id),
        );
        self.draw_preview_canvas();
        self.draw_preview_hud(project, state);
        if state.menu.open {
            self.draw_add_node_menu(state);
        }
        self.draw_divider();
        self.window
            .update_with_buffer(&self.rgb, self.width, self.height)?;
        Ok(())
    }

    /// Update window title string.
    pub(crate) fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    fn draw_preview_canvas(&mut self) {
        for y in 0..self.preview_height {
            let row = y * self.width + self.panel_width;
            for x in 0..self.preview_width {
                self.rgb[row + x] = PREVIEW_BG;
            }
        }
    }

    fn draw_preview_hud(&mut self, project: &GuiProject, state: &PreviewState) {
        let status = if state.paused { "PAUSED" } else { "RUNNING" };
        let text = format!(
            "TOP VIEWPORT  {}  {:.1} FPS  F{}  {}  {}x{}",
            status,
            state.avg_fps,
            state.frame_index,
            project.name,
            project.preview_width,
            project.preview_height
        );
        draw_text(
            &mut self.rgb,
            self.width,
            self.height,
            (self.panel_width + 12) as i32,
            12,
            &text,
            HUD_TEXT_COLOR,
        );
        draw_text(
            &mut self.rgb,
            self.width,
            self.height,
            (self.panel_width + 12) as i32,
            28,
            "No connected TOP output yet.",
            0xFF9CA3AF,
        );
    }

    fn draw_add_node_menu(&mut self, state: &PreviewState) {
        let rect = state.menu.rect();
        fill_rect(&mut self.rgb, self.width, self.height, rect, MENU_BG);
        stroke_rect(&mut self.rgb, self.width, self.height, rect, MENU_BORDER);
        draw_text(
            &mut self.rgb,
            self.width,
            self.height,
            rect.x + 8,
            rect.y + 8,
            "Add Node",
            MENU_TEXT,
        );

        for (index, option) in ADD_NODE_OPTIONS.iter().enumerate() {
            let Some(item) = state.menu.item_rect(index) else {
                continue;
            };
            let is_selected = index == state.menu.selected;
            let is_hovered = state.hover_menu_item == Some(index);
            if is_selected || is_hovered {
                fill_rect(&mut self.rgb, self.width, self.height, item, MENU_SELECTED);
            }
            draw_text(
                &mut self.rgb,
                self.width,
                self.height,
                item.x + 8,
                item.y + 8,
                option.label,
                MENU_TEXT,
            );
        }
    }

    fn mouse_pos_in_buffer_space(&self) -> Option<(i32, i32)> {
        let (window_w, window_h) = self.window.get_size();
        if window_w == 0 || window_h == 0 {
            return None;
        }
        self.window.get_mouse_pos(MouseMode::Clamp).map(|(x, y)| {
            let nx = x.max(0.0) * self.width as f32 / window_w as f32;
            let ny = y.max(0.0) * self.height as f32 / window_h as f32;
            (nx.floor() as i32, ny.floor() as i32)
        })
    }

    fn draw_divider(&mut self) {
        let x = self.panel_width as i32 - 1;
        draw_line(
            &mut self.rgb,
            self.width,
            self.height,
            x,
            0,
            x,
            self.height as i32 - 1,
            DIVIDER_COLOR,
        );
    }
}

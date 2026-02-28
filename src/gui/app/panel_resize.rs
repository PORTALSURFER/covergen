//! Panel divider sizing and cursor interaction helpers.

use super::*;

/// Active divider drag metadata for panel resizing.
#[derive(Clone, Copy, Debug)]
pub(super) struct PanelResizeDrag {
    pub(super) grab_offset_px: i32,
}

impl GuiApp {
    pub(super) fn apply_panel_resize_input(&mut self, input: &InputSnapshot) -> (bool, bool) {
        let mut changed = false;
        let mut consumed = false;
        if input.left_clicked && self.try_begin_panel_resize(input.mouse_pos) {
            consumed = true;
        }
        let Some(drag) = self.panel_resize_drag else {
            return (changed, consumed);
        };
        consumed = true;
        if !input.left_down {
            self.panel_resize_drag = None;
            self.update_resize_cursor(input.mouse_pos);
            return (changed, consumed);
        }
        let Some((mx, _)) = input.mouse_pos else {
            return (changed, consumed);
        };
        let requested = (mx - drag.grab_offset_px + 1).max(1) as usize;
        let next_width = clamp_panel_width(requested, self.renderer.width());
        if next_width != self.panel_width {
            self.panel_width = next_width;
            changed = true;
        }
        (changed, consumed)
    }

    pub(super) fn try_begin_panel_resize(&mut self, mouse_pos: Option<(i32, i32)>) -> bool {
        let Some((mx, my)) = mouse_pos else {
            return false;
        };
        if !on_panel_divider(mx, my, self.panel_width, self.renderer.height()) {
            return false;
        }
        let divider_x = self.panel_width as i32 - 1;
        self.panel_resize_drag = Some(PanelResizeDrag {
            grab_offset_px: mx - divider_x,
        });
        self.state.drag = None;
        self.state.wire_drag = None;
        self.state.hover_param_target = None;
        self.state.hover_param = None;
        self.state.hover_insert_link = None;
        true
    }

    pub(super) fn update_resize_cursor(&mut self, mouse_pos: Option<(i32, i32)>) {
        let resize_active = self.panel_resize_drag.is_some()
            || mouse_pos
                .map(|(mx, my)| on_panel_divider(mx, my, self.panel_width, self.renderer.height()))
                .unwrap_or(false);
        if resize_active == self.resize_cursor_active {
            return;
        }
        self.resize_cursor_active = resize_active;
        let icon = if resize_active {
            CursorIcon::EwResize
        } else {
            CursorIcon::Default
        };
        self.window.set_cursor_icon(icon);
    }
}

pub(super) fn clamp_panel_width(requested: usize, viewport_width: usize) -> usize {
    if viewport_width <= 1 {
        return 1;
    }
    let hard_max = viewport_width - 1;
    let min_width = MIN_PANEL_WIDTH.min(hard_max);
    let max_width = hard_max.saturating_sub(MIN_PREVIEW_WIDTH).max(min_width);
    requested.clamp(min_width, max_width)
}

/// Return initial editor-panel width so the right preview starts near 1/3.
pub(super) fn launch_panel_width(viewport_width: usize) -> usize {
    viewport_width.saturating_mul(2) / 3
}

fn on_panel_divider(mx: i32, my: i32, panel_width: usize, panel_height: usize) -> bool {
    let editor_h = editor_panel_height(panel_height) as i32;
    if my < 0 || my >= editor_h {
        return false;
    }
    let divider_x = panel_width as i32 - 1;
    (mx - divider_x).abs() <= DIVIDER_HIT_SLOP_PX
}

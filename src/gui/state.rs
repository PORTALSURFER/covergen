//! Shared GUI state and menu models.

use super::geometry::Rect;
use super::project::ProjectNodeKind;
use crate::runtime_config::V2Config;

/// Width of add-node popup menu.
pub(crate) const MENU_WIDTH: i32 = 220;
/// Height of add-node menu header row.
pub(crate) const MENU_HEADER_HEIGHT: i32 = 26;
/// Inner menu padding from the outer frame.
pub(crate) const MENU_INNER_PADDING: i32 = 4;
/// Height of one add-node menu row.
pub(crate) const MENU_ITEM_HEIGHT: i32 = 26;

/// One add-node menu entry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AddNodeOption {
    pub(crate) kind: ProjectNodeKind,
}

impl AddNodeOption {
    /// Return display label used by the add-node popup list.
    pub(crate) const fn label(self) -> &'static str {
        self.kind.label()
    }
}

/// Menu entries currently exposed in the graph editor.
pub(crate) const ADD_NODE_OPTIONS: [AddNodeOption; 2] = [
    AddNodeOption {
        kind: ProjectNodeKind::TopBasic,
    },
    AddNodeOption {
        kind: ProjectNodeKind::Output,
    },
];

/// Snapshot of one frame's input state.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct InputSnapshot {
    pub(crate) mouse_pos: Option<(i32, i32)>,
    pub(crate) left_down: bool,
    pub(crate) left_clicked: bool,
    pub(crate) toggle_pause: bool,
    pub(crate) new_project: bool,
    pub(crate) toggle_add_menu: bool,
    pub(crate) menu_up: bool,
    pub(crate) menu_down: bool,
    pub(crate) menu_accept: bool,
}

/// Active node drag state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DragState {
    pub(crate) node_id: u32,
    pub(crate) offset_x: i32,
    pub(crate) offset_y: i32,
}

/// Active wire-drag state from a source output pin.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WireDragState {
    pub(crate) source_node_id: u32,
    pub(crate) cursor_x: i32,
    pub(crate) cursor_y: i32,
}

/// Add-node popup menu state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AddNodeMenuState {
    pub(crate) open: bool,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) selected: usize,
}

impl AddNodeMenuState {
    /// Return closed menu state.
    pub(crate) fn closed() -> Self {
        Self {
            open: false,
            x: 0,
            y: 0,
            selected: 0,
        }
    }

    /// Create an opened menu clamped to panel bounds.
    pub(crate) fn open_at(x: i32, y: i32, panel_width: usize, panel_height: usize) -> Self {
        let menu_h = menu_height();
        let max_x = (panel_width as i32 - MENU_WIDTH - 8).max(8);
        let max_y = (panel_height as i32 - menu_h - 8).max(8);
        Self {
            open: true,
            x: x.clamp(8, max_x),
            y: y.clamp(8, max_y),
            selected: 0,
        }
    }

    /// Return menu rectangle in panel coordinates.
    pub(crate) fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, MENU_WIDTH, menu_height())
    }

    /// Return one item rectangle in panel coordinates.
    pub(crate) fn item_rect(&self, index: usize) -> Option<Rect> {
        if index >= ADD_NODE_OPTIONS.len() {
            return None;
        }
        let y = self.y + MENU_HEADER_HEIGHT + index as i32 * MENU_ITEM_HEIGHT;
        Some(Rect::new(
            self.x + MENU_INNER_PADDING,
            y,
            MENU_WIDTH - (MENU_INNER_PADDING * 2),
            MENU_ITEM_HEIGHT - 2,
        ))
    }

    /// Return hovered item index for cursor position.
    pub(crate) fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        for index in 0..ADD_NODE_OPTIONS.len() {
            let Some(rect) = self.item_rect(index) else {
                continue;
            };
            if rect.contains(x, y) {
                return Some(index);
            }
        }
        None
    }
}

/// Runtime animation/editor state for one GUI session.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PreviewState {
    pub(crate) frame_index: u32,
    pub(crate) timeline_accum_secs: f32,
    pub(crate) paused: bool,
    pub(crate) avg_fps: f32,
    pub(crate) prev_left_down: bool,
    pub(crate) drag: Option<DragState>,
    pub(crate) wire_drag: Option<WireDragState>,
    pub(crate) menu: AddNodeMenuState,
    pub(crate) hover_node: Option<u32>,
    pub(crate) hover_output_pin: Option<u32>,
    pub(crate) hover_input_pin: Option<u32>,
    pub(crate) hover_menu_item: Option<usize>,
}

impl PreviewState {
    /// Create initial GUI state for one run.
    pub(crate) fn new(_config: &V2Config) -> Self {
        Self {
            frame_index: 0,
            timeline_accum_secs: 0.0,
            paused: false,
            avg_fps: 0.0,
            prev_left_down: false,
            drag: None,
            wire_drag: None,
            menu: AddNodeMenuState::closed(),
            hover_node: None,
            hover_output_pin: None,
            hover_input_pin: None,
            hover_menu_item: None,
        }
    }
}

/// Return full popup menu height.
pub(crate) fn menu_height() -> i32 {
    MENU_HEADER_HEIGHT + (ADD_NODE_OPTIONS.len() as i32 * MENU_ITEM_HEIGHT) + 8
}

#[cfg(test)]
mod tests {
    use super::AddNodeMenuState;

    #[test]
    fn menu_item_hit_test_matches_item_rects() {
        let menu = AddNodeMenuState::open_at(100, 100, 420, 400);
        for index in 0..2 {
            let rect = menu.item_rect(index).expect("item rect should exist");
            let hit = menu.item_at(rect.x + 2, rect.y + 2);
            assert_eq!(hit, Some(index));
        }
    }
}

//! Shared GUI state and menu models.

use super::draw::Rect;
use super::project::ProjectNodeKind;
use crate::runtime_config::V2Config;

/// Width of add-node popup menu.
pub(crate) const MENU_WIDTH: i32 = 220;
/// Height of one add-node menu row.
pub(crate) const MENU_ITEM_HEIGHT: i32 = 26;

/// One add-node menu entry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AddNodeOption {
    pub(crate) label: &'static str,
    pub(crate) kind: ProjectNodeKind,
}

/// Menu entries currently exposed in the graph editor.
pub(crate) const ADD_NODE_OPTIONS: [AddNodeOption; 2] = [
    AddNodeOption {
        label: "TOP Basic",
        kind: ProjectNodeKind::TopBasic,
    },
    AddNodeOption {
        label: "Output",
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

    /// Return hovered item index for cursor position.
    pub(crate) fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.rect().contains(x, y) {
            return None;
        }
        let local_y = y - self.y - 26;
        if local_y < 0 {
            return None;
        }
        let index = (local_y / MENU_ITEM_HEIGHT) as usize;
        if index < ADD_NODE_OPTIONS.len() {
            Some(index)
        } else {
            None
        }
    }
}

/// Runtime animation/editor state for one GUI session.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PreviewState {
    pub(crate) frame_index: u32,
    pub(crate) paused: bool,
    pub(crate) avg_fps: f32,
    pub(crate) prev_left_down: bool,
    pub(crate) drag: Option<DragState>,
    pub(crate) menu: AddNodeMenuState,
}

impl PreviewState {
    /// Create initial GUI state for one run.
    pub(crate) fn new(_config: &V2Config) -> Self {
        Self {
            frame_index: 0,
            paused: false,
            avg_fps: 0.0,
            prev_left_down: false,
            drag: None,
            menu: AddNodeMenuState::closed(),
        }
    }
}

/// Return full popup menu height.
pub(crate) fn menu_height() -> i32 {
    26 + (ADD_NODE_OPTIONS.len() as i32 * MENU_ITEM_HEIGHT) + 8
}

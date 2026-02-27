//! Backtick-triggered top-level main-menu popup.

use super::super::popup_list;
use crate::gui::geometry::Rect;

/// Main-menu popup width in panel-space pixels.
pub(crate) const MAIN_MENU_WIDTH: i32 = 180;
const MAIN_MENU_ITEM_HEIGHT: i32 = 24;
const MAIN_MENU_INNER_PADDING: i32 = 6;
const MAIN_MENU_TITLE_HEIGHT: i32 = 24;
const MAIN_MENU_BOTTOM_PADDING: i32 = 8;

/// Selectable rows in the top-level main menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainMenuItem {
    New,
    Save,
    Load,
    Export,
    Exit,
}

impl MainMenuItem {
    /// Return the display label for this menu item.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::New => "New Project",
            Self::Save => "Save Project",
            Self::Load => "Load Project",
            Self::Export => "Export",
            Self::Exit => "Exit",
        }
    }
}

const MAIN_MENU_ITEMS: [MainMenuItem; 5] = [
    MainMenuItem::New,
    MainMenuItem::Save,
    MainMenuItem::Load,
    MainMenuItem::Export,
    MainMenuItem::Exit,
];

/// Runtime state for the top-level main menu popup.
#[derive(Clone, Debug)]
pub(crate) struct MainMenuState {
    pub(crate) open: bool,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) selected: usize,
}

impl MainMenuState {
    /// Return a closed popup state.
    pub(crate) const fn closed() -> Self {
        Self {
            open: false,
            x: 0,
            y: 0,
            selected: 0,
        }
    }

    /// Return an opened popup state clamped to editor bounds.
    pub(crate) fn open_at(x: i32, y: i32, panel_width: usize, panel_height: usize) -> Self {
        let max_x = (panel_width as i32 - MAIN_MENU_WIDTH - 8).max(8);
        let max_y = (panel_height as i32 - main_menu_height() - 8).max(8);
        Self {
            open: true,
            x: x.clamp(8, max_x),
            y: y.clamp(8, max_y),
            selected: 0,
        }
    }

    /// Return popup bounds in panel-space coordinates.
    pub(crate) fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, MAIN_MENU_WIDTH, main_menu_height())
    }

    /// Return the currently selected item.
    pub(crate) fn selected_item(&self) -> MainMenuItem {
        MAIN_MENU_ITEMS[self.selected.min(MAIN_MENU_ITEMS.len() - 1)]
    }

    /// Return the hovered row index at one cursor point.
    pub(crate) fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        popup_list::item_at(MAIN_MENU_ITEMS.len(), x, y, |index| self.entry_rect(index))
    }

    /// Return one row bounds in panel-space coordinates.
    pub(crate) fn entry_rect(&self, index: usize) -> Option<Rect> {
        if index >= MAIN_MENU_ITEMS.len() {
            return None;
        }
        let y = self.y + MAIN_MENU_TITLE_HEIGHT + index as i32 * MAIN_MENU_ITEM_HEIGHT;
        Some(Rect::new(
            self.x + MAIN_MENU_INNER_PADDING,
            y,
            MAIN_MENU_WIDTH - MAIN_MENU_INNER_PADDING * 2,
            MAIN_MENU_ITEM_HEIGHT - 2,
        ))
    }

    /// Select a specific row index.
    pub(crate) fn select_index(&mut self, index: usize) -> bool {
        popup_list::select_index(&mut self.selected, index, MAIN_MENU_ITEMS.len())
    }

    /// Select the previous row.
    pub(crate) fn select_prev(&mut self) -> bool {
        popup_list::select_prev(&mut self.selected)
    }

    /// Select the next row.
    pub(crate) fn select_next(&mut self) -> bool {
        popup_list::select_next(&mut self.selected, MAIN_MENU_ITEMS.len())
    }

    /// Return immutable row metadata for rendering.
    pub(crate) const fn items(&self) -> &'static [MainMenuItem] {
        &MAIN_MENU_ITEMS
    }
}

fn main_menu_height() -> i32 {
    MAIN_MENU_TITLE_HEIGHT
        + MAIN_MENU_ITEM_HEIGHT * MAIN_MENU_ITEMS.len() as i32
        + MAIN_MENU_BOTTOM_PADDING
}

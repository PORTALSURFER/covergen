//! Add-node popup menu model, filtering, and layout helpers.

use crate::gui::geometry::Rect;
use crate::gui::project::ProjectNodeKind;

/// Width of add-node popup menu.
pub(crate) const MENU_WIDTH: i32 = 340;
/// Height of add-node menu title row.
pub(crate) const MENU_TITLE_HEIGHT: i32 = 24;
/// Height of add-node search row.
pub(crate) const MENU_SEARCH_HEIGHT: i32 = 24;
/// Height of one add-node category row.
pub(crate) const MENU_CATEGORY_HEIGHT: i32 = 18;
/// Inner menu padding from the outer frame.
pub(crate) const MENU_INNER_PADDING: i32 = 6;
/// Height of one add-node menu row.
pub(crate) const MENU_ITEM_HEIGHT: i32 = 22;
/// Vertical gap between title/search/content blocks.
pub(crate) const MENU_BLOCK_GAP: i32 = 4;
const MENU_BOTTOM_PAD: i32 = 8;

/// Category for one add-node menu option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddNodeCategory {
    Texture,
    Buffer,
    Scene,
    Render,
    Control,
    Io,
}

impl AddNodeCategory {
    /// Return display label used in category rows.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Texture => "Texture",
            Self::Buffer => "Buffer",
            Self::Scene => "Scene",
            Self::Render => "Render",
            Self::Control => "Control",
            Self::Io => "IO",
        }
    }
}

/// One add-node menu option.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AddNodeOption {
    pub(crate) kind: ProjectNodeKind,
    pub(crate) category: AddNodeCategory,
}

impl AddNodeOption {
    /// Return menu label for this option.
    pub(crate) const fn label(self) -> &'static str {
        self.kind.label()
    }
}

/// Menu entries currently exposed in the graph editor.
pub(crate) const ADD_NODE_OPTIONS: [AddNodeOption; 9] = [
    AddNodeOption {
        kind: ProjectNodeKind::TexSolid,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexCircle,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::BufSphere,
        category: AddNodeCategory::Buffer,
    },
    AddNodeOption {
        kind: ProjectNodeKind::SceneEntity,
        category: AddNodeCategory::Scene,
    },
    AddNodeOption {
        kind: ProjectNodeKind::SceneBuild,
        category: AddNodeCategory::Scene,
    },
    AddNodeOption {
        kind: ProjectNodeKind::RenderScenePass,
        category: AddNodeCategory::Render,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexTransform2D,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::CtlLfo,
        category: AddNodeCategory::Control,
    },
    AddNodeOption {
        kind: ProjectNodeKind::IoWindowOut,
        category: AddNodeCategory::Io,
    },
];

/// One visible row in the add-node popup content list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddNodeMenuRow {
    Category(AddNodeCategory),
    Option(usize),
}

/// Add-node popup menu state.
#[derive(Clone, Debug)]
pub(crate) struct AddNodeMenuState {
    pub(crate) open: bool,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) selected: usize,
    pub(crate) query: String,
}

impl AddNodeMenuState {
    /// Return closed menu state.
    pub(crate) fn closed() -> Self {
        Self {
            open: false,
            x: 0,
            y: 0,
            selected: 0,
            query: String::new(),
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
            query: String::new(),
        }
    }

    /// Return menu rectangle in panel coordinates.
    pub(crate) fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, MENU_WIDTH, menu_height())
    }

    /// Return query-edit rectangle in panel coordinates.
    pub(crate) fn search_rect(&self) -> Rect {
        Rect::new(
            self.x + MENU_INNER_PADDING,
            self.y + MENU_TITLE_HEIGHT,
            MENU_WIDTH - (MENU_INNER_PADDING * 2),
            MENU_SEARCH_HEIGHT,
        )
    }

    /// Return currently visible option indices after query filtering.
    pub(crate) fn visible_option_indices(&self) -> Vec<usize> {
        let query = self.query.trim().to_lowercase();
        let mut out = Vec::new();
        for (index, option) in ADD_NODE_OPTIONS.iter().copied().enumerate() {
            if query.is_empty() || option_matches_query(option, query.as_str()) {
                out.push(index);
            }
        }
        out
    }

    /// Return all visible content rows, including category separators.
    pub(crate) fn visible_rows(&self) -> Vec<AddNodeMenuRow> {
        let visible = self.visible_option_indices();
        if visible.is_empty() {
            return Vec::new();
        }
        if !self.query.trim().is_empty() {
            return visible.into_iter().map(AddNodeMenuRow::Option).collect();
        }
        let mut rows = Vec::new();
        let mut previous = None;
        for index in visible {
            let category = ADD_NODE_OPTIONS[index].category;
            if previous != Some(category) {
                rows.push(AddNodeMenuRow::Category(category));
                previous = Some(category);
            }
            rows.push(AddNodeMenuRow::Option(index));
        }
        rows
    }

    /// Return one option rectangle in panel coordinates.
    pub(crate) fn item_rect(&self, option_index: usize) -> Option<Rect> {
        let rows = self.visible_rows();
        let mut y = self.y + MENU_TITLE_HEIGHT + MENU_BLOCK_GAP + MENU_SEARCH_HEIGHT + MENU_BLOCK_GAP;
        for row in rows {
            match row {
                AddNodeMenuRow::Category(_) => y += MENU_CATEGORY_HEIGHT,
                AddNodeMenuRow::Option(index) => {
                    let rect = Rect::new(
                        self.x + MENU_INNER_PADDING,
                        y,
                        MENU_WIDTH - (MENU_INNER_PADDING * 2),
                        MENU_ITEM_HEIGHT - 2,
                    );
                    if index == option_index {
                        return Some(rect);
                    }
                    y += MENU_ITEM_HEIGHT;
                }
            }
        }
        None
    }

    /// Return hovered option index for cursor position.
    pub(crate) fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        for row in self.visible_rows() {
            let AddNodeMenuRow::Option(index) = row else {
                continue;
            };
            let Some(rect) = self.item_rect(index) else {
                continue;
            };
            if rect.contains(x, y) {
                return Some(index);
            }
        }
        None
    }

    /// Return selected option index in `ADD_NODE_OPTIONS`, if any.
    pub(crate) fn selected_option_index(&self) -> Option<usize> {
        let visible = self.visible_option_indices();
        if visible.is_empty() {
            return None;
        }
        Some(visible[self.selected.min(visible.len() - 1)])
    }

    /// Keep selected row inside current visible option range.
    pub(crate) fn clamp_selection(&mut self) -> bool {
        let visible = self.visible_option_indices();
        if visible.is_empty() {
            if self.selected != 0 {
                self.selected = 0;
                return true;
            }
            return false;
        }
        let clamped = self.selected.min(visible.len() - 1);
        if clamped == self.selected {
            return false;
        }
        self.selected = clamped;
        true
    }

    /// Select the previous visible option.
    pub(crate) fn select_prev(&mut self) -> bool {
        let old = self.selected;
        self.selected = self.selected.saturating_sub(1);
        old != self.selected
    }

    /// Select the next visible option.
    pub(crate) fn select_next(&mut self) -> bool {
        let visible = self.visible_option_indices();
        if visible.is_empty() {
            return false;
        }
        let old = self.selected;
        self.selected = (self.selected + 1).min(visible.len() - 1);
        old != self.selected
    }

    /// Select a visible option by concrete option index.
    pub(crate) fn select_option_index(&mut self, option_index: usize) -> bool {
        let visible = self.visible_option_indices();
        let Some(position) = visible.iter().position(|index| *index == option_index) else {
            return false;
        };
        if self.selected == position {
            return false;
        }
        self.selected = position;
        true
    }

    /// Append free-text query input and optionally remove one character.
    pub(crate) fn apply_query_input(&mut self, typed: &str, backspace: bool) -> bool {
        let mut changed = false;
        if backspace && !self.query.is_empty() {
            self.query.pop();
            changed = true;
        }
        if !typed.is_empty() {
            self.query.push_str(typed);
            changed = true;
        }
        if changed {
            self.selected = 0;
            let _ = self.clamp_selection();
        }
        changed
    }
}

/// Return full popup menu height.
pub(crate) fn menu_height() -> i32 {
    let mut categories = 0i32;
    let mut previous = None;
    for option in ADD_NODE_OPTIONS {
        if previous != Some(option.category) {
            categories += 1;
            previous = Some(option.category);
        }
    }
    MENU_TITLE_HEIGHT
        + MENU_BLOCK_GAP
        + MENU_SEARCH_HEIGHT
        + MENU_BLOCK_GAP
        + (ADD_NODE_OPTIONS.len() as i32 * MENU_ITEM_HEIGHT)
        + (categories * MENU_CATEGORY_HEIGHT)
        + MENU_BOTTOM_PAD
}

fn option_matches_query(option: AddNodeOption, query: &str) -> bool {
    let label = option.label().to_lowercase();
    if label.contains(query) {
        return true;
    }
    option.category.label().to_lowercase().contains(query)
}

#[cfg(test)]
mod tests {
    use super::{AddNodeMenuRow, AddNodeMenuState};
    use crate::gui::project::ProjectNodeKind;

    #[test]
    fn menu_item_hit_test_matches_item_rects() {
        let menu = AddNodeMenuState::open_at(100, 100, 420, 400);
        for index in menu.visible_option_indices() {
            let rect = menu.item_rect(index).expect("item rect should exist");
            let hit = menu.item_at(rect.x + 2, rect.y + 2);
            assert_eq!(hit, Some(index));
        }
    }

    #[test]
    fn menu_rows_group_by_category_without_query() {
        let menu = AddNodeMenuState::open_at(100, 100, 420, 400);
        let rows = menu.visible_rows();
        assert!(rows
            .iter()
            .any(|row| matches!(row, AddNodeMenuRow::Category(_))));
    }

    #[test]
    fn query_filters_option_set_and_resets_selection() {
        let mut menu = AddNodeMenuState::open_at(100, 100, 420, 400);
        menu.selected = 3;
        assert!(menu.apply_query_input("lfo", false));
        let visible = menu.visible_option_indices();
        assert_eq!(visible.len(), 1);
        assert_eq!(
            super::ADD_NODE_OPTIONS[visible[0]].kind,
            ProjectNodeKind::CtlLfo
        );
        assert_eq!(menu.selected, 0);
    }
}

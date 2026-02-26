//! Add-node popup menu model, filtering, and staged category navigation.

use crate::gui::geometry::Rect;
use crate::gui::project::ProjectNodeKind;

/// Add-node popup geometry constants.
pub(crate) const MENU_WIDTH: i32 = 260;
pub(crate) const MENU_TITLE_HEIGHT: i32 = 24;
pub(crate) const MENU_SEARCH_HEIGHT: i32 = 24;
pub(crate) const MENU_INNER_PADDING: i32 = 6;
pub(crate) const MENU_ITEM_HEIGHT: i32 = 22;
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
pub(crate) const ADD_NODE_OPTIONS: [AddNodeOption; 12] = [
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
        kind: ProjectNodeKind::BufCircleNurbs,
        category: AddNodeCategory::Buffer,
    },
    AddNodeOption {
        kind: ProjectNodeKind::BufNoise,
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
        kind: ProjectNodeKind::RenderCamera,
        category: AddNodeCategory::Render,
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

/// One visible row in the add-node popup list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddNodeMenuEntry {
    Category(AddNodeCategory),
    Back,
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
    pub(crate) active_category: Option<AddNodeCategory>,
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
            active_category: None,
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
            active_category: None,
        }
    }

    /// Return true when the picker is in the top-level category stage.
    pub(crate) const fn is_category_picker(&self) -> bool {
        self.active_category.is_none()
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

    /// Enter the secondary picker for a category.
    pub(crate) fn open_category(&mut self, category: AddNodeCategory) -> bool {
        if self.active_category == Some(category) && self.selected == 0 {
            return false;
        }
        self.active_category = Some(category);
        self.query.clear();
        self.selected = 0;
        true
    }

    /// Return from secondary picker back to category picker.
    pub(crate) fn close_category(&mut self) -> bool {
        if self.active_category.is_none() {
            return false;
        }
        self.active_category = None;
        self.query.clear();
        self.selected = 0;
        true
    }

    /// Return visible category list in menu order.
    pub(crate) fn visible_categories(&self) -> Vec<AddNodeCategory> {
        unique_category_order()
    }

    /// Return currently visible option indices after category and query filtering.
    pub(crate) fn visible_option_indices(&self) -> Vec<usize> {
        let Some(category) = self.active_category else {
            return Vec::new();
        };
        let query = self.query.trim().to_lowercase();
        let mut out = Vec::new();
        for (index, option) in ADD_NODE_OPTIONS.iter().copied().enumerate() {
            if option.category != category {
                continue;
            }
            if query.is_empty() || option_matches_query(option, query.as_str()) {
                out.push(index);
            }
        }
        out
    }

    /// Return all visible entries in current picker stage.
    pub(crate) fn visible_entries(&self) -> Vec<AddNodeMenuEntry> {
        if self.active_category.is_none() {
            return self
                .visible_categories()
                .into_iter()
                .map(AddNodeMenuEntry::Category)
                .collect();
        }
        let mut out = Vec::new();
        out.push(AddNodeMenuEntry::Back);
        for index in self.visible_option_indices() {
            out.push(AddNodeMenuEntry::Option(index));
        }
        out
    }

    /// Return one entry rectangle in panel coordinates.
    pub(crate) fn entry_rect(&self, entry_index: usize) -> Option<Rect> {
        let entries = self.visible_entries();
        if entry_index >= entries.len() {
            return None;
        }
        let y = self.y
            + MENU_TITLE_HEIGHT
            + MENU_BLOCK_GAP
            + MENU_SEARCH_HEIGHT
            + MENU_BLOCK_GAP
            + entry_index as i32 * MENU_ITEM_HEIGHT;
        Some(Rect::new(
            self.x + MENU_INNER_PADDING,
            y,
            MENU_WIDTH - (MENU_INNER_PADDING * 2),
            MENU_ITEM_HEIGHT - 2,
        ))
    }

    /// Return hovered entry index for cursor position.
    pub(crate) fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        let entries = self.visible_entries();
        for index in 0..entries.len() {
            let Some(rect) = self.entry_rect(index) else {
                continue;
            };
            if rect.contains(x, y) {
                return Some(index);
            }
        }
        None
    }

    /// Return selected entry in current picker stage.
    pub(crate) fn selected_entry(&self) -> Option<AddNodeMenuEntry> {
        let entries = self.visible_entries();
        entries.get(self.selected.min(entries.len().saturating_sub(1))).copied()
    }

    /// Keep selected row inside current visible entry range.
    pub(crate) fn clamp_selection(&mut self) -> bool {
        let entries = self.visible_entries();
        if entries.is_empty() {
            if self.selected != 0 {
                self.selected = 0;
                return true;
            }
            return false;
        }
        let clamped = self.selected.min(entries.len() - 1);
        if clamped == self.selected {
            return false;
        }
        self.selected = clamped;
        true
    }

    /// Select one row index in current visible entry range.
    pub(crate) fn select_index(&mut self, index: usize) -> bool {
        let entries = self.visible_entries();
        if entries.is_empty() {
            return false;
        }
        let next = index.min(entries.len() - 1);
        if self.selected == next {
            return false;
        }
        self.selected = next;
        true
    }

    /// Select the previous visible entry.
    pub(crate) fn select_prev(&mut self) -> bool {
        let old = self.selected;
        self.selected = self.selected.saturating_sub(1);
        old != self.selected
    }

    /// Select the next visible entry.
    pub(crate) fn select_next(&mut self) -> bool {
        let entries = self.visible_entries();
        if entries.is_empty() {
            return false;
        }
        let old = self.selected;
        self.selected = (self.selected + 1).min(entries.len() - 1);
        old != self.selected
    }

    /// Append search text in secondary picker and optionally remove one char.
    pub(crate) fn apply_query_input(&mut self, typed: &str, backspace: bool) -> bool {
        if self.active_category.is_none() {
            return false;
        }
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
    let row_count = (ADD_NODE_OPTIONS.len() + 1).max(category_count()) as i32;
    MENU_TITLE_HEIGHT
        + MENU_BLOCK_GAP
        + MENU_SEARCH_HEIGHT
        + MENU_BLOCK_GAP
        + (row_count * MENU_ITEM_HEIGHT)
        + MENU_BOTTOM_PAD
}

fn category_count() -> usize {
    unique_category_order().len()
}

fn unique_category_order() -> Vec<AddNodeCategory> {
    let mut out = Vec::new();
    for option in ADD_NODE_OPTIONS {
        if !out.contains(&option.category) {
            out.push(option.category);
        }
    }
    out
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
    use super::{AddNodeCategory, AddNodeMenuEntry, AddNodeMenuState};
    use crate::gui::project::ProjectNodeKind;

    #[test]
    fn category_stage_transitions_and_query_filtering_work() {
        let mut menu = AddNodeMenuState::open_at(100, 100, 420, 400);
        assert!(menu.open_category(AddNodeCategory::Control));
        assert!(matches!(menu.selected_entry(), Some(AddNodeMenuEntry::Back)));
        assert!(menu.apply_query_input("lfo", false));
        assert!(menu.select_next());
        let Some(AddNodeMenuEntry::Option(option_index)) = menu.selected_entry() else {
            panic!("selected option expected after filtering");
        };
        assert_eq!(
            super::ADD_NODE_OPTIONS[option_index].kind,
            ProjectNodeKind::CtlLfo
        );
        assert!(menu.close_category());
        assert!(menu.is_category_picker());
    }

    #[test]
    fn buffer_category_lists_circle_nurbs_option() {
        let mut menu = AddNodeMenuState::open_at(100, 100, 420, 400);
        assert!(menu.open_category(AddNodeCategory::Buffer));
        assert!(menu.apply_query_input("circle_nurbs", false));
        assert!(menu.select_next());
        let Some(AddNodeMenuEntry::Option(option_index)) = menu.selected_entry() else {
            panic!("selected option expected after filtering");
        };
        assert_eq!(
            super::ADD_NODE_OPTIONS[option_index].kind,
            ProjectNodeKind::BufCircleNurbs
        );
    }

    #[test]
    fn buffer_category_lists_noise_option() {
        let mut menu = AddNodeMenuState::open_at(100, 100, 420, 400);
        assert!(menu.open_category(AddNodeCategory::Buffer));
        assert!(menu.apply_query_input("noise", false));
        assert!(menu.select_next());
        let Some(AddNodeMenuEntry::Option(option_index)) = menu.selected_entry() else {
            panic!("selected option expected after filtering");
        };
        assert_eq!(
            super::ADD_NODE_OPTIONS[option_index].kind,
            ProjectNodeKind::BufNoise
        );
    }
}

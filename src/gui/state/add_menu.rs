//! Add-node popup menu model, filtering, and staged category navigation.

use std::cell::RefCell;

use super::popup_list;
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
pub(crate) const ADD_NODE_OPTIONS: [AddNodeOption; 15] = [
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
        kind: ProjectNodeKind::TexLevel,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexFeedback,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexBlend,
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

const ADD_NODE_CATEGORIES: [AddNodeCategory; 6] = [
    AddNodeCategory::Texture,
    AddNodeCategory::Buffer,
    AddNodeCategory::Scene,
    AddNodeCategory::Render,
    AddNodeCategory::Control,
    AddNodeCategory::Io,
];

/// One visible row in the add-node popup list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddNodeMenuEntry {
    Category(AddNodeCategory),
    Back,
    Option(usize),
}

/// Cached visible list state for one query/category combination.
#[derive(Clone, Debug, Default)]
struct VisibleEntriesCache {
    valid: bool,
    active_category: Option<AddNodeCategory>,
    query_key: String,
    option_indices: Vec<usize>,
    entries: Vec<AddNodeMenuEntry>,
}

/// Add-node popup menu state.
#[derive(Clone, Debug)]
pub(crate) struct AddNodeMenuState {
    pub(crate) open: bool,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) open_cursor_x: i32,
    pub(crate) open_cursor_y: i32,
    pub(crate) selected: usize,
    pub(crate) query: String,
    pub(crate) active_category: Option<AddNodeCategory>,
    query_norm: String,
    visible_cache: RefCell<VisibleEntriesCache>,
}

impl AddNodeMenuState {
    /// Return closed menu state.
    pub(crate) fn closed() -> Self {
        Self {
            open: false,
            x: 0,
            y: 0,
            open_cursor_x: 0,
            open_cursor_y: 0,
            selected: 0,
            query: String::new(),
            active_category: None,
            query_norm: String::new(),
            visible_cache: RefCell::new(VisibleEntriesCache::default()),
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
            open_cursor_x: x,
            open_cursor_y: y,
            selected: 0,
            query: String::new(),
            active_category: None,
            query_norm: String::new(),
            visible_cache: RefCell::new(VisibleEntriesCache::default()),
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
        self.query_norm.clear();
        self.selected = 0;
        self.invalidate_visible_cache();
        true
    }

    /// Return from secondary picker back to category picker.
    pub(crate) fn close_category(&mut self) -> bool {
        if self.active_category.is_none() {
            return false;
        }
        self.active_category = None;
        self.query.clear();
        self.query_norm.clear();
        self.selected = 0;
        self.invalidate_visible_cache();
        true
    }

    /// Return visible entry count in current picker stage.
    pub(crate) fn visible_entry_count(&self) -> usize {
        self.ensure_visible_cache();
        self.visible_cache.borrow().entries.len()
    }

    /// Return one visible entry by index in current picker stage.
    pub(crate) fn visible_entry(&self, index: usize) -> Option<AddNodeMenuEntry> {
        self.ensure_visible_cache();
        self.visible_cache.borrow().entries.get(index).copied()
    }

    /// Return one entry rectangle in panel coordinates.
    pub(crate) fn entry_rect(&self, entry_index: usize) -> Option<Rect> {
        if entry_index >= self.visible_entry_count() {
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
        popup_list::item_at(self.visible_entry_count(), x, y, |index| {
            self.entry_rect(index)
        })
    }

    /// Return selected entry in current picker stage.
    pub(crate) fn selected_entry(&self) -> Option<AddNodeMenuEntry> {
        let count = self.visible_entry_count();
        if count == 0 {
            return None;
        }
        self.visible_entry(self.selected.min(count.saturating_sub(1)))
    }

    /// Keep selected row inside current visible entry range.
    pub(crate) fn clamp_selection(&mut self) -> bool {
        let count = self.visible_entry_count();
        popup_list::clamp_selection(&mut self.selected, count)
    }

    /// Select one row index in current visible entry range.
    pub(crate) fn select_index(&mut self, index: usize) -> bool {
        let count = self.visible_entry_count();
        popup_list::select_index(&mut self.selected, index, count)
    }

    /// Select the previous visible entry.
    pub(crate) fn select_prev(&mut self) -> bool {
        popup_list::select_prev(&mut self.selected)
    }

    /// Select the next visible entry.
    pub(crate) fn select_next(&mut self) -> bool {
        let count = self.visible_entry_count();
        popup_list::select_next(&mut self.selected, count)
    }

    /// Append search text in secondary picker and optionally remove one char.
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
            self.refresh_query_norm();
            self.invalidate_visible_cache();
            self.selected = 0;
            let _ = self.clamp_selection();
        }
        changed
    }

    /// Reset visible-entry cache after any filter-stage mutation.
    fn invalidate_visible_cache(&self) {
        self.visible_cache.borrow_mut().valid = false;
    }

    /// Keep normalized query in sync with user text for cheap cache checks.
    fn refresh_query_norm(&mut self) {
        self.query_norm = self.query.trim().to_lowercase();
    }

    /// Ensure cached visible entries match current category/query state.
    fn ensure_visible_cache(&self) {
        let key_category = self.active_category;
        let key_query = self.query_norm.as_str();
        let mut cache = self.visible_cache.borrow_mut();
        if cache.valid && cache.active_category == key_category && cache.query_key == key_query {
            return;
        }
        cache.valid = true;
        cache.active_category = key_category;
        cache.query_key.clear();
        cache.query_key.push_str(key_query);
        cache.option_indices.clear();
        cache.entries.clear();
        let Some(category) = key_category else {
            cache
                .entries
                .extend(ADD_NODE_CATEGORIES.into_iter().filter_map(|candidate| {
                    if key_query.is_empty() || category_matches_query(candidate, key_query) {
                        Some(AddNodeMenuEntry::Category(candidate))
                    } else {
                        None
                    }
                }));
            return;
        };
        cache.entries.push(AddNodeMenuEntry::Back);
        for (index, option) in ADD_NODE_OPTIONS.iter().copied().enumerate() {
            if option.category != category {
                continue;
            }
            if key_query.is_empty() || option_matches_query(option, key_query) {
                cache.option_indices.push(index);
                cache.entries.push(AddNodeMenuEntry::Option(index));
            }
        }
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
    ADD_NODE_CATEGORIES.len()
}

fn option_matches_query(option: AddNodeOption, query: &str) -> bool {
    let label = option.label().to_lowercase();
    if fuzzy_query_match(label.as_str(), query) {
        return true;
    }
    fuzzy_query_match(option.category.label().to_lowercase().as_str(), query)
}

fn category_matches_query(category: AddNodeCategory, query: &str) -> bool {
    fuzzy_query_match(category.label().to_lowercase().as_str(), query)
}

fn fuzzy_query_match(text: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    if text.contains(query) {
        return true;
    }
    let mut query_chars = query.chars().filter(|ch| !ch.is_whitespace());
    let Some(mut needle) = query_chars.next() else {
        return true;
    };
    for hay in text.chars() {
        if hay == needle {
            if let Some(next) = query_chars.next() {
                needle = next;
            } else {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{AddNodeCategory, AddNodeMenuEntry, AddNodeMenuState};
    use crate::gui::project::ProjectNodeKind;

    #[test]
    fn category_stage_transitions_and_query_filtering_work() {
        let mut menu = AddNodeMenuState::open_at(100, 100, 420, 400);
        assert!(menu.open_category(AddNodeCategory::Control));
        assert!(matches!(
            menu.selected_entry(),
            Some(AddNodeMenuEntry::Back)
        ));
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

    #[test]
    fn category_stage_query_filters_categories() {
        let mut menu = AddNodeMenuState::open_at(100, 100, 420, 400);
        assert!(menu.is_category_picker());
        assert!(menu.apply_query_input("tex", false));
        assert_eq!(menu.visible_entry_count(), 1);
        assert!(matches!(
            menu.visible_entry(0),
            Some(AddNodeMenuEntry::Category(AddNodeCategory::Texture))
        ));
    }

    #[test]
    fn option_stage_query_uses_fuzzy_matching() {
        let mut menu = AddNodeMenuState::open_at(100, 100, 420, 400);
        assert!(menu.open_category(AddNodeCategory::Texture));
        assert!(menu.apply_query_input("txfb", false));
        let entries: Vec<_> = (0..menu.visible_entry_count())
            .filter_map(|index| menu.visible_entry(index))
            .collect();
        assert!(entries.contains(&AddNodeMenuEntry::Back));
        assert!(
            entries
                .iter()
                .any(|entry| matches!(entry, AddNodeMenuEntry::Option(option_index) if super::ADD_NODE_OPTIONS[*option_index].kind == ProjectNodeKind::TexFeedback))
        );
    }
}

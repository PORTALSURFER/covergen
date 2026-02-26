//! Shared popup-list navigation and hit-testing primitives.

use crate::gui::geometry::Rect;

/// Clamp one index into a popup list and report whether it changed.
pub(crate) fn clamp_selection(selected: &mut usize, entry_count: usize) -> bool {
    let clamped = if entry_count == 0 {
        0
    } else {
        (*selected).min(entry_count - 1)
    };
    if *selected == clamped {
        return false;
    }
    *selected = clamped;
    true
}

/// Select one explicit entry index inside a popup list.
pub(crate) fn select_index(selected: &mut usize, index: usize, entry_count: usize) -> bool {
    if entry_count == 0 {
        return false;
    }
    let next = index.min(entry_count - 1);
    if *selected == next {
        return false;
    }
    *selected = next;
    true
}

/// Move popup selection one row up.
pub(crate) fn select_prev(selected: &mut usize) -> bool {
    let old = *selected;
    *selected = (*selected).saturating_sub(1);
    old != *selected
}

/// Move popup selection one row down.
pub(crate) fn select_next(selected: &mut usize, entry_count: usize) -> bool {
    if entry_count == 0 {
        return false;
    }
    let old = *selected;
    *selected = (*selected + 1).min(entry_count - 1);
    old != *selected
}

/// Return the hovered entry index by checking row rectangles in order.
pub(crate) fn item_at(
    entry_count: usize,
    x: i32,
    y: i32,
    mut row_rect: impl FnMut(usize) -> Option<Rect>,
) -> Option<usize> {
    for index in 0..entry_count {
        let Some(rect) = row_rect(index) else {
            continue;
        };
        if rect.contains(x, y) {
            return Some(index);
        }
    }
    None
}

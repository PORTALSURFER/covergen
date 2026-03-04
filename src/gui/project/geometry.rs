use super::*;
use crate::gui::geometry::Rect;

impl GuiProject {
    pub(crate) fn node_at(&self, x: i32, y: i32) -> Option<u32> {
        self.ensure_hit_test_cache();
        let key = hit_bin_key_for_point(x, y);
        let cache = self.hit_test_cache.borrow();
        let candidates = cache.node_bins.get(&key)?;
        for node_id in candidates.iter().rev() {
            self.bump_hit_test_scan_count(1);
            let Some(index) = self.node_index_lookup.get(node_id).copied() else {
                continue;
            };
            let Some(node) = self.nodes.get(index) else {
                continue;
            };
            if x >= node.x()
                && x < node.x() + NODE_WIDTH
                && y >= node.y()
                && y < node.y() + node.card_height()
            {
                return Some(*node_id);
            }
        }
        None
    }

    /// Return world-space graph bounds for all current nodes.
    pub(crate) fn graph_bounds(&self) -> Option<GraphBounds> {
        let first = self.nodes.first()?;
        let mut min_x = first.x();
        let mut min_y = first.y();
        let mut max_x = first.x() + NODE_WIDTH;
        let mut max_y = first.y() + first.card_height();
        for node in self.nodes.iter().skip(1) {
            min_x = min_x.min(node.x());
            min_y = min_y.min(node.y());
            max_x = max_x.max(node.x() + NODE_WIDTH);
            max_y = max_y.max(node.y() + node.card_height());
        }
        Some(GraphBounds {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    /// Return node ids whose cards overlap one graph-space rectangle.
    ///
    /// This uses the cached node-bin index for broad-phase candidate lookup and
    /// runs exact card-rectangle overlap checks as a narrow phase.
    pub(crate) fn node_ids_overlapping_graph_rect(
        &self,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
    ) -> Vec<u32> {
        let (min_x, max_x) = if min_x <= max_x {
            (min_x, max_x)
        } else {
            (max_x, min_x)
        };
        let (min_y, max_y) = if min_y <= max_y {
            (min_y, max_y)
        } else {
            (max_y, min_y)
        };
        self.ensure_hit_test_cache();
        let cache = self.hit_test_cache.borrow();
        let mut seen = std::mem::take(&mut *self.hit_test_seen_scratch.borrow_mut());
        let mut candidates = std::mem::take(&mut *self.hit_test_candidates_scratch.borrow_mut());
        seen.clear();
        candidates.clear();
        for by in hit_bin_coord(min_y)..=hit_bin_coord(max_y) {
            for bx in hit_bin_coord(min_x)..=hit_bin_coord(max_x) {
                let Some(ids) = cache.node_bins.get(&hit_bin_key(bx, by)) else {
                    continue;
                };
                self.bump_hit_test_scan_count(ids.len() as u64);
                for node_id in ids {
                    if seen.insert(*node_id) {
                        candidates.push(*node_id);
                    }
                }
            }
        }
        if !candidates.is_empty() {
            candidates.sort_unstable_by_key(|node_id| {
                self.node_index_lookup
                    .get(node_id)
                    .copied()
                    .unwrap_or(usize::MAX)
            });
        }

        let mut out = Vec::with_capacity(candidates.len());
        for node_id in candidates.iter().copied() {
            let Some(index) = self.node_index_lookup.get(&node_id).copied() else {
                continue;
            };
            if let Some(node) = self.nodes.get(index) {
                let nx0 = node.x();
                let ny0 = node.y();
                let nx1 = nx0.saturating_add(NODE_WIDTH);
                let ny1 = ny0.saturating_add(node.card_height());
                if min_x <= nx1 && max_x >= nx0 && min_y <= ny1 && max_y >= ny0 {
                    out.push(node_id);
                }
            }
        }
        candidates.clear();
        seen.clear();
        *self.hit_test_candidates_scratch.borrow_mut() = candidates;
        *self.hit_test_seen_scratch.borrow_mut() = seen;
        out
    }

    /// Return the node id whose output pin is hit by the cursor.
    pub(crate) fn output_pin_at(&self, x: i32, y: i32, radius_px: i32) -> Option<u32> {
        self.pin_at(x, y, radius_px, None, output_pin_center, PinHitKind::Output)
    }

    /// Return the node id whose input pin is hit by the cursor.
    pub(crate) fn input_pin_at(
        &self,
        x: i32,
        y: i32,
        radius_px: i32,
        disallow_source: Option<u32>,
    ) -> Option<u32> {
        self.pin_at(
            x,
            y,
            radius_px,
            disallow_source,
            input_pin_center,
            PinHitKind::Input,
        )
    }

    fn pin_at(
        &self,
        x: i32,
        y: i32,
        radius_px: i32,
        disallow_source: Option<u32>,
        center_for_node: fn(&ProjectNode) -> Option<(i32, i32)>,
        pin_kind: PinHitKind,
    ) -> Option<u32> {
        self.ensure_hit_test_cache();
        let radius_sq = radius_px.saturating_mul(radius_px);
        let min_x = x.saturating_sub(radius_px);
        let max_x = x.saturating_add(radius_px);
        let min_y = y.saturating_sub(radius_px);
        let max_y = y.saturating_add(radius_px);
        let mut seen = std::mem::take(&mut *self.hit_test_seen_scratch.borrow_mut());
        seen.clear();
        let mut hit = None;
        let mut hit_z = 0_usize;

        let cache = self.hit_test_cache.borrow();
        let bins = match pin_kind {
            PinHitKind::Output => &cache.output_pin_bins,
            PinHitKind::Input => &cache.input_pin_bins,
        };
        for by in hit_bin_coord(min_y)..=hit_bin_coord(max_y) {
            for bx in hit_bin_coord(min_x)..=hit_bin_coord(max_x) {
                let key = hit_bin_key(bx, by);
                let Some(candidates) = bins.get(&key) else {
                    continue;
                };
                for node_id in candidates.iter().rev() {
                    self.bump_hit_test_scan_count(1);
                    if Some(*node_id) == disallow_source || !seen.insert(*node_id) {
                        continue;
                    }
                    let Some(index) = self.node_index_lookup.get(node_id).copied() else {
                        continue;
                    };
                    let Some(node) = self.nodes.get(index) else {
                        continue;
                    };
                    let Some((px, py)) = center_for_node(node) else {
                        continue;
                    };
                    if distance_sq(x, y, px, py) <= radius_sq && (hit.is_none() || index >= hit_z) {
                        hit = Some(*node_id);
                        hit_z = index;
                    }
                }
            }
        }
        seen.clear();
        *self.hit_test_seen_scratch.borrow_mut() = seen;
        hit
    }
}

pub(crate) fn output_pin_center(node: &ProjectNode) -> Option<(i32, i32)> {
    if !node.kind().has_output_pin() {
        return None;
    }
    let x = snap_to_node_grid(node.x() + NODE_WIDTH);
    let y = snap_to_node_grid(node.y() + (node.card_height() / 2));
    Some((x, y))
}

/// Return panel-space center of a node input pin.
pub(crate) fn input_pin_center(node: &ProjectNode) -> Option<(i32, i32)> {
    if !node.kind().has_input_pin() {
        return None;
    }
    let x = snap_to_node_grid(node.x());
    let y = snap_to_node_grid(node.y() + (node.card_height() / 2));
    Some((x, y))
}

/// Return graph-space center of collapsed parameter-entry pin.
///
/// This pin is shown when parameter rows are hidden so parameter bindings stay
/// visually anchored to the node.
pub(crate) fn collapsed_param_entry_pin_center(node: &ProjectNode) -> Option<(i32, i32)> {
    if node.expanded() || !node.kind().accepts_signal_bindings() || node.params.is_empty() {
        return None;
    }
    let x = snap_to_node_grid(node.x() + NODE_WIDTH);
    let y = snap_to_node_grid(
        (node.y() + NODE_HEIGHT / 2 + NODE_PIN_SIZE + 2)
            .min(node.y() + NODE_HEIGHT - NODE_PIN_SIZE),
    );
    Some((x, y))
}

/// Return one pin rectangle centered around a pin position.
pub(crate) fn pin_rect(cx: i32, cy: i32) -> Rect {
    Rect::new(
        cx - NODE_PIN_HALF,
        cy - NODE_PIN_HALF,
        NODE_PIN_SIZE,
        NODE_PIN_SIZE,
    )
}

/// Return node header expand/collapse toggle rectangle in graph-space coordinates.
pub(crate) fn node_expand_toggle_rect(node: &ProjectNode) -> Option<Rect> {
    if !node.supports_expand_toggle() {
        return None;
    }
    Some(Rect::new(
        node.x() + NODE_TOGGLE_MARGIN,
        node.y() + NODE_TOGGLE_MARGIN,
        NODE_TOGGLE_SIZE,
        NODE_TOGGLE_SIZE,
    ))
}

/// Return one parameter row rectangle in graph-space coordinates.
pub(crate) fn node_param_row_rect(node: &ProjectNode, param_index: usize) -> Option<Rect> {
    if !node.expanded() || param_index >= node.params.len() {
        return None;
    }
    let row_y = node.y() + NODE_HEIGHT + param_index as i32 * NODE_PARAM_ROW_HEIGHT;
    Some(Rect::new(
        node.x() + NODE_PARAM_ROW_PAD_X,
        row_y,
        NODE_WIDTH - NODE_PARAM_ROW_PAD_X * 2,
        NODE_PARAM_ROW_HEIGHT,
    ))
}

/// Return one parameter value input box rectangle in graph-space coordinates.
pub(crate) fn node_param_value_rect(node: &ProjectNode, param_index: usize) -> Option<Rect> {
    let row = node_param_row_rect(node, param_index)?;
    let width = NODE_PARAM_VALUE_BOX_WIDTH
        .min(row.w.saturating_sub(8))
        .max(20);
    let x = row.x + row.w - width - NODE_PARAM_VALUE_BOX_RIGHT_PAD;
    Some(Rect::new(x, row.y + 1, width, row.h.saturating_sub(2)))
}

/// Return one parameter dropdown popup rectangle in graph-space coordinates.
pub(crate) fn node_param_dropdown_rect(
    node: &ProjectNode,
    param_index: usize,
    option_count: usize,
) -> Option<Rect> {
    if option_count == 0 {
        return None;
    }
    let value_rect = node_param_value_rect(node, param_index)?;
    Some(Rect::new(
        value_rect.x,
        value_rect.y + value_rect.h + 1,
        value_rect.w,
        option_count as i32 * NODE_PARAM_DROPDOWN_ROW_HEIGHT,
    ))
}

fn distance_sq(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    let dx = ax - bx;
    let dy = ay - by;
    dx.saturating_mul(dx) + dy.saturating_mul(dy)
}

/// Collect all broad-phase hit-test bin keys touched by one node card rect.
pub(super) fn collect_node_rect_bin_keys(x: i32, y: i32, card_height: i32, out: &mut Vec<i64>) {
    out.clear();
    if card_height <= 0 {
        return;
    }
    let max_x = x.saturating_add(NODE_WIDTH.saturating_sub(1));
    let max_y = y.saturating_add(card_height.saturating_sub(1));
    for by in hit_bin_coord(y)..=hit_bin_coord(max_y) {
        for bx in hit_bin_coord(x)..=hit_bin_coord(max_x) {
            out.push(hit_bin_key(bx, by));
        }
    }
}

pub(super) fn hit_bin_coord(value: i32) -> i32 {
    value.div_euclid(HIT_BIN_SIZE)
}

pub(super) fn hit_bin_key_for_point(x: i32, y: i32) -> i64 {
    hit_bin_key(hit_bin_coord(x), hit_bin_coord(y))
}

pub(super) fn hit_bin_key(x: i32, y: i32) -> i64 {
    ((x as i64) << 32) | ((y as u32) as i64)
}

//! A* search grid internals for octilinear wire routing.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::gui::geometry::Rect;

use super::{
    ceil_to_step, floor_to_step, BlockedEdgeSets, RouteDirection, RouteEdgeKey, BEND_135_COST,
    BEND_180_COST, BEND_45_COST, BEND_90_COST, DIR_STATE_COUNT, ENDPOINT_CORRIDOR_CELLS,
    GRID_PITCH_PX, MAX_GRID_CELLS, ROUTE_DIRECTIONS, ROUTE_PADDING_CELLS, START_DIR_INDEX,
    STEP_CARDINAL_COST, STEP_DIAGONAL_COST,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cell {
    x: i32,
    y: i32,
}

/// Mutable state reused across routing searches to avoid hot-path reallocations.
#[derive(Debug, Default)]
pub(super) struct RouteSearchWorkspace {
    best_cost: Vec<i32>,
    parent: Vec<usize>,
    state_generation: Vec<u32>,
    current_generation: u32,
    touched_states: Vec<usize>,
    open: BinaryHeap<(Reverse<i32>, Reverse<i32>, usize)>,
    obstacle_candidate_indices: Vec<usize>,
    obstacle_visit_generation: Vec<u32>,
    obstacle_current_generation: u32,
}

impl RouteSearchWorkspace {
    pub(super) fn prepare(&mut self, state_len: usize) {
        if self.best_cost.len() < state_len {
            self.best_cost.resize(state_len, i32::MAX);
        }
        if self.parent.len() < state_len {
            self.parent.resize(state_len, usize::MAX);
        }
        if self.state_generation.len() < state_len {
            self.state_generation.resize(state_len, 0);
        }
        self.current_generation = self.current_generation.wrapping_add(1);
        if self.current_generation == 0 {
            self.state_generation.fill(0);
            self.current_generation = 1;
        }
        self.touched_states.clear();
        self.open.clear();
    }

    fn best_cost(&self, key: usize) -> i32 {
        if self.state_generation.get(key).copied() == Some(self.current_generation) {
            self.best_cost[key]
        } else {
            i32::MAX
        }
    }

    fn parent(&self, key: usize) -> usize {
        if self.state_generation.get(key).copied() == Some(self.current_generation) {
            self.parent[key]
        } else {
            usize::MAX
        }
    }

    fn set_state(&mut self, key: usize, cost: i32, parent: usize) {
        if self.state_generation[key] != self.current_generation {
            self.state_generation[key] = self.current_generation;
            self.touched_states.push(key);
        }
        self.best_cost[key] = cost;
        self.parent[key] = parent;
    }

    pub(super) fn prepare_obstacle_candidate_pass(&mut self, obstacle_count: usize) {
        if self.obstacle_visit_generation.len() < obstacle_count {
            self.obstacle_visit_generation.resize(obstacle_count, 0);
        }
        self.obstacle_current_generation = self.obstacle_current_generation.wrapping_add(1);
        if self.obstacle_current_generation == 0 {
            self.obstacle_visit_generation.fill(0);
            self.obstacle_current_generation = 1;
        }
        self.obstacle_candidate_indices.clear();
    }

    pub(super) fn push_unique_obstacle_candidate(&mut self, index: usize) {
        let Some(slot) = self.obstacle_visit_generation.get_mut(index) else {
            return;
        };
        if *slot == self.obstacle_current_generation {
            return;
        }
        *slot = self.obstacle_current_generation;
        self.obstacle_candidate_indices.push(index);
    }

    pub(super) fn obstacle_candidate_indices(&self) -> &[usize] {
        self.obstacle_candidate_indices.as_slice()
    }
}

/// Search-space grid for one route attempt.
#[derive(Clone, Debug)]
pub(super) struct SearchGrid {
    origin_x: i32,
    origin_y: i32,
    cols: i32,
    rows: i32,
    blocked: Vec<bool>,
}

impl SearchGrid {
    pub(super) fn build(
        blocked_rects: &[Rect],
        start: (i32, i32),
        end: (i32, i32),
    ) -> Option<Self> {
        let mut min_x = start.0.min(end.0);
        let mut min_y = start.1.min(end.1);
        let mut max_x = start.0.max(end.0);
        let mut max_y = start.1.max(end.1);
        for rect in blocked_rects {
            min_x = min_x.min(rect.x);
            min_y = min_y.min(rect.y);
            max_x = max_x.max(rect.x + rect.w);
            max_y = max_y.max(rect.y + rect.h);
        }
        let pad = ROUTE_PADDING_CELLS * GRID_PITCH_PX;
        min_x = floor_to_step(min_x - pad, GRID_PITCH_PX);
        min_y = floor_to_step(min_y - pad, GRID_PITCH_PX);
        max_x = ceil_to_step(max_x + pad, GRID_PITCH_PX);
        max_y = ceil_to_step(max_y + pad, GRID_PITCH_PX);
        let cols = ((max_x - min_x) / GRID_PITCH_PX) + 1;
        let rows = ((max_y - min_y) / GRID_PITCH_PX) + 1;
        if cols <= 0 || rows <= 0 {
            return None;
        }
        let len = cols.saturating_mul(rows) as usize;
        if len > MAX_GRID_CELLS {
            return None;
        }
        let mut blocked = vec![false; len];
        for rect in blocked_rects {
            if rect.w <= 0 || rect.h <= 0 {
                continue;
            }
            let rect_max_x_exclusive = rect.x.saturating_add(rect.w);
            let rect_max_y_exclusive = rect.y.saturating_add(rect.h);
            let rect_max_x_inclusive = rect_max_x_exclusive.saturating_sub(1);
            let rect_max_y_inclusive = rect_max_y_exclusive.saturating_sub(1);
            let min_col = ceil_div(rect.x.saturating_sub(min_x), GRID_PITCH_PX).clamp(0, cols - 1);
            let max_col = rect_max_x_inclusive
                .saturating_sub(min_x)
                .div_euclid(GRID_PITCH_PX)
                .clamp(0, cols - 1);
            let min_row = ceil_div(rect.y.saturating_sub(min_y), GRID_PITCH_PX).clamp(0, rows - 1);
            let max_row = rect_max_y_inclusive
                .saturating_sub(min_y)
                .div_euclid(GRID_PITCH_PX)
                .clamp(0, rows - 1);
            if min_col > max_col || min_row > max_row {
                continue;
            }
            for y in min_row..=max_row {
                let row_offset = (y * cols) as usize;
                for x in min_col..=max_col {
                    blocked[row_offset + x as usize] = true;
                }
            }
        }
        Some(Self {
            origin_x: min_x,
            origin_y: min_y,
            cols,
            rows,
            blocked,
        })
    }

    pub(super) fn carve_corridor(&mut self, start: (i32, i32), direction: RouteDirection) {
        for step in 0..=ENDPOINT_CORRIDOR_CELLS {
            let px = start.0 + direction.dx() * step * GRID_PITCH_PX;
            let py = start.1 + direction.dy() * step * GRID_PITCH_PX;
            let cell = self.point_to_cell((px, py));
            if let Some(index) = self.cell_index(cell) {
                self.blocked[index] = false;
            }
        }
    }

    pub(super) fn find_path_with_blocked_edges(
        &self,
        start: (i32, i32),
        end: (i32, i32),
        blocked_edges: Option<BlockedEdgeSets<'_>>,
        workspace: &mut RouteSearchWorkspace,
    ) -> Option<Vec<Cell>> {
        let start_cell = self.point_to_cell(start);
        let end_cell = self.point_to_cell(end);
        let start_cell_index = self.cell_index(start_cell)?;
        let end_cell_index = self.cell_index(end_cell)?;

        let state_len = self.blocked.len().saturating_mul(DIR_STATE_COUNT);
        workspace.prepare(state_len);
        let start_key = state_key(start_cell_index, START_DIR_INDEX);
        workspace.set_state(start_key, 0, usize::MAX);

        let h0 = octile_heuristic(start_cell, end_cell);
        workspace.open.push((Reverse(h0), Reverse(0), start_key));

        let mut goal_key = None;
        while let Some((Reverse(_f), Reverse(g), key)) = workspace.open.pop() {
            if g > workspace.best_cost(key) {
                continue;
            }
            let (cell_index, dir_index) = decode_state_key(key);
            let cell = self.index_cell(cell_index);
            if cell_index == end_cell_index {
                goal_key = Some(key);
                break;
            }

            for (next_dir_index, direction) in ROUTE_DIRECTIONS.iter().copied().enumerate() {
                let next = Cell {
                    x: cell.x + direction.dx(),
                    y: cell.y + direction.dy(),
                };
                let Some(next_cell_index) = self.cell_index(next) else {
                    continue;
                };
                if blocked_edges
                    .map(|blocked| {
                        blocked.contains(&RouteEdgeKey::new(
                            self.cell_point(cell),
                            self.cell_point(next),
                        ))
                    })
                    .unwrap_or(false)
                {
                    continue;
                }
                if self.blocked[next_cell_index] && next_cell_index != end_cell_index {
                    continue;
                }
                if direction.is_diagonal()
                    && !self.diagonal_corner_clear(cell, direction, end_cell_index)
                {
                    continue;
                }
                let step_cost = direction.step_cost();
                let bend_cost = bend_penalty(dir_index, next_dir_index);
                let next_cost = g.saturating_add(step_cost).saturating_add(bend_cost);
                let next_key = state_key(next_cell_index, next_dir_index);
                if next_cost >= workspace.best_cost(next_key) {
                    continue;
                }
                workspace.set_state(next_key, next_cost, key);
                let heuristic = octile_heuristic(next, end_cell);
                let next_f = next_cost.saturating_add(heuristic);
                workspace
                    .open
                    .push((Reverse(next_f), Reverse(next_cost), next_key));
            }
        }

        let goal_key = goal_key?;
        let mut cells = Vec::new();
        let mut cursor = goal_key;
        loop {
            let (cell_index, _dir_index) = decode_state_key(cursor);
            cells.push(self.index_cell(cell_index));
            if cursor == start_key {
                break;
            }
            let next = workspace.parent(cursor);
            if next == usize::MAX {
                return None;
            }
            cursor = next;
        }
        cells.reverse();
        Some(cells)
    }

    fn diagonal_corner_clear(
        &self,
        cell: Cell,
        direction: RouteDirection,
        goal_cell_index: usize,
    ) -> bool {
        let side_a = Cell {
            x: cell.x + direction.dx(),
            y: cell.y,
        };
        let side_b = Cell {
            x: cell.x,
            y: cell.y + direction.dy(),
        };
        for side in [side_a, side_b] {
            let Some(index) = self.cell_index(side) else {
                return false;
            };
            if self.blocked[index] && index != goal_cell_index {
                return false;
            }
        }
        true
    }

    fn point_to_cell(&self, point: (i32, i32)) -> Cell {
        let x = ((point.0 - self.origin_x) / GRID_PITCH_PX).clamp(0, self.cols - 1);
        let y = ((point.1 - self.origin_y) / GRID_PITCH_PX).clamp(0, self.rows - 1);
        Cell { x, y }
    }

    pub(super) fn cell_point(&self, cell: Cell) -> (i32, i32) {
        (
            self.origin_x + cell.x * GRID_PITCH_PX,
            self.origin_y + cell.y * GRID_PITCH_PX,
        )
    }

    fn cell_index(&self, cell: Cell) -> Option<usize> {
        if cell.x < 0 || cell.y < 0 || cell.x >= self.cols || cell.y >= self.rows {
            return None;
        }
        Some((cell.y * self.cols + cell.x) as usize)
    }

    fn index_cell(&self, index: usize) -> Cell {
        let cols = self.cols.max(1) as usize;
        let x = (index % cols) as i32;
        let y = (index / cols) as i32;
        Cell { x, y }
    }
}

fn ceil_div(value: i32, divisor: i32) -> i32 {
    let base = value.div_euclid(divisor);
    if value.rem_euclid(divisor) == 0 {
        base
    } else {
        base.saturating_add(1)
    }
}

fn bend_penalty(prev_dir_index: usize, next_dir_index: usize) -> i32 {
    if prev_dir_index == START_DIR_INDEX {
        return 0;
    }
    let diff = prev_dir_index
        .abs_diff(next_dir_index)
        .min(8 - prev_dir_index.abs_diff(next_dir_index));
    match diff {
        0 => 0,
        1 => BEND_45_COST,
        2 => BEND_90_COST,
        3 => BEND_135_COST,
        _ => BEND_180_COST,
    }
}

fn octile_heuristic(from: Cell, to: Cell) -> i32 {
    let dx = (from.x - to.x).abs();
    let dy = (from.y - to.y).abs();
    let diag = dx.min(dy);
    let straight = dx.max(dy) - diag;
    diag * STEP_DIAGONAL_COST + straight * STEP_CARDINAL_COST
}

fn state_key(cell_index: usize, dir_index: usize) -> usize {
    cell_index
        .saturating_mul(DIR_STATE_COUNT)
        .saturating_add(dir_index)
}

fn decode_state_key(key: usize) -> (usize, usize) {
    (key / DIR_STATE_COUNT, key % DIR_STATE_COUNT)
}

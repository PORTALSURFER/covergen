//! Grid-aligned octilinear routing for graph wires.
//!
//! The router operates on an integer grid and uses A* with direction-aware
//! state so bend penalties are deterministic. Node rectangles are treated as
//! hard obstacles with clearance inflation, while endpoint corridors are carved
//! to guarantee stable entry/exit near pin boundaries.

use crate::gui::geometry::Rect;
use crate::gui::project::NODE_GRID_PITCH;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

const GRID_PITCH_PX: i32 = NODE_GRID_PITCH;
const ROUTE_PADDING_CELLS: i32 = 6;
const OBSTACLE_CLEARANCE_CELLS: i32 = 2;
const ENDPOINT_CORRIDOR_CELLS: i32 = 3;
const ENDPOINT_TAIL_CELLS: i32 = 2;
pub(crate) const DEFAULT_ENDPOINT_TAIL_CELLS: i32 = ENDPOINT_TAIL_CELLS;
const MAX_GRID_CELLS: usize = 48_000;

const STEP_CARDINAL_COST: i32 = 10;
const STEP_DIAGONAL_COST: i32 = 14;
const BEND_45_COST: i32 = 50;
const BEND_90_COST: i32 = 80;
const BEND_135_COST: i32 = 120;
const BEND_180_COST: i32 = 160;

const START_DIR_INDEX: usize = 8;
const DIR_STATE_COUNT: usize = 9;

/// One node obstacle in graph/world coordinates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeObstacle {
    pub(crate) rect: Rect,
}

/// One endpoint routing direction used for pin corridor carving.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RouteDirection {
    East,
    NorthEast,
    North,
    NorthWest,
    West,
    SouthWest,
    South,
    SouthEast,
}

impl RouteDirection {
    const fn dx(self) -> i32 {
        match self {
            Self::East | Self::NorthEast | Self::SouthEast => 1,
            Self::West | Self::NorthWest | Self::SouthWest => -1,
            Self::North | Self::South => 0,
        }
    }

    const fn dy(self) -> i32 {
        match self {
            Self::South | Self::SouthEast | Self::SouthWest => 1,
            Self::North | Self::NorthEast | Self::NorthWest => -1,
            Self::East | Self::West => 0,
        }
    }

    const fn is_diagonal(self) -> bool {
        self.dx() != 0 && self.dy() != 0
    }

    const fn step_cost(self) -> i32 {
        if self.is_diagonal() {
            STEP_DIAGONAL_COST
        } else {
            STEP_CARDINAL_COST
        }
    }
}

const ROUTE_DIRECTIONS: [RouteDirection; 8] = [
    RouteDirection::East,
    RouteDirection::NorthEast,
    RouteDirection::North,
    RouteDirection::NorthWest,
    RouteDirection::West,
    RouteDirection::SouthWest,
    RouteDirection::South,
    RouteDirection::SouthEast,
];

/// One routed endpoint with pin direction and corridor carving hint.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RouteEndpoint {
    pub(crate) point: (i32, i32),
    pub(crate) corridor_dir: RouteDirection,
}

/// Precomputed obstacle data reused across multiple route queries.
#[derive(Clone, Debug, Default)]
pub(crate) struct RouteObstacleMap {
    blocked_rects: Vec<Rect>,
}

impl RouteObstacleMap {
    /// Build one reusable obstacle map from graph-space node obstacles.
    pub(crate) fn from_obstacles(obstacles: &[NodeObstacle]) -> Self {
        Self {
            blocked_rects: collect_blocked_rects(obstacles),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RouteEdgeKey {
    a: (i32, i32),
    b: (i32, i32),
}

impl RouteEdgeKey {
    fn new(a: (i32, i32), b: (i32, i32)) -> Self {
        if a <= b {
            Self { a, b }
        } else {
            Self { a: b, b: a }
        }
    }
}

/// Accumulated routed edges used to avoid direct wire overlap on reroutes.
///
/// Only horizontal endpoint tail segments are excluded from overlap blocking.
#[derive(Clone, Debug, Default)]
pub(crate) struct RouteOccupiedEdges {
    blocked: HashSet<RouteEdgeKey>,
}

impl RouteOccupiedEdges {
    /// Record one already-routed polyline.
    ///
    /// This blocks all edges except horizontal first/last segments so multiple
    /// wires may still share short pin exit/entry tails.
    pub(crate) fn record_path_non_tail(&mut self, points: &[(i32, i32)]) {
        if points.len() < 2 {
            return;
        }
        let last_segment = points.len().saturating_sub(2);
        for (segment_index, pair) in points.windows(2).enumerate() {
            let is_endpoint_segment = segment_index == 0 || segment_index == last_segment;
            let is_horizontal = pair[0].1 == pair[1].1;
            if is_endpoint_segment && is_horizontal {
                continue;
            }
            self.block_segment(pair[0], pair[1]);
        }
    }

    fn block_segment(&mut self, from: (i32, i32), to: (i32, i32)) {
        let dx = to.0 - from.0;
        let dy = to.1 - from.1;
        let steps = (dx.abs().max(dy.abs()) / GRID_PITCH_PX).max(1);
        let step_x = dx.signum() * GRID_PITCH_PX;
        let step_y = dy.signum() * GRID_PITCH_PX;
        let mut current = from;
        for _ in 0..steps {
            let next = (current.0 + step_x, current.1 + step_y);
            self.blocked.insert(RouteEdgeKey::new(current, next));
            current = next;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    x: i32,
    y: i32,
}

/// Build an obstacle-avoiding octilinear path between two graph points.
///
/// This compatibility wrapper keeps signal-link call sites unchanged and
/// assumes both endpoints carve east-facing pin corridors.
#[cfg(test)]
pub(crate) fn route_param_path(
    start: (i32, i32),
    end: (i32, i32),
    obstacles: &[NodeObstacle],
) -> Vec<(i32, i32)> {
    let obstacle_map = RouteObstacleMap::from_obstacles(obstacles);
    route_param_path_with_map(start, end, &obstacle_map)
}

/// Build one obstacle-avoiding octilinear path using a precomputed map.
#[cfg(test)]
pub(crate) fn route_param_path_with_map(
    start: (i32, i32),
    end: (i32, i32),
    obstacle_map: &RouteObstacleMap,
) -> Vec<(i32, i32)> {
    route_wire_path_with_map(
        RouteEndpoint {
            point: start,
            corridor_dir: RouteDirection::East,
        },
        RouteEndpoint {
            point: end,
            corridor_dir: RouteDirection::East,
        },
        obstacle_map,
    )
}

/// Build one obstacle-avoiding octilinear path between two directed endpoints.
#[cfg(test)]
pub(crate) fn route_wire_path(
    start: RouteEndpoint,
    end: RouteEndpoint,
    obstacles: &[NodeObstacle],
) -> Vec<(i32, i32)> {
    let obstacle_map = RouteObstacleMap::from_obstacles(obstacles);
    route_wire_path_with_map(start, end, &obstacle_map)
}

/// Build one obstacle-avoiding octilinear path with a precomputed obstacle map.
#[cfg(test)]
pub(crate) fn route_wire_path_with_map(
    start: RouteEndpoint,
    end: RouteEndpoint,
    obstacle_map: &RouteObstacleMap,
) -> Vec<(i32, i32)> {
    route_wire_path_internal(start, end, obstacle_map, None)
}

fn route_wire_path_internal(
    start: RouteEndpoint,
    end: RouteEndpoint,
    obstacle_map: &RouteObstacleMap,
    blocked_edges: Option<&HashSet<RouteEdgeKey>>,
) -> Vec<(i32, i32)> {
    let start_point = snap_endpoint_to_grid(start);
    let end_point = snap_endpoint_to_grid(end);
    if start_point == end_point {
        return vec![start_point];
    }

    let Some(mut grid) = SearchGrid::build(
        obstacle_map.blocked_rects.as_slice(),
        start_point,
        end_point,
    ) else {
        return fallback_octilinear(start_point, end_point);
    };

    grid.carve_corridor(start_point, start.corridor_dir);
    grid.carve_corridor(end_point, end.corridor_dir);

    let Some(cells) = grid.find_path_with_blocked_edges(start_point, end_point, blocked_edges)
    else {
        return fallback_octilinear(start_point, end_point);
    };

    let mut points = Vec::with_capacity(cells.len());
    for cell in cells {
        points.push(grid.cell_point(cell));
    }
    dedupe_points(&mut points);
    collapse_collinear_octilinear(&mut points);
    if points.is_empty() {
        return vec![start_point, end_point];
    }
    points
}

/// Build one obstacle-avoiding octilinear path with guaranteed straight
/// endpoint tails that follow each endpoint corridor direction.
///
/// This is used by scene and interaction call sites so rendered geometry and
/// cut-hit detection stay in sync. The returned polyline starts and ends at
/// the snapped endpoint pin coordinates.
pub(crate) fn route_wire_path_with_tails_with_map(
    start: RouteEndpoint,
    end: RouteEndpoint,
    obstacle_map: &RouteObstacleMap,
) -> Vec<(i32, i32)> {
    route_wire_path_with_tails_internal(
        start,
        end,
        obstacle_map,
        None,
        ENDPOINT_TAIL_CELLS,
        ENDPOINT_TAIL_CELLS,
    )
}

/// Build one obstacle-avoiding path with per-endpoint horizontal-tail lengths
/// while avoiding already occupied wire edges.
pub(crate) fn route_wire_path_with_tail_cells_avoiding_overlaps_with_map(
    start: RouteEndpoint,
    end: RouteEndpoint,
    obstacle_map: &RouteObstacleMap,
    occupied_edges: &RouteOccupiedEdges,
    start_tail_cells: i32,
    end_tail_cells: i32,
) -> Vec<(i32, i32)> {
    route_wire_path_with_tails_internal(
        start,
        end,
        obstacle_map,
        Some(&occupied_edges.blocked),
        start_tail_cells,
        end_tail_cells,
    )
}

fn route_wire_path_with_tails_internal(
    start: RouteEndpoint,
    end: RouteEndpoint,
    obstacle_map: &RouteObstacleMap,
    blocked_edges: Option<&HashSet<RouteEdgeKey>>,
    start_tail_cells: i32,
    end_tail_cells: i32,
) -> Vec<(i32, i32)> {
    let start_pin = snap_endpoint_to_grid(start);
    let end_pin = snap_endpoint_to_grid(end);
    let start_tail = step_point(start_pin, start.corridor_dir, start_tail_cells.max(0));
    let end_tail = step_point(end_pin, end.corridor_dir, end_tail_cells.max(0));

    let core = route_wire_path_internal(
        RouteEndpoint {
            point: start_tail,
            corridor_dir: start.corridor_dir,
        },
        RouteEndpoint {
            point: end_tail,
            corridor_dir: end.corridor_dir,
        },
        obstacle_map,
        blocked_edges,
    );

    let mut points = Vec::with_capacity(core.len().saturating_add(4));
    points.push(start_pin);
    if points.last().copied() != Some(start_tail) {
        points.push(start_tail);
    }
    for point in core {
        if points.last().copied() != Some(point) {
            points.push(point);
        }
    }
    if points.last().copied() != Some(end_tail) {
        points.push(end_tail);
    }
    if points.last().copied() != Some(end_pin) {
        points.push(end_pin);
    }
    dedupe_points(&mut points);
    points
}

fn collect_blocked_rects(obstacles: &[NodeObstacle]) -> Vec<Rect> {
    let mut blocked = Vec::new();
    let pad = OBSTACLE_CLEARANCE_CELLS * GRID_PITCH_PX;
    for obstacle in obstacles {
        blocked.push(inflate_rect(obstacle.rect, pad));
    }
    blocked
}

fn inflate_rect(rect: Rect, pad: i32) -> Rect {
    Rect::new(
        rect.x - pad,
        rect.y - pad,
        rect.w + pad * 2,
        rect.h + pad * 2,
    )
}

fn fallback_octilinear(start: (i32, i32), end: (i32, i32)) -> Vec<(i32, i32)> {
    if start == end {
        return vec![start];
    }
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let diag_steps = dx.abs().min(dy.abs());
    if diag_steps == 0 {
        return vec![start, end];
    }
    let diag = (
        start.0 + dx.signum() * diag_steps,
        start.1 + dy.signum() * diag_steps,
    );
    let mut out = vec![start, diag];
    if diag != end {
        out.push(end);
    }
    collapse_collinear_octilinear(&mut out);
    out
}

fn dedupe_points(points: &mut Vec<(i32, i32)>) {
    if points.is_empty() {
        return;
    }
    let mut write = 1usize;
    for read in 1..points.len() {
        if points[read] == points[write - 1] {
            continue;
        }
        points[write] = points[read];
        write += 1;
    }
    points.truncate(write);
}

fn collapse_collinear_octilinear(points: &mut Vec<(i32, i32)>) {
    if points.len() < 3 {
        return;
    }
    let mut write = 1usize;
    for read in 1..points.len() - 1 {
        let prev = points[write - 1];
        let curr = points[read];
        let next = points[read + 1];
        let prev_dir = unit_dir(prev, curr);
        let next_dir = unit_dir(curr, next);
        if prev_dir == next_dir {
            continue;
        }
        points[write] = curr;
        write += 1;
    }
    points[write] = points[points.len() - 1];
    points.truncate(write + 1);
}

fn unit_dir(a: (i32, i32), b: (i32, i32)) -> (i32, i32) {
    ((b.0 - a.0).signum(), (b.1 - a.1).signum())
}

#[derive(Clone, Debug)]
struct SearchGrid {
    origin_x: i32,
    origin_y: i32,
    cols: i32,
    rows: i32,
    blocked: Vec<bool>,
}

impl SearchGrid {
    fn build(blocked_rects: &[Rect], start: (i32, i32), end: (i32, i32)) -> Option<Self> {
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
        for y in 0..rows {
            for x in 0..cols {
                let index = (y * cols + x) as usize;
                let point = (min_x + x * GRID_PITCH_PX, min_y + y * GRID_PITCH_PX);
                blocked[index] = blocked_rects
                    .iter()
                    .any(|rect| rect.contains(point.0, point.1));
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

    fn carve_corridor(&mut self, start: (i32, i32), direction: RouteDirection) {
        for step in 0..=ENDPOINT_CORRIDOR_CELLS {
            let px = start.0 + direction.dx() * step * GRID_PITCH_PX;
            let py = start.1 + direction.dy() * step * GRID_PITCH_PX;
            let cell = self.point_to_cell((px, py));
            if let Some(index) = self.cell_index(cell) {
                self.blocked[index] = false;
            }
        }
    }

    fn find_path_with_blocked_edges(
        &self,
        start: (i32, i32),
        end: (i32, i32),
        blocked_edges: Option<&HashSet<RouteEdgeKey>>,
    ) -> Option<Vec<Cell>> {
        let start_cell = self.point_to_cell(start);
        let end_cell = self.point_to_cell(end);
        let start_cell_index = self.cell_index(start_cell)?;
        let end_cell_index = self.cell_index(end_cell)?;

        let state_len = self.blocked.len().saturating_mul(DIR_STATE_COUNT);
        let mut best_cost = vec![i32::MAX; state_len];
        let mut parent = vec![usize::MAX; state_len];
        let start_key = state_key(start_cell_index, START_DIR_INDEX);
        best_cost[start_key] = 0;

        let mut open = BinaryHeap::new();
        let h0 = octile_heuristic(start_cell, end_cell);
        open.push((Reverse(h0), Reverse(0), start_key));

        let mut goal_key = None;
        while let Some((Reverse(_f), Reverse(g), key)) = open.pop() {
            if g > best_cost[key] {
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
                if next_cost >= best_cost[next_key] {
                    continue;
                }
                best_cost[next_key] = next_cost;
                parent[next_key] = key;
                let heuristic = octile_heuristic(next, end_cell);
                let next_f = next_cost.saturating_add(heuristic);
                open.push((Reverse(next_f), Reverse(next_cost), next_key));
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
            let next = parent[cursor];
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

    fn cell_point(&self, cell: Cell) -> (i32, i32) {
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

fn floor_to_step(value: i32, step: i32) -> i32 {
    value.div_euclid(step) * step
}

fn ceil_to_step(value: i32, step: i32) -> i32 {
    let q = value.div_euclid(step);
    let r = value.rem_euclid(step);
    if r == 0 {
        q * step
    } else {
        (q + 1) * step
    }
}

fn snap_to_grid(value: i32) -> i32 {
    let base = floor_to_step(value, GRID_PITCH_PX);
    let next = base + GRID_PITCH_PX;
    if (value - base).abs() <= (next - value).abs() {
        base
    } else {
        next
    }
}

fn snap_endpoint_to_grid(endpoint: RouteEndpoint) -> (i32, i32) {
    let x = match endpoint.corridor_dir.dx() {
        1 => ceil_to_step(endpoint.point.0, GRID_PITCH_PX),
        -1 => floor_to_step(endpoint.point.0, GRID_PITCH_PX),
        _ => snap_to_grid(endpoint.point.0),
    };
    let y = match endpoint.corridor_dir.dy() {
        1 => ceil_to_step(endpoint.point.1, GRID_PITCH_PX),
        -1 => floor_to_step(endpoint.point.1, GRID_PITCH_PX),
        _ => snap_to_grid(endpoint.point.1),
    };
    (x, y)
}

fn step_point(point: (i32, i32), direction: RouteDirection, steps: i32) -> (i32, i32) {
    (
        point.0 + direction.dx() * steps * GRID_PITCH_PX,
        point.1 + direction.dy() * steps * GRID_PITCH_PX,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        route_param_path, route_wire_path, route_wire_path_with_map,
        route_wire_path_with_tails_with_map, snap_endpoint_to_grid, NodeObstacle, RouteDirection,
        RouteEndpoint, RouteObstacleMap,
    };
    use crate::gui::geometry::Rect;

    fn assert_octilinear(points: &[(i32, i32)]) {
        for segment in points.windows(2) {
            let dx = (segment[1].0 - segment[0].0).abs();
            let dy = (segment[1].1 - segment[0].1).abs();
            assert!(
                dx == 0 || dy == 0 || dx == dy,
                "segment is not octilinear: {:?}",
                segment
            );
        }
    }

    #[test]
    fn route_avoids_middle_obstacle() {
        let obstacles = [NodeObstacle {
            rect: Rect::new(60, 30, 60, 60),
        }];
        let path = route_param_path((8, 56), (184, 56), &obstacles);
        assert!(path.len() >= 3);
        assert_octilinear(path.as_slice());
        for point in path {
            assert!(!obstacles[0].rect.contains(point.0, point.1));
        }
    }

    #[test]
    fn route_with_cached_obstacle_map_matches_direct_route_endpoints() {
        let obstacles = [
            NodeObstacle {
                rect: Rect::new(40, 30, 40, 40),
            },
            NodeObstacle {
                rect: Rect::new(120, 20, 40, 60),
            },
        ];
        let direct = route_wire_path(
            RouteEndpoint {
                point: (8, 48),
                corridor_dir: RouteDirection::East,
            },
            RouteEndpoint {
                point: (208, 48),
                corridor_dir: RouteDirection::West,
            },
            &obstacles,
        );
        let cached = RouteObstacleMap::from_obstacles(&obstacles);
        let from_cache = route_wire_path_with_map(
            RouteEndpoint {
                point: (8, 48),
                corridor_dir: RouteDirection::East,
            },
            RouteEndpoint {
                point: (208, 48),
                corridor_dir: RouteDirection::West,
            },
            &cached,
        );
        assert_eq!(from_cache.first().copied(), direct.first().copied());
        assert_eq!(from_cache.last().copied(), direct.last().copied());
        assert_octilinear(from_cache.as_slice());
    }

    #[test]
    fn endpoint_snap_projects_outward_for_face_directions() {
        let point = (322, 141);
        let east = snap_endpoint_to_grid(RouteEndpoint {
            point,
            corridor_dir: RouteDirection::East,
        });
        assert!(east.0 >= point.0);
        assert_eq!(east.0.rem_euclid(4), 0);
        assert_eq!(east.1.rem_euclid(4), 0);

        let west = snap_endpoint_to_grid(RouteEndpoint {
            point,
            corridor_dir: RouteDirection::West,
        });
        assert!(west.0 <= point.0);
        assert_eq!(west.0.rem_euclid(4), 0);
        assert_eq!(west.1.rem_euclid(4), 0);

        let north = snap_endpoint_to_grid(RouteEndpoint {
            point,
            corridor_dir: RouteDirection::North,
        });
        assert!(north.1 <= point.1);
        assert_eq!(north.0.rem_euclid(4), 0);
        assert_eq!(north.1.rem_euclid(4), 0);

        let south = snap_endpoint_to_grid(RouteEndpoint {
            point,
            corridor_dir: RouteDirection::South,
        });
        assert!(south.1 >= point.1);
        assert_eq!(south.0.rem_euclid(4), 0);
        assert_eq!(south.1.rem_euclid(4), 0);
    }

    #[test]
    fn routed_path_includes_horizontal_endpoint_tails() {
        let map = RouteObstacleMap::default();
        let path = route_wire_path_with_tails_with_map(
            RouteEndpoint {
                point: (8, 40),
                corridor_dir: RouteDirection::East,
            },
            RouteEndpoint {
                point: (208, 40),
                corridor_dir: RouteDirection::West,
            },
            &map,
        );
        assert!(
            path.len() >= 4,
            "path too short for explicit tails: {path:?}"
        );
        let start = path[0];
        let start_tail = path[1];
        assert_eq!(start.1, start_tail.1, "start tail should be horizontal");
        assert!(
            start_tail.0 > start.0,
            "start tail should exit to the right"
        );

        let end = path[path.len() - 1];
        let end_tail = path[path.len() - 2];
        assert_eq!(end.1, end_tail.1, "end tail should be horizontal");
        assert!(end_tail.0 < end.0, "end tail should enter from the left");
    }
}

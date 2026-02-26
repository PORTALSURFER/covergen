//! Orthogonal path routing for parameter wires in the graph scene.
//!
//! The router builds a coarse grid around endpoints and node obstacles, then
//! runs a bounded BFS to find a path that does not cross blocked cells.

use crate::gui::geometry::Rect;
use std::collections::VecDeque;

const GRID_STEP_PX: i32 = 14;
const ROUTE_PADDING_PX: i32 = GRID_STEP_PX * 4;
const OBSTACLE_MARGIN_PX: i32 = 8;
const MAX_GRID_CELLS: usize = 12_000;

/// One node obstacle in panel coordinates for pathfinding.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeObstacle {
    pub(crate) rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    x: i32,
    y: i32,
}

/// Build an obstacle-avoiding, orthogonal path between two panel points.
///
/// The returned list includes `start` and `end`. When no route is found within
/// grid limits, the router falls back to a best-effort orthogonal detour.
pub(crate) fn route_param_path(
    start: (i32, i32),
    end: (i32, i32),
    obstacles: &[NodeObstacle],
) -> Vec<(i32, i32)> {
    let blocked = collect_blocked_rects(obstacles);
    if blocked.is_empty() {
        return vec![start, end];
    }
    let Some(grid) = Grid::build(start, end, blocked.as_slice()) else {
        return fallback_route(start, end, blocked.as_slice());
    };
    let Some(cells) = grid.find_path() else {
        return fallback_route(start, end, blocked.as_slice());
    };
    let mut points = Vec::with_capacity(cells.len() + 2);
    points.push(start);
    for cell in cells {
        points.push(grid.cell_point(cell));
    }
    points.push(end);
    dedupe_points(&mut points);
    simplify_collinear(&mut points);
    if points.len() < 2 {
        return vec![start, end];
    }
    points
}

fn collect_blocked_rects(obstacles: &[NodeObstacle]) -> Vec<Rect> {
    let mut blocked = Vec::new();
    for obstacle in obstacles {
        blocked.push(inflate_rect(obstacle.rect, OBSTACLE_MARGIN_PX));
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

fn fallback_route(start: (i32, i32), end: (i32, i32), blocked: &[Rect]) -> Vec<(i32, i32)> {
    let horizontal_first = vec![start, (end.0, start.1), end];
    if path_clear(horizontal_first.as_slice(), blocked) {
        return horizontal_first;
    }
    let vertical_first = vec![start, (start.0, end.1), end];
    if path_clear(vertical_first.as_slice(), blocked) {
        return vertical_first;
    }

    let mut min_y = start.1.min(end.1);
    let mut max_y = start.1.max(end.1);
    for rect in blocked {
        min_y = min_y.min(rect.y);
        max_y = max_y.max(rect.y + rect.h);
    }
    let up_y = min_y - ROUTE_PADDING_PX;
    let down_y = max_y + ROUTE_PADDING_PX;
    let up = vec![start, (start.0, up_y), (end.0, up_y), end];
    if path_clear(up.as_slice(), blocked) {
        return up;
    }
    vec![start, (start.0, down_y), (end.0, down_y), end]
}

fn path_clear(points: &[(i32, i32)], blocked: &[Rect]) -> bool {
    if points.len() < 2 {
        return true;
    }
    for segment in points.windows(2) {
        if axis_segment_hits_any(segment[0], segment[1], blocked) {
            return false;
        }
    }
    true
}

fn axis_segment_hits_any(a: (i32, i32), b: (i32, i32), blocked: &[Rect]) -> bool {
    for rect in blocked {
        if axis_segment_hits_rect(a, b, *rect) {
            return true;
        }
    }
    false
}

fn axis_segment_hits_rect(a: (i32, i32), b: (i32, i32), rect: Rect) -> bool {
    let (x0, y0) = a;
    let (x1, y1) = b;
    let rx0 = rect.x;
    let ry0 = rect.y;
    let rx1 = rect.x + rect.w;
    let ry1 = rect.y + rect.h;
    if x0 == x1 {
        if x0 < rx0 || x0 > rx1 {
            return false;
        }
        let seg_min = y0.min(y1);
        let seg_max = y0.max(y1);
        return seg_max >= ry0 && seg_min <= ry1;
    }
    if y0 == y1 {
        if y0 < ry0 || y0 > ry1 {
            return false;
        }
        let seg_min = x0.min(x1);
        let seg_max = x0.max(x1);
        return seg_max >= rx0 && seg_min <= rx1;
    }
    false
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
    for index in write..points.len() {
        points[index] = points[write - 1];
    }
    points.truncate(write);
}

fn simplify_collinear(points: &mut Vec<(i32, i32)>) {
    if points.len() < 3 {
        return;
    }
    let mut write = 1usize;
    for read in 1..points.len() - 1 {
        let prev = points[write - 1];
        let curr = points[read];
        let next = points[read + 1];
        let vertical = prev.0 == curr.0 && curr.0 == next.0;
        let horizontal = prev.1 == curr.1 && curr.1 == next.1;
        if vertical || horizontal {
            continue;
        }
        points[write] = curr;
        write += 1;
    }
    points[write] = points[points.len() - 1];
    write += 1;
    for index in write..points.len() {
        points[index] = points[write - 1];
    }
    points.truncate(write);
}

struct Grid {
    origin_x: i32,
    origin_y: i32,
    cols: i32,
    rows: i32,
    start: Cell,
    end: Cell,
    blocked: Vec<bool>,
}

impl Grid {
    fn build(start: (i32, i32), end: (i32, i32), blocked: &[Rect]) -> Option<Self> {
        let (mut min_x, mut max_x) = (start.0.min(end.0), start.0.max(end.0));
        let (mut min_y, mut max_y) = (start.1.min(end.1), start.1.max(end.1));
        for rect in blocked {
            min_x = min_x.min(rect.x);
            min_y = min_y.min(rect.y);
            max_x = max_x.max(rect.x + rect.w);
            max_y = max_y.max(rect.y + rect.h);
        }
        min_x = floor_to_step(min_x - ROUTE_PADDING_PX, GRID_STEP_PX);
        min_y = floor_to_step(min_y - ROUTE_PADDING_PX, GRID_STEP_PX);
        max_x = ceil_to_step(max_x + ROUTE_PADDING_PX, GRID_STEP_PX);
        max_y = ceil_to_step(max_y + ROUTE_PADDING_PX, GRID_STEP_PX);
        let cols = ((max_x - min_x) / GRID_STEP_PX) + 1;
        let rows = ((max_y - min_y) / GRID_STEP_PX) + 1;
        let len = cols.saturating_mul(rows) as usize;
        if cols <= 0 || rows <= 0 || len > MAX_GRID_CELLS {
            return None;
        }

        let mut grid = Self {
            origin_x: min_x,
            origin_y: min_y,
            cols,
            rows,
            start: Cell { x: 0, y: 0 },
            end: Cell { x: 0, y: 0 },
            blocked: vec![false; len],
        };
        grid.start = grid.point_to_cell(start);
        grid.end = grid.point_to_cell(end);
        for y in 0..rows {
            for x in 0..cols {
                let cell = Cell { x, y };
                let p = grid.cell_point(cell);
                let index = grid.cell_index(cell)?;
                grid.blocked[index] = blocked.iter().any(|rect| rect.contains(p.0, p.1));
            }
        }
        if let Some(index) = grid.cell_index(grid.start) {
            grid.blocked[index] = false;
        }
        if let Some(index) = grid.cell_index(grid.end) {
            grid.blocked[index] = false;
        }
        Some(grid)
    }

    fn find_path(&self) -> Option<Vec<Cell>> {
        let len = self.blocked.len();
        let mut visited = vec![false; len];
        let mut parent = vec![usize::MAX; len];
        let start = self.cell_index(self.start)?;
        let end = self.cell_index(self.end)?;
        let mut queue = VecDeque::new();
        queue.push_back(self.start);
        visited[start] = true;

        while let Some(cell) = queue.pop_front() {
            let index = self.cell_index(cell)?;
            if index == end {
                break;
            }
            for next in self.neighbors(cell) {
                let Some(next_index) = self.cell_index(next) else {
                    continue;
                };
                if self.blocked[next_index] || visited[next_index] {
                    continue;
                }
                visited[next_index] = true;
                parent[next_index] = index;
                queue.push_back(next);
            }
        }
        if !visited[end] {
            return None;
        }
        let mut out = Vec::new();
        let mut cursor = end;
        while cursor != start {
            out.push(self.index_cell(cursor));
            cursor = parent[cursor];
        }
        out.push(self.start);
        out.reverse();
        Some(out)
    }

    fn neighbors(&self, cell: Cell) -> [Cell; 4] {
        [
            Cell {
                x: cell.x - 1,
                y: cell.y,
            },
            Cell {
                x: cell.x + 1,
                y: cell.y,
            },
            Cell {
                x: cell.x,
                y: cell.y - 1,
            },
            Cell {
                x: cell.x,
                y: cell.y + 1,
            },
        ]
    }

    fn cell_index(&self, cell: Cell) -> Option<usize> {
        if cell.x < 0 || cell.y < 0 || cell.x >= self.cols || cell.y >= self.rows {
            return None;
        }
        Some((cell.y * self.cols + cell.x) as usize)
    }

    fn index_cell(&self, index: usize) -> Cell {
        let x = (index as i32) % self.cols;
        let y = (index as i32) / self.cols;
        Cell { x, y }
    }

    fn point_to_cell(&self, point: (i32, i32)) -> Cell {
        let x = ((point.0 - self.origin_x) as f32 / GRID_STEP_PX as f32).round() as i32;
        let y = ((point.1 - self.origin_y) as f32 / GRID_STEP_PX as f32).round() as i32;
        Cell {
            x: x.clamp(0, self.cols - 1),
            y: y.clamp(0, self.rows - 1),
        }
    }

    fn cell_point(&self, cell: Cell) -> (i32, i32) {
        (
            self.origin_x + cell.x * GRID_STEP_PX,
            self.origin_y + cell.y * GRID_STEP_PX,
        )
    }
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

#[cfg(test)]
mod tests {
    use super::{route_param_path, NodeObstacle};
    use crate::gui::geometry::Rect;

    #[test]
    fn route_avoids_middle_obstacle() {
        let obstacles = [NodeObstacle {
            rect: Rect::new(60, 30, 60, 60),
        }];
        let path = route_param_path((10, 60), (180, 60), &obstacles);
        assert!(path.len() >= 3);
        for segment in path[1..path.len() - 1].windows(2) {
            assert!(segment[0].0 == segment[1].0 || segment[0].1 == segment[1].1);
        }
        for point in path {
            assert!(!obstacles[0].rect.contains(point.0, point.1));
        }
    }

    #[test]
    fn route_avoids_obstacle_without_ignore_exceptions() {
        let obstacles = [NodeObstacle {
            rect: Rect::new(40, 30, 40, 40),
        }];
        let path = route_param_path((10, 50), (110, 50), &obstacles);
        assert_eq!(path.first().copied(), Some((10, 50)));
        assert_eq!(path.last().copied(), Some((110, 50)));
        assert!(path.len() >= 3);
    }
}

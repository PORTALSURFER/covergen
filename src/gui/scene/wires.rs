//! Wire geometry and bridge helpers for scene rendering.

use std::collections::HashMap;

use super::wire_route;
use crate::gui::geometry::segments_intersect as geometry_segments_intersect;
use crate::gui::state::PreviewState;

pub(super) const WIRE_ENDPOINT_RADIUS_PX: i32 = 2;
pub(super) const PARAM_BIND_TARGET_RADIUS_PX: i32 = 3;
pub(super) const PARAM_WIRE_EXIT_TAIL_PX: i32 = 18;
pub(super) const PARAM_WIRE_ENTRY_TAIL_PX: i32 = 18;
pub(super) const PARAM_WIRE_ENDPOINT_STRAIGHT_PX: i32 = 10;

const PARAM_WIRE_ROUTE_LEAD_PX: i32 = 12;
const PARAM_WIRE_CORNER_RADIUS_MIN_PX: i32 = 3;
const PARAM_WIRE_CORNER_RADIUS_MAX_PX: i32 = 8;
const PARAM_WIRE_CURVE_STEPS: usize = 4;
const WIRE_BRIDGE_SPAN_PX: f32 = 16.0;
const WIRE_BRIDGE_HEIGHT_PX: f32 = 6.0;
const WIRE_BRIDGE_LINK_THRESHOLD_PX: f32 = 14.0;
const WIRE_BRIDGE_CORNER_GUARD_PX: f32 = 10.0;
const WIRE_BRIDGE_STEPS: usize = 6;
const WIRE_BRIDGE_HASH_CELL_PX: i32 = 64;
const WIRE_TAIL_STAGGER_STEP_CELLS: i32 = 1;
const WIRE_TAIL_STAGGER_MAX_EXTRA_CELLS: i32 = 8;

#[derive(Clone, Copy, Debug)]
struct ParamWireAnchors {
    source_exit: (i32, i32),
    route_start: (i32, i32),
    route_end: (i32, i32),
    target_entry: (i32, i32),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DrawnWireSegment {
    pub(super) from: (i32, i32),
    pub(super) to: (i32, i32),
}

/// Spatial hash over already drawn wire segments for bridge candidate lookup.
#[derive(Debug, Default)]
pub(super) struct BridgeSegmentSpatialHash {
    buckets: HashMap<(i32, i32), Vec<usize>>,
}

impl BridgeSegmentSpatialHash {
    pub(super) fn clear(&mut self) {
        self.buckets.clear();
    }

    pub(super) fn insert_segment(&mut self, segment: DrawnWireSegment, segment_index: usize) {
        let (min_x, min_y, max_x, max_y) = segment_bounds(segment);
        let min_bucket_x = min_x.div_euclid(WIRE_BRIDGE_HASH_CELL_PX);
        let max_bucket_x = max_x.div_euclid(WIRE_BRIDGE_HASH_CELL_PX);
        let min_bucket_y = min_y.div_euclid(WIRE_BRIDGE_HASH_CELL_PX);
        let max_bucket_y = max_y.div_euclid(WIRE_BRIDGE_HASH_CELL_PX);
        for bucket_y in min_bucket_y..=max_bucket_y {
            for bucket_x in min_bucket_x..=max_bucket_x {
                self.buckets
                    .entry((bucket_x, bucket_y))
                    .or_default()
                    .push(segment_index);
            }
        }
    }

    pub(super) fn collect_candidates(&self, segment: DrawnWireSegment, out: &mut Vec<usize>) {
        out.clear();
        let (min_x, min_y, max_x, max_y) = segment_bounds(segment);
        let min_bucket_x = min_x.div_euclid(WIRE_BRIDGE_HASH_CELL_PX);
        let max_bucket_x = max_x.div_euclid(WIRE_BRIDGE_HASH_CELL_PX);
        let min_bucket_y = min_y.div_euclid(WIRE_BRIDGE_HASH_CELL_PX);
        let max_bucket_y = max_y.div_euclid(WIRE_BRIDGE_HASH_CELL_PX);
        for bucket_y in min_bucket_y..=max_bucket_y {
            for bucket_x in min_bucket_x..=max_bucket_x {
                let Some(indices) = self.buckets.get(&(bucket_x, bucket_y)) else {
                    continue;
                };
                out.extend(indices.iter().copied());
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

pub(super) fn next_staggered_tail_cells(
    slots: &mut HashMap<((i32, i32), wire_route::RouteDirection), i32>,
    endpoint: wire_route::RouteEndpoint,
) -> i32 {
    let slot = slots
        .entry((endpoint.point, endpoint.corridor_dir))
        .or_insert(0);
    let extra_cells = slot
        .saturating_mul(WIRE_TAIL_STAGGER_STEP_CELLS)
        .min(WIRE_TAIL_STAGGER_MAX_EXTRA_CELLS);
    *slot = slot.saturating_add(1);
    wire_route::DEFAULT_ENDPOINT_TAIL_CELLS + extra_cells
}

fn segment_length(from: (i32, i32), to: (i32, i32)) -> f32 {
    let dx = (to.0 - from.0) as f32;
    let dy = (to.1 - from.1) as f32;
    (dx * dx + dy * dy).sqrt()
}

fn segment_bounds(segment: DrawnWireSegment) -> (i32, i32, i32, i32) {
    let min_x = segment.from.0.min(segment.to.0);
    let min_y = segment.from.1.min(segment.to.1);
    let max_x = segment.from.0.max(segment.to.0);
    let max_y = segment.from.1.max(segment.to.1);
    (min_x, min_y, max_x, max_y)
}

fn segment_crossing_distance(a: DrawnWireSegment, b: DrawnWireSegment) -> Option<f32> {
    let p = (a.from.0 as f32, a.from.1 as f32);
    let p2 = (a.to.0 as f32, a.to.1 as f32);
    let q = (b.from.0 as f32, b.from.1 as f32);
    let q2 = (b.to.0 as f32, b.to.1 as f32);
    let r = (p2.0 - p.0, p2.1 - p.1);
    let s = (q2.0 - q.0, q2.1 - q.1);
    let denom = r.0 * s.1 - r.1 * s.0;
    if denom.abs() <= f32::EPSILON {
        return None;
    }
    let qp = (q.0 - p.0, q.1 - p.1);
    let t = (qp.0 * s.1 - qp.1 * s.0) / denom;
    let u = (qp.0 * r.1 - qp.1 * r.0) / denom;
    const ENDPOINT_EPS: f32 = 0.001;
    if t <= ENDPOINT_EPS || t >= 1.0 - ENDPOINT_EPS || u <= ENDPOINT_EPS || u >= 1.0 - ENDPOINT_EPS
    {
        return None;
    }
    Some(segment_length(a.from, a.to) * t)
}

pub(super) fn bridge_distance_allowed(
    segment_index: usize,
    total_segments: usize,
    segment_len: f32,
    distance: f32,
    bridge_scale: f32,
) -> bool {
    let endpoint_radius = (WIRE_ENDPOINT_RADIUS_PX as f32 * bridge_scale).max(1.0);
    let mut min_distance = endpoint_radius + 1.0;
    let mut max_distance = (segment_len - (endpoint_radius + 1.0)).max(0.0);
    if segment_index > 0 {
        min_distance = min_distance.max(WIRE_BRIDGE_CORNER_GUARD_PX * bridge_scale);
    }
    if segment_index + 1 < total_segments {
        max_distance = max_distance.min(segment_len - WIRE_BRIDGE_CORNER_GUARD_PX * bridge_scale);
    }
    if max_distance <= min_distance {
        return false;
    }
    distance > min_distance && distance < max_distance
}

pub(super) fn cluster_bridge_ranges_into(
    crossings: &[f32],
    segment_len: f32,
    bridge_scale: f32,
    out: &mut Vec<(f32, f32)>,
) {
    out.clear();
    if crossings.is_empty() {
        return;
    }
    let half_span = WIRE_BRIDGE_SPAN_PX * 0.5 * bridge_scale;
    let mut start = (crossings[0] - half_span).max(0.0);
    let mut end = (crossings[0] + half_span).min(segment_len);
    for &distance in crossings.iter().skip(1) {
        let next_start = (distance - half_span).max(0.0);
        let next_end = (distance + half_span).min(segment_len);
        if next_start <= end + WIRE_BRIDGE_LINK_THRESHOLD_PX * bridge_scale {
            end = end.max(next_end);
        } else {
            if end > start {
                out.push((start, end));
            }
            start = next_start;
            end = next_end;
        }
    }
    if end > start {
        out.push((start, end));
    }
}

pub(super) fn bridged_segment_points_into(
    segment: DrawnWireSegment,
    bridges: &[(f32, f32)],
    bridge_scale: f32,
    out: &mut Vec<(i32, i32)>,
) {
    out.clear();
    let segment_len = segment_length(segment.from, segment.to);
    if segment_len <= 0.0 {
        out.push(segment.from);
        return;
    }
    if bridges.is_empty() {
        out.push(segment.from);
        out.push(segment.to);
        return;
    }
    out.push(segment.from);
    let mut cursor = 0.0_f32;
    for &(start, end) in bridges {
        let start = start.clamp(0.0, segment_len);
        let end = end.clamp(0.0, segment_len);
        if end <= start {
            continue;
        }
        if start > cursor {
            let point = point_along_segment(segment, start);
            if out.last().copied() != Some(point) {
                out.push(point);
            }
        }
        for step in 1..=WIRE_BRIDGE_STEPS {
            let t = step as f32 / (WIRE_BRIDGE_STEPS as f32 + 1.0);
            let distance = start + (end - start) * t;
            let lift = (std::f32::consts::PI * t).sin() * WIRE_BRIDGE_HEIGHT_PX * bridge_scale;
            let point = offset_point_from_segment(segment, distance, lift);
            if out.last().copied() != Some(point) {
                out.push(point);
            }
        }
        let exit = point_along_segment(segment, end);
        if out.last().copied() != Some(exit) {
            out.push(exit);
        }
        cursor = end;
    }
    if out.last().copied() != Some(segment.to) {
        out.push(segment.to);
    }
    dedupe_adjacent_points(out);
}

fn point_along_segment(segment: DrawnWireSegment, distance: f32) -> (i32, i32) {
    offset_point_from_segment(segment, distance, 0.0)
}

fn offset_point_from_segment(
    segment: DrawnWireSegment,
    distance: f32,
    normal_offset: f32,
) -> (i32, i32) {
    let dx = (segment.to.0 - segment.from.0) as f32;
    let dy = (segment.to.1 - segment.from.1) as f32;
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let tx = dx / len;
    let ty = dy / len;
    let nx = -ty;
    let ny = tx;
    let x = segment.from.0 as f32 + tx * distance + nx * normal_offset;
    let y = segment.from.1 as f32 + ty * distance + ny * normal_offset;
    (x.round() as i32, y.round() as i32)
}

fn rounded_corner_radius(ax: i32, ay: i32, bx: i32, by: i32, cx: i32, cy: i32) -> i32 {
    let in_len = (bx - ax).abs() + (by - ay).abs();
    let out_len = (cx - bx).abs() + (cy - by).abs();
    if in_len < 2 || out_len < 2 {
        return 0;
    }
    (in_len.min(out_len) / 2).clamp(2, 12)
}

fn axis_segment_len(a: (i32, i32), b: (i32, i32)) -> Option<i32> {
    if a.0 == b.0 {
        Some((a.1 - b.1).abs())
    } else if a.1 == b.1 {
        Some((a.0 - b.0).abs())
    } else {
        None
    }
}

fn is_orthogonal_turn(prev: (i32, i32), corner: (i32, i32), next: (i32, i32)) -> bool {
    let incoming_horizontal = prev.1 == corner.1;
    let outgoing_horizontal = next.1 == corner.1;
    incoming_horizontal != outgoing_horizontal
}

/// Return fixed pin-tail and route-anchor points for one parameter wire.
fn param_wire_anchors(from_x: i32, from_y: i32, to_x: i32, to_y: i32) -> ParamWireAnchors {
    let source_exit = (from_x.saturating_add(PARAM_WIRE_EXIT_TAIL_PX), from_y);
    let route_start = (
        source_exit.0.saturating_add(PARAM_WIRE_ROUTE_LEAD_PX),
        from_y,
    );
    let target_entry = (to_x.saturating_add(PARAM_WIRE_ENTRY_TAIL_PX), to_y);
    let route_end = (
        target_entry.0.saturating_add(PARAM_WIRE_ROUTE_LEAD_PX),
        to_y,
    );
    ParamWireAnchors {
        source_exit,
        route_start,
        route_end,
        target_entry,
    }
}

/// Build one smoothed parameter wire polyline with guaranteed straight pin tails.
pub(super) fn build_smoothed_param_wire(
    start: (i32, i32),
    route: &[(i32, i32)],
    end: (i32, i32),
) -> Vec<(i32, i32)> {
    let anchors = param_wire_anchors(start.0, start.1, end.0, end.1);
    let mut full = Vec::with_capacity(route.len().saturating_add(6));
    full.push(start);
    full.push(anchors.source_exit);
    full.push(anchors.route_start);
    if !route.is_empty() {
        full.extend(route.iter().copied());
    } else {
        full.push(anchors.route_end);
    }
    if full.last().copied() != Some(anchors.route_end) {
        full.push(anchors.route_end);
    }
    full.push(anchors.target_entry);
    full.push(end);
    dedupe_adjacent_points(&mut full);
    smooth_param_wire_path(full.as_slice())
}

/// Backward-compatible wrapper for call sites that provide route-only points.
pub(super) fn smooth_param_wire_path_with_end_caps(
    start: (i32, i32),
    route: &[(i32, i32)],
    end: (i32, i32),
) -> Vec<(i32, i32)> {
    build_smoothed_param_wire(start, route, end)
}

pub(super) fn smooth_param_wire_path(points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * 2);
    out.push(points[0]);
    for index in 1..points.len() - 1 {
        let prev = points[index - 1];
        let corner = points[index];
        let next = points[index + 1];
        let Some(in_len) = axis_segment_len(prev, corner) else {
            out.push(corner);
            continue;
        };
        let Some(out_len) = axis_segment_len(corner, next) else {
            out.push(corner);
            continue;
        };
        if !is_orthogonal_turn(prev, corner, next) {
            out.push(corner);
            continue;
        }
        let target_radius =
            rounded_corner_radius(prev.0, prev.1, corner.0, corner.1, next.0, next.1).clamp(
                PARAM_WIRE_CORNER_RADIUS_MIN_PX,
                PARAM_WIRE_CORNER_RADIUS_MAX_PX,
            );
        let mut local_max = in_len.min(out_len).saturating_sub(1);
        if index == 1 {
            local_max = local_max.min(in_len.saturating_sub(PARAM_WIRE_ENDPOINT_STRAIGHT_PX));
        }
        if index + 2 == points.len() {
            local_max = local_max.min(out_len.saturating_sub(PARAM_WIRE_ENDPOINT_STRAIGHT_PX));
        }
        let radius = target_radius.min(local_max);
        if radius <= 0 {
            out.push(corner);
            continue;
        }
        let entry = step_towards_point(corner, prev, radius);
        let exit = step_towards_point(corner, next, radius);
        if out.last().copied() != Some(entry) {
            out.push(entry);
        }
        for step in 1..=PARAM_WIRE_CURVE_STEPS {
            let t = step as f32 / (PARAM_WIRE_CURVE_STEPS as f32 + 1.0);
            let curve_point = quadratic_wire_point(entry, corner, exit, t);
            if out.last().copied() != Some(curve_point) {
                out.push(curve_point);
            }
        }
        if out.last().copied() != Some(exit) {
            out.push(exit);
        }
    }
    if let Some(end) = points.last().copied() {
        if out.last().copied() != Some(end) {
            out.push(end);
        }
    }
    out
}

fn dedupe_adjacent_points(points: &mut Vec<(i32, i32)>) {
    if points.len() < 2 {
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

fn step_towards_point(from: (i32, i32), to: (i32, i32), distance: i32) -> (i32, i32) {
    let dx = (to.0 - from.0).signum();
    let dy = (to.1 - from.1).signum();
    (
        from.0.saturating_add(dx.saturating_mul(distance)),
        from.1.saturating_add(dy.saturating_mul(distance)),
    )
}

fn quadratic_wire_point(a: (i32, i32), b: (i32, i32), c: (i32, i32), t: f32) -> (i32, i32) {
    let one = 1.0 - t;
    let x = one * one * a.0 as f32 + 2.0 * one * t * b.0 as f32 + t * t * c.0 as f32;
    let y = one * one * a.1 as f32 + 2.0 * one * t * b.1 as f32 + t * t * c.1 as f32;
    (x.round() as i32, y.round() as i32)
}

fn edge_intersects_cut_line(state: &PreviewState, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    let Some(cut) = state.link_cut else {
        return false;
    };
    geometry_segments_intersect(
        (cut.start_x, cut.start_y),
        (cut.cursor_x, cut.cursor_y),
        (x0, y0),
        (x1, y1),
    )
}

pub(super) fn path_intersects_cut_line(state: &PreviewState, points: &[(i32, i32)]) -> bool {
    if points.len() < 2 {
        return false;
    }
    for segment in points.windows(2) {
        if edge_intersects_cut_line(
            state,
            segment[0].0,
            segment[0].1,
            segment[1].0,
            segment[1].1,
        ) {
            return true;
        }
    }
    false
}

pub(super) fn segment_crossings(
    segment: DrawnWireSegment,
    drawn_segments: &[DrawnWireSegment],
    candidate_indices: &[usize],
    total_segments: usize,
    segment_index: usize,
    bridge_scale: f32,
    crossings_out: &mut Vec<f32>,
) {
    crossings_out.clear();
    let segment_len = segment_length(segment.from, segment.to);
    if segment_len <= 0.0 {
        return;
    }
    for &candidate in candidate_indices {
        let Some(previous) = drawn_segments.get(candidate).copied() else {
            continue;
        };
        let Some(distance) = segment_crossing_distance(segment, previous) else {
            continue;
        };
        if bridge_distance_allowed(
            segment_index,
            total_segments,
            segment_len,
            distance,
            bridge_scale,
        ) {
            crossings_out.push(distance);
        }
    }
}

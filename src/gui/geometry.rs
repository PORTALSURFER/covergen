//! Shared integer geometry utilities for GUI layout and hit testing.

/// Integer rectangle used by editor layout and input hit detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
}

impl Rect {
    /// Construct one rectangle from integer position and size.
    pub(crate) const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// Return true when `(px, py)` lies inside this rectangle.
    pub(crate) const fn contains(self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Transform one graph-space point into panel space using zoom + pan.
pub(crate) fn graph_point_to_panel(
    point: (i32, i32),
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
) -> (i32, i32) {
    let sx = (point.0 as f32 * zoom + pan_x).round() as i32;
    let sy = (point.1 as f32 * zoom + pan_y).round() as i32;
    (sx, sy)
}

/// Transform one graph-space rectangle into panel space using zoom + pan.
pub(crate) fn graph_rect_to_panel(rect: Rect, zoom: f32, pan_x: f32, pan_y: f32) -> Rect {
    let (x, y) = graph_point_to_panel((rect.x, rect.y), zoom, pan_x, pan_y);
    let w = (rect.w as f32 * zoom).round().max(1.0) as i32;
    let h = (rect.h as f32 * zoom).round().max(1.0) as i32;
    Rect::new(x, y, w, h)
}

/// Transform one panel-space point into graph space using zoom + pan.
pub(crate) fn screen_point_to_graph(
    point: (i32, i32),
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
) -> (i32, i32) {
    let zoom = zoom.max(0.001);
    let gx = ((point.0 as f32 - pan_x) / zoom).round() as i32;
    let gy = ((point.1 as f32 - pan_y) / zoom).round() as i32;
    (gx, gy)
}

/// Map one graph-space polyline into panel space in-place.
pub(crate) fn map_graph_path_to_panel_into(
    points: &[(i32, i32)],
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    out: &mut Vec<(i32, i32)>,
) {
    out.clear();
    out.extend(
        points
            .iter()
            .copied()
            .map(|point| graph_point_to_panel(point, zoom, pan_x, pan_y)),
    );
}

/// Return true when line segments `ab` and `cd` intersect.
pub(crate) fn segments_intersect(
    a: (i32, i32),
    b: (i32, i32),
    c: (i32, i32),
    d: (i32, i32),
) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    if o1 == 0 && on_segment(a, b, c) {
        return true;
    }
    if o2 == 0 && on_segment(a, b, d) {
        return true;
    }
    if o3 == 0 && on_segment(c, d, a) {
        return true;
    }
    if o4 == 0 && on_segment(c, d, b) {
        return true;
    }
    (o1 > 0) != (o2 > 0) && (o3 > 0) != (o4 > 0)
}

fn orient(a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> i64 {
    let abx = (b.0 - a.0) as i64;
    let aby = (b.1 - a.1) as i64;
    let acx = (c.0 - a.0) as i64;
    let acy = (c.1 - a.1) as i64;
    abx * acy - aby * acx
}

fn on_segment(a: (i32, i32), b: (i32, i32), p: (i32, i32)) -> bool {
    p.0 >= a.0.min(b.0) && p.0 <= a.0.max(b.0) && p.1 >= a.1.min(b.1) && p.1 <= a.1.max(b.1)
}

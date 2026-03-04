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

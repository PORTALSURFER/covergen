//! Scene graph/panel layout conversion helpers.

use crate::gui::geometry::Rect;
use crate::gui::project::{ProjectNode, NODE_WIDTH};
use crate::gui::state::{PreviewState, RightMarqueeState};

const WIRE_LAYOUT_BASE_ZOOM: f32 = 0.35;

/// Return one node card rect transformed from graph to panel space.
pub(super) fn node_rect(node: &ProjectNode, state: &PreviewState) -> Rect {
    graph_rect_to_panel(
        Rect::new(node.x(), node.y(), NODE_WIDTH, node.card_height()),
        state,
    )
}

/// Return one graph-space rectangle transformed to panel space.
pub(super) fn graph_rect_to_panel(rect: Rect, state: &PreviewState) -> Rect {
    let x = (rect.x as f32 * state.zoom + state.pan_x).round() as i32;
    let y = (rect.y as f32 * state.zoom + state.pan_y).round() as i32;
    let w = (rect.w as f32 * state.zoom).round().max(1.0) as i32;
    let h = (rect.h as f32 * state.zoom).round().max(1.0) as i32;
    Rect::new(x, y, w, h)
}

/// Return one graph-space point transformed to panel space.
pub(super) fn graph_point_to_panel(x: i32, y: i32, state: &PreviewState) -> (i32, i32) {
    let sx = (x as f32 * state.zoom + state.pan_x).round() as i32;
    let sy = (y as f32 * state.zoom + state.pan_y).round() as i32;
    (sx, sy)
}

/// Return scale multiplier used by zoom-normalized wire layout helpers.
pub(super) fn wire_layout_scale(zoom: f32) -> f32 {
    (zoom / WIRE_LAYOUT_BASE_ZOOM).max(0.001)
}

/// Map one graph-space polyline to panel-space points.
pub(super) fn map_graph_path_to_panel_into(
    points: &[(i32, i32)],
    state: &PreviewState,
    panel_points: &mut Vec<(i32, i32)>,
) {
    panel_points.clear();
    panel_points.extend(
        points
            .iter()
            .copied()
            .map(|(x, y)| graph_point_to_panel(x, y, state)),
    );
}

/// Return marquee selection rect in panel space once drag exceeds threshold.
pub(super) fn marquee_panel_rect(marquee: RightMarqueeState) -> Option<Rect> {
    let x0 = marquee.start_x.min(marquee.cursor_x);
    let y0 = marquee.start_y.min(marquee.cursor_y);
    let x1 = marquee.start_x.max(marquee.cursor_x);
    let y1 = marquee.start_y.max(marquee.cursor_y);
    let w = x1 - x0;
    let h = y1 - y0;
    if w <= 4 || h <= 4 {
        return None;
    }
    Some(Rect::new(x0, y0, w, h))
}

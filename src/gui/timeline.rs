//! Timeline layout and frame-clock helpers for the graph editor.

use super::geometry::Rect;

/// First frame index on the timeline.
pub(crate) const TIMELINE_START_FRAME: u32 = 0;
/// Total frame count represented by the timeline.
pub(crate) const TIMELINE_TOTAL_FRAMES: u32 = 1_800;
/// Last frame index on the timeline (inclusive).
pub(crate) const TIMELINE_END_FRAME: u32 = TIMELINE_START_FRAME + TIMELINE_TOTAL_FRAMES - 1;
/// Fixed timeline strip height in panel pixels.
pub(crate) const TIMELINE_HEIGHT_PX: i32 = 30;

const TIMELINE_PAD_X: i32 = 6;
const TIMELINE_PAD_Y: i32 = 4;
const TRANSPORT_BTN_W: i32 = 20;
const TRANSPORT_GAP: i32 = 6;
const TRACK_LEFT_GAP: i32 = 12;
const TRACK_RIGHT_PAD: i32 = 12;
const TRACK_HEIGHT: i32 = 6;

/// Return panel height available for content above the timeline strip.
pub(crate) fn editor_panel_height(panel_height: usize) -> usize {
    panel_height.saturating_sub(TIMELINE_HEIGHT_PX.max(0) as usize)
}

/// Return bottom timeline rectangle across the full root viewport.
pub(crate) fn timeline_rect(viewport_width: usize, panel_height: usize) -> Rect {
    let height = TIMELINE_HEIGHT_PX.min(panel_height as i32).max(1);
    Rect::new(
        0,
        panel_height as i32 - height,
        viewport_width as i32,
        height.max(1),
    )
}

/// Return play-button rectangle within the timeline.
pub(crate) fn play_button_rect(timeline: Rect) -> Rect {
    Rect::new(
        timeline.x + TIMELINE_PAD_X,
        timeline.y + TIMELINE_PAD_Y,
        TRANSPORT_BTN_W,
        (timeline.h - TIMELINE_PAD_Y * 2).max(14),
    )
}

/// Return pause-button rectangle within the timeline.
pub(crate) fn pause_button_rect(timeline: Rect) -> Rect {
    let play = play_button_rect(timeline);
    Rect::new(
        play.x + play.w + TRANSPORT_GAP,
        play.y,
        TRANSPORT_BTN_W,
        play.h,
    )
}

/// Return scrub track rectangle within the timeline.
pub(crate) fn track_rect(timeline: Rect) -> Rect {
    let pause = pause_button_rect(timeline);
    let x = pause.x + pause.w + TRACK_LEFT_GAP;
    let y = timeline.y + (timeline.h - TRACK_HEIGHT) / 2;
    let w = (timeline.x + timeline.w - TRACK_RIGHT_PAD - x).max(24);
    Rect::new(x, y, w, TRACK_HEIGHT)
}

/// Clamp one frame to timeline bounds.
pub(crate) fn clamp_frame(frame: u32) -> u32 {
    frame.clamp(TIMELINE_START_FRAME, TIMELINE_END_FRAME)
}

/// Advance one timeline frame with loop wrap at the end.
pub(crate) fn next_looped_frame(frame: u32) -> u32 {
    if frame >= TIMELINE_END_FRAME {
        TIMELINE_START_FRAME
    } else {
        frame + 1
    }
}

/// Convert one x-position on the track to the nearest timeline frame.
pub(crate) fn frame_from_track_x(track: Rect, x: i32) -> u32 {
    if track.w <= 1 {
        return TIMELINE_START_FRAME;
    }
    let clamped_x = x.clamp(track.x, track.x + track.w - 1);
    let t = (clamped_x - track.x) as f32 / (track.w - 1) as f32;
    let range = (TIMELINE_END_FRAME - TIMELINE_START_FRAME) as f32;
    clamp_frame((TIMELINE_START_FRAME as f32 + t * range).round() as u32)
}

/// Convert one frame index to an x-position on the track.
pub(crate) fn track_x_for_frame(track: Rect, frame: u32) -> i32 {
    if track.w <= 1 {
        return track.x;
    }
    let frame = clamp_frame(frame);
    let range = (TIMELINE_END_FRAME - TIMELINE_START_FRAME).max(1) as f32;
    let t = (frame - TIMELINE_START_FRAME) as f32 / range;
    track.x + (t * (track.w - 1) as f32).round() as i32
}

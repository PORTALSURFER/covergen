//! Timeline layout and frame-clock helpers for the graph editor.

use super::geometry::Rect;

/// First frame index on the timeline.
pub(crate) const TIMELINE_START_FRAME: u32 = 0;
/// Default timeline frame count (30s at 60fps).
pub(crate) const TIMELINE_DEFAULT_TOTAL_FRAMES: u32 = 1_800;
/// Fixed timeline strip height in panel pixels.
pub(crate) const TIMELINE_HEIGHT_PX: i32 = 60;

const TIMELINE_PAD_X: i32 = 8;
const TIMELINE_PAD_Y: i32 = 6;
const TRANSPORT_BTN_W: i32 = 20;
const TRANSPORT_GAP: i32 = 6;
const TRACK_LEFT_GAP: i32 = 12;
const TRACK_RIGHT_PAD: i32 = 8;
const TRACK_HEIGHT: i32 = 8;

const CONTROL_GAP: i32 = 10;
const BPM_BTN_W: i32 = 18;
const BPM_VALUE_W: i32 = 72;
const VOLUME_W_MIN: i32 = 48;
const VOLUME_W_TARGET: i32 = 136;
const WAV_W_MIN: i32 = 64;

/// Timeline control-row layout for audio/BPM widgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TimelineControlLayout {
    pub(crate) wav_drop: Rect,
    pub(crate) volume_slider: Rect,
    pub(crate) bpm_down: Rect,
    pub(crate) bpm_value: Rect,
    pub(crate) bpm_up: Rect,
}

fn top_row_rect(timeline: Rect) -> Rect {
    let y = timeline.y + TIMELINE_PAD_Y;
    let h = ((timeline.h - TIMELINE_PAD_Y * 3) / 2).max(14);
    Rect::new(
        timeline.x + TIMELINE_PAD_X,
        y,
        (timeline.w - TIMELINE_PAD_X * 2).max(1),
        h,
    )
}

fn control_row_rect(timeline: Rect) -> Rect {
    let top = top_row_rect(timeline);
    let y = top.y + top.h + TIMELINE_PAD_Y;
    let h = (timeline.y + timeline.h - TIMELINE_PAD_Y - y).max(14);
    Rect::new(
        timeline.x + TIMELINE_PAD_X,
        y,
        (timeline.w - TIMELINE_PAD_X * 2).max(1),
        h,
    )
}

/// Return last frame index for one timeline length.
pub(crate) fn end_frame(total_frames: u32) -> u32 {
    TIMELINE_START_FRAME + total_frames.max(1) - 1
}

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
    let top = top_row_rect(timeline);
    Rect::new(top.x, top.y, TRANSPORT_BTN_W, top.h)
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
    let top = top_row_rect(timeline);
    let pause = pause_button_rect(timeline);
    let x = pause.x + pause.w + TRACK_LEFT_GAP;
    let y = top.y + (top.h - TRACK_HEIGHT) / 2;
    let w = (top.x + top.w - TRACK_RIGHT_PAD - x).max(24);
    Rect::new(x, y, w, TRACK_HEIGHT)
}

/// Return timeline control widget rectangles.
pub(crate) fn timeline_control_layout(timeline: Rect) -> TimelineControlLayout {
    let row = control_row_rect(timeline);
    let bpm_up_x = row.x + row.w - BPM_BTN_W;
    let bpm_value_x = bpm_up_x - 4 - BPM_VALUE_W;
    let bpm_down_x = bpm_value_x - 4 - BPM_BTN_W;
    let bpm_up = Rect::new(bpm_up_x, row.y, BPM_BTN_W, row.h);
    let bpm_value = Rect::new(bpm_value_x, row.y, BPM_VALUE_W, row.h);
    let bpm_down = Rect::new(bpm_down_x, row.y, BPM_BTN_W, row.h);

    let left_available = (bpm_down.x - CONTROL_GAP - row.x).max(WAV_W_MIN + VOLUME_W_MIN);
    let volume_w = VOLUME_W_TARGET
        .min((left_available - CONTROL_GAP - WAV_W_MIN).max(VOLUME_W_MIN))
        .max(VOLUME_W_MIN);
    let wav_w = (left_available - volume_w - CONTROL_GAP).max(WAV_W_MIN);
    let wav_drop = Rect::new(row.x, row.y, wav_w, row.h);
    let volume_slider = Rect::new(
        wav_drop.x + wav_drop.w + CONTROL_GAP,
        row.y,
        volume_w,
        row.h,
    );

    TimelineControlLayout {
        wav_drop,
        volume_slider,
        bpm_down,
        bpm_value,
        bpm_up,
    }
}

/// Clamp one frame to timeline bounds.
pub(crate) fn clamp_frame(frame: u32, total_frames: u32) -> u32 {
    frame.clamp(TIMELINE_START_FRAME, end_frame(total_frames))
}

/// Advance one timeline frame with loop wrap at the end.
pub(crate) fn next_looped_frame(frame: u32, total_frames: u32) -> u32 {
    let end = end_frame(total_frames);
    if frame >= end {
        TIMELINE_START_FRAME
    } else {
        frame + 1
    }
}

/// Convert one x-position on the track to the nearest timeline frame.
pub(crate) fn frame_from_track_x(track: Rect, x: i32, total_frames: u32) -> u32 {
    if track.w <= 1 {
        return TIMELINE_START_FRAME;
    }
    let clamped_x = x.clamp(track.x, track.x + track.w - 1);
    let t = (clamped_x - track.x) as f32 / (track.w - 1) as f32;
    let range = (end_frame(total_frames) - TIMELINE_START_FRAME) as f32;
    clamp_frame(
        (TIMELINE_START_FRAME as f32 + t * range).round() as u32,
        total_frames,
    )
}

/// Convert one frame index to an x-position on the track.
pub(crate) fn track_x_for_frame(track: Rect, frame: u32, total_frames: u32) -> i32 {
    if track.w <= 1 {
        return track.x;
    }
    let frame = clamp_frame(frame, total_frames);
    let range = (end_frame(total_frames) - TIMELINE_START_FRAME).max(1) as f32;
    let t = (frame - TIMELINE_START_FRAME) as f32 / range;
    track.x + (t * (track.w - 1) as f32).round() as i32
}

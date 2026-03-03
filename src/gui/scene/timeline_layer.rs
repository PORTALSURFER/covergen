//! Timeline layer geometry composition for [`SceneBuilder`].

use super::timeline_helpers::timeline_beat_indicator_on;
use super::*;
use crate::gui::state::TimelineBpmEditState;
use crate::gui::timeline::{
    end_frame, pause_button_rect, play_button_rect, timeline_control_layout, timeline_rect,
    track_rect, track_x_for_frame, TIMELINE_START_FRAME,
};
use std::fmt::Write as _;
use std::path::Path;

pub(super) fn push_timeline(
    scene: &mut SceneBuilder,
    state: &PreviewState,
    viewport_width: usize,
    panel_height: usize,
    timeline_fps: u32,
) {
    if viewport_width == 0 || panel_height == 0 {
        return;
    }
    let timeline = timeline_rect(viewport_width, panel_height);
    let play_btn = play_button_rect(timeline);
    let pause_btn = pause_button_rect(timeline);
    let track = track_rect(timeline);
    let controls = timeline_control_layout(timeline);
    let total_frames = state.export_menu.timeline_total_frames(timeline_fps);
    let end_frame = end_frame(total_frames);
    scene.push_rect(timeline, TIMELINE_BG);
    scene.push_border(timeline, TIMELINE_BORDER);

    scene.push_rect(
        play_btn,
        if !state.paused {
            TIMELINE_BTN_ACTIVE
        } else {
            TIMELINE_BTN_IDLE
        },
    );
    scene.push_border(play_btn, TIMELINE_BORDER);
    let tri_x = play_btn.x + 8;
    let tri_y = play_btn.y + 5;
    scene.push_line(tri_x, tri_y, tri_x, tri_y + play_btn.h - 10, TIMELINE_TEXT);
    scene.push_line(
        tri_x,
        tri_y,
        tri_x + play_btn.w - 10,
        play_btn.y + play_btn.h / 2,
        TIMELINE_TEXT,
    );
    scene.push_line(
        tri_x + play_btn.w - 10,
        play_btn.y + play_btn.h / 2,
        tri_x,
        tri_y + play_btn.h - 10,
        TIMELINE_TEXT,
    );

    scene.push_rect(
        pause_btn,
        if state.paused {
            TIMELINE_BTN_ACTIVE
        } else {
            TIMELINE_BTN_IDLE
        },
    );
    scene.push_border(pause_btn, TIMELINE_BORDER);
    let bar_h = (pause_btn.h - 10).max(4);
    scene.push_rect(
        Rect::new(pause_btn.x + 7, pause_btn.y + 5, 3, bar_h),
        TIMELINE_TEXT,
    );
    scene.push_rect(
        Rect::new(pause_btn.x + pause_btn.w - 10, pause_btn.y + 5, 3, bar_h),
        TIMELINE_TEXT,
    );

    scene.push_rect(track, TIMELINE_TRACK_BG);
    scene.push_border(track, TIMELINE_BORDER);
    let thumb_x = track_x_for_frame(track, state.frame_index, total_frames);
    let fill_w = (thumb_x - track.x + 1).max(1).min(track.w);
    scene.push_rect(
        Rect::new(track.x, track.y, fill_w, track.h),
        TIMELINE_TRACK_FILL,
    );
    scene.push_rect(
        Rect::new(thumb_x - 1, track.y - 3, 3, track.h + 6),
        TIMELINE_TEXT,
    );

    let mut label = std::mem::take(&mut scene.label_scratch);
    label.clear();
    let derived_bars = state.export_menu.derived_bars_from_audio();
    if let Some(derived_bars) = derived_bars {
        let _ = write!(
            &mut label,
            "Frame {}  [{}, {}]  |  bars {:.2} (audio)",
            state.frame_index, TIMELINE_START_FRAME, end_frame, derived_bars,
        );
    } else {
        let _ = write!(
            &mut label,
            "Frame {}  [{}, {}]  |  bars {:.2}",
            state.frame_index,
            TIMELINE_START_FRAME,
            end_frame,
            state.export_menu.parsed_bar_length(),
        );
    }
    scene.push_rect(controls.frame_status, TIMELINE_TRACK_BG);
    scene.push_border(controls.frame_status, TIMELINE_BORDER);
    scene.push_text(
        controls.frame_status.x + 4,
        controls.frame_status.y + 3,
        label.as_str(),
        TIMELINE_TEXT,
    );
    let beat_rect = controls.beat_indicator;
    if timeline_beat_indicator_on(
        state.frame_index,
        timeline_fps,
        state.export_menu.parsed_bpm(),
    ) {
        scene.push_rect(beat_rect, TIMELINE_BEAT_ON);
    } else {
        scene.push_rect(beat_rect, TIMELINE_TRACK_BG);
    }
    scene.push_border(beat_rect, TIMELINE_BORDER);

    scene.push_rect(controls.wav_drop, TIMELINE_TRACK_BG);
    scene.push_border(controls.wav_drop, TIMELINE_BORDER);
    label.clear();
    if state.export_menu.audio_wav.trim().is_empty() {
        label.push_str("Drop WAV file here");
    } else {
        let display = Path::new(state.export_menu.audio_wav.trim())
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| state.export_menu.audio_wav.trim());
        label.push_str(display);
    }
    scene.push_text(
        controls.wav_drop.x + 4,
        controls.wav_drop.y + 3,
        label.as_str(),
        TIMELINE_TEXT,
    );

    scene.push_rect(controls.volume_slider, TIMELINE_TRACK_BG);
    scene.push_border(controls.volume_slider, TIMELINE_BORDER);
    let slider_track = Rect::new(
        controls.volume_slider.x + 6,
        controls.volume_slider.y + controls.volume_slider.h - 8,
        (controls.volume_slider.w - 12).max(8),
        4,
    );
    scene.push_rect(slider_track, TIMELINE_TRACK_BG);
    scene.push_border(slider_track, TIMELINE_BORDER);
    let volume = state.export_menu.parsed_audio_volume();
    let volume_t = (volume / 2.0).clamp(0.0, 1.0);
    let fill_w = ((slider_track.w - 1) as f32 * volume_t).round() as i32 + 1;
    scene.push_rect(
        Rect::new(
            slider_track.x,
            slider_track.y,
            fill_w.clamp(1, slider_track.w),
            slider_track.h,
        ),
        TIMELINE_TRACK_FILL,
    );
    let thumb_x = slider_track.x + ((slider_track.w - 1) as f32 * volume_t).round() as i32;
    scene.push_rect(
        Rect::new(thumb_x - 1, slider_track.y - 3, 3, slider_track.h + 6),
        TIMELINE_TEXT,
    );
    label.clear();
    let _ = write!(&mut label, "VOL {:.2}", volume);
    scene.push_text(
        controls.volume_slider.x + 4,
        controls.volume_slider.y + 2,
        label.as_str(),
        TIMELINE_TEXT,
    );

    scene.push_rect(controls.bpm_down, TIMELINE_BTN_IDLE);
    scene.push_border(controls.bpm_down, TIMELINE_BORDER);
    scene.push_text(
        controls.bpm_down.x + 6,
        controls.bpm_down.y + 3,
        "-",
        TIMELINE_TEXT,
    );
    scene.push_rect(controls.bpm_value, TIMELINE_TRACK_BG);
    let bpm_edit = state.timeline_bpm_edit.as_ref();
    let bpm_text = bpm_edit
        .map(|edit| edit.buffer.as_str())
        .unwrap_or(state.export_menu.bpm.as_str());
    push_timeline_value_editor_text(scene, controls.bpm_value, bpm_text, bpm_edit, TIMELINE_TEXT);
    scene.push_border(
        controls.bpm_value,
        if bpm_edit.is_some() {
            PARAM_VALUE_ACTIVE
        } else {
            TIMELINE_BORDER
        },
    );
    scene.push_rect(controls.bpm_up, TIMELINE_BTN_IDLE);
    scene.push_border(controls.bpm_up, TIMELINE_BORDER);
    scene.push_text(
        controls.bpm_up.x + 6,
        controls.bpm_up.y + 3,
        "+",
        TIMELINE_TEXT,
    );
    let bars_overridden = derived_bars.is_some();
    let bar_edit = if bars_overridden {
        None
    } else {
        state.timeline_bar_edit.as_ref()
    };
    let mut bars_display = String::new();
    let bar_text = if let Some(edit) = bar_edit {
        edit.buffer.as_str()
    } else if let Some(derived) = derived_bars {
        let _ = write!(&mut bars_display, "{derived:.2}");
        bars_display.as_str()
    } else {
        state.export_menu.bar_length.as_str()
    };
    let bar_bg = if bars_overridden {
        TIMELINE_TRACK_BG_MUTED
    } else {
        TIMELINE_TRACK_BG
    };
    let bar_text_color = if bars_overridden {
        TIMELINE_TEXT_MUTED
    } else {
        TIMELINE_TEXT
    };
    scene.push_rect(controls.bar_value, bar_bg);
    push_timeline_value_editor_text(
        scene,
        controls.bar_value,
        bar_text,
        bar_edit,
        bar_text_color,
    );
    scene.push_border(
        controls.bar_value,
        if bar_edit.is_some() {
            PARAM_VALUE_ACTIVE
        } else {
            TIMELINE_BORDER
        },
    );
    scene.label_scratch = label;
}

fn push_timeline_value_editor_text(
    scene: &mut SceneBuilder,
    value_rect: Rect,
    text: &str,
    edit: Option<&TimelineBpmEditState>,
    color: Color,
) {
    let metrics = scene.text_renderer.metrics_scaled(1.0);
    let text_x = value_rect.x + 4;
    let text_y = value_rect.y + ((value_rect.h - metrics.line_height_px).max(0) / 2);
    if let Some(edit_state) = edit {
        let mut cursor = edit_state.cursor.min(text.len());
        let mut anchor = edit_state.anchor.min(text.len());
        if anchor > cursor {
            std::mem::swap(&mut anchor, &mut cursor);
        }
        if anchor != cursor {
            let start_w = scene.text_renderer.cursor_offset(text, anchor, 1.0);
            let end_w = scene.text_renderer.cursor_offset(text, cursor, 1.0);
            let highlight_x = text_x + start_w;
            let highlight_w = (end_w - start_w).max(1);
            let left = highlight_x.max(value_rect.x + 1);
            let right = (highlight_x + highlight_w).min(value_rect.x + value_rect.w - 1);
            let clamped = Rect::new(left, text_y, right - left, metrics.line_height_px.max(1));
            if clamped.w > 0 && clamped.h > 0 {
                scene.push_rect(clamped, PARAM_VALUE_SELECTION);
            }
        }
    }
    scene.push_text(text_x, text_y, text, color);
    if let Some(edit_state) = edit {
        let caret_index = edit_state.cursor.min(text.len());
        let caret_x = text_x + scene.text_renderer.cursor_offset(text, caret_index, 1.0);
        let caret_top = text_y;
        let caret_bottom = text_y + metrics.line_height_px.max(1) - 1;
        scene.push_line(caret_x, caret_top, caret_x, caret_bottom, PARAM_VALUE_CARET);
    }
}

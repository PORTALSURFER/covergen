//! Timeline transport and text-edit input handling.

use crate::gui::geometry::Rect;
use crate::gui::state::{InputSnapshot, ParamEditState, PreviewState, TimelineBpmEditState};
use crate::gui::timeline::{
    frame_from_track_x, pause_button_rect, play_button_rect, timeline_control_layout,
    timeline_rect, track_rect,
};

use super::param_edit;

/// Handle timeline controls, scrubbing, and timeline text-edit fields.
pub(super) fn handle_timeline_input(
    input: &InputSnapshot,
    viewport_width: usize,
    panel_height: usize,
    timeline_fps: u32,
    state: &mut PreviewState,
) -> (bool, bool) {
    let mut changed = apply_timeline_bpm_text_edits(input, state);
    changed |= apply_timeline_bar_text_edits(input, state);
    let mut consumed = false;
    let timeline = timeline_rect(viewport_width, panel_height);
    let play = play_button_rect(timeline);
    let pause = pause_button_rect(timeline);
    let track = track_rect(timeline);
    let controls = timeline_control_layout(timeline);
    let total_frames = state.export_menu.timeline_total_frames(timeline_fps);
    let mouse_pos = input.mouse_pos;
    if !input.left_down && (state.timeline_scrub_active || state.timeline_volume_drag_active) {
        state.timeline_scrub_active = false;
        state.timeline_volume_drag_active = false;
        return (changed, true);
    }
    if let Some((mx, my)) = mouse_pos {
        if input.left_clicked && controls.bpm_value.contains(mx, my) {
            changed |= start_timeline_bpm_edit(state);
            return (changed, true);
        }
        if input.left_clicked
            && controls.bar_value.contains(mx, my)
            && !state.export_menu.bar_length_overridden_by_audio()
        {
            changed |= start_timeline_bar_edit(state);
            return (changed, true);
        }
        if input.left_clicked
            && (state.timeline_bpm_edit.is_some() || state.timeline_bar_edit.is_some())
        {
            changed |= finish_timeline_bpm_edit(state);
            changed |= finish_timeline_bar_edit(state);
            consumed = true;
        }
        if input.left_clicked && play.contains(mx, my) {
            state.paused = false;
            state.timeline_scrub_active = false;
            state.timeline_volume_drag_active = false;
            return (true, true);
        }
        if input.left_clicked && pause.contains(mx, my) {
            state.paused = true;
            state.timeline_scrub_active = false;
            state.timeline_volume_drag_active = false;
            return (true, true);
        }
        if input.left_clicked && controls.bpm_down.contains(mx, my) {
            changed |= adjust_timeline_bpm(state, -1.0);
            return (changed, true);
        }
        if input.left_clicked && controls.bpm_up.contains(mx, my) {
            changed |= adjust_timeline_bpm(state, 1.0);
            return (changed, true);
        }
        if controls.bpm_value.contains(mx, my) && input.wheel_lines_y.abs() > f32::EPSILON {
            if state.timeline_bpm_edit.is_some() {
                changed |= finish_timeline_bpm_edit(state);
            }
            if state.timeline_bar_edit.is_some() {
                changed |= finish_timeline_bar_edit(state);
            }
            changed |= adjust_timeline_bpm(state, input.wheel_lines_y.signum());
            return (changed, true);
        }
        if input.left_clicked && controls.volume_slider.contains(mx, my) {
            state.timeline_volume_drag_active = true;
            changed |= set_timeline_volume_from_slider_x(state, controls.volume_slider, mx);
            consumed = true;
        } else if state.timeline_volume_drag_active && input.left_down {
            changed |= set_timeline_volume_from_slider_x(state, controls.volume_slider, mx);
            consumed = true;
        }
        if input.left_clicked && track.contains(mx, my) {
            state.timeline_scrub_active = true;
            state.timeline_volume_drag_active = false;
            consumed = true;
            changed |= scrub_frame_from_timeline(track, mx, total_frames, state);
        } else if state.timeline_scrub_active && input.left_down {
            consumed = true;
            changed |= scrub_frame_from_timeline(track, mx, total_frames, state);
        }
        if input.left_clicked && timeline.contains(mx, my) {
            consumed = true;
        }
    } else if state.timeline_scrub_active || state.timeline_volume_drag_active {
        consumed = true;
    }
    (changed, consumed)
}

fn start_timeline_bpm_edit(state: &mut PreviewState) -> bool {
    let Some(active) = state.timeline_bpm_edit.as_mut() else {
        let cursor = state.export_menu.bpm.len();
        state.timeline_bpm_edit = Some(TimelineBpmEditState {
            buffer: state.export_menu.bpm.clone(),
            cursor,
            anchor: 0,
        });
        return true;
    };
    let end = active.buffer.len();
    if active.cursor == end && active.anchor == end {
        return false;
    }
    active.cursor = end;
    active.anchor = end;
    true
}

fn finish_timeline_bpm_edit(state: &mut PreviewState) -> bool {
    let Some(edit) = state.timeline_bpm_edit.take() else {
        return false;
    };
    let _ = commit_timeline_bpm_buffer(state, edit.buffer.as_str());
    true
}

fn start_timeline_bar_edit(state: &mut PreviewState) -> bool {
    if state.export_menu.bar_length_overridden_by_audio() {
        state.timeline_bar_edit = None;
        return false;
    }
    let Some(active) = state.timeline_bar_edit.as_mut() else {
        let cursor = state.export_menu.bar_length.len();
        state.timeline_bar_edit = Some(TimelineBpmEditState {
            buffer: state.export_menu.bar_length.clone(),
            cursor,
            anchor: 0,
        });
        return true;
    };
    let end = active.buffer.len();
    if active.cursor == end && active.anchor == end {
        return false;
    }
    active.cursor = end;
    active.anchor = end;
    true
}

fn finish_timeline_bar_edit(state: &mut PreviewState) -> bool {
    let Some(edit) = state.timeline_bar_edit.take() else {
        return false;
    };
    let _ = commit_timeline_bar_length_buffer(state, edit.buffer.as_str());
    true
}

fn apply_timeline_bpm_text_edits(input: &InputSnapshot, state: &mut PreviewState) -> bool {
    let Some(edit) = state.timeline_bpm_edit.take() else {
        return false;
    };
    let mut draft = timeline_bpm_edit_to_param(edit);
    let mut changed = false;
    if input.param_cancel {
        return true;
    }
    if input.param_select_all {
        changed |= param_edit::select_all_param_text(&mut draft);
    }
    if input.param_dec {
        changed |= param_edit::move_param_cursor_left(&mut draft, input.shift_down);
    }
    if input.param_inc {
        changed |= param_edit::move_param_cursor_right(&mut draft, input.shift_down);
    }
    if input.param_backspace {
        changed |= param_edit::backspace_param_text(&mut draft);
    }
    if input.param_delete {
        changed |= param_edit::delete_param_text(&mut draft);
    }
    if !input.typed_text.is_empty() {
        for ch in input.typed_text.chars() {
            if param_edit::insert_param_char(&mut draft, ch) {
                changed = true;
            }
        }
    }
    if input.param_commit && commit_timeline_bpm_buffer(state, draft.buffer.as_str()) {
        return true;
    }
    state.timeline_bpm_edit = Some(timeline_bpm_edit_from_param(draft));
    changed
}

fn apply_timeline_bar_text_edits(input: &InputSnapshot, state: &mut PreviewState) -> bool {
    let Some(edit) = state.timeline_bar_edit.take() else {
        return false;
    };
    if state.export_menu.bar_length_overridden_by_audio() {
        return true;
    }
    let mut draft = timeline_bpm_edit_to_param(edit);
    let mut changed = false;
    if input.param_cancel {
        return true;
    }
    if input.param_select_all {
        changed |= param_edit::select_all_param_text(&mut draft);
    }
    if input.param_dec {
        changed |= param_edit::move_param_cursor_left(&mut draft, input.shift_down);
    }
    if input.param_inc {
        changed |= param_edit::move_param_cursor_right(&mut draft, input.shift_down);
    }
    if input.param_backspace {
        changed |= param_edit::backspace_param_text(&mut draft);
    }
    if input.param_delete {
        changed |= param_edit::delete_param_text(&mut draft);
    }
    if !input.typed_text.is_empty() {
        for ch in input.typed_text.chars() {
            if param_edit::insert_param_char(&mut draft, ch) {
                changed = true;
            }
        }
    }
    if input.param_commit && commit_timeline_bar_length_buffer(state, draft.buffer.as_str()) {
        return true;
    }
    state.timeline_bar_edit = Some(timeline_bpm_edit_from_param(draft));
    changed
}

fn timeline_bpm_edit_to_param(edit: TimelineBpmEditState) -> ParamEditState {
    ParamEditState {
        node_id: 0,
        param_index: 0,
        buffer: edit.buffer,
        cursor: edit.cursor,
        anchor: edit.anchor,
    }
}

fn timeline_bpm_edit_from_param(edit: ParamEditState) -> TimelineBpmEditState {
    TimelineBpmEditState {
        buffer: edit.buffer,
        cursor: edit.cursor,
        anchor: edit.anchor,
    }
}

fn commit_timeline_bpm_buffer(state: &mut PreviewState, buffer: &str) -> bool {
    let Ok(value) = buffer.trim().parse::<f32>() else {
        return false;
    };
    state.export_menu.bpm = format_timeline_bpm(value.clamp(1.0, 400.0));
    true
}

fn commit_timeline_bar_length_buffer(state: &mut PreviewState, buffer: &str) -> bool {
    if state.export_menu.bar_length_overridden_by_audio() {
        return false;
    }
    let Ok(value) = buffer.trim().parse::<f32>() else {
        return false;
    };
    state.export_menu.bar_length = format_timeline_bar_length(value.clamp(0.01, 10_000.0));
    true
}

fn format_timeline_bpm(value: f32) -> String {
    if (value - value.round()).abs() < 0.001 {
        format!("{}", value.round() as u32)
    } else {
        format!("{value:.2}")
    }
}

fn format_timeline_bar_length(value: f32) -> String {
    if (value - value.round()).abs() < 0.001 {
        format!("{}", value.round() as u32)
    } else {
        format!("{value:.2}")
    }
}

fn adjust_timeline_bpm(state: &mut PreviewState, delta: f32) -> bool {
    let current = state.export_menu.parsed_bpm();
    let next = (current + delta).clamp(1.0, 400.0);
    if (next - current).abs() < f32::EPSILON {
        return false;
    }
    state.export_menu.bpm = format_timeline_bpm(next);
    true
}

fn set_timeline_volume_from_slider_x(state: &mut PreviewState, slider: Rect, mouse_x: i32) -> bool {
    if slider.w <= 1 {
        return false;
    }
    let clamped_x = mouse_x.clamp(slider.x, slider.x + slider.w - 1);
    let t = (clamped_x - slider.x) as f32 / (slider.w - 1) as f32;
    let next = (t * 2.0).clamp(0.0, 2.0);
    let current = state.export_menu.parsed_audio_volume();
    if (next - current).abs() < 0.000_5 {
        return false;
    }
    state.export_menu.audio_volume = format!("{next:.2}");
    true
}

fn scrub_frame_from_timeline(
    track: Rect,
    mouse_x: i32,
    total_frames: u32,
    state: &mut PreviewState,
) -> bool {
    let frame = frame_from_track_x(track, mouse_x, total_frames);
    if frame == state.frame_index {
        return false;
    }
    state.frame_index = frame;
    state.timeline_accum_secs = 0.0;
    true
}

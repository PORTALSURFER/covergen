//! Timeline-scoped utility helpers for beat/pulse visualization.

/// Return `true` while beat pulse highlight should be shown.
pub(super) fn timeline_beat_indicator_on(frame_index: u32, timeline_fps: u32, bpm: f32) -> bool {
    if timeline_fps == 0 || !bpm.is_finite() || bpm <= 0.0 {
        return false;
    }
    let beat_frames = (timeline_fps as f32 * 60.0 / bpm).max(1.0);
    let pulse_frames = (beat_frames * 0.2).clamp(1.0, 6.0);
    let phase_frames = frame_index as f32 % beat_frames;
    phase_frames < pulse_frames
}

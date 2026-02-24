//! Terminal progress rendering helpers for animation execution.

use std::io::{self, Write};

/// Render one terminal progress row for clip/frame animation execution.
pub(crate) fn print_animation_progress(
    frame_done: u32,
    frame_total: u32,
    elapsed_secs: f64,
    clip_total: u32,
    clip_index: u32,
) {
    let percent = if frame_total == 0 {
        0.0
    } else {
        (frame_done as f64 / frame_total as f64) * 100.0
    };
    let fps = if elapsed_secs > 0.0 {
        frame_done as f64 / elapsed_secs
    } else {
        0.0
    };
    let eta_secs = if frame_done > 0 && fps > 0.0 {
        (frame_total.saturating_sub(frame_done)) as f64 / fps
    } else {
        0.0
    };
    let bar = progress_bar(frame_done, frame_total, 28);
    let _ = write!(
        io::stderr(),
        "\r[v2] clip {}/{} {} {:>6.2}% frame {}/{} | {:>5.1} fps | eta {}",
        clip_index + 1,
        clip_total,
        bar,
        percent,
        frame_done,
        frame_total,
        fps,
        format_eta(eta_secs),
    );
    let _ = io::stderr().flush();
}

/// End the current terminal progress line.
pub(crate) fn finish_animation_progress_line() {
    let _ = writeln!(io::stderr());
    let _ = io::stderr().flush();
}

fn progress_bar(done: u32, total: u32, width: usize) -> String {
    let clamped_total = total.max(1);
    let filled = ((done.min(clamped_total) as usize) * width) / clamped_total as usize;
    format!(
        "[{}{}]",
        "=".repeat(filled),
        "-".repeat(width.saturating_sub(filled))
    )
}

fn format_eta(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins:02}:{secs:02}")
}

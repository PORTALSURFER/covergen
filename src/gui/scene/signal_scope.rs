//! Signal-scope sampling and value-to-screen mapping helpers.

use super::Rect;

/// Per-scope hard cap to avoid zoom-driven sample explosions.
pub(super) const SIGNAL_SCOPE_MAX_SAMPLES: usize = 192;

/// Cached scope sample window for one node.
#[derive(Debug, Default)]
pub(super) struct SignalScopeCacheEntry {
    pub(super) sample_count: usize,
    pub(super) window_secs_bits: u32,
    pub(super) tex_eval_epoch: u64,
    pub(super) start_time: f32,
    pub(super) step_secs: f32,
    pub(super) values: Vec<f32>,
}

/// Inputs required to refresh one signal-scope cache entry.
#[derive(Clone, Copy, Debug)]
pub(super) struct SignalScopeRecomputeConfig {
    pub(super) start_time: f32,
    pub(super) sample_count: usize,
    pub(super) step_secs: f32,
    pub(super) window_secs_bits: u32,
    pub(super) tex_eval_epoch: u64,
}

/// Compute plotted range with guard rails for zero/one guide lines.
pub(super) fn signal_scope_range(values: &[f32]) -> (f32, f32) {
    let mut min_value = f32::INFINITY;
    let mut max_value = f32::NEG_INFINITY;
    for value in values.iter().copied().filter(|value| value.is_finite()) {
        min_value = min_value.min(value);
        max_value = max_value.max(value);
    }
    if !min_value.is_finite() || !max_value.is_finite() {
        return (-0.05, 1.05);
    }
    min_value = min_value.min(0.0);
    max_value = max_value.max(1.0);
    if (max_value - min_value).abs() <= 1e-5 {
        min_value -= 0.5;
        max_value += 0.5;
    }
    let pad = ((max_value - min_value) * 0.08).max(0.05);
    (min_value - pad, max_value + pad)
}

/// Map one scope value to Y pixel in an inner plotting rectangle.
pub(super) fn signal_scope_y(value: f32, min_value: f32, max_value: f32, inner: Rect) -> i32 {
    let span = (max_value - min_value).max(1e-5);
    let t = ((value - min_value) / span).clamp(0.0, 1.0);
    inner.y + ((1.0 - t) * (inner.h - 1) as f32).round() as i32
}

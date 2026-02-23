//! Luminance statistics helpers used during image synthesis.
//!
//! These helpers are kept in a separate module so we can keep the generation
//! loop focused on orchestration while statistics collection is grouped in one
//! location.

/// Summary statistics for a luminance buffer in `[0, 1]`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LumaStats {
    /// Smallest sample in the buffer.
    pub(crate) min: f32,
    /// Largest sample in the buffer.
    pub(crate) max: f32,
    /// Mean pixel value.
    pub(crate) mean: f32,
    /// Standard deviation of values.
    pub(crate) std: f32,
}

/// Combined metrics collected for a single luma pass.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LumaMetrics {
    /// Per-buffer statistics used for tone normalization and diagnostics.
    pub(crate) stats: LumaStats,
    /// Average edge contrast sampled with right/down neighbors.
    pub(crate) edge_energy: f32,
}

/// Collect mean/min/max/std and a cheap local edge-energy proxy in one pass.
pub(crate) fn collect_luma_metrics(src: &[f32], width: u32, height: u32) -> LumaMetrics {
    if src.is_empty() || width == 0 || height == 0 {
        return LumaMetrics {
            stats: LumaStats {
                min: 1.0,
                max: 0.0,
                mean: 0.0,
                std: 0.0,
            },
            edge_energy: 0.0,
        };
    }

    let width_usize = width as usize;
    let mut min = 1.0f32;
    let mut max = 0.0f32;
    let mut mean = 0.0f32;
    let mut m2 = 0.0f32;
    let mut count = 0u64;

    let mut edge_sum = 0.0f32;
    let mut edge_count = 0u64;

    for y in 0..height as usize {
        let row = y * width_usize;
        for x in 0..width_usize {
            let idx = row + x;
            let value = src[idx];
            min = min.min(value);
            max = max.max(value);

            count += 1;
            let delta = value - mean;
            mean += delta / (count as f32);
            m2 += delta * (value - mean);

            let right = if x + 1 < width_usize {
                src[idx + 1]
            } else {
                value
            };
            let down = if y + 1 < height as usize {
                src[idx + width_usize]
            } else {
                value
            };
            edge_sum += (right - value).abs() + (down - value).abs();
            edge_count += 2;
        }
    }

    let variance = if count > 1 { m2 / (count as f32) } else { 0.0 };
    let edge_energy = if edge_count > 0 {
        (edge_sum / (edge_count as f32)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    LumaMetrics {
        stats: LumaStats {
            min,
            max,
            mean,
            std: variance.sqrt(),
        },
        edge_energy,
    }
}

/// Collect luma metrics from a bounded sample set instead of the full frame.
///
/// This is intended for fast-path complexity decisions where exact values are
/// less important than stable thresholds. When `max_samples` is too small or
/// larger than the image area, this falls back to full-frame collection.
pub(crate) fn collect_luma_metrics_sampled(
    src: &[f32],
    width: u32,
    height: u32,
    max_samples: usize,
) -> LumaMetrics {
    let pixel_count = (width as usize).saturating_mul(height as usize);
    if max_samples < 16 || pixel_count <= max_samples {
        return collect_luma_metrics(src, width, height);
    }

    if src.is_empty() || width == 0 || height == 0 {
        return collect_luma_metrics(src, width, height);
    }

    let width_usize = width as usize;
    let height_usize = height as usize;
    let stride = ((pixel_count as f64 / max_samples as f64).sqrt().floor() as usize).max(1);

    let mut min = 1.0f32;
    let mut max = 0.0f32;
    let mut mean = 0.0f32;
    let mut m2 = 0.0f32;
    let mut count = 0u64;
    let mut edge_sum = 0.0f32;
    let mut edge_count = 0u64;

    for y in (0..height_usize).step_by(stride) {
        let row = y * width_usize;
        for x in (0..width_usize).step_by(stride) {
            let idx = row + x;
            let value = src[idx];
            min = min.min(value);
            max = max.max(value);

            count += 1;
            let delta = value - mean;
            mean += delta / (count as f32);
            m2 += delta * (value - mean);

            let sample_dx = (x + stride).min(width_usize - 1);
            let sample_dy = (y + stride).min(height_usize - 1);
            let right = src[row + sample_dx];
            let down = src[sample_dy * width_usize + x];
            edge_sum += (right - value).abs() + (down - value).abs();
            edge_count += 2;
        }
    }

    let variance = if count > 1 { m2 / (count as f32) } else { 0.0 };
    let edge_energy = if edge_count > 0 {
        (edge_sum / (edge_count as f32)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    LumaMetrics {
        stats: LumaStats {
            min,
            max,
            mean,
            std: variance.sqrt(),
        },
        edge_energy,
    }
}

/// Returns whether the pass should be enriched with additional complexity.
pub(crate) fn needs_complexity_fix(stats: &LumaStats, edge_energy: f32) -> bool {
    let span = stats.max - stats.min;
    stats.std < 0.16 || span < 0.34 || edge_energy < 0.09
}

#[cfg(test)]
mod tests {
    use super::{
        LumaStats, collect_luma_metrics, collect_luma_metrics_sampled, needs_complexity_fix,
    };

    #[test]
    fn metrics_capture_basic_values() {
        let src = [0.0f32, 1.0, 1.0, 0.0];
        let metrics = collect_luma_metrics(&src, 2, 2);

        assert_eq!(metrics.stats.min, 0.0);
        assert_eq!(metrics.stats.max, 1.0);
        assert!((metrics.stats.mean - 0.5).abs() < f32::EPSILON);
        assert!((metrics.stats.std - 0.5).abs() < 0.1);
        assert!((metrics.edge_energy - 0.5).abs() < 0.0001);
    }

    #[test]
    fn complexity_fix_thresholds() {
        let simple = LumaStats {
            min: 0.2,
            max: 0.6,
            mean: 0.4,
            std: 0.11,
        };
        assert!(needs_complexity_fix(&simple, 0.07));
    }

    #[test]
    fn complexity_fix_relaxes_for_structured_data() {
        let complex = LumaStats {
            min: 0.1,
            max: 0.95,
            mean: 0.5,
            std: 0.2,
        };
        assert!(!needs_complexity_fix(&complex, 0.2));
    }

    #[test]
    fn sampled_metrics_falls_back_to_full_scan() {
        let src = [0.0f32, 0.5, 1.0, 0.25];
        let full = collect_luma_metrics(&src, 2, 2);
        let sampled = collect_luma_metrics_sampled(&src, 2, 2, 64);

        assert!((full.stats.mean - sampled.stats.mean).abs() < f32::EPSILON);
        assert!((full.stats.std - sampled.stats.std).abs() < f32::EPSILON);
        assert!((full.edge_energy - sampled.edge_energy).abs() < f32::EPSILON);
    }

    #[test]
    fn sampled_metrics_track_full_metrics() {
        let width = 96usize;
        let height = 96usize;
        let mut src = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let xf = x as f32 / width as f32;
                let yf = y as f32 / height as f32;
                src[idx] = (0.35 * xf + 0.45 * yf + 0.2 * ((xf * 9.0).sin() * (yf * 7.0).cos()))
                    .clamp(0.0, 1.0);
            }
        }

        let full = collect_luma_metrics(&src, width as u32, height as u32);
        let sampled = collect_luma_metrics_sampled(&src, width as u32, height as u32, 1024);

        assert!((full.stats.mean - sampled.stats.mean).abs() < 0.03);
        assert!((full.stats.std - sampled.stats.std).abs() < 0.03);
        assert!((full.edge_energy - sampled.edge_energy).abs() < 0.03);
    }
}

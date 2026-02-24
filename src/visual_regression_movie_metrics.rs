//! Movie-quality metrics for animation regression gates.
//!
//! The visual snapshot hashes detect exact-output drift for sampled frames.
//! These metrics provide additional temporal quality gates that track flicker
//! and motion continuity characteristics across sampled animation frames.

use std::error::Error;

/// Threshold contract for one animation case.
#[derive(Clone, Copy, Debug)]
pub(super) struct MovieQualityBounds {
    /// Minimum average frame-to-frame delta required to avoid static clips.
    pub min_pair_delta_mean: f32,
    /// Maximum average frame-to-frame delta tolerated before excessive flicker.
    pub max_pair_delta_mean: f32,
    /// Maximum p95 frame-to-frame delta tolerated to avoid flash spikes.
    pub max_pair_delta_p95: f32,
    /// Minimum continuity score required for smooth temporal evolution.
    pub min_continuity_score: f32,
}

/// Computed movie-quality metrics for one sampled animation sequence.
#[derive(Clone, Copy, Debug)]
pub(super) struct MovieQualityMetrics {
    /// Mean normalized absolute frame-to-frame delta in `[0, 1]`.
    pub pair_delta_mean: f32,
    /// 95th percentile of normalized frame-to-frame deltas.
    pub pair_delta_p95: f32,
    /// Mean absolute change-of-change in frame deltas.
    pub delta_jerk_mean: f32,
    /// Continuity score in `[0, 1]` where higher is smoother.
    pub continuity_score: f32,
}

/// Compute temporal quality metrics from ordered grayscale frames.
pub(super) fn compute_movie_quality_metrics(
    frames: &[Vec<u8>],
) -> Result<MovieQualityMetrics, Box<dyn Error>> {
    if frames.len() < 2 {
        return Err("movie-quality metrics require at least two frames".into());
    }
    let pixel_count = frames[0].len();
    if pixel_count == 0 {
        return Err("movie-quality metrics require non-empty frames".into());
    }
    for (index, frame) in frames.iter().enumerate().skip(1) {
        if frame.len() != pixel_count {
            return Err(format!(
                "frame {} has mismatched pixel count: expected {}, got {}",
                index,
                pixel_count,
                frame.len()
            )
            .into());
        }
    }

    let mut deltas = Vec::with_capacity(frames.len() - 1);
    for pair in frames.windows(2) {
        deltas.push(mean_abs_delta(&pair[0], &pair[1]));
    }

    let pair_delta_mean = mean(&deltas);
    let pair_delta_p95 = percentile_nearest_rank(&deltas, 0.95);
    let delta_jerk_mean = mean_abs_adjacent_delta(&deltas);
    let continuity_score = if pair_delta_mean <= 1e-6 {
        0.0
    } else {
        (1.0 - delta_jerk_mean / pair_delta_mean).clamp(0.0, 1.0)
    };

    Ok(MovieQualityMetrics {
        pair_delta_mean,
        pair_delta_p95,
        delta_jerk_mean,
        continuity_score,
    })
}

/// Validate one case against temporal quality bounds.
pub(super) fn assert_movie_quality_bounds(
    case_name: &str,
    metrics: MovieQualityMetrics,
    bounds: MovieQualityBounds,
) -> Result<(), Box<dyn Error>> {
    if metrics.pair_delta_mean < bounds.min_pair_delta_mean {
        return Err(format!(
            "movie-quality gate '{}' failed: pair_delta_mean {:.6} < min {:.6}",
            case_name, metrics.pair_delta_mean, bounds.min_pair_delta_mean
        )
        .into());
    }
    if metrics.pair_delta_mean > bounds.max_pair_delta_mean {
        return Err(format!(
            "movie-quality gate '{}' failed: pair_delta_mean {:.6} > max {:.6}",
            case_name, metrics.pair_delta_mean, bounds.max_pair_delta_mean
        )
        .into());
    }
    if metrics.pair_delta_p95 > bounds.max_pair_delta_p95 {
        return Err(format!(
            "movie-quality gate '{}' failed: pair_delta_p95 {:.6} > max {:.6}",
            case_name, metrics.pair_delta_p95, bounds.max_pair_delta_p95
        )
        .into());
    }
    if metrics.continuity_score < bounds.min_continuity_score {
        return Err(format!(
            "movie-quality gate '{}' failed: continuity_score {:.6} < min {:.6}",
            case_name, metrics.continuity_score, bounds.min_continuity_score
        )
        .into());
    }
    Ok(())
}

fn mean_abs_delta(a: &[u8], b: &[u8]) -> f32 {
    let mut sum = 0.0f32;
    for (&left, &right) in a.iter().zip(b.iter()) {
        sum += (left as f32 - right as f32).abs() / 255.0;
    }
    sum / a.len() as f32
}

fn mean(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f32>() / values.len() as f32
}

fn mean_abs_adjacent_delta(values: &[f32]) -> f32 {
    if values.len() < 2 {
        return 0.0;
    }
    let mut sum = 0.0f32;
    for pair in values.windows(2) {
        sum += (pair[1] - pair[0]).abs();
    }
    sum / (values.len() - 1) as f32
}

fn percentile_nearest_rank(values: &[f32], percentile: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let rank = ((sorted.len() as f32 - 1.0) * percentile.clamp(0.0, 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

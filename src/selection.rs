//! Candidate scoring helpers for generate-score-select rendering.
//!
//! The scoring model combines composition quality, novelty against prior
//! explored candidates, and temporal stability under a small modulation step.

use std::cmp::Ordering;

/// Weighted score breakdown for one explored candidate.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CandidateScore {
    /// Candidate index in exploration order.
    pub candidate_index: u32,
    /// Seed offset used by graph evaluation for this candidate.
    pub seed_offset: u32,
    /// Final weighted score.
    pub total: f32,
    /// Composition quality component.
    pub composition: f32,
    /// Novelty component.
    pub novelty: f32,
    /// Temporal stability component.
    pub stability: f32,
}

/// Scoring payload used by runtime selection loop.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScoreBreakdown {
    /// Final weighted score.
    pub total: f32,
    /// Composition quality component.
    pub composition: f32,
    /// Novelty component.
    pub novelty: f32,
    /// Temporal stability component.
    pub stability: f32,
    /// Histogram captured for novelty comparison with future candidates.
    pub histogram: [f32; 16],
}

/// Compute a candidate score from low-res still and temporal probe frames.
pub(crate) fn score_candidate(
    primary: &[u8],
    temporal_probe: &[u8],
    width: u32,
    height: u32,
    prior_histograms: &[[f32; 16]],
) -> ScoreBreakdown {
    let (histogram, composition, stability) =
        fused_primary_metrics(primary, temporal_probe, width, height);
    let novelty = novelty_score(&histogram, prior_histograms);
    let total = (composition * 0.45) + (novelty * 0.35) + (stability * 0.20);
    ScoreBreakdown {
        total,
        composition,
        novelty,
        stability,
        histogram,
    }
}

/// Return top-k candidates ordered from highest to lowest score.
pub(crate) fn top_k(mut scored: Vec<CandidateScore>, k: usize) -> Vec<CandidateScore> {
    scored.sort_by(|a, b| compare_score_desc(*a, *b));
    scored.truncate(k.min(scored.len()));
    scored
}

fn compare_score_desc(a: CandidateScore, b: CandidateScore) -> Ordering {
    b.total
        .partial_cmp(&a.total)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.candidate_index.cmp(&b.candidate_index))
}

fn novelty_score(histogram: &[f32; 16], prior_histograms: &[[f32; 16]]) -> f32 {
    if prior_histograms.is_empty() {
        return 0.5;
    }
    let mut total = 0.0f32;
    for prior in prior_histograms {
        total += l1_distance(histogram, prior);
    }
    let average = total / prior_histograms.len() as f32;
    (average / 2.0).clamp(0.0, 1.0)
}

fn l1_distance(a: &[f32; 16], b: &[f32; 16]) -> f32 {
    let mut total = 0.0f32;
    for index in 0..16 {
        total += (a[index] - b[index]).abs();
    }
    total
}

fn fused_primary_metrics(
    primary: &[u8],
    temporal_probe: &[u8],
    width: u32,
    height: u32,
) -> ([f32; 16], f32, f32) {
    let width = width as usize;
    let height = height as usize;
    let expected = width.saturating_mul(height);
    if expected == 0 || primary.len() < expected || temporal_probe.len() < expected {
        return ([0.0; 16], 0.0, 0.0);
    }

    let mut hist = [0u32; 16];
    let mut sum = 0.0f32;
    let mut sum_sq = 0.0f32;
    let mut sum_abs_delta = 0.0f32;
    let mut edge_total = 0.0f32;
    let mut edge_samples = 0usize;

    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let primary_value = primary[index] as f32;
            let temporal_value = temporal_probe[index] as f32;
            hist[((primary_value as usize) / 16).min(15)] += 1;
            sum += primary_value;
            sum_sq += primary_value * primary_value;
            sum_abs_delta += (primary_value - temporal_value).abs();

            if x + 1 < width && y + 1 < height {
                let dx = (primary_value - primary[index + 1] as f32).abs();
                let dy = (primary_value - primary[index + width] as f32).abs();
                edge_total += (dx + dy) / 510.0;
                edge_samples = edge_samples.saturating_add(1);
            }
        }
    }

    let denom = expected as f32;
    let mean = sum / denom;
    let variance = (sum_sq / denom - mean * mean).max(0.0);
    let stddev = variance.sqrt();
    let contrast = (stddev / 80.0).clamp(0.0, 1.0);
    let exposure = (1.0 - ((mean / 255.0) - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    let edge = if edge_samples == 0 {
        0.0
    } else {
        (edge_total / edge_samples as f32).clamp(0.0, 1.0)
    };
    let composition = (contrast * 0.5) + (edge * 0.3) + (exposure * 0.2);
    let stability = (1.0 - (sum_abs_delta / denom / 255.0)).clamp(0.0, 1.0);

    let mut histogram = [0.0f32; 16];
    for (index, count) in hist.into_iter().enumerate() {
        histogram[index] = count as f32 / denom;
    }
    (histogram, composition, stability)
}

#[cfg(test)]
mod tests {
    use super::{score_candidate, top_k, CandidateScore};

    #[test]
    fn top_k_orders_descending_by_score() {
        let sorted = top_k(
            vec![
                CandidateScore {
                    candidate_index: 0,
                    seed_offset: 1,
                    total: 0.2,
                    composition: 0.2,
                    novelty: 0.2,
                    stability: 0.2,
                },
                CandidateScore {
                    candidate_index: 1,
                    seed_offset: 2,
                    total: 0.9,
                    composition: 0.9,
                    novelty: 0.9,
                    stability: 0.9,
                },
            ],
            1,
        );
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].candidate_index, 1);
    }

    #[test]
    fn score_candidate_returns_normalized_components() {
        let a = vec![0u8; 64];
        let b = vec![10u8; 64];
        let score = score_candidate(&a, &b, 8, 8, &[]);
        assert!((0.0..=1.0).contains(&score.composition));
        assert!((0.0..=1.0).contains(&score.novelty));
        assert!((0.0..=1.0).contains(&score.stability));
        assert!((0.0..=1.0).contains(&score.total));
    }
}

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
    let histogram = histogram_16(primary);
    let composition = composition_score(primary, width, height);
    let novelty = novelty_score(&histogram, prior_histograms);
    let stability = temporal_stability_score(primary, temporal_probe);
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

fn histogram_16(frame: &[u8]) -> [f32; 16] {
    if frame.is_empty() {
        return [0.0; 16];
    }
    let mut hist = [0u32; 16];
    for value in frame {
        let bin = (*value as usize) / 16;
        hist[bin.min(15)] += 1;
    }
    let denom = frame.len() as f32;
    let mut normalized = [0.0f32; 16];
    for (index, value) in hist.into_iter().enumerate() {
        normalized[index] = value as f32 / denom;
    }
    normalized
}

fn composition_score(frame: &[u8], width: u32, height: u32) -> f32 {
    if frame.is_empty() || width == 0 || height == 0 {
        return 0.0;
    }

    let (mean, variance) = mean_variance(frame);
    let stddev = variance.sqrt();
    let contrast = (stddev / 80.0).clamp(0.0, 1.0);
    let exposure = (1.0 - ((mean / 255.0) - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    let edge = edge_energy(frame, width as usize, height as usize);
    (contrast * 0.5) + (edge * 0.3) + (exposure * 0.2)
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

fn temporal_stability_score(primary: &[u8], temporal_probe: &[u8]) -> f32 {
    if primary.is_empty() || primary.len() != temporal_probe.len() {
        return 0.0;
    }
    let mut sum_abs_delta = 0.0f32;
    for (a, b) in primary.iter().zip(temporal_probe.iter()) {
        sum_abs_delta += (*a as f32 - *b as f32).abs();
    }
    let mean_delta = sum_abs_delta / primary.len() as f32;
    (1.0 - (mean_delta / 255.0)).clamp(0.0, 1.0)
}

fn l1_distance(a: &[f32; 16], b: &[f32; 16]) -> f32 {
    let mut total = 0.0f32;
    for index in 0..16 {
        total += (a[index] - b[index]).abs();
    }
    total
}

fn mean_variance(frame: &[u8]) -> (f32, f32) {
    let len = frame.len() as f32;
    if len <= 0.0 {
        return (0.0, 0.0);
    }
    let sum = frame.iter().map(|value| *value as f32).sum::<f32>();
    let mean = sum / len;
    let variance = frame
        .iter()
        .map(|value| {
            let delta = *value as f32 - mean;
            delta * delta
        })
        .sum::<f32>()
        / len;
    (mean, variance)
}

fn edge_energy(frame: &[u8], width: usize, height: usize) -> f32 {
    if width < 2 || height < 2 {
        return 0.0;
    }
    let mut total = 0.0f32;
    let mut samples = 0usize;
    for y in 0..(height - 1) {
        for x in 0..(width - 1) {
            let idx = y * width + x;
            let dx = (frame[idx] as f32 - frame[idx + 1] as f32).abs();
            let dy = (frame[idx] as f32 - frame[idx + width] as f32).abs();
            total += (dx + dy) / 510.0;
            samples += 1;
        }
    }
    if samples == 0 {
        0.0
    } else {
        (total / samples as f32).clamp(0.0, 1.0)
    }
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

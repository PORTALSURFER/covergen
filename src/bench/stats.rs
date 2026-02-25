//! Statistical aggregation helpers for benchmark telemetry.

use std::collections::HashMap;

use crate::telemetry::CaptureReport;

use super::ScenarioSample;

/// Aggregated latency/memory/throughput summary for one benchmark scenario.
#[derive(Clone, Debug)]
pub(super) struct ScenarioSummary {
    pub(super) name: &'static str,
    pub(super) sample_count: usize,
    pub(super) p50_ms: f64,
    pub(super) p95_ms: f64,
    pub(super) memory_p50_mb: f64,
    pub(super) memory_p95_mb: f64,
    pub(super) throughput_p50: f64,
    pub(super) throughput_p95: f64,
    pub(super) frame_time_p50_ms: f64,
    pub(super) frame_time_p95_ms: f64,
}

/// Aggregated timing summary for one GPU node scope.
#[derive(Clone, Debug)]
pub(super) struct NodeTimingSummary {
    pub(super) scope: String,
    pub(super) sample_count: usize,
    pub(super) total_ms: f64,
    pub(super) p50_ms: f64,
    pub(super) p95_ms: f64,
}

/// Aggregated scalar counter summary for one telemetry scope.
#[derive(Clone, Debug)]
pub(super) struct CounterSummary {
    pub(super) scope: String,
    pub(super) sample_count: usize,
    pub(super) total: f64,
    pub(super) p50: f64,
    pub(super) p95: f64,
}

/// Build one scenario summary from raw benchmark samples.
pub(super) fn summarize_scenario(
    name: &'static str,
    samples: &[ScenarioSample],
) -> ScenarioSummary {
    let _has_labels = samples
        .iter()
        .any(|sample| !sample.capture.run_label.is_empty());
    let mut latencies: Vec<f64> = samples.iter().map(|sample| sample.elapsed_ms).collect();
    let mut memory: Vec<f64> = samples
        .iter()
        .map(|sample| max_memory_mb(&sample.capture))
        .collect();
    let mut throughput: Vec<f64> = samples
        .iter()
        .map(|sample| throughput_fps(sample.frame_count, sample.elapsed_ms))
        .collect();
    let mut frame_times: Vec<f64> = samples
        .iter()
        .flat_map(|sample| frame_timings_ms(&sample.capture))
        .collect();

    ScenarioSummary {
        name,
        sample_count: samples.len(),
        p50_ms: percentile(&mut latencies, 0.50),
        p95_ms: percentile(&mut latencies, 0.95),
        memory_p50_mb: percentile(&mut memory, 0.50),
        memory_p95_mb: percentile(&mut memory, 0.95),
        throughput_p50: percentile(&mut throughput, 0.50),
        throughput_p95: percentile(&mut throughput, 0.95),
        frame_time_p50_ms: percentile(&mut frame_times, 0.50),
        frame_time_p95_ms: percentile(&mut frame_times, 0.95),
    }
}

/// Aggregate per-scope V2 GPU node timings from still and animation samples.
pub(super) fn summarize_node_timings(
    still: &[ScenarioSample],
    animation: &[ScenarioSample],
) -> Vec<NodeTimingSummary> {
    let mut grouped: HashMap<String, Vec<f64>> = HashMap::new();
    for sample in still.iter().chain(animation.iter()) {
        for timing in &sample.capture.timings {
            if timing.scope.starts_with("v2.gpu.node.") {
                grouped
                    .entry(timing.scope.clone())
                    .or_default()
                    .push(timing.ms);
            }
        }
    }

    let mut rows = Vec::with_capacity(grouped.len());
    for (scope, mut values) in grouped {
        let sample_count = values.len();
        let total_ms: f64 = values.iter().sum();
        let p50_ms = percentile(&mut values, 0.50);
        let p95_ms = percentile(&mut values, 0.95);
        rows.push(NodeTimingSummary {
            scope,
            sample_count,
            total_ms,
            p50_ms,
            p95_ms,
        });
    }

    rows.sort_by(|left, right| {
        right
            .total_ms
            .partial_cmp(&left.total_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
}

/// Aggregate per-scope scalar counters across benchmark scenarios.
pub(super) fn summarize_counters(scenarios: &[&[ScenarioSample]]) -> Vec<CounterSummary> {
    let mut grouped: HashMap<String, Vec<f64>> = HashMap::new();
    for scenario in scenarios {
        for sample in scenario.iter() {
            for counter in &sample.capture.counters {
                grouped
                    .entry(counter.scope.clone())
                    .or_default()
                    .push(counter.value);
            }
        }
    }

    let mut rows = Vec::with_capacity(grouped.len());
    for (scope, mut values) in grouped {
        let sample_count = values.len();
        let total: f64 = values.iter().sum();
        let p50 = percentile(&mut values, 0.50);
        let p95 = percentile(&mut values, 0.95);
        rows.push(CounterSummary {
            scope,
            sample_count,
            total,
            p50,
            p95,
        });
    }
    rows.sort_by(|left, right| {
        right
            .total
            .partial_cmp(&left.total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
}

fn max_memory_mb(capture: &CaptureReport) -> f64 {
    let peak = capture
        .memory
        .iter()
        .filter(|sample| !sample.label.is_empty())
        .map(|sample| sample.rss_bytes.max(sample.hwm_bytes))
        .max()
        .unwrap_or(0);
    peak as f64 / (1024.0 * 1024.0)
}

fn frame_timings_ms(capture: &CaptureReport) -> impl Iterator<Item = f64> + '_ {
    capture
        .frames
        .iter()
        .filter(|sample| sample.scope == "v2.animation.frame.total")
        .map(|sample| sample.ms)
}

fn throughput_fps(frames: u32, elapsed_ms: f64) -> f64 {
    if elapsed_ms <= 0.0 {
        return 0.0;
    }
    frames as f64 / (elapsed_ms / 1000.0)
}

fn percentile(values: &mut [f64], ratio: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let ratio = ratio.clamp(0.0, 1.0);
    let index = ((values.len() - 1) as f64 * ratio).round() as usize;
    values[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_returns_expected_values() {
        let mut values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        assert_eq!(percentile(&mut values, 0.5), 30.0);
        assert_eq!(percentile(&mut values, 0.95), 50.0);
    }

    #[test]
    fn throughput_handles_zero_duration() {
        assert_eq!(throughput_fps(42, 0.0), 0.0);
        assert!(throughput_fps(60, 1000.0) > 59.0);
    }
}

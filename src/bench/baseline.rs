//! Baseline and threshold artifacts for tiered benchmark cutover gating.

use std::collections::HashMap;
use std::error::Error;
use std::path::Path;

use super::stats::ScenarioSummary;

const FORMAT_VERSION: &str = "1";
const LATENCY_P50_MAX_MULTIPLIER: f64 = 1.10;
const LATENCY_P95_MAX_MULTIPLIER: f64 = 1.15;
const FRAME_TIME_P50_MAX_MULTIPLIER: f64 = 1.10;
const FRAME_TIME_P95_MAX_MULTIPLIER: f64 = 1.15;
const THROUGHPUT_P50_MIN_MULTIPLIER: f64 = 0.92;
const THROUGHPUT_P95_MIN_MULTIPLIER: f64 = 0.90;

#[derive(Clone, Copy, Debug)]
struct ScenarioMetrics {
    latency_p50_ms: f64,
    latency_p95_ms: f64,
    frame_time_p50_ms: f64,
    frame_time_p95_ms: f64,
    throughput_p50_fps: f64,
    throughput_p95_fps: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScenarioThresholds {
    latency_p50_ms_max: f64,
    latency_p95_ms_max: f64,
    frame_time_p50_ms_max: f64,
    frame_time_p95_ms_max: f64,
    throughput_p50_fps_min: f64,
    throughput_p95_fps_min: f64,
}

/// Write a machine-readable metric snapshot for one benchmark tier run.
pub(super) fn write_metrics_snapshot(
    path: &Path,
    tier: &str,
    summaries: &[ScenarioSummary],
) -> Result<(), Box<dyn Error>> {
    let scenarios = summaries_to_metrics(summaries);
    let mut out = String::new();
    out.push_str("# covergen bench metrics snapshot\n");
    out.push_str(&format!("version={FORMAT_VERSION}\n"));
    out.push_str(&format!("tier={tier}\n\n"));
    for (scenario, metrics) in scenarios {
        out.push_str(&format!("[{scenario}]\n"));
        out.push_str(&format!("latency_p50_ms={:.6}\n", metrics.latency_p50_ms));
        out.push_str(&format!("latency_p95_ms={:.6}\n", metrics.latency_p95_ms));
        out.push_str(&format!(
            "frame_time_p50_ms={:.6}\n",
            metrics.frame_time_p50_ms
        ));
        out.push_str(&format!(
            "frame_time_p95_ms={:.6}\n",
            metrics.frame_time_p95_ms
        ));
        out.push_str(&format!(
            "throughput_p50_fps={:.6}\n",
            metrics.throughput_p50_fps
        ));
        out.push_str(&format!(
            "throughput_p95_fps={:.6}\n\n",
            metrics.throughput_p95_fps
        ));
    }
    std::fs::write(path, out)?;
    Ok(())
}

/// Write an absolute cutover-threshold file derived from current benchmark metrics.
pub(super) fn write_locked_thresholds(
    path: &Path,
    tier: &str,
    summaries: &[ScenarioSummary],
) -> Result<(), Box<dyn Error>> {
    let metrics = summaries_to_metrics(summaries);
    let mut out = String::new();
    out.push_str("# covergen cutover thresholds\n");
    out.push_str("# generated from benchmark metrics with locked multipliers\n");
    out.push_str(&format!("version={FORMAT_VERSION}\n"));
    out.push_str(&format!("tier={tier}\n\n"));
    for (scenario, row) in metrics {
        let thresholds = thresholds_from_metrics(row);
        out.push_str(&format!("[{scenario}]\n"));
        out.push_str(&format!(
            "latency_p50_ms_max={:.6}\n",
            thresholds.latency_p50_ms_max
        ));
        out.push_str(&format!(
            "latency_p95_ms_max={:.6}\n",
            thresholds.latency_p95_ms_max
        ));
        out.push_str(&format!(
            "frame_time_p50_ms_max={:.6}\n",
            thresholds.frame_time_p50_ms_max
        ));
        out.push_str(&format!(
            "frame_time_p95_ms_max={:.6}\n",
            thresholds.frame_time_p95_ms_max
        ));
        out.push_str(&format!(
            "throughput_p50_fps_min={:.6}\n",
            thresholds.throughput_p50_fps_min
        ));
        out.push_str(&format!(
            "throughput_p95_fps_min={:.6}\n\n",
            thresholds.throughput_p95_fps_min
        ));
    }
    std::fs::write(path, out)?;
    Ok(())
}

/// Validate benchmark summaries against a locked threshold file for one tier.
pub(super) fn validate_thresholds(
    path: &Path,
    tier: &str,
    summaries: &[ScenarioSummary],
) -> Result<Vec<String>, Box<dyn Error>> {
    let data = std::fs::read_to_string(path)?;
    let parsed = parse_thresholds(&data)?;
    if parsed.tier != tier {
        return Err(format!(
            "threshold tier mismatch: expected '{tier}', found '{}'",
            parsed.tier
        )
        .into());
    }

    let metrics = summaries_to_metrics(summaries);
    let mut violations = Vec::new();
    for scenario in metrics.keys() {
        if !parsed.scenarios.contains_key(scenario) {
            violations.push(format!("missing scenario '{scenario}' in threshold file"));
        }
    }
    for (scenario, expected) in parsed.scenarios {
        let Some(actual) = metrics.get(&scenario).copied() else {
            violations.push(format!("missing scenario '{scenario}' in benchmark output"));
            continue;
        };
        check_metric_max(
            &mut violations,
            &scenario,
            "latency_p50_ms",
            actual.latency_p50_ms,
            expected.latency_p50_ms_max,
        );
        check_metric_max(
            &mut violations,
            &scenario,
            "latency_p95_ms",
            actual.latency_p95_ms,
            expected.latency_p95_ms_max,
        );
        check_metric_max(
            &mut violations,
            &scenario,
            "frame_time_p50_ms",
            actual.frame_time_p50_ms,
            expected.frame_time_p50_ms_max,
        );
        check_metric_max(
            &mut violations,
            &scenario,
            "frame_time_p95_ms",
            actual.frame_time_p95_ms,
            expected.frame_time_p95_ms_max,
        );
        check_metric_min(
            &mut violations,
            &scenario,
            "throughput_p50_fps",
            actual.throughput_p50_fps,
            expected.throughput_p50_fps_min,
        );
        check_metric_min(
            &mut violations,
            &scenario,
            "throughput_p95_fps",
            actual.throughput_p95_fps,
            expected.throughput_p95_fps_min,
        );
    }

    Ok(violations)
}

fn summaries_to_metrics(summaries: &[ScenarioSummary]) -> HashMap<String, ScenarioMetrics> {
    let mut out = HashMap::new();
    for summary in summaries {
        if summary.sample_count == 0 {
            continue;
        }
        out.insert(
            scenario_key(summary.name),
            ScenarioMetrics {
                latency_p50_ms: summary.p50_ms,
                latency_p95_ms: summary.p95_ms,
                frame_time_p50_ms: summary.frame_time_p50_ms,
                frame_time_p95_ms: summary.frame_time_p95_ms,
                throughput_p50_fps: summary.throughput_p50,
                throughput_p95_fps: summary.throughput_p95,
            },
        );
    }
    out
}

fn thresholds_from_metrics(metrics: ScenarioMetrics) -> ScenarioThresholds {
    ScenarioThresholds {
        latency_p50_ms_max: metrics.latency_p50_ms * LATENCY_P50_MAX_MULTIPLIER,
        latency_p95_ms_max: metrics.latency_p95_ms * LATENCY_P95_MAX_MULTIPLIER,
        frame_time_p50_ms_max: metrics.frame_time_p50_ms * FRAME_TIME_P50_MAX_MULTIPLIER,
        frame_time_p95_ms_max: metrics.frame_time_p95_ms * FRAME_TIME_P95_MAX_MULTIPLIER,
        throughput_p50_fps_min: metrics.throughput_p50_fps * THROUGHPUT_P50_MIN_MULTIPLIER,
        throughput_p95_fps_min: metrics.throughput_p95_fps * THROUGHPUT_P95_MIN_MULTIPLIER,
    }
}

fn check_metric_max(out: &mut Vec<String>, scenario: &str, metric: &str, actual: f64, max: f64) {
    if actual > max {
        out.push(format!(
            "{scenario}.{metric} exceeded max ({actual:.3} > {max:.3})"
        ));
    }
}

fn check_metric_min(out: &mut Vec<String>, scenario: &str, metric: &str, actual: f64, min: f64) {
    if actual < min {
        out.push(format!(
            "{scenario}.{metric} dropped below min ({actual:.3} < {min:.3})"
        ));
    }
}

fn scenario_key(name: &str) -> String {
    let mut key = String::with_capacity(name.len());
    let mut last_underscore = false;
    for ch in name.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };
        if mapped == '_' {
            if !last_underscore {
                key.push('_');
            }
            last_underscore = true;
        } else {
            key.push(mapped);
            last_underscore = false;
        }
    }
    key.trim_matches('_').to_string()
}

struct ParsedThresholds {
    tier: String,
    scenarios: HashMap<String, ScenarioThresholds>,
}

fn parse_thresholds(input: &str) -> Result<ParsedThresholds, Box<dyn Error>> {
    let mut version = None;
    let mut tier = None;
    let mut current_section = None::<String>;
    let mut scenarios: HashMap<String, ScenarioThresholds> = HashMap::new();

    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = Some(line[1..line.len() - 1].trim().to_string());
            continue;
        }

        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("invalid thresholds line: {line}"))?;
        let key = key.trim();
        let value = value.trim();
        if current_section.is_none() {
            match key {
                "version" => version = Some(value.to_string()),
                "tier" => tier = Some(value.to_string()),
                _ => return Err(format!("unknown header key '{key}' in thresholds").into()),
            }
            continue;
        }

        let section = current_section.as_ref().expect("checked is_some");
        let parsed_value: f64 = value.parse()?;
        let row = scenarios.entry(section.clone()).or_default();
        match key {
            "latency_p50_ms_max" => row.latency_p50_ms_max = parsed_value,
            "latency_p95_ms_max" => row.latency_p95_ms_max = parsed_value,
            "frame_time_p50_ms_max" => row.frame_time_p50_ms_max = parsed_value,
            "frame_time_p95_ms_max" => row.frame_time_p95_ms_max = parsed_value,
            "throughput_p50_fps_min" => row.throughput_p50_fps_min = parsed_value,
            "throughput_p95_fps_min" => row.throughput_p95_fps_min = parsed_value,
            _ => return Err(format!("unknown metric key '{key}' in section [{section}]").into()),
        }
    }

    if version.as_deref() != Some(FORMAT_VERSION) {
        return Err(format!(
            "unsupported thresholds version '{:?}', expected {}",
            version, FORMAT_VERSION
        )
        .into());
    }
    let tier = tier.ok_or("missing 'tier' header in thresholds")?;
    if scenarios.is_empty() {
        return Err("threshold file contains no scenario sections".into());
    }

    Ok(ParsedThresholds { tier, scenarios })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_key_normalizes_names() {
        assert_eq!(scenario_key("V2 still"), "v2_still");
        assert_eq!(scenario_key("V2 animation"), "v2_animation");
    }

    #[test]
    fn parse_and_validate_thresholds() {
        let input = r#"
version=1
tier=desktop_mid

[v2_still]
latency_p50_ms_max=110.0
latency_p95_ms_max=140.0
frame_time_p50_ms_max=12.0
frame_time_p95_ms_max=16.0
throughput_p50_fps_min=45.0
throughput_p95_fps_min=40.0
"#;
        let parsed = parse_thresholds(input).expect("thresholds parse");
        assert_eq!(parsed.tier, "desktop_mid");
        assert_eq!(parsed.scenarios.len(), 1);
    }

    #[test]
    fn validate_thresholds_reports_missing_threshold_scenario() {
        let path = std::env::temp_dir().join(format!(
            "covergen_thresholds_missing_{}_{}.ini",
            std::process::id(),
            "scenario"
        ));
        let input = r#"
version=1
tier=desktop_mid

    [v2_compile]
    latency_p50_ms_max=110.0
    latency_p95_ms_max=140.0
frame_time_p50_ms_max=12.0
frame_time_p95_ms_max=16.0
throughput_p50_fps_min=45.0
throughput_p95_fps_min=40.0
"#;
        std::fs::write(&path, input).expect("write thresholds fixture");
        let summaries = vec![
            ScenarioSummary {
                name: "V2 compile",
                sample_count: 1,
                p50_ms: 100.0,
                p95_ms: 120.0,
                memory_p50_mb: 0.0,
                memory_p95_mb: 0.0,
                throughput_p50: 50.0,
                throughput_p95: 45.0,
                frame_time_p50_ms: 10.0,
                frame_time_p95_ms: 14.0,
            },
            ScenarioSummary {
                name: "V2 still",
                sample_count: 1,
                p50_ms: 90.0,
                p95_ms: 100.0,
                memory_p50_mb: 0.0,
                memory_p95_mb: 0.0,
                throughput_p50: 55.0,
                throughput_p95: 50.0,
                frame_time_p50_ms: 8.0,
                frame_time_p95_ms: 10.0,
            },
        ];
        let violations =
            validate_thresholds(&path, "desktop_mid", &summaries).expect("validate thresholds");
        assert!(
            violations
                .iter()
                .any(|line| line.contains("missing scenario 'v2_still' in threshold file"))
        );
        let _ = std::fs::remove_file(&path);
    }
}

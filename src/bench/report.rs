//! Markdown report rendering for benchmark output.

use crate::v2::cli::V2Profile;

use super::BenchConfig;
use super::stats::{NodeTimingSummary, ScenarioSummary};

/// Render benchmark summaries into a markdown report.
pub(super) fn render_report(
    config: &BenchConfig,
    summaries: &[ScenarioSummary],
    node_timing: &[NodeTimingSummary],
    skip_notes: &[String],
) -> String {
    let mut out = String::new();
    out.push_str("# covergen benchmark report\n\n");
    out.push_str("## configuration\n\n");
    out.push_str(&format!(
        "- samples: {}\n- animation samples: {}\n- size: {}\n- v2 preset/profile: {}/{}\n- animation: {}s @ {}fps\n\n",
        config.samples,
        config.animation_samples,
        config.size,
        config.preset,
        profile_label(config.profile),
        config.animation_seconds,
        config.animation_fps
    ));

    out.push_str("## scenario summary\n\n");
    out.push_str("| scenario | samples | p50 latency (ms) | p95 latency (ms) | p50 memory (MB) | p95 memory (MB) | p50 throughput (fps) | p95 throughput (fps) | p50 frame time (ms) | p95 frame time (ms) |\n");
    out.push_str("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for summary in summaries {
        out.push_str(&format!(
            "| {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
            summary.name,
            summary.sample_count,
            summary.p50_ms,
            summary.p95_ms,
            summary.memory_p50_mb,
            summary.memory_p95_mb,
            summary.throughput_p50,
            summary.throughput_p95,
            summary.frame_time_p50_ms,
            summary.frame_time_p95_ms,
        ));
    }

    out.push_str("\n## v2 gpu node timing\n\n");
    out.push_str("| node scope | samples | total (ms) | p50 (ms) | p95 (ms) |\n");
    out.push_str("|---|---:|---:|---:|---:|\n");
    if node_timing.is_empty() {
        out.push_str("| none | 0 | 0.00 | 0.00 | 0.00 |\n");
    } else {
        for row in node_timing {
            out.push_str(&format!(
                "| `{}` | {} | {:.2} | {:.2} | {:.2} |\n",
                row.scope, row.sample_count, row.total_ms, row.p50_ms, row.p95_ms,
            ));
        }
    }

    out.push_str("\n## notes\n\n");
    out.push_str(
        "- Latency/throughput percentiles are computed from wall-clock benchmark samples.\n",
    );
    out.push_str("- Memory uses process VmRSS/VmHWM snapshots from `/proc/self/status`.\n");
    out.push_str(
        "- Frame throughput is measured as rendered frames divided by sample wall time.\n",
    );
    out.push_str("- Animation render time is represented by the V2 animation scenario latency percentiles.\n");
    if !skip_notes.is_empty() {
        out.push_str("- Skipped scenarios:\n");
        for note in skip_notes {
            out.push_str(&format!("  - {note}\n"));
        }
    }

    out
}

fn profile_label(profile: V2Profile) -> &'static str {
    match profile {
        V2Profile::Quality => "quality",
        V2Profile::Performance => "performance",
    }
}

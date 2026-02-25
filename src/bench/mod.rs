//! V2 benchmark suite and performance report generator.
//!
//! This module executes V2 compile, still, and animation workloads with
//! telemetry capture enabled and writes a markdown report containing latency
//! percentiles, GPU-node timings, memory usage, frame throughput, animation
//! timing, and tiered cutover threshold artifacts.

mod baseline;
pub(crate) mod cli;
mod contracts;
mod report;
mod stats;

use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::animation::total_frames;
use crate::compiler::compile_graph;
use crate::presets::build_preset_graph;
use crate::runtime::execute_compiled;
use crate::runtime_config::{
    AnimationConfig, AnimationMotion, SelectionConfig, V2Config, V2Profile,
};
use crate::telemetry::{self, CaptureReport};

use baseline::{validate_thresholds, write_locked_thresholds, write_metrics_snapshot};
use contracts::validate_output_contract_for_bench;
use report::render_report;
use stats::{summarize_node_timings, summarize_scenario};

/// Parsed CLI options for `covergen bench`.
#[derive(Clone, Debug)]
pub(super) struct BenchConfig {
    tier: String,
    samples: u32,
    animation_samples: u32,
    require_v2_scenarios: bool,
    size: u32,
    seed: u32,
    out_dir: PathBuf,
    keep_artifacts: bool,
    preset: String,
    profile: V2Profile,
    animation_seconds: u32,
    animation_fps: u32,
    thresholds_path: Option<PathBuf>,
    lock_thresholds_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct ScenarioSample {
    elapsed_ms: f64,
    frame_count: u32,
    capture: CaptureReport,
}

/// Run the benchmark suite from a prevalidated configuration.
pub(crate) async fn run_with_config(config: BenchConfig) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(&config.out_dir)?;
    let skip_notes = Vec::new();
    let mut cutover_notes = Vec::new();

    let mut v2_compile_samples = Vec::with_capacity(config.samples as usize);
    let mut v2_still_samples = Vec::with_capacity(config.samples as usize);
    let mut v2_animation_samples = Vec::with_capacity(config.animation_samples as usize);
    let output_contract = probe_v2_output_contract(&config)?;
    cutover_notes.push(format!(
        "v2 output contract: primary={}, taps={}, total={}",
        output_contract.primary_count, output_contract.tap_count, output_contract.total_count
    ));

    for index in 0..config.samples {
        eprintln!("[bench] V2 compile sample {}/{}", index + 1, config.samples);
        v2_compile_samples.push(run_v2_compile_sample(&config, index)?);
    }

    for index in 0..config.samples {
        eprintln!("[bench] V2 still sample {}/{}", index + 1, config.samples);
        v2_still_samples.push(run_v2_still_sample(&config, index).await?);
    }

    for index in 0..config.animation_samples {
        eprintln!(
            "[bench] V2 animation sample {}/{}",
            index + 1,
            config.animation_samples
        );
        v2_animation_samples.push(run_v2_animation_sample(&config, index).await?);
    }

    let mut scenario_failures = Vec::new();
    if config.require_v2_scenarios {
        if v2_still_samples.is_empty() {
            scenario_failures.push(
                "V2 still scenario is required but no V2 still samples were captured".to_string(),
            );
        }
        if v2_animation_samples.is_empty() {
            scenario_failures.push(
                "V2 animation scenario is required but no V2 animation samples were captured"
                    .to_string(),
            );
        }
    }

    let summaries = vec![
        summarize_scenario("V2 compile", &v2_compile_samples),
        summarize_scenario("V2 still", &v2_still_samples),
        summarize_scenario("V2 animation", &v2_animation_samples),
    ];
    let metrics_path = config.out_dir.join("benchmark_metrics.ini");
    write_metrics_snapshot(&metrics_path, &config.tier, &summaries)?;
    cutover_notes.push(format!("metrics snapshot: {}", metrics_path.display()));

    if let Some(path) = config.lock_thresholds_path.as_ref() {
        if scenario_failures.is_empty() {
            write_locked_thresholds(path, &config.tier, &summaries)?;
            cutover_notes.push(format!("locked thresholds written: {}", path.display()));
        } else {
            cutover_notes.push(format!(
                "skipped locking thresholds due to missing required scenarios: {}",
                path.display()
            ));
        }
    }

    let mut threshold_failures = Vec::new();
    if let Some(path) = config.thresholds_path.as_ref() {
        let violations = validate_thresholds(path, &config.tier, &summaries)?;
        if violations.is_empty() {
            cutover_notes.push(format!("threshold validation passed: {}", path.display()));
        } else {
            cutover_notes.push(format!(
                "threshold validation failed ({} violations): {}",
                violations.len(),
                path.display()
            ));
            threshold_failures = violations;
        }
    }

    let node_timing = summarize_node_timings(&v2_still_samples, &v2_animation_samples);
    let report = render_report(
        &config,
        &summaries,
        &node_timing,
        &skip_notes,
        &cutover_notes,
    );

    let report_path = config.out_dir.join("benchmark_report.md");
    std::fs::write(&report_path, report)?;

    println!("Benchmark report written to {}", report_path.display());
    let mut failures = Vec::new();
    failures.extend(scenario_failures);
    failures.extend(threshold_failures);
    if !failures.is_empty() {
        let details = failures
            .into_iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("benchmark gate validation failed:\n{details}").into());
    }
    Ok(())
}

fn run_v2_compile_sample(
    config: &BenchConfig,
    sample: u32,
) -> Result<ScenarioSample, Box<dyn Error>> {
    let output_path = config
        .out_dir
        .join(format!("v2_compile_sample_{sample}.png"));
    let mut run_cfg = v2_base_config(config, output_path.to_string_lossy().into_owned());
    run_cfg.seed = run_cfg.seed.wrapping_add(sample.wrapping_mul(0x9E37_79B9));
    let run_label = format!("v2.compile.sample.{sample}");
    telemetry::begin_capture(run_label);
    telemetry::snapshot_memory("run.start");
    let start = Instant::now();
    let graph = build_preset_graph(&run_cfg)?;
    let compiled = compile_graph(&graph)?;
    validate_output_contract_for_bench(&compiled)?;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    telemetry::snapshot_memory("run.end");
    let capture =
        telemetry::end_capture().ok_or("missing telemetry capture for v2 compile sample")?;

    Ok(ScenarioSample {
        elapsed_ms: elapsed,
        frame_count: 1,
        capture,
    })
}

async fn run_v2_still_sample(
    config: &BenchConfig,
    sample: u32,
) -> Result<ScenarioSample, Box<dyn Error>> {
    let output_path = config.out_dir.join(format!("v2_still_sample_{sample}.png"));
    let run_cfg = v2_base_config(config, output_path.to_string_lossy().into_owned());

    let run_label = format!("v2.still.sample.{sample}");
    telemetry::begin_capture(run_label);
    telemetry::snapshot_memory("run.start");
    let start = Instant::now();
    execute_v2_once(&run_cfg).await?;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    telemetry::snapshot_memory("run.end");
    let capture =
        telemetry::end_capture().ok_or("missing telemetry capture for v2 still sample")?;

    cleanup_artifact(&output_path, config.keep_artifacts);

    Ok(ScenarioSample {
        elapsed_ms: elapsed,
        frame_count: 1,
        capture,
    })
}

async fn run_v2_animation_sample(
    config: &BenchConfig,
    sample: u32,
) -> Result<ScenarioSample, Box<dyn Error>> {
    let output_path = config
        .out_dir
        .join(format!("v2_animation_sample_{sample}.mp4"));
    let mut run_cfg = v2_base_config(config, output_path.to_string_lossy().into_owned());
    run_cfg.animation = AnimationConfig {
        enabled: true,
        seconds: config.animation_seconds,
        fps: config.animation_fps,
        keep_frames: false,
        motion: AnimationMotion::Normal,
    };

    let frame_count = total_frames(&run_cfg.animation);
    let run_label = format!("v2.animation.sample.{sample}");
    telemetry::begin_capture(run_label);
    telemetry::snapshot_memory("run.start");
    let start = Instant::now();
    execute_v2_once(&run_cfg).await?;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    telemetry::snapshot_memory("run.end");
    let capture =
        telemetry::end_capture().ok_or("missing telemetry capture for v2 animation sample")?;

    cleanup_artifact(&output_path, config.keep_artifacts);

    Ok(ScenarioSample {
        elapsed_ms: elapsed,
        frame_count,
        capture,
    })
}

fn v2_base_config(config: &BenchConfig, output: String) -> V2Config {
    V2Config {
        width: config.size,
        height: config.size,
        seed: config.seed,
        count: 1,
        output,
        layers: 4,
        antialias: 1,
        preset: config.preset.clone(),
        profile: config.profile,
        manifest_out: None,
        manifest_in: None,
        art_direction: crate::art_direction::ArtDirectionConfig::default(),
        animation: AnimationConfig {
            enabled: false,
            seconds: config.animation_seconds,
            fps: config.animation_fps,
            keep_frames: false,
            motion: AnimationMotion::Normal,
        },
        selection: SelectionConfig {
            explore_candidates: 0,
            explore_size: 320,
        },
        gui: crate::runtime_config::GuiConfig::default(),
    }
}

async fn execute_v2_once(config: &V2Config) -> Result<(), Box<dyn Error>> {
    let graph = build_preset_graph(config)?;
    let compiled = compile_graph(&graph)?;
    execute_compiled(config, &compiled, None).await
}

fn probe_v2_output_contract(
    config: &BenchConfig,
) -> Result<contracts::OutputContractSummary, Box<dyn Error>> {
    let probe_cfg = v2_base_config(config, "bench_contract_probe.png".to_string());
    let graph = build_preset_graph(&probe_cfg)?;
    let compiled = compile_graph(&graph)?;
    validate_output_contract_for_bench(&compiled)
}

fn cleanup_artifact(path: &Path, keep: bool) {
    if keep {
        return;
    }
    let _ = std::fs::remove_file(path);
}

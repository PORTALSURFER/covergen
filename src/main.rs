//! Binary entrypoint for the generative cover generator.

use std::error::Error;
use std::path::Path;

mod animation;
mod art_direction;
mod bench;
mod chop;
mod cli;
mod compiler;
mod gpu_render;
mod gpu_retained;
mod graph;
mod image_ops;
mod manifest;
mod model;
mod node;
mod presets;
mod proc_graph;
mod runtime;
mod runtime_config;
#[cfg(test)]
mod runtime_config_tests;
#[cfg(test)]
mod runtime_eval;
mod runtime_gpu;
#[cfg(test)]
mod runtime_ops;
mod runtime_progress;
mod runtime_selection;
#[cfg(test)]
mod runtime_test_support;
mod selection;
mod shaders;
mod sop;
mod telemetry;
mod temporal;
#[cfg(test)]
mod visual_regression;
#[cfg(test)]
mod visual_regression_fixtures;
#[cfg(test)]
mod visual_regression_gpu;
#[cfg(test)]
mod visual_regression_movie_metrics;

use clap::Parser;
use cli::{CovergenCli, CovergenCommand};
use compiler::compile_graph;
use graph::GpuGraph;
use manifest::{load_manifest, save_manifest};
use presets::build_preset_graph;
use runtime::execute_compiled;
use runtime_config::{V2Args, V2Config};

/// Parse CLI arguments and execute the selected command.
fn run() -> Result<(), Box<dyn Error>> {
    let cli = CovergenCli::parse();
    match cli.command {
        Some(CovergenCommand::Bench(args)) => {
            let config = bench::cli::bench_config_from_args(args)?;
            pollster::block_on(bench::run_with_config(config))
        }
        None => pollster::block_on(run_covergen(cli.run)),
    }
}

async fn run_covergen(args: V2Args) -> Result<(), Box<dyn Error>> {
    let config = V2Config::from_args(args)?;
    if let Some(path) = config.manifest_in.as_deref() {
        return run_manifest_replay(&config, path).await;
    }

    let low_res_config = config.low_res_explore_config();
    let graph = build_preset_graph(&config)?;
    let compiled = compile_graph(&graph)?;
    maybe_save_manifest(config.manifest_out.as_deref(), &config, &graph)?;
    let low_res_compiled = if let Some(low_res) = low_res_config.as_ref() {
        let low_graph = build_preset_graph(low_res)?;
        Some(compile_graph(&low_graph)?)
    } else {
        None
    };
    execute_compiled(
        &config,
        &compiled,
        low_res_config.as_ref().zip(low_res_compiled.as_ref()),
    )
    .await
}

/// Execute one graph replay from a saved manifest.
async fn run_manifest_replay(cli_config: &V2Config, path: &str) -> Result<(), Box<dyn Error>> {
    let manifest = load_manifest(Path::new(path))?;
    let mut replay_config = manifest.config;
    replay_config.manifest_in = Some(path.to_string());
    replay_config.manifest_out = cli_config.manifest_out.clone();

    let compiled = compile_graph(&manifest.graph)?;
    maybe_save_manifest(
        replay_config.manifest_out.as_deref(),
        &replay_config,
        &manifest.graph,
    )?;
    execute_compiled(&replay_config, &compiled, None).await
}

/// Persist the generated graph/config pair when manifest output is enabled.
fn maybe_save_manifest(
    manifest_out: Option<&str>,
    config: &V2Config,
    graph: &GpuGraph,
) -> Result<(), Box<dyn Error>> {
    let Some(path) = manifest_out else {
        return Ok(());
    };
    let saved = save_manifest(Path::new(path), config, graph)?;
    println!("[v2] wrote replay manifest {}", saved.display());
    Ok(())
}

/// Program entrypoint with explicit process exit handling.
fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        if error_requires_hardware_gpu(err.as_ref()) {
            eprintln!(
                "Warning: covergen requires a hardware GPU. Software adapters and CPU fallback are disabled."
            );
        }
        std::process::exit(1);
    }
}

/// Return true when the failure indicates a missing hardware-GPU requirement.
fn error_requires_hardware_gpu(err: &(dyn Error + 'static)) -> bool {
    err.to_string().contains("requires a hardware GPU adapter")
}

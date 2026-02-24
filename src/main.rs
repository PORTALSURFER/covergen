//! Binary entrypoint for the generative cover generator.

use std::error::Error;

mod animation;
mod bench;
mod cli;
mod compiler;
mod gpu_render;
mod gpu_retained;
mod graph;
mod image_ops;
mod model;
mod node;
mod presets;
mod runtime;
mod runtime_config;
#[cfg(test)]
mod runtime_eval;
mod runtime_gpu;
#[cfg(test)]
mod runtime_ops;
#[cfg(test)]
mod runtime_test_support;
mod shaders;
mod telemetry;
mod temporal;
#[cfg(test)]
mod visual_regression;
#[cfg(test)]
mod visual_regression_fixtures;
#[cfg(test)]
mod visual_regression_gpu;

use clap::Parser;
use cli::{CovergenCli, CovergenCommand};
use compiler::compile_graph;
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
    let graph = build_preset_graph(&config)?;
    let compiled = compile_graph(&graph)?;
    execute_compiled(&config, &compiled).await
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

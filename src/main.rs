//! Binary entrypoint for the generative cover generator.

use std::error::Error;

mod bench;
mod cli;
mod gpu_render;
mod gpu_retained;
mod image_ops;
mod model;
mod shaders;
mod telemetry;
mod v2;

use clap::Parser;
use cli::{CovergenCli, CovergenCommand};

/// Parse CLI arguments and execute the selected command.
fn run() -> Result<(), Box<dyn Error>> {
    let cli = CovergenCli::parse();
    match cli.command {
        Some(CovergenCommand::V2(args)) => pollster::block_on(v2::run_from_cli_args(args)),
        Some(CovergenCommand::Bench(args)) => {
            let config = bench::cli::bench_config_from_args(args)?;
            pollster::block_on(bench::run_with_config(config))
        }
        None => pollster::block_on(v2::run_from_cli_args(cli.v2)),
    }
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

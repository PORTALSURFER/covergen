//! Binary entrypoint for the generative cover generator.

use std::error::Error;

mod analysis;
mod bench;
mod blending;
mod cli;
mod config;
mod engine;
mod gpu_render;
mod gpu_retained;
mod image_ops;
mod model;
mod progress;
mod randomization;
mod render_workspace;
mod shaders;
mod strategies;
mod telemetry;
mod v2;

use clap::Parser;
use cli::{CovergenCli, CovergenCommand};

fn main() -> Result<(), Box<dyn Error>> {
    let cli = CovergenCli::parse();
    match cli.command {
        Some(CovergenCommand::V1) => {
            Err("`covergen v1` was removed on February 24, 2026 after the V2 cutover stabilization window. Use `covergen v2 ...`.".into())
        }
        Some(CovergenCommand::V2(args)) => pollster::block_on(v2::run_from_cli_args(args)),
        Some(CovergenCommand::Bench(args)) => {
            let config = bench::cli::bench_config_from_args(args)?;
            pollster::block_on(bench::run_with_config(config))
        }
        None => pollster::block_on(v2::run_from_cli_args(cli.v2)),
    }
}

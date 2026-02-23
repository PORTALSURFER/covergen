//! Binary entrypoint for the generative cover generator.

use std::error::Error;

mod analysis;
mod bench;
mod blending;
mod config;
mod engine;
mod gpu_render;
mod gpu_retained;
mod image_ops;
mod model;
mod progress;
mod randomization;
mod render_workspace;
mod strategies;
mod telemetry;
mod v2;

use crate::config::Config;
use crate::engine::run;

fn main() -> Result<(), Box<dyn Error>> {
    let mut argv = std::env::args();
    let _bin = argv.next();
    match argv.next().as_deref() {
        Some("v2") => return pollster::block_on(v2::run_from_args(argv.collect())),
        Some("bench") => return pollster::block_on(bench::run_from_args(argv.collect())),
        _ => {}
    }

    eprintln!("[legacy] running V1 pipeline; use `covergen v2 ...` for the graph runtime");
    let config = Config::from_env()?;
    pollster::block_on(run(config))
}

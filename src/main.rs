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
mod shaders;
mod strategies;
mod telemetry;
mod v2;

use crate::config::Config;
use crate::engine::run;

/// Execute the legacy V1 pipeline using environment-driven configuration.
fn run_v1() -> Result<(), Box<dyn Error>> {
    eprintln!(
        "[legacy] running V1 pipeline (deprecated); default path is now V2. \
use `covergen v2 ...` or `covergen v1` explicitly."
    );
    let config = Config::from_env()?;
    pollster::block_on(run(config))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut argv = std::env::args();
    let _bin = argv.next();
    let first = argv.next();
    match first.as_deref() {
        Some("v1") => return run_v1(),
        Some("v2") => return pollster::block_on(v2::run_from_args(argv.collect())),
        Some("bench") => return pollster::block_on(bench::run_from_args(argv.collect())),
        Some(other) => {
            let mut forwarded = vec![other.to_string()];
            forwarded.extend(argv);
            return pollster::block_on(v2::run_from_args(forwarded));
        }
        None => return pollster::block_on(v2::run_from_args(Vec::new())),
    }
}

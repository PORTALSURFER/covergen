//! Binary entrypoint for the generative cover generator.

use std::error::Error;

mod analysis;
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
mod v2;

use crate::config::Config;
use crate::engine::run;

fn main() -> Result<(), Box<dyn Error>> {
    let mut argv = std::env::args();
    let _bin = argv.next();
    if matches!(argv.next().as_deref(), Some("v2")) {
        return pollster::block_on(v2::run_from_args(argv.collect()));
    }

    let config = Config::from_env()?;
    pollster::block_on(run(config))
}

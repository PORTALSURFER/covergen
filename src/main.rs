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

use crate::config::Config;
use crate::engine::run;

fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    pollster::block_on(run(config))
}

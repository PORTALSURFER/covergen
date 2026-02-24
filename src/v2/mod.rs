//! V2 generated node-graph runtime.
//!
//! This module hosts the clean-break architecture for graph-authored, GPU-first
//! rendering. Graphs are generated in code, compiled into an execution plan,
//! and executed via retained GPU buffers with one image-end readback.

pub mod animation;
pub mod cli;
pub mod compiler;
pub mod graph;
pub mod node;
pub mod presets;
pub mod runtime;
#[cfg(test)]
mod runtime_eval;
mod runtime_gpu;
#[cfg(test)]
mod runtime_ops;
#[cfg(test)]
mod runtime_test_support;
pub mod temporal;
#[cfg(test)]
mod visual_regression;
#[cfg(test)]
mod visual_regression_fixtures;
#[cfg(test)]
mod visual_regression_gpu;

use std::error::Error;

use cli::{V2Args, V2Config};
use compiler::compile_graph;
use presets::build_preset_graph;
use runtime::execute_compiled;

/// Convert parsed V2 arguments and execute the V2 graph runtime.
pub async fn run_from_cli_args(args: V2Args) -> Result<(), Box<dyn Error>> {
    let config = V2Config::from_args(args)?;
    run_with_config(config).await
}

/// Execute the V2 graph runtime from a prevalidated configuration.
pub async fn run_with_config(config: V2Config) -> Result<(), Box<dyn Error>> {
    let graph = build_preset_graph(&config)?;
    let compiled = compile_graph(&graph)?;
    execute_compiled(&config, &compiled).await
}

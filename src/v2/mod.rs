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
mod runtime_eval;
mod runtime_ops;

use std::error::Error;

use cli::V2Config;
use compiler::compile_graph;
use presets::build_preset_graph;
use runtime::execute_compiled;

/// Parse `v2` arguments and execute the V2 graph runtime.
pub async fn run_from_args(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let config = V2Config::parse(args)?;
    let graph = build_preset_graph(&config)?;
    let compiled = compile_graph(&graph)?;
    execute_compiled(&config, &compiled).await
}

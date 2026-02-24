//! Output-contract checks for benchmark V2 graph compilation.
//!
//! Benchmarks are intended to exercise multi-output graph behavior. This module
//! validates that benchmark presets compile with one primary output and at
//! least one tap output so contract regressions are detected early.

use std::error::Error;

use crate::compiler::CompiledGraph;
use crate::node::OutputRole;

/// Summary of compiled output bindings for a benchmark graph.
#[derive(Clone, Copy, Debug)]
pub(super) struct OutputContractSummary {
    pub(super) primary_count: usize,
    pub(super) tap_count: usize,
    pub(super) total_count: usize,
}

/// Validate that benchmark graph output contracts include one primary output
/// and at least one tap output.
pub(super) fn validate_output_contract_for_bench(
    compiled: &CompiledGraph,
) -> Result<OutputContractSummary, Box<dyn Error>> {
    let mut primary_count = 0usize;
    let mut tap_count = 0usize;
    for binding in &compiled.output_bindings {
        match binding.role {
            OutputRole::Primary => primary_count += 1,
            OutputRole::Tap => tap_count += 1,
        }
    }

    if primary_count != 1 {
        return Err(format!(
            "benchmark graph requires exactly one primary output binding, got {}",
            primary_count
        )
        .into());
    }
    if tap_count == 0 {
        return Err("benchmark graph requires at least one tap output binding".into());
    }

    Ok(OutputContractSummary {
        primary_count,
        tap_count,
        total_count: compiled.output_bindings.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::compile_graph;
    use crate::graph::GraphBuilder;
    use crate::node::{PortType, SourceNoiseNode, SourceNoiseTemporal};

    #[test]
    fn benchmark_contract_accepts_primary_and_tap_outputs() {
        let compiled = compile_fixture(true);
        let summary = validate_output_contract_for_bench(&compiled)
            .expect("benchmark contract should accept primary + tap");
        assert_eq!(summary.primary_count, 1);
        assert_eq!(summary.tap_count, 1);
        assert_eq!(summary.total_count, 2);
    }

    #[test]
    fn benchmark_contract_rejects_missing_tap_output() {
        let compiled = compile_fixture(false);
        let err = validate_output_contract_for_bench(&compiled)
            .expect_err("benchmark contract should reject missing tap");
        assert!(err.to_string().contains("tap output binding"));
    }

    fn compile_fixture(include_tap: bool) -> CompiledGraph {
        let mut builder = GraphBuilder::new(128, 128, 0xAA55_0033);
        let source = builder.add_source_noise(SourceNoiseNode {
            seed: 0xBB66_1144,
            scale: 3.2,
            octaves: 4,
            amplitude: 1.0,
            output_port: PortType::LumaTexture,
            temporal: SourceNoiseTemporal::default(),
        });
        if include_tap {
            let tap = builder.add_output_tap(1);
            builder.connect_luma(source, tap);
        }
        let output = builder.add_output();
        builder.connect_luma(source, output);
        let graph = builder.build().expect("build fixture graph");
        compile_graph(&graph).expect("compile fixture graph")
    }
}

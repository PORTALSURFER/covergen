//! Shared helpers for executing reusable subgraph modules from presets.

use crate::graph::{GraphBuildError, GraphBuilder, NodeId};
use crate::model::XorShift32;

use super::preset_catalog::PresetContext;
use super::subgraph_catalog::{ModuleBuildContext, ModuleParams, ModuleRequest, ModuleResult};

/// Execute the `noise-mask` module with default parameters.
pub(super) fn module_noise_mask(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    seed: u32,
) -> Result<ModuleResult, GraphBuildError> {
    module_noise_mask_with_params(
        builder,
        ctx,
        rng,
        seed,
        ModuleParams {
            intensity: 1.0,
            variation: 0.5,
            blend_bias: 0.5,
        },
    )
}

/// Execute the `noise-mask` module with explicit parameters.
pub(super) fn module_noise_mask_with_params(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    seed: u32,
    params: ModuleParams,
) -> Result<ModuleResult, GraphBuildError> {
    execute_module(builder, ctx, rng, "noise-mask", Vec::new(), seed, params)
}

/// Execute the `masked-blend` module with default parameters.
pub(super) fn module_masked_blend(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    inputs: Vec<NodeId>,
    seed: u32,
) -> Result<ModuleResult, GraphBuildError> {
    module_masked_blend_with_params(
        builder,
        ctx,
        rng,
        inputs,
        seed,
        ModuleParams {
            intensity: 1.0,
            variation: 0.5,
            blend_bias: 0.5,
        },
    )
}

/// Execute the `masked-blend` module with explicit parameters.
pub(super) fn module_masked_blend_with_params(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    inputs: Vec<NodeId>,
    seed: u32,
    params: ModuleParams,
) -> Result<ModuleResult, GraphBuildError> {
    execute_module(builder, ctx, rng, "masked-blend", inputs, seed, params)
}

/// Execute one named motif module with explicit parameters.
pub(super) fn module_motif(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    key: &str,
    inputs: Vec<NodeId>,
    seed: u32,
    params: ModuleParams,
) -> Result<ModuleResult, GraphBuildError> {
    execute_module(builder, ctx, rng, key, inputs, seed, params)
}

fn execute_module(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    key: &str,
    inputs: Vec<NodeId>,
    seed: u32,
    params: ModuleParams,
) -> Result<ModuleResult, GraphBuildError> {
    let mut module_ctx = ModuleBuildContext {
        builder,
        nodes: ctx.nodes,
        rng,
    };
    let result = ctx.modules.execute(
        key,
        &mut module_ctx,
        ModuleRequest::new(seed, ctx.config.profile, inputs).with_params(params),
    )?;
    let _extra_count = result.extra_outputs.len();
    Ok(result)
}

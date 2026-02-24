//! Parameterized motif subgraph blocks for reusable preset composition.

use crate::graph::{GraphBuildError, NodeId, StatefulFeedbackNode};
use crate::model::LayerBlendMode;

use super::node_catalog::NodePayload;
use super::primitives::{random_blend, random_tonemap, random_warp};
use super::subgraph_catalog::{ModuleBuildContext, ModuleRequest, ModuleResult};

/// Build a flowing warp+tone motif with masked ribbon blending.
pub(super) fn build_motif_ribbon(
    context: &mut ModuleBuildContext<'_>,
    request: ModuleRequest,
) -> Result<ModuleResult, GraphBuildError> {
    let input = primary_input("motif-ribbon", &request)?;
    let params = request.params.clamped();
    let warp_scale = 0.55 + params.intensity * (0.8 + params.variation * 0.5);
    let warp = context.nodes.create(
        context.builder,
        "warp",
        NodePayload::WarpTransform(random_warp(context.rng, warp_scale)),
    )?;
    context.builder.connect_luma(input, warp);

    let mut tone_spec = random_tonemap(context.rng);
    tone_spec.contrast = (tone_spec.contrast * (0.88 + 0.30 * params.intensity)
        + params.blend_bias * 0.14)
        .clamp(0.85, 3.2);
    let tone = context
        .nodes
        .create(context.builder, "tone", NodePayload::ToneMap(tone_spec))?;
    context.builder.connect_luma(warp, tone);

    let blend = context.nodes.create(
        context.builder,
        "blend",
        NodePayload::Blend(random_blend(
            context.rng,
            LayerBlendMode::Screen,
            0.28 + 0.24 * params.blend_bias,
            0.66 + 0.28 * params.intensity,
        )),
    )?;
    context.builder.connect_luma_input(input, blend, 0);
    context.builder.connect_luma_input(tone, blend, 1);
    Ok(ModuleResult {
        primary: blend,
        extra_outputs: vec![warp, tone],
    })
}

/// Build a feedback-driven echo motif that adds temporal memory texture.
pub(super) fn build_motif_echo(
    context: &mut ModuleBuildContext<'_>,
    request: ModuleRequest,
) -> Result<ModuleResult, GraphBuildError> {
    let input = primary_input("motif-echo", &request)?;
    let params = request.params.clamped();
    let feedback = context.nodes.create(
        context.builder,
        "feedback",
        NodePayload::StatefulFeedback(StatefulFeedbackNode {
            mix: (0.14 + params.intensity * 0.46 + params.variation * 0.18).clamp(0.05, 0.9),
        }),
    )?;
    context.builder.connect_luma(input, feedback);

    let warp = context.nodes.create(
        context.builder,
        "warp",
        NodePayload::WarpTransform(random_warp(context.rng, 0.5 + params.intensity * 0.65)),
    )?;
    context.builder.connect_luma(feedback, warp);

    let blend = context.nodes.create(
        context.builder,
        "blend",
        NodePayload::Blend(random_blend(
            context.rng,
            LayerBlendMode::Overlay,
            0.20 + 0.25 * params.blend_bias,
            0.58 + 0.30 * params.intensity,
        )),
    )?;
    context.builder.connect_luma_input(input, blend, 0);
    context.builder.connect_luma_input(warp, blend, 1);
    Ok(ModuleResult {
        primary: blend,
        extra_outputs: vec![feedback, warp],
    })
}

/// Build a dual-tone motif that separates low/high tone treatment before recombine.
pub(super) fn build_motif_dual_tone(
    context: &mut ModuleBuildContext<'_>,
    request: ModuleRequest,
) -> Result<ModuleResult, GraphBuildError> {
    let input = primary_input("motif-dual-tone", &request)?;
    let params = request.params.clamped();

    let mut tone_a = random_tonemap(context.rng);
    tone_a.contrast = (tone_a.contrast * (0.82 + 0.24 * params.intensity)).clamp(0.85, 2.6);
    tone_a.low_pct = (tone_a.low_pct + params.variation * 0.02).clamp(0.0, 0.4);
    let low_tone = context
        .nodes
        .create(context.builder, "tone", NodePayload::ToneMap(tone_a))?;
    context.builder.connect_luma(input, low_tone);

    let mut tone_b = random_tonemap(context.rng);
    tone_b.contrast = (tone_b.contrast * (0.95 + 0.34 * params.intensity)).clamp(0.95, 3.2);
    tone_b.high_pct = (tone_b.high_pct - params.variation * 0.04).clamp(0.6, 1.0);
    let high_tone = context
        .nodes
        .create(context.builder, "tone", NodePayload::ToneMap(tone_b))?;
    context.builder.connect_luma(input, high_tone);

    let blend = context.nodes.create(
        context.builder,
        "blend",
        NodePayload::Blend(random_blend(
            context.rng,
            LayerBlendMode::Lighten,
            0.24 + 0.24 * params.blend_bias,
            0.62 + 0.32 * params.intensity,
        )),
    )?;
    context.builder.connect_luma_input(low_tone, blend, 0);
    context.builder.connect_luma_input(high_tone, blend, 1);
    Ok(ModuleResult {
        primary: blend,
        extra_outputs: vec![low_tone, high_tone],
    })
}

fn primary_input(module_key: &str, request: &ModuleRequest) -> Result<NodeId, GraphBuildError> {
    request.inputs.first().copied().ok_or_else(|| {
        GraphBuildError::new(format!(
            "module '{module_key}' expects one luma input in request.inputs[0]"
        ))
    })
}

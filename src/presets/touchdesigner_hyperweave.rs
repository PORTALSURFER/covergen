//! TouchDesigner-style hyperweave preset with divergent intermediate output taps.

use crate::chop::{ChopMathMode, ChopMathNode, ChopWave};
use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder, NodeId};
use crate::model::{LayerBlendMode, XorShift32};
use crate::node::OutputNode;

use super::node_catalog::NodePayload;
use super::preset_catalog::PresetContext;
use super::primitives::{
    generate_layer_node, random_blend, random_tonemap, random_warp, render_size,
};
use super::subgraph_catalog::{ModuleBuildContext, ModuleRequest, ModuleResult};
use super::touchdesigner_stage_primitives::{
    add_camera, add_circle, add_lfo, add_remap, add_sphere, pick,
};

#[derive(Clone, Copy)]
struct ControlBus {
    zoom: NodeId,
    warp: NodeId,
    tone: NodeId,
}

#[derive(Clone, Copy)]
struct StageOutputs {
    merged: NodeId,
    warped: NodeId,
    toned: NodeId,
}

/// Build a cross-woven TD network and expose multiple meaningful output taps.
pub(super) fn build_td_hyperweave(ctx: PresetContext<'_>) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0xAA49_31C7);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0x17C0_5D93);

    let controls = build_controls(&mut builder, ctx, &mut rng)?;
    let cameras = build_cameras(&mut builder, ctx, &mut rng, controls)?;
    let layers = build_layers(&mut builder, ctx, &mut rng)?;

    let stages = build_hyperweave_stages(&mut builder, ctx, &mut rng, controls, &cameras, &layers)?;
    let outputs = finalize_hyperweave(&mut builder, ctx, &mut rng, controls, stages)?;
    wire_outputs(&mut builder, ctx, outputs)?;

    builder.build()
}

fn build_controls(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<ControlBus, GraphBuildError> {
    let lfo_a = add_lfo(builder, ctx, rng, ChopWave::Sine, 0.13, 0.48)?;
    let lfo_b = add_lfo(builder, ctx, rng, ChopWave::Triangle, 0.09, 0.34)?;
    let lfo_c = add_lfo(builder, ctx, rng, ChopWave::Saw, 0.06, 0.23)?;

    let mix = ctx.nodes.create(
        builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.32 + rng.next_f32() * 0.44,
        }),
    )?;
    builder.connect_channel_input(lfo_a, mix, 0);
    builder.connect_channel_input(lfo_b, mix, 1);

    let warp_mix = ctx.nodes.create(
        builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Multiply,
            value: 1.0,
            blend: 0.5,
        }),
    )?;
    builder.connect_channel_input(lfo_b, warp_mix, 0);
    builder.connect_channel_input(lfo_c, warp_mix, 1);

    let zoom = add_remap(builder, ctx, 0.76, 1.58)?;
    builder.connect_channel_input(mix, zoom, 0);

    let warp = add_remap(builder, ctx, 0.28, 1.78)?;
    builder.connect_channel_input(warp_mix, warp, 0);

    let tone = add_remap(builder, ctx, 0.72, 1.42)?;
    builder.connect_channel_input(lfo_a, tone, 0);

    Ok(ControlBus { zoom, warp, tone })
}

fn build_cameras(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut shapes = Vec::with_capacity(6);
    for _ in 0..3 {
        shapes.push(add_circle(builder, ctx, rng)?);
    }
    for _ in 0..3 {
        shapes.push(add_sphere(builder, ctx, rng)?);
    }

    let count = ctx.config.layers.clamp(5, 8);
    let mut cameras = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let camera = add_camera(builder, ctx, rng)?;
        builder.connect_sop_input(pick(&shapes, rng)?, camera, 0);
        builder.connect_channel_input(controls.zoom, camera, 1);
        cameras.push(camera);
    }
    Ok(cameras)
}

fn build_layers(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut layers = Vec::with_capacity(4);
    for i in 0..4 {
        let layer = generate_layer_node(i, 4, ctx.config.profile, rng, true);
        layers.push(ctx.nodes.create(
            builder,
            "generate-layer",
            NodePayload::GenerateLayer(layer),
        )?);
    }
    Ok(layers)
}

fn build_hyperweave_stages(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
    cameras: &[NodeId],
    layers: &[NodeId],
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut branches = Vec::with_capacity(cameras.len());

    for (i, camera) in cameras.iter().enumerate() {
        let layer = layers[i % layers.len()];

        let masked = module(
            builder,
            ctx,
            rng,
            "masked-blend",
            ctx.config.seed ^ (i as u32).wrapping_mul(0x91A5_4001),
            vec![*camera, layer],
        )?
        .primary;

        let warp = ctx.nodes.create(
            builder,
            "warp-transform",
            NodePayload::WarpTransform({
                let warp_scale = 0.72 + rng.next_f32() * 0.5;
                random_warp(rng, warp_scale)
            }),
        )?;
        builder.connect_luma(masked, warp);
        builder.connect_channel_input(controls.warp, warp, 1);

        let tone = ctx.nodes.create(
            builder,
            "tone-map",
            NodePayload::ToneMap(random_tonemap(rng)),
        )?;
        builder.connect_luma(warp, tone);
        builder.connect_channel_input(controls.tone, tone, 1);

        branches.push(tone);
    }

    Ok(branches)
}

fn finalize_hyperweave(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
    mut branches: Vec<NodeId>,
) -> Result<StageOutputs, GraphBuildError> {
    if branches.is_empty() {
        return Err(GraphBuildError::new(
            "hyperweave requires at least one branch",
        ));
    }

    let mut seed = ctx.config.seed ^ 0xDD11_7301;
    while branches.len() > 1 {
        let a = branches
            .pop()
            .ok_or_else(|| GraphBuildError::new("missing branch a"))?;
        let b = branches
            .pop()
            .ok_or_else(|| GraphBuildError::new("missing branch b"))?;
        let merged = module(builder, ctx, rng, "masked-blend", seed, vec![a, b])?.primary;
        branches.push(merged);
        seed = seed.wrapping_add(0x9E37_79B9);
    }

    let merged = branches[0];

    let warp = ctx.nodes.create(
        builder,
        "warp-transform",
        NodePayload::WarpTransform(random_warp(rng, 0.9)),
    )?;
    builder.connect_luma(merged, warp);
    builder.connect_channel_input(controls.warp, warp, 1);

    let tone = ctx.nodes.create(
        builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(rng)),
    )?;
    builder.connect_luma(warp, tone);
    builder.connect_channel_input(controls.tone, tone, 1);

    let polish = ctx.nodes.create(
        builder,
        "blend",
        NodePayload::Blend(random_blend(rng, LayerBlendMode::Overlay, 0.34, 0.88)),
    )?;
    builder.connect_luma_input(tone, polish, 0);
    builder.connect_luma_input(tone, polish, 1);

    Ok(StageOutputs {
        merged,
        warped: warp,
        toned: polish,
    })
}

fn wire_outputs(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    stages: StageOutputs,
) -> Result<(), GraphBuildError> {
    let tap_merge = ctx
        .nodes
        .create(builder, "output", NodePayload::Output(OutputNode::tap(1)))?;
    builder.connect_luma(stages.merged, tap_merge);

    let tap_warp = ctx
        .nodes
        .create(builder, "output", NodePayload::Output(OutputNode::tap(2)))?;
    builder.connect_luma(stages.warped, tap_warp);

    let tap_tone = ctx
        .nodes
        .create(builder, "output", NodePayload::Output(OutputNode::tap(3)))?;
    builder.connect_luma(stages.toned, tap_tone);

    let output = ctx.nodes.create(
        builder,
        "output",
        NodePayload::Output(OutputNode::primary()),
    )?;
    builder.connect_luma(stages.toned, output);
    Ok(())
}

fn module(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    key: &str,
    seed: u32,
    inputs: Vec<NodeId>,
) -> Result<ModuleResult, GraphBuildError> {
    let mut module_ctx = ModuleBuildContext {
        builder,
        nodes: ctx.nodes,
        rng,
    };
    ctx.modules.execute(
        key,
        &mut module_ctx,
        ModuleRequest {
            seed,
            profile: ctx.config.profile,
            inputs,
        },
    )
}

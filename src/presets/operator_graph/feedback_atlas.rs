//! operator-family feedback atlas preset with staged masked reinjection.

use crate::chop::{ChopMathMode, ChopMathNode, ChopWave};
use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder, NodeId};
use crate::model::{LayerBlendMode, XorShift32};
use crate::node::OutputNode;

use super::node_catalog::NodePayload;
use super::operator_graph_stage_primitives::{
    add_camera, add_circle, add_lfo, add_remap, add_sphere, pick,
};
use super::preset_catalog::PresetContext;
use super::primitives::{
    generate_layer_node, random_blend, random_tonemap, random_warp, render_size,
};
use super::subgraph_catalog::{ModuleBuildContext, ModuleRequest, ModuleResult};

#[derive(Clone, Copy)]
struct ControlBus {
    zoom: NodeId,
    warp: NodeId,
    tone: NodeId,
}

/// Build an operator-family feedback atlas graph with multiple tap outputs.
pub(super) fn build_operator_feedback_atlas(
    ctx: PresetContext<'_>,
) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0xB481_26AF);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0x395A_F1D3);

    let controls = build_controls(&mut builder, ctx, &mut rng)?;
    let cameras = build_camera_pool(&mut builder, ctx, &mut rng, controls)?;
    let layers = build_layer_pool(&mut builder, ctx, &mut rng)?;

    let mut branches = Vec::with_capacity(cameras.len());
    for (index, camera) in cameras.iter().enumerate() {
        let branch = build_branch(
            &mut builder,
            ctx,
            &mut rng,
            controls,
            *camera,
            layers[index % layers.len()],
            (ctx.config.seed ^ 0x7AC1_2201).wrapping_add(index as u32),
        )?;
        branches.push(branch);
    }

    let merged = merge_branches(&mut builder, ctx, &mut rng, branches)?;
    let finished = add_finish_stage(&mut builder, ctx, &mut rng, controls, merged)?;
    add_outputs(&mut builder, ctx, finished)?;
    builder.build()
}

fn build_controls(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<ControlBus, GraphBuildError> {
    let lfo_a = add_lfo(builder, ctx, rng, ChopWave::Sine, 0.10, 0.42)?;
    let lfo_b = add_lfo(builder, ctx, rng, ChopWave::Triangle, 0.08, 0.35)?;
    let lfo_c = add_lfo(builder, ctx, rng, ChopWave::Saw, 0.06, 0.25)?;

    let mix = ctx.nodes.create(
        builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.28 + rng.next_f32() * 0.5,
        }),
    )?;
    builder.connect_channel_input(lfo_a, mix, 0);
    builder.connect_channel_input(lfo_b, mix, 1);

    let zoom = add_remap(builder, ctx, 0.72, 1.58)?;
    builder.connect_channel_input(mix, zoom, 0);

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

    let warp = add_remap(builder, ctx, 0.30, 1.76)?;
    builder.connect_channel_input(warp_mix, warp, 0);

    let tone = add_remap(builder, ctx, 0.76, 1.40)?;
    builder.connect_channel_input(lfo_a, tone, 0);

    Ok(ControlBus { zoom, warp, tone })
}

fn build_camera_pool(
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

    let count = ctx.config.layers.clamp(4, 8);
    let mut cameras = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let camera = add_camera(builder, ctx, rng)?;
        builder.connect_sop_input(pick(&shapes, rng)?, camera, 0);
        builder.connect_channel_input(controls.zoom, camera, 1);
        cameras.push(camera);
    }
    Ok(cameras)
}

fn build_layer_pool(
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

fn build_branch(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
    camera: NodeId,
    layer: NodeId,
    seed: u32,
) -> Result<NodeId, GraphBuildError> {
    let noise_mask = run_module(builder, ctx, rng, "noise-mask", seed, vec![])?;
    let mask = noise_mask.primary;
    let noise = *noise_mask
        .extra_outputs
        .first()
        .ok_or_else(|| GraphBuildError::new("noise-mask module should produce source output"))?;

    let mixed = run_module(
        builder,
        ctx,
        rng,
        "masked-blend",
        seed ^ 0x1010,
        vec![camera, layer, mask],
    )?;

    let reinject = ctx.nodes.create(
        builder,
        "blend",
        NodePayload::Blend(random_blend(rng, LayerBlendMode::Screen, 0.22, 0.70)),
    )?;
    builder.connect_luma_input(mixed.primary, reinject, 0);
    builder.connect_luma_input(noise, reinject, 1);
    builder.connect_mask_input(mask, reinject, 2);

    let warped = run_module(
        builder,
        ctx,
        rng,
        "warp-tone",
        seed ^ 0x2020,
        vec![reinject],
    )?
    .primary;

    let toned = ctx.nodes.create(
        builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(rng)),
    )?;
    builder.connect_luma(warped, toned);
    builder.connect_channel_input(controls.tone, toned, 1);
    Ok(toned)
}

fn merge_branches(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    mut branches: Vec<NodeId>,
) -> Result<NodeId, GraphBuildError> {
    if branches.is_empty() {
        return Err(GraphBuildError::new(
            "feedback atlas needs at least one branch",
        ));
    }

    let mut seed = ctx.config.seed ^ 0xD0D0_1111;
    while branches.len() > 1 {
        let a = branches
            .pop()
            .ok_or_else(|| GraphBuildError::new("missing merge node a"))?;
        let b = branches
            .pop()
            .ok_or_else(|| GraphBuildError::new("missing merge node b"))?;
        let merged = run_module(builder, ctx, rng, "masked-blend", seed, vec![a, b])?.primary;
        branches.push(merged);
        seed = seed.wrapping_add(0x9E37_79B9);
    }

    Ok(branches[0])
}

fn add_finish_stage(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
    source: NodeId,
) -> Result<NodeId, GraphBuildError> {
    let warp = ctx.nodes.create(
        builder,
        "warp-transform",
        NodePayload::WarpTransform(random_warp(rng, 0.85)),
    )?;
    builder.connect_luma(source, warp);
    builder.connect_channel_input(controls.warp, warp, 1);

    let tone = ctx.nodes.create(
        builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(rng)),
    )?;
    builder.connect_luma(warp, tone);
    builder.connect_channel_input(controls.tone, tone, 1);
    Ok(tone)
}

fn add_outputs(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    source: NodeId,
) -> Result<(), GraphBuildError> {
    for slot in [1u8, 2u8, 3u8] {
        let tap = ctx.nodes.create(
            builder,
            "output",
            NodePayload::Output(OutputNode::tap(slot)),
        )?;
        builder.connect_luma(source, tap);
    }

    let output = ctx.nodes.create(
        builder,
        "output",
        NodePayload::Output(OutputNode::primary()),
    )?;
    builder.connect_luma(source, output);
    Ok(())
}

fn run_module(
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
        ModuleRequest::new(seed, ctx.config.profile, inputs),
    )
}

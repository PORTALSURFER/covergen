//! TouchDesigner-style router preset with parallel lanes and routed tap outputs.

use crate::chop::{ChopMathMode, ChopMathNode, ChopWave};
use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder, NodeId};
use crate::model::{LayerBlendMode, XorShift32};
use crate::node::{OutputNode, OutputRole};

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
struct Controls {
    zoom: NodeId,
    warp: NodeId,
    tone: NodeId,
}

#[derive(Clone, Copy)]
struct RoutedLanes {
    geometry_lane: NodeId,
    fractal_lane: NodeId,
    fused_lane: NodeId,
}

/// Build a router-style TD graph with explicit parallel lanes and routed outputs.
pub(super) fn build_td_router(ctx: PresetContext<'_>) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0x3C11_DA25);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0x88F1_0479);

    let controls = build_controls(&mut builder, ctx, &mut rng)?;
    let geometry_sources = build_geometry_sources(&mut builder, ctx, &mut rng, controls)?;
    let fractal_sources = build_fractal_sources(&mut builder, ctx, &mut rng)?;

    let lanes = route_parallel_lanes(
        &mut builder,
        ctx,
        &mut rng,
        controls,
        &geometry_sources,
        &fractal_sources,
    )?;
    let final_node = add_final_router_mix(&mut builder, ctx, &mut rng, controls, lanes)?;
    wire_router_outputs(&mut builder, ctx, lanes, final_node)?;

    builder.build()
}

fn build_controls(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<Controls, GraphBuildError> {
    let a = add_lfo(builder, ctx, rng, ChopWave::Sine, 0.12, 0.46)?;
    let b = add_lfo(builder, ctx, rng, ChopWave::Triangle, 0.08, 0.30)?;
    let c = add_lfo(builder, ctx, rng, ChopWave::Saw, 0.06, 0.22)?;

    let mix = ctx.nodes.create(
        builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.34 + rng.next_f32() * 0.42,
        }),
    )?;
    builder.connect_channel_input(a, mix, 0);
    builder.connect_channel_input(b, mix, 1);

    let warp_mix = ctx.nodes.create(
        builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Multiply,
            value: 1.0,
            blend: 0.5,
        }),
    )?;
    builder.connect_channel_input(b, warp_mix, 0);
    builder.connect_channel_input(c, warp_mix, 1);

    let zoom = add_remap(builder, ctx, 0.76, 1.6)?;
    builder.connect_channel_input(mix, zoom, 0);

    let warp = add_remap(builder, ctx, 0.3, 1.74)?;
    builder.connect_channel_input(warp_mix, warp, 0);

    let tone = add_remap(builder, ctx, 0.74, 1.44)?;
    builder.connect_channel_input(a, tone, 0);

    Ok(Controls { zoom, warp, tone })
}

fn build_geometry_sources(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: Controls,
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut shapes = Vec::with_capacity(6);
    for _ in 0..3 {
        shapes.push(add_circle(builder, ctx, rng)?);
    }
    for _ in 0..3 {
        shapes.push(add_sphere(builder, ctx, rng)?);
    }

    let count = ctx.config.layers.clamp(4, 8);
    let mut sources = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let camera = add_camera(builder, ctx, rng)?;
        builder.connect_sop_input(pick(&shapes, rng)?, camera, 0);
        builder.connect_channel_input(controls.zoom, camera, 1);
        sources.push(camera);
    }
    Ok(sources)
}

fn build_fractal_sources(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut sources = Vec::with_capacity(4);
    for i in 0..4 {
        let layer = generate_layer_node(i, 4, ctx.config.profile, rng, true);
        sources.push(ctx.nodes.create(
            builder,
            "generate-layer",
            NodePayload::GenerateLayer(layer),
        )?);
    }
    Ok(sources)
}

fn route_parallel_lanes(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: Controls,
    geometry: &[NodeId],
    fractals: &[NodeId],
) -> Result<RoutedLanes, GraphBuildError> {
    let geometry_lane = reduce_lane(
        builder,
        ctx,
        rng,
        geometry,
        ctx.config.seed ^ 0x1001_A1A1,
        controls,
    )?;
    let fractal_lane = reduce_lane(
        builder,
        ctx,
        rng,
        fractals,
        ctx.config.seed ^ 0x2002_B2B2,
        controls,
    )?;

    let fused_seed = ctx.config.seed ^ 0x3003_C3C3;
    let fused_lane = module(
        builder,
        ctx,
        rng,
        "masked-blend",
        fused_seed,
        vec![geometry_lane, fractal_lane],
    )?
    .primary;

    Ok(RoutedLanes {
        geometry_lane,
        fractal_lane,
        fused_lane,
    })
}

fn reduce_lane(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    nodes: &[NodeId],
    mut seed: u32,
    controls: Controls,
) -> Result<NodeId, GraphBuildError> {
    if nodes.is_empty() {
        return Err(GraphBuildError::new(
            "router lane requires at least one source",
        ));
    }

    let mut current = nodes[0];
    for node in nodes.iter().skip(1) {
        current = module(
            builder,
            ctx,
            rng,
            "masked-blend",
            seed,
            vec![current, *node],
        )?
        .primary;
        seed = seed.wrapping_add(0x9E37_79B9);
    }

    let warped = ctx.nodes.create(
        builder,
        "warp-transform",
        NodePayload::WarpTransform(random_warp(rng, 0.8)),
    )?;
    builder.connect_luma(current, warped);
    builder.connect_channel_input(controls.warp, warped, 1);

    let toned = ctx.nodes.create(
        builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(rng)),
    )?;
    builder.connect_luma(warped, toned);
    builder.connect_channel_input(controls.tone, toned, 1);
    Ok(toned)
}

fn add_final_router_mix(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: Controls,
    lanes: RoutedLanes,
) -> Result<NodeId, GraphBuildError> {
    let blend = ctx.nodes.create(
        builder,
        "blend",
        NodePayload::Blend(random_blend(rng, LayerBlendMode::Overlay, 0.35, 0.9)),
    )?;
    builder.connect_luma_input(lanes.fused_lane, blend, 0);
    builder.connect_luma_input(lanes.geometry_lane, blend, 1);

    let warp = ctx.nodes.create(
        builder,
        "warp-transform",
        NodePayload::WarpTransform(random_warp(rng, 0.92)),
    )?;
    builder.connect_luma(blend, warp);
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

fn wire_router_outputs(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    lanes: RoutedLanes,
    primary_source: NodeId,
) -> Result<(), GraphBuildError> {
    let tap_geometry =
        ctx.nodes
            .create(builder, "output", NodePayload::Output(OutputNode::tap(1)))?;
    builder.connect_luma(lanes.geometry_lane, tap_geometry);

    let tap_fractal =
        ctx.nodes
            .create(builder, "output", NodePayload::Output(OutputNode::tap(2)))?;
    builder.connect_luma(lanes.fractal_lane, tap_fractal);

    let tap_fused = ctx
        .nodes
        .create(builder, "output", NodePayload::Output(OutputNode::tap(3)))?;
    builder.connect_luma(lanes.fused_lane, tap_fused);

    let output = ctx.nodes.create(
        builder,
        "output",
        NodePayload::Output(OutputNode {
            role: OutputRole::Primary,
            slot: 0,
        }),
    )?;
    builder.connect_luma(primary_source, output);
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

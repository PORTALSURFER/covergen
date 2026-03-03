//! operator-family signal-lab preset with explicit SOP/tex/CHOP buses.

use crate::chop::{ChopMathMode, ChopMathNode, ChopWave};
use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder, NodeId};
use crate::model::{LayerBlendMode, XorShift32};
use crate::node::OutputNode;

use super::node_catalog::NodePayload;
use super::operator_graph_stage_primitives::{
    add_camera, add_circle, add_lfo, add_noise_mask_pair, add_remap, add_sphere, pick,
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

#[derive(Clone, Copy)]
struct LaneSet {
    geometry: NodeId,
    texture: NodeId,
    mixed: NodeId,
}

struct BuildCtx<'a, 'b> {
    builder: &'a mut GraphBuilder,
    ctx: PresetContext<'b>,
    rng: &'a mut XorShift32,
}

/// Build a graph-native SOP/tex/CHOP lab with lane taps and mixed final output.
pub(super) fn build_operator_signal_lab(
    ctx: PresetContext<'_>,
) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0x41B2_7023);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0xD583_1769);
    {
        let mut bx = BuildCtx {
            builder: &mut builder,
            ctx,
            rng: &mut rng,
        };

        let controls = build_controls(&mut bx)?;
        let geometry = build_geometry_lane(&mut bx, controls)?;
        let texture = build_texture_lane(&mut bx, controls)?;
        let mixed = build_mixed_lane(&mut bx, controls, geometry, texture)?;
        wire_outputs(
            &mut bx,
            LaneSet {
                geometry,
                texture,
                mixed,
            },
        )?;
    }
    builder.build()
}

fn build_controls(bx: &mut BuildCtx<'_, '_>) -> Result<ControlBus, GraphBuildError> {
    let lfo_a = add_lfo(bx.builder, bx.ctx, bx.rng, ChopWave::Sine, 0.11, 0.40)?;
    let lfo_b = add_lfo(bx.builder, bx.ctx, bx.rng, ChopWave::Triangle, 0.06, 0.27)?;
    let lfo_c = add_lfo(bx.builder, bx.ctx, bx.rng, ChopWave::Saw, 0.07, 0.22)?;

    let mixed = bx.ctx.nodes.create(
        bx.builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.33 + bx.rng.next_f32() * 0.42,
        }),
    )?;
    bx.builder.connect_channel_input(lfo_a, mixed, 0);
    bx.builder.connect_channel_input(lfo_b, mixed, 1);

    let warp_mul = bx.ctx.nodes.create(
        bx.builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Multiply,
            value: 1.0,
            blend: 0.5,
        }),
    )?;
    bx.builder.connect_channel_input(lfo_b, warp_mul, 0);
    bx.builder.connect_channel_input(lfo_c, warp_mul, 1);

    let zoom = add_remap(bx.builder, bx.ctx, 0.74, 1.52)?;
    bx.builder.connect_channel_input(mixed, zoom, 0);
    let warp = add_remap(bx.builder, bx.ctx, 0.28, 1.66)?;
    bx.builder.connect_channel_input(warp_mul, warp, 0);
    let tone = add_remap(bx.builder, bx.ctx, 0.78, 1.48)?;
    bx.builder.connect_channel_input(lfo_a, tone, 0);
    Ok(ControlBus { zoom, warp, tone })
}

fn build_geometry_lane(
    bx: &mut BuildCtx<'_, '_>,
    controls: ControlBus,
) -> Result<NodeId, GraphBuildError> {
    let mut shapes = Vec::with_capacity(6);
    for _ in 0..3 {
        shapes.push(add_circle(bx.builder, bx.ctx, bx.rng)?);
    }
    for _ in 0..3 {
        shapes.push(add_sphere(bx.builder, bx.ctx, bx.rng)?);
    }

    let count = bx.ctx.config.layers.clamp(4, 7) as usize;
    let mut cameras = Vec::with_capacity(count);
    for _ in 0..count {
        let camera = add_camera(bx.builder, bx.ctx, bx.rng)?;
        bx.builder
            .connect_sop_input(pick(&shapes, bx.rng)?, camera, 0);
        bx.builder.connect_channel_input(controls.zoom, camera, 1);
        cameras.push(camera);
    }

    let mut lane = reduce_masked_chain(bx, cameras, bx.ctx.config.seed ^ 0x1077_1199)?;
    lane = warp_tone_stage(bx, lane, controls, 0.84)?;
    Ok(lane)
}

fn build_texture_lane(
    bx: &mut BuildCtx<'_, '_>,
    controls: ControlBus,
) -> Result<NodeId, GraphBuildError> {
    let mut sources = Vec::with_capacity(6);
    for i in 0..4 {
        let layer = generate_layer_node(i, 4, bx.ctx.config.profile, bx.rng, true);
        let layer_id = bx.ctx.nodes.create(
            bx.builder,
            "generate-layer",
            NodePayload::GenerateLayer(layer),
        )?;
        sources.push(layer_id);
    }

    for _ in 0..2 {
        sources.push(add_noise_mask_pair(bx.builder, bx.ctx, bx.rng)?.noise);
    }

    let mut lane = reduce_masked_chain(bx, sources, bx.ctx.config.seed ^ 0x2277_55AB)?;
    lane = warp_tone_stage(bx, lane, controls, 0.96)?;
    Ok(lane)
}

fn build_mixed_lane(
    bx: &mut BuildCtx<'_, '_>,
    controls: ControlBus,
    geometry: NodeId,
    texture: NodeId,
) -> Result<NodeId, GraphBuildError> {
    let mixed = module(
        bx,
        "masked-blend",
        bx.ctx.config.seed ^ 0x51DE_46C3,
        vec![geometry, texture],
    )?
    .primary;

    let polish = bx.ctx.nodes.create(
        bx.builder,
        "blend",
        NodePayload::Blend(random_blend(bx.rng, LayerBlendMode::Overlay, 0.34, 0.86)),
    )?;
    bx.builder.connect_luma_input(mixed, polish, 0);
    bx.builder.connect_luma_input(geometry, polish, 1);

    let final_lane = warp_tone_stage(bx, polish, controls, 0.9)?;
    Ok(final_lane)
}

fn reduce_masked_chain(
    bx: &mut BuildCtx<'_, '_>,
    nodes: Vec<NodeId>,
    mut seed: u32,
) -> Result<NodeId, GraphBuildError> {
    if nodes.is_empty() {
        return Err(GraphBuildError::new("signal lab requires non-empty lane"));
    }
    let mut iter = nodes.into_iter();
    let mut acc = iter
        .next()
        .ok_or_else(|| GraphBuildError::new("missing lane seed node"))?;
    for node in iter {
        acc = module(bx, "masked-blend", seed, vec![acc, node])?.primary;
        seed = seed.wrapping_add(0x9E37_79B9);
    }
    Ok(acc)
}

fn warp_tone_stage(
    bx: &mut BuildCtx<'_, '_>,
    source: NodeId,
    controls: ControlBus,
    warp_scale: f32,
) -> Result<NodeId, GraphBuildError> {
    let warp = bx.ctx.nodes.create(
        bx.builder,
        "warp-transform",
        NodePayload::WarpTransform(random_warp(bx.rng, warp_scale)),
    )?;
    bx.builder.connect_luma(source, warp);
    bx.builder.connect_channel_input(controls.warp, warp, 1);

    let tone = bx.ctx.nodes.create(
        bx.builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(bx.rng)),
    )?;
    bx.builder.connect_luma(warp, tone);
    bx.builder.connect_channel_input(controls.tone, tone, 1);
    Ok(tone)
}

fn wire_outputs(bx: &mut BuildCtx<'_, '_>, lanes: LaneSet) -> Result<(), GraphBuildError> {
    let tap_geometry = bx.ctx.nodes.create(
        bx.builder,
        "output",
        NodePayload::Output(OutputNode::tap(1)),
    )?;
    bx.builder.connect_luma(lanes.geometry, tap_geometry);

    let tap_texture = bx.ctx.nodes.create(
        bx.builder,
        "output",
        NodePayload::Output(OutputNode::tap(2)),
    )?;
    bx.builder.connect_luma(lanes.texture, tap_texture);

    let tap_mix = bx.ctx.nodes.create(
        bx.builder,
        "output",
        NodePayload::Output(OutputNode::tap(3)),
    )?;
    bx.builder.connect_luma(lanes.mixed, tap_mix);

    let output = bx.ctx.nodes.create(
        bx.builder,
        "output",
        NodePayload::Output(OutputNode::primary()),
    )?;
    bx.builder.connect_luma(lanes.mixed, output);
    Ok(())
}

fn module(
    bx: &mut BuildCtx<'_, '_>,
    key: &str,
    seed: u32,
    inputs: Vec<NodeId>,
) -> Result<ModuleResult, GraphBuildError> {
    let request = ModuleRequest::new(seed, bx.ctx.config.profile, inputs);
    let mut module_ctx = ModuleBuildContext {
        builder: bx.builder,
        nodes: bx.ctx.nodes,
        rng: bx.rng,
    };
    bx.ctx.modules.execute(key, &mut module_ctx, request)
}

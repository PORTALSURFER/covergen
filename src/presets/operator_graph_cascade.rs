//! operator-graph cascading preset that mixes SOP cameras with tex fractal stages.

use crate::chop::{ChopMathMode, ChopMathNode, ChopWave};
use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder, NodeId};
use crate::model::{LayerBlendMode, XorShift32};

use super::node_catalog::NodePayload;
use super::preset_catalog::PresetContext;
use super::primitives::{
    generate_layer_node, random_blend, random_tonemap, random_warp, render_size,
};
use super::operator_graph_stage_primitives::{
    add_camera, add_circle, add_lfo, add_noise_mask_pair, add_outputs, add_remap, add_sphere, pick,
    pop_optional, pop_random, TextureFx,
};

#[derive(Clone, Copy)]
struct ControlLanes {
    zoom: NodeId,
    warp: NodeId,
    tone: NodeId,
}

/// Build a staged graph that cascades SOP camera outputs through texture/fractal operators.
pub(super) fn build_operator_cascade_lab(ctx: PresetContext<'_>) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0x11A9_4FD3);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0xA3E0_5B17);

    let controls = build_controls(&mut builder, ctx, &mut rng)?;
    let cameras = build_camera_sources(&mut builder, ctx, &mut rng, controls)?;
    let layers = build_layer_sources(&mut builder, ctx, &mut rng)?;
    let textures = build_texture_sources(&mut builder, ctx, &mut rng)?;
    let staged = build_cascade_stages(
        &mut builder,
        ctx,
        &mut rng,
        controls,
        &cameras,
        &layers,
        &textures,
    )?;
    let merged = merge_stages(&mut builder, ctx, &mut rng, &staged, &textures)?;

    let finish = add_finish_stage(&mut builder, ctx, &mut rng, controls, merged)?;
    add_outputs(&mut builder, ctx, finish)?;
    builder.build()
}

fn build_controls(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<ControlLanes, GraphBuildError> {
    let a = add_lfo(builder, ctx, rng, ChopWave::Sine, 0.16, 0.58)?;
    let b = add_lfo(builder, ctx, rng, ChopWave::Triangle, 0.10, 0.44)?;
    let c = add_lfo(builder, ctx, rng, ChopWave::Saw, 0.08, 0.22)?;

    let mix = ctx.nodes.create(
        builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.35 + rng.next_f32() * 0.4,
        }),
    )?;
    builder.connect_channel_input(a, mix, 0);
    builder.connect_channel_input(b, mix, 1);

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
    builder.connect_channel_input(b, warp_mix, 0);
    builder.connect_channel_input(c, warp_mix, 1);

    let warp = add_remap(builder, ctx, 0.32, 1.74)?;
    builder.connect_channel_input(warp_mix, warp, 0);

    let tone = add_remap(builder, ctx, 0.76, 1.42)?;
    builder.connect_channel_input(a, tone, 0);

    Ok(ControlLanes { zoom, warp, tone })
}

fn build_camera_sources(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlLanes,
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut shapes = Vec::with_capacity(5);
    for _ in 0..3 {
        shapes.push(add_circle(builder, ctx, rng)?);
    }
    for _ in 0..2 {
        shapes.push(add_sphere(builder, ctx, rng)?);
    }

    let mut cameras = Vec::with_capacity(4);
    for _ in 0..4 {
        let camera = add_camera(builder, ctx, rng)?;
        builder.connect_sop_input(pick(&shapes, rng)?, camera, 0);
        builder.connect_channel_input(controls.zoom, camera, 1);
        cameras.push(camera);
    }
    Ok(cameras)
}

fn build_layer_sources(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut layers = Vec::with_capacity(3);
    for i in 0..3 {
        let node = generate_layer_node(i, 3, ctx.config.profile, rng, true);
        layers.push(ctx.nodes.create(
            builder,
            "generate-layer",
            NodePayload::GenerateLayer(node),
        )?);
    }
    Ok(layers)
}

fn build_texture_sources(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<Vec<TextureFx>, GraphBuildError> {
    let mut textures = Vec::with_capacity(4);
    for _ in 0..4 {
        textures.push(add_noise_mask_pair(builder, ctx, rng)?);
    }
    Ok(textures)
}

fn build_cascade_stages(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlLanes,
    cameras: &[NodeId],
    layers: &[NodeId],
    textures: &[TextureFx],
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut stages = Vec::with_capacity(cameras.len());
    for source in cameras {
        let layer = pick(layers, rng)?;
        let texture = pick(textures, rng)?;

        let base = ctx.nodes.create(
            builder,
            "blend",
            NodePayload::Blend(random_blend(rng, LayerBlendMode::Overlay, 0.24, 0.72)),
        )?;
        builder.connect_luma_input(*source, base, 0);
        builder.connect_luma_input(layer, base, 1);

        let textured = ctx.nodes.create(
            builder,
            "blend",
            NodePayload::Blend(random_blend(rng, LayerBlendMode::Screen, 0.20, 0.68)),
        )?;
        builder.connect_luma_input(base, textured, 0);
        builder.connect_luma_input(texture.noise, textured, 1);
        builder.connect_mask_input(texture.mask, textured, 2);

        let warp = ctx.nodes.create(
            builder,
            "warp-transform",
            NodePayload::WarpTransform({
                let warp_scale = 0.7 + rng.next_f32() * 0.5;
                random_warp(rng, warp_scale)
            }),
        )?;
        builder.connect_luma(textured, warp);
        builder.connect_channel_input(controls.warp, warp, 1);

        let tone = ctx.nodes.create(
            builder,
            "tone-map",
            NodePayload::ToneMap(random_tonemap(rng)),
        )?;
        builder.connect_luma(warp, tone);
        builder.connect_channel_input(controls.tone, tone, 1);
        stages.push(tone);
    }
    Ok(stages)
}

fn merge_stages(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    staged: &[NodeId],
    textures: &[TextureFx],
) -> Result<NodeId, GraphBuildError> {
    let mut remaining = staged.to_vec();
    let mut current = pop_random(&mut remaining, rng)?;

    while let Some(next) = pop_optional(&mut remaining, rng) {
        let blend_mode = LayerBlendMode::from_u32(rng.next_u32());
        let blend = ctx.nodes.create(
            builder,
            "blend",
            NodePayload::Blend(random_blend(rng, blend_mode, 0.30, 0.86)),
        )?;
        builder.connect_luma_input(current, blend, 0);
        builder.connect_luma_input(next, blend, 1);
        if rng.next_f32() < 0.7 {
            builder.connect_mask_input(pick(textures, rng)?.mask, blend, 2);
        }
        current = blend;
    }

    Ok(current)
}

fn add_finish_stage(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlLanes,
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

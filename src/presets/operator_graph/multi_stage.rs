//! Expanded operator-graph preset with staged CHOP/SOP/tex composition.

use crate::chop::{ChopMathMode, ChopMathNode, ChopWave};
use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder, NodeId};
use crate::model::{LayerBlendMode, XorShift32};
use crate::runtime_config::V2Profile;

use super::node_catalog::NodePayload;
use super::operator_graph_stage_primitives::{
    add_camera, add_circle, add_lfo, add_noise_mask_pair, add_outputs, add_remap, add_sphere, pick,
    pop_optional, pop_random, TextureFx,
};
use super::preset_catalog::PresetContext;
use super::primitives::{random_blend, random_tonemap, random_warp, render_size};

#[derive(Clone, Copy)]
struct ControlBus {
    zoom: NodeId,
    warp: NodeId,
    tone: NodeId,
    blend: NodeId,
}

/// Build a deeper staged operator-family graph with structured branching.
pub(super) fn build_operator_multi_stage(
    ctx: PresetContext<'_>,
) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0xC341_92D1);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0x57A1_8B3D);

    let controls = build_control_bus(&mut builder, ctx, &mut rng)?;
    let shapes = build_shape_pool(&mut builder, ctx, &mut rng)?;
    let cameras = build_camera_pool(&mut builder, ctx, &mut rng, controls, &shapes)?;
    let textures = build_texture_pool(&mut builder, ctx, &mut rng)?;
    let branches = build_branch_pool(&mut builder, ctx, &mut rng, controls, &cameras, &textures)?;
    let merged = merge_branches(&mut builder, ctx, &mut rng, controls, &branches, &textures)?;
    let final_node = add_final_stage(&mut builder, ctx, &mut rng, controls, merged)?;
    add_outputs(&mut builder, ctx, final_node)?;
    builder.build()
}

fn build_control_bus(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<ControlBus, GraphBuildError> {
    let lfo_a = add_lfo(builder, ctx, rng, ChopWave::Sine, 0.22, 0.65)?;
    let lfo_b = add_lfo(builder, ctx, rng, ChopWave::Triangle, 0.15, 0.45)?;
    let lfo_c = add_lfo(builder, ctx, rng, ChopWave::Saw, 0.08, 0.28)?;

    let blend = ctx.nodes.create(
        builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.42 + rng.next_f32() * 0.32,
        }),
    )?;
    builder.connect_channel_input(lfo_a, blend, 0);
    builder.connect_channel_input(lfo_b, blend, 1);

    let zoom = add_remap(builder, ctx, 0.72, 1.64)?;
    builder.connect_channel_input(blend, zoom, 0);

    let warp = add_remap(builder, ctx, 0.35, 1.7)?;
    builder.connect_channel_input(lfo_c, warp, 0);

    let tone = add_remap(builder, ctx, 0.7, 1.45)?;
    builder.connect_channel_input(lfo_b, tone, 0);

    Ok(ControlBus {
        zoom,
        warp,
        tone,
        blend,
    })
}

fn build_shape_pool(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut shapes = Vec::with_capacity(5);
    for _ in 0..3 {
        shapes.push(add_circle(builder, ctx, rng)?);
    }
    for _ in 0..2 {
        shapes.push(add_sphere(builder, ctx, rng)?);
    }
    Ok(shapes)
}

fn build_camera_pool(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
    shapes: &[NodeId],
) -> Result<Vec<NodeId>, GraphBuildError> {
    let base = if matches!(ctx.config.profile, V2Profile::Performance) {
        3
    } else {
        4
    };
    let count = (base + (ctx.config.layers / 4)).clamp(3, 6);

    let mut cameras = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let camera = add_camera(builder, ctx, rng)?;
        builder.connect_sop_input(pick(shapes, rng)?, camera, 0);
        builder.connect_channel_input(controls.zoom, camera, 1);
        cameras.push(camera);
    }
    Ok(cameras)
}

fn build_texture_pool(
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

fn build_branch_pool(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
    cameras: &[NodeId],
    textures: &[TextureFx],
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut branches = Vec::with_capacity(cameras.len());
    for camera in cameras {
        let mut current = add_branch_warp(builder, ctx, rng, controls, *camera)?;
        current = add_branch_tone(builder, ctx, rng, controls, current)?;
        current = add_branch_texture_mix(builder, ctx, rng, controls, current, textures)?;
        branches.push(current);
    }
    Ok(branches)
}

fn add_branch_warp(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
    source: NodeId,
) -> Result<NodeId, GraphBuildError> {
    let warp_scale = 0.8 + rng.next_f32() * 0.5;
    let warp = ctx.nodes.create(
        builder,
        "warp-transform",
        NodePayload::WarpTransform(random_warp(rng, warp_scale)),
    )?;
    builder.connect_luma(source, warp);
    builder.connect_channel_input(controls.warp, warp, 1);
    Ok(warp)
}

fn add_branch_tone(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
    source: NodeId,
) -> Result<NodeId, GraphBuildError> {
    let tone = ctx.nodes.create(
        builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(rng)),
    )?;
    builder.connect_luma(source, tone);
    builder.connect_channel_input(controls.tone, tone, 1);
    Ok(tone)
}

fn add_branch_texture_mix(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
    source: NodeId,
    textures: &[TextureFx],
) -> Result<NodeId, GraphBuildError> {
    let fx = pick(textures, rng)?;
    let blend = ctx.nodes.create(
        builder,
        "blend",
        NodePayload::Blend(random_blend(rng, LayerBlendMode::Screen, 0.24, 0.74)),
    )?;
    builder.connect_luma_input(source, blend, 0);
    builder.connect_luma_input(fx.noise, blend, 1);
    builder.connect_mask_input(fx.mask, blend, 2);
    if rng.next_f32() < 0.65 {
        let tone = ctx.nodes.create(
            builder,
            "tone-map",
            NodePayload::ToneMap(random_tonemap(rng)),
        )?;
        builder.connect_luma(blend, tone);
        builder.connect_channel_input(controls.blend, tone, 1);
        return Ok(tone);
    }
    Ok(blend)
}

fn merge_branches(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    _controls: ControlBus,
    branches: &[NodeId],
    textures: &[TextureFx],
) -> Result<NodeId, GraphBuildError> {
    let mut remaining = branches.to_vec();
    let mut current = pop_random(&mut remaining, rng)?;
    while let Some(next) = pop_optional(&mut remaining, rng) {
        let mode = LayerBlendMode::from_u32(rng.next_u32());
        let blend = ctx.nodes.create(
            builder,
            "blend",
            NodePayload::Blend(random_blend(rng, mode, 0.28, 0.88)),
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

fn add_final_stage(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
    source: NodeId,
) -> Result<NodeId, GraphBuildError> {
    let warp = ctx.nodes.create(
        builder,
        "warp-transform",
        NodePayload::WarpTransform(random_warp(rng, 0.9)),
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

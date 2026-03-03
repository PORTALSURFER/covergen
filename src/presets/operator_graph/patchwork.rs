//! operator-family patchwork preset with cross-wired stage composition.

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
struct Controls {
    zoom: NodeId,
    tone: NodeId,
    warp: NodeId,
}

/// Build a patchwork-style operator graph with cross-wired camera/layer/mask stages.
pub(super) fn build_operator_patchwork(
    ctx: PresetContext<'_>,
) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0x6114_A9FD);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0x1F02_7D31);

    let controls = build_controls(&mut builder, ctx, &mut rng)?;
    let cameras = build_cameras(&mut builder, ctx, &mut rng, controls)?;
    let layers = build_layers(&mut builder, ctx, &mut rng)?;

    let mut stages = Vec::with_capacity(cameras.len());
    for i in 0..cameras.len() {
        let stage = build_patch_stage(
            &mut builder,
            ctx,
            &mut rng,
            controls,
            cameras[i],
            layers[i % layers.len()],
            (ctx.config.seed ^ 0xA1B2_0081).wrapping_add(i as u32),
        )?;
        stages.push(stage);
    }

    let merged = merge_patch_stages(&mut builder, ctx, &mut rng, stages)?;
    let final_node = add_final_stage(&mut builder, ctx, &mut rng, controls, merged)?;
    add_outputs(&mut builder, ctx, final_node)?;
    builder.build()
}

fn build_controls(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<Controls, GraphBuildError> {
    let a = add_lfo(builder, ctx, rng, ChopWave::Sine, 0.14, 0.48)?;
    let b = add_lfo(builder, ctx, rng, ChopWave::Triangle, 0.09, 0.33)?;
    let c = add_lfo(builder, ctx, rng, ChopWave::Saw, 0.07, 0.25)?;

    let mix = ctx.nodes.create(
        builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.32 + rng.next_f32() * 0.46,
        }),
    )?;
    builder.connect_channel_input(a, mix, 0);
    builder.connect_channel_input(b, mix, 1);

    let zoom = add_remap(builder, ctx, 0.74, 1.56)?;
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

    let warp = add_remap(builder, ctx, 0.35, 1.70)?;
    builder.connect_channel_input(warp_mix, warp, 0);

    let tone = add_remap(builder, ctx, 0.78, 1.38)?;
    builder.connect_channel_input(a, tone, 0);

    Ok(Controls { zoom, tone, warp })
}

fn build_cameras(
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

    let count = ctx.config.layers.clamp(4, 7);
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
        let node =
            ctx.nodes
                .create(builder, "generate-layer", NodePayload::GenerateLayer(layer))?;
        layers.push(node);
    }
    Ok(layers)
}

fn build_patch_stage(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: Controls,
    camera: NodeId,
    layer: NodeId,
    seed: u32,
) -> Result<NodeId, GraphBuildError> {
    let noise_mask = module(builder, ctx, rng, "noise-mask", seed, vec![])?;
    let mask = noise_mask.primary;

    let base = module(
        builder,
        ctx,
        rng,
        "masked-blend",
        seed ^ 0x1010_0001,
        vec![camera, layer, mask],
    )?
    .primary;

    let warp = ctx.nodes.create(
        builder,
        "warp-transform",
        NodePayload::WarpTransform({
            let warp_scale = 0.72 + rng.next_f32() * 0.52;
            random_warp(rng, warp_scale)
        }),
    )?;
    builder.connect_luma(base, warp);
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

fn merge_patch_stages(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    mut stages: Vec<NodeId>,
) -> Result<NodeId, GraphBuildError> {
    if stages.is_empty() {
        return Err(GraphBuildError::new(
            "patchwork preset requires stage nodes",
        ));
    }

    let mut seed = ctx.config.seed ^ 0x4477_1010;
    while stages.len() > 1 {
        let a = stages
            .pop()
            .ok_or_else(|| GraphBuildError::new("missing stage a"))?;
        let b = stages
            .pop()
            .ok_or_else(|| GraphBuildError::new("missing stage b"))?;

        let merged = module(builder, ctx, rng, "masked-blend", seed, vec![a, b])?.primary;
        stages.push(merged);
        seed = seed.wrapping_add(0x9E37_79B9);
    }

    Ok(stages[0])
}

fn add_final_stage(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: Controls,
    source: NodeId,
) -> Result<NodeId, GraphBuildError> {
    let blend = ctx.nodes.create(
        builder,
        "blend",
        NodePayload::Blend(random_blend(rng, LayerBlendMode::Overlay, 0.35, 0.88)),
    )?;
    builder.connect_luma_input(source, blend, 0);
    builder.connect_luma_input(source, blend, 1);

    let tone = ctx.nodes.create(
        builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(rng)),
    )?;
    builder.connect_luma(blend, tone);
    builder.connect_channel_input(controls.tone, tone, 1);
    Ok(tone)
}

fn add_outputs(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    source: NodeId,
) -> Result<(), GraphBuildError> {
    for slot in [1u8, 2u8] {
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
        ModuleRequest::new(seed, ctx.config.profile, inputs),
    )
}

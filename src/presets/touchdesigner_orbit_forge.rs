//! TouchDesigner-style orbit-forge preset with geometry/material/gate lanes.

use crate::chop::{ChopMathMode, ChopMathNode, ChopWave};
use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder, MaskNode, NodeId};
use crate::model::{LayerBlendMode, XorShift32};
use crate::node::{MaskTemporal, OutputNode};

use super::node_catalog::NodePayload;
use super::preset_catalog::PresetContext;
use super::primitives::{
    generate_layer_node, random_blend, random_tonemap, random_warp, render_size,
};
use super::subgraph_catalog::{ModuleBuildContext, ModuleRequest, ModuleResult};
use super::touchdesigner_stage_primitives::{
    add_camera, add_circle, add_lfo, add_noise_mask_pair, add_remap, add_sphere, pick,
};

#[derive(Clone, Copy)]
struct ControlBus {
    zoom: NodeId,
    warp: NodeId,
    tone: NodeId,
    gate: NodeId,
}

#[derive(Clone, Copy)]
struct LaneSet {
    geometry: NodeId,
    material: NodeId,
    gate_luma: NodeId,
    mixed: NodeId,
}

struct BuildCtx<'a, 'b> {
    builder: &'a mut GraphBuilder,
    ctx: PresetContext<'b>,
    rng: &'a mut XorShift32,
}

/// Build an orbit-focused TD preset with deterministic multi-lane signal flow.
pub(super) fn build_td_orbit_forge(ctx: PresetContext<'_>) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0x61E7_8B39);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0x4A20_17CF);
    {
        let mut bx = BuildCtx {
            builder: &mut builder,
            ctx,
            rng: &mut rng,
        };
        let controls = build_controls(&mut bx)?;
        let geometry = build_geometry_lane(&mut bx, controls)?;
        let material = build_material_lane(&mut bx, controls)?;
        let gate_luma = build_gate_lane(&mut bx, controls)?;
        let mixed = build_mixed_lane(&mut bx, controls, geometry, material, gate_luma)?;
        wire_outputs(
            &mut bx,
            LaneSet {
                geometry,
                material,
                gate_luma,
                mixed,
            },
        )?;
    }
    builder.build()
}

fn build_controls(bx: &mut BuildCtx<'_, '_>) -> Result<ControlBus, GraphBuildError> {
    let a = add_lfo(bx.builder, bx.ctx, bx.rng, ChopWave::Sine, 0.10, 0.35)?;
    let b = add_lfo(bx.builder, bx.ctx, bx.rng, ChopWave::Triangle, 0.07, 0.24)?;
    let c = add_lfo(bx.builder, bx.ctx, bx.rng, ChopWave::Saw, 0.05, 0.18)?;
    let d = add_lfo(bx.builder, bx.ctx, bx.rng, ChopWave::Saw, 0.03, 0.12)?;

    let mix = bx.ctx.nodes.create(
        bx.builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.30 + bx.rng.next_f32() * 0.48,
        }),
    )?;
    bx.builder.connect_channel_input(a, mix, 0);
    bx.builder.connect_channel_input(b, mix, 1);

    let warp_mix = bx.ctx.nodes.create(
        bx.builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Multiply,
            value: 1.0,
            blend: 0.5,
        }),
    )?;
    bx.builder.connect_channel_input(b, warp_mix, 0);
    bx.builder.connect_channel_input(c, warp_mix, 1);

    let gate_mix = bx.ctx.nodes.create(
        bx.builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Add,
            value: 0.0,
            blend: 0.5,
        }),
    )?;
    bx.builder.connect_channel_input(c, gate_mix, 0);
    bx.builder.connect_channel_input(d, gate_mix, 1);

    let zoom = add_remap(bx.builder, bx.ctx, 0.72, 1.56)?;
    let warp = add_remap(bx.builder, bx.ctx, 0.27, 1.72)?;
    let tone = add_remap(bx.builder, bx.ctx, 0.74, 1.42)?;
    let gate = add_remap(bx.builder, bx.ctx, 0.18, 0.82)?;

    bx.builder.connect_channel_input(mix, zoom, 0);
    bx.builder.connect_channel_input(warp_mix, warp, 0);
    bx.builder.connect_channel_input(a, tone, 0);
    bx.builder.connect_channel_input(gate_mix, gate, 0);
    Ok(ControlBus {
        zoom,
        warp,
        tone,
        gate,
    })
}

fn build_geometry_lane(
    bx: &mut BuildCtx<'_, '_>,
    controls: ControlBus,
) -> Result<NodeId, GraphBuildError> {
    let mut shapes = Vec::with_capacity(8);
    for _ in 0..4 {
        shapes.push(add_circle(bx.builder, bx.ctx, bx.rng)?);
    }
    for _ in 0..4 {
        shapes.push(add_sphere(bx.builder, bx.ctx, bx.rng)?);
    }

    let mut cameras = Vec::with_capacity(7);
    let count = bx.ctx.config.layers.clamp(5, 8) as usize;
    for _ in 0..count {
        let camera = add_camera(bx.builder, bx.ctx, bx.rng)?;
        bx.builder
            .connect_sop_input(pick(&shapes, bx.rng)?, camera, 0);
        bx.builder.connect_channel_input(controls.zoom, camera, 1);
        cameras.push(camera);
    }

    let chain = reduce_masked_chain(bx, cameras, bx.ctx.config.seed ^ 0x10F0_C113)?;
    warp_tone_stage(bx, chain, controls, 0.88)
}

fn build_material_lane(
    bx: &mut BuildCtx<'_, '_>,
    controls: ControlBus,
) -> Result<NodeId, GraphBuildError> {
    let mut sources = Vec::with_capacity(7);
    for i in 0..5 {
        let layer = generate_layer_node(i, 5, bx.ctx.config.profile, bx.rng, true);
        sources.push(bx.ctx.nodes.create(
            bx.builder,
            "generate-layer",
            NodePayload::GenerateLayer(layer),
        )?);
    }
    for _ in 0..2 {
        sources.push(add_noise_mask_pair(bx.builder, bx.ctx, bx.rng)?.noise);
    }

    let chain = reduce_masked_chain(bx, sources, bx.ctx.config.seed ^ 0x20F0_C113)?;
    warp_tone_stage(bx, chain, controls, 0.96)
}

fn build_gate_lane(
    bx: &mut BuildCtx<'_, '_>,
    controls: ControlBus,
) -> Result<NodeId, GraphBuildError> {
    let mut noise = Vec::with_capacity(3);
    for _ in 0..3 {
        noise.push(add_noise_mask_pair(bx.builder, bx.ctx, bx.rng)?.noise);
    }
    let merged = reduce_masked_chain(bx, noise, bx.ctx.config.seed ^ 0x30F0_C113)?;

    let gate_mask = bx.ctx.nodes.create(
        bx.builder,
        "mask",
        NodePayload::Mask(MaskNode {
            threshold: 0.36 + bx.rng.next_f32() * 0.28,
            softness: 0.08 + bx.rng.next_f32() * 0.24,
            invert: false,
            temporal: MaskTemporal::default(),
        }),
    )?;
    bx.builder.connect_luma(merged, gate_mask);

    let gate_tone = bx.ctx.nodes.create(
        bx.builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(bx.rng)),
    )?;
    bx.builder.connect_luma(merged, gate_tone);
    bx.builder
        .connect_channel_input(controls.gate, gate_tone, 1);
    let _ = gate_mask;
    Ok(gate_tone)
}

fn build_mixed_lane(
    bx: &mut BuildCtx<'_, '_>,
    controls: ControlBus,
    geometry: NodeId,
    material: NodeId,
    gate_luma: NodeId,
) -> Result<NodeId, GraphBuildError> {
    let gate_mask = bx.ctx.nodes.create(
        bx.builder,
        "mask",
        NodePayload::Mask(MaskNode {
            threshold: 0.35 + bx.rng.next_f32() * 0.22,
            softness: 0.09 + bx.rng.next_f32() * 0.17,
            invert: false,
            temporal: MaskTemporal::default(),
        }),
    )?;
    bx.builder.connect_luma(gate_luma, gate_mask);

    let base_mix = module(
        bx,
        "masked-blend",
        bx.ctx.config.seed ^ 0x6541_1302,
        vec![geometry, material, gate_mask],
    )?
    .primary;

    let polish = bx.ctx.nodes.create(
        bx.builder,
        "blend",
        NodePayload::Blend(random_blend(bx.rng, LayerBlendMode::Overlay, 0.36, 0.88)),
    )?;
    bx.builder.connect_luma_input(base_mix, polish, 0);
    bx.builder.connect_luma_input(gate_luma, polish, 1);

    warp_tone_stage(bx, polish, controls, 0.92)
}

fn reduce_masked_chain(
    bx: &mut BuildCtx<'_, '_>,
    nodes: Vec<NodeId>,
    mut seed: u32,
) -> Result<NodeId, GraphBuildError> {
    if nodes.is_empty() {
        return Err(GraphBuildError::new("orbit forge requires non-empty lane"));
    }
    let mut iter = nodes.into_iter();
    let mut acc = iter
        .next()
        .ok_or_else(|| GraphBuildError::new("missing lane head node"))?;
    for next in iter {
        acc = module(bx, "masked-blend", seed, vec![acc, next])?.primary;
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
    let taps = [
        (lanes.geometry, OutputNode::tap(1)),
        (lanes.material, OutputNode::tap(2)),
        (lanes.gate_luma, OutputNode::tap(3)),
        (lanes.mixed, OutputNode::tap(4)),
    ];
    for (source, spec) in taps {
        let tap = bx
            .ctx
            .nodes
            .create(bx.builder, "output", NodePayload::Output(spec))?;
        bx.builder.connect_luma(source, tap);
    }
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

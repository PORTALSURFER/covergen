//! operator-family modular network preset built from reusable subgraph modules.

use crate::chop::{ChopMathMode, ChopMathNode, ChopWave};
use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder, NodeId};
use crate::model::XorShift32;
use crate::node::OutputNode;

use super::node_catalog::NodePayload;
use super::preset_catalog::PresetContext;
use super::primitives::{generate_layer_node, random_tonemap, render_size};
use super::subgraph_catalog::{ModuleBuildContext, ModuleParams, ModuleRequest};
use super::operator_graph_stage_primitives::{
    add_camera, add_circle, add_lfo, add_remap, add_sphere, pick,
};

#[derive(Clone, Copy)]
struct ControlBus {
    zoom: NodeId,
    tone: NodeId,
}

/// Build a reusable-module-oriented operator-graph with staged fan-in merging.
pub(super) fn build_operator_modular_network(
    ctx: PresetContext<'_>,
) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0x97C1_2F5B);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0xD311_A087);

    let controls = build_controls(&mut builder, ctx, &mut rng)?;
    let cameras = build_camera_pool(&mut builder, ctx, &mut rng, controls)?;
    let layers = build_layer_pool(&mut builder, ctx, &mut rng)?;

    let mut branches = Vec::with_capacity(cameras.len());
    {
        let mut module_ctx = ModuleBuildContext {
            builder: &mut builder,
            nodes: ctx.nodes,
            rng: &mut rng,
        };

        for (index, camera) in cameras.iter().enumerate() {
            let motif_params = ModuleParams {
                intensity: 0.85 + module_ctx.rng.next_f32() * 0.65,
                variation: 0.25 + module_ctx.rng.next_f32() * 0.65,
                blend_bias: 0.25 + module_ctx.rng.next_f32() * 0.70,
            };
            let warped = ctx
                .modules
                .execute(
                    "warp-tone",
                    &mut module_ctx,
                    ModuleRequest::new(
                        ctx.config.seed ^ (index as u32).wrapping_mul(0x91E1_2203),
                        ctx.config.profile,
                        vec![*camera],
                    )
                    .with_params(motif_params),
                )?
                .primary;
            let motif_key = match index % 3 {
                0 => "motif-ribbon",
                1 => "motif-echo",
                _ => "motif-dual-tone",
            };
            let motif = ctx
                .modules
                .execute(
                    motif_key,
                    &mut module_ctx,
                    ModuleRequest::new(
                        ctx.config.seed ^ (index as u32).wrapping_mul(0x4F11_8305),
                        ctx.config.profile,
                        vec![warped],
                    )
                    .with_params(motif_params),
                )?
                .primary;

            let layer = layers[index % layers.len()];
            let mixed = ctx
                .modules
                .execute(
                    "masked-blend",
                    &mut module_ctx,
                    ModuleRequest::new(
                        ctx.config.seed ^ (index as u32).wrapping_mul(0xA35D_0011),
                        ctx.config.profile,
                        vec![motif, layer],
                    )
                    .with_params(motif_params),
                )?
                .primary;
            branches.push(mixed);
        }
    }

    let merged = merge_branches_with_modules(&mut builder, ctx, &mut rng, branches)?;
    let finish = add_finish_tone(&mut builder, ctx, &mut rng, controls, merged)?;
    add_outputs_with_two_taps(&mut builder, ctx, finish)?;
    builder.build()
}

fn build_controls(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<ControlBus, GraphBuildError> {
    let a = add_lfo(builder, ctx, rng, ChopWave::Sine, 0.12, 0.42)?;
    let b = add_lfo(builder, ctx, rng, ChopWave::Triangle, 0.08, 0.28)?;

    let mix = ctx.nodes.create(
        builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.3 + rng.next_f32() * 0.45,
        }),
    )?;
    builder.connect_channel_input(a, mix, 0);
    builder.connect_channel_input(b, mix, 1);

    let zoom = add_remap(builder, ctx, 0.74, 1.52)?;
    builder.connect_channel_input(mix, zoom, 0);

    let tone = add_remap(builder, ctx, 0.78, 1.35)?;
    builder.connect_channel_input(a, tone, 0);

    Ok(ControlBus { zoom, tone })
}

fn build_camera_pool(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    controls: ControlBus,
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut shapes = Vec::with_capacity(5);
    for _ in 0..3 {
        shapes.push(add_circle(builder, ctx, rng)?);
    }
    for _ in 0..2 {
        shapes.push(add_sphere(builder, ctx, rng)?);
    }

    let camera_count = ctx.config.layers.clamp(4, 7);
    let mut cameras = Vec::with_capacity(camera_count as usize);
    for _ in 0..camera_count {
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
    let mut layers = Vec::with_capacity(3);
    for i in 0..3 {
        let node = generate_layer_node(i, 3, ctx.config.profile, rng, true);
        let id = ctx
            .nodes
            .create(builder, "generate-layer", NodePayload::GenerateLayer(node))?;
        layers.push(id);
    }
    Ok(layers)
}

fn merge_branches_with_modules(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    mut branches: Vec<NodeId>,
) -> Result<NodeId, GraphBuildError> {
    if branches.is_empty() {
        return Err(GraphBuildError::new(
            "op-modular-network requires at least one branch",
        ));
    }

    let mut module_ctx = ModuleBuildContext {
        builder,
        nodes: ctx.nodes,
        rng,
    };

    let mut current = branches.remove(0);
    for (index, branch) in branches.into_iter().enumerate() {
        current = ctx
            .modules
            .execute(
                "masked-blend",
                &mut module_ctx,
                ModuleRequest::new(
                    ctx.config.seed ^ (index as u32).wrapping_mul(0xE91D_1007),
                    ctx.config.profile,
                    vec![current, branch],
                ),
            )?
            .primary;
    }

    Ok(current)
}

fn add_finish_tone(
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

fn add_outputs_with_two_taps(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    source: NodeId,
) -> Result<(), GraphBuildError> {
    let tap_a = ctx
        .nodes
        .create(builder, "output", NodePayload::Output(OutputNode::tap(1)))?;
    builder.connect_luma(source, tap_a);

    let tap_b = ctx
        .nodes
        .create(builder, "output", NodePayload::Output(OutputNode::tap(2)))?;
    builder.connect_luma(source, tap_b);

    let output = ctx.nodes.create(
        builder,
        "output",
        NodePayload::Output(OutputNode::primary()),
    )?;
    builder.connect_luma(source, output);
    Ok(())
}

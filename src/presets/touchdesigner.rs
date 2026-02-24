//! TouchDesigner-inspired preset builders using CHOP/SOP/TOP composition.

use crate::chop::{ChopLfoNode, ChopMathMode, ChopMathNode, ChopRemapNode, ChopWave};
use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder, NodeId};
use crate::model::{LayerBlendMode, XorShift32};
use crate::node::OutputNode;
use crate::runtime_config::V2Profile;
use crate::sop::{SopCircleNode, SopSphereNode, TopCameraRenderNode};

use super::node_catalog::NodePayload;
use super::preset_catalog::PresetContext;
use super::primitives::{random_blend, random_tonemap, random_warp, render_size};

/// Build a CHOP/SOP/TOP chain with basic camera-rendered primitives.
pub(super) fn build_td_primitive_stage(
    ctx: PresetContext<'_>,
) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0xA7D1_2213);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0x4F92_13B1);

    let lfo_a = add_lfo(&mut builder, ctx, &mut rng, ChopWave::Sine)?;
    let lfo_b = add_lfo(&mut builder, ctx, &mut rng, ChopWave::Triangle)?;
    let zoom_mod = ctx.nodes.create(
        &mut builder,
        "chop-remap",
        NodePayload::ChopRemap(ChopRemapNode {
            in_min: -1.0,
            in_max: 1.0,
            out_min: 0.7,
            out_max: 1.6,
            clamp: true,
        }),
    )?;
    builder.connect_channel_input(lfo_a, zoom_mod, 0);

    let tone_mod = ctx.nodes.create(
        &mut builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.55,
        }),
    )?;
    builder.connect_channel_input(lfo_a, tone_mod, 0);
    builder.connect_channel_input(lfo_b, tone_mod, 1);

    let circle = add_circle(&mut builder, ctx, &mut rng)?;
    let sphere = add_sphere(&mut builder, ctx, &mut rng)?;

    let camera_a = add_camera(&mut builder, ctx, &mut rng)?;
    builder.connect_sop_input(circle, camera_a, 0);
    builder.connect_channel_input(zoom_mod, camera_a, 1);

    let camera_b = add_camera(&mut builder, ctx, &mut rng)?;
    builder.connect_sop_input(sphere, camera_b, 0);
    builder.connect_channel_input(zoom_mod, camera_b, 1);

    let blend = ctx.nodes.create(
        &mut builder,
        "blend",
        NodePayload::Blend(random_blend(&mut rng, LayerBlendMode::Screen, 0.45, 0.90)),
    )?;
    builder.connect_luma_input(camera_a, blend, 0);
    builder.connect_luma_input(camera_b, blend, 1);

    let tone = ctx.nodes.create(
        &mut builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(&mut rng)),
    )?;
    builder.connect_luma(blend, tone);
    builder.connect_channel_input(tone_mod, tone, 1);

    add_outputs(&mut builder, ctx, tone)?;
    builder.build()
}

/// Build a constrained random TouchDesigner-style network with CHOP/SOP/TOP branches.
pub(super) fn build_td_random_network(ctx: PresetContext<'_>) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0x58C3_1D27);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0x9A27_5B41);

    let profile_scale = if matches!(ctx.config.profile, V2Profile::Performance) {
        0
    } else {
        1
    };
    let channel_count = (ctx.config.layers + 2 + profile_scale).clamp(3, 8);
    let sop_count = (ctx.config.layers + 1 + profile_scale).clamp(2, 7);
    let camera_count = (ctx.config.layers + profile_scale).clamp(2, 6);

    let mut channel_pool = Vec::with_capacity((channel_count * 2) as usize);
    for _ in 0..channel_count {
        let wave = ChopWave::from_u32(rng.next_u32());
        channel_pool.push(add_lfo(&mut builder, ctx, &mut rng, wave)?);
    }
    for _ in 0..(channel_count / 2) {
        let node = ctx.nodes.create(
            &mut builder,
            "chop-math",
            NodePayload::ChopMath(ChopMathNode {
                mode: ChopMathMode::from_u32(rng.next_u32()),
                value: 0.8 + rng.next_f32() * 0.7,
                blend: 0.2 + rng.next_f32() * 0.7,
            }),
        )?;
        let a = choose_node(&channel_pool, &mut rng)?;
        builder.connect_channel_input(a, node, 0);
        if rng.next_f32() < 0.75 {
            let b = choose_node(&channel_pool, &mut rng)?;
            builder.connect_channel_input(b, node, 1);
        }
        channel_pool.push(node);

        let remap = ctx.nodes.create(
            &mut builder,
            "chop-remap",
            NodePayload::ChopRemap(ChopRemapNode {
                in_min: -1.0,
                in_max: 1.0,
                out_min: 0.4 + rng.next_f32() * 0.4,
                out_max: 1.0 + rng.next_f32() * 1.1,
                clamp: true,
            }),
        )?;
        builder.connect_channel_input(node, remap, 0);
        channel_pool.push(remap);
    }

    let mut sop_pool = Vec::with_capacity(sop_count as usize);
    for _ in 0..sop_count {
        let node = if rng.next_f32() < 0.55 {
            add_circle(&mut builder, ctx, &mut rng)?
        } else {
            add_sphere(&mut builder, ctx, &mut rng)?
        };
        sop_pool.push(node);
    }

    let mut top_pool = Vec::with_capacity(camera_count as usize);
    for _ in 0..camera_count {
        let camera = add_camera(&mut builder, ctx, &mut rng)?;
        let primitive = choose_node(&sop_pool, &mut rng)?;
        builder.connect_sop_input(primitive, camera, 0);
        if rng.next_f32() < 0.85 {
            let ch = choose_node(&channel_pool, &mut rng)?;
            builder.connect_channel_input(ch, camera, 1);
        }
        top_pool.push(camera);
    }

    let mut current = choose_node(&top_pool, &mut rng)?;
    let mut remaining: Vec<NodeId> = top_pool.into_iter().filter(|id| *id != current).collect();
    while let Some(next) = pop_random(&mut remaining, &mut rng) {
        let blend_mode = LayerBlendMode::from_u32(rng.next_u32());
        let blend = ctx.nodes.create(
            &mut builder,
            "blend",
            NodePayload::Blend(random_blend(&mut rng, blend_mode, 0.28, 0.88)),
        )?;
        builder.connect_luma_input(current, blend, 0);
        builder.connect_luma_input(next, blend, 1);
        current = blend;
    }

    if rng.next_f32() < 0.75 {
        let warp_scale = 0.8 + rng.next_f32() * 0.6;
        let warp = ctx.nodes.create(
            &mut builder,
            "warp-transform",
            NodePayload::WarpTransform(random_warp(&mut rng, warp_scale)),
        )?;
        builder.connect_luma(current, warp);
        if rng.next_f32() < 0.8 {
            let ch = choose_node(&channel_pool, &mut rng)?;
            builder.connect_channel_input(ch, warp, 1);
        }
        current = warp;
    }

    let tone = ctx.nodes.create(
        &mut builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(&mut rng)),
    )?;
    builder.connect_luma(current, tone);
    if rng.next_f32() < 0.9 {
        let ch = choose_node(&channel_pool, &mut rng)?;
        builder.connect_channel_input(ch, tone, 1);
    }

    add_outputs(&mut builder, ctx, tone)?;
    builder.build()
}

fn add_lfo(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    wave: ChopWave,
) -> Result<NodeId, GraphBuildError> {
    ctx.nodes.create(
        builder,
        "chop-lfo",
        NodePayload::ChopLfo(ChopLfoNode {
            wave,
            frequency: 0.18 + rng.next_f32() * 1.4,
            phase: rng.next_f32(),
            amplitude: 0.35 + rng.next_f32() * 0.75,
            offset: 0.65 + rng.next_f32() * 0.55,
        }),
    )
}

fn add_circle(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<NodeId, GraphBuildError> {
    ctx.nodes.create(
        builder,
        "sop-circle",
        NodePayload::SopCircle(SopCircleNode {
            radius: 0.16 + rng.next_f32() * 0.22,
            feather: 0.01 + rng.next_f32() * 0.05,
            center_x: (rng.next_f32() - 0.5) * 0.42,
            center_y: (rng.next_f32() - 0.5) * 0.42,
        }),
    )
}

fn add_sphere(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<NodeId, GraphBuildError> {
    ctx.nodes.create(
        builder,
        "sop-sphere",
        NodePayload::SopSphere(SopSphereNode {
            radius: 0.18 + rng.next_f32() * 0.20,
            center_x: (rng.next_f32() - 0.5) * 0.36,
            center_y: (rng.next_f32() - 0.5) * 0.36,
            light_x: (rng.next_f32() - 0.5) * 1.8,
            light_y: (rng.next_f32() - 0.5) * 1.8,
            ambient: 0.12 + rng.next_f32() * 0.36,
        }),
    )
}

fn add_camera(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<NodeId, GraphBuildError> {
    ctx.nodes.create(
        builder,
        "top-camera-render",
        NodePayload::TopCameraRender(TopCameraRenderNode {
            exposure: 0.85 + rng.next_f32() * 1.1,
            gamma: 0.8 + rng.next_f32() * 0.55,
            zoom: 0.8 + rng.next_f32() * 0.85,
            pan_x: (rng.next_f32() - 0.5) * 0.3,
            pan_y: (rng.next_f32() - 0.5) * 0.3,
            rotate: (rng.next_f32() - 0.5) * 1.25,
            invert: rng.next_f32() < 0.15,
        }),
    )
}

fn add_outputs(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    source: NodeId,
) -> Result<(), GraphBuildError> {
    let tap = ctx
        .nodes
        .create(builder, "output", NodePayload::Output(OutputNode::tap(1)))?;
    builder.connect_luma(source, tap);
    let output = ctx.nodes.create(
        builder,
        "output",
        NodePayload::Output(OutputNode::primary()),
    )?;
    builder.connect_luma(source, output);
    Ok(())
}

fn choose_node(pool: &[NodeId], rng: &mut XorShift32) -> Result<NodeId, GraphBuildError> {
    if pool.is_empty() {
        return Err(GraphBuildError::new(
            "touchdesigner preset has no available nodes",
        ));
    }
    let index = (rng.next_u32() as usize) % pool.len();
    Ok(pool[index])
}

fn pop_random(pool: &mut Vec<NodeId>, rng: &mut XorShift32) -> Option<NodeId> {
    if pool.is_empty() {
        None
    } else {
        let index = (rng.next_u32() as usize) % pool.len();
        Some(pool.swap_remove(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::NodeKind;
    use crate::presets::node_catalog::NodeCatalog;
    use crate::presets::subgraph_catalog::SubgraphCatalog;
    use crate::runtime_config::{AnimationConfig, AnimationMotion, V2Config};

    fn config(seed: u32) -> V2Config {
        V2Config {
            width: 512,
            height: 512,
            seed,
            count: 1,
            output: "test.png".to_string(),
            layers: 5,
            antialias: 1,
            preset: "td-random-network".to_string(),
            profile: V2Profile::Quality,
            animation: AnimationConfig {
                enabled: false,
                seconds: 30,
                fps: 30,
                keep_frames: false,
                motion: AnimationMotion::Normal,
            },
        }
    }

    #[test]
    fn td_random_network_is_seed_deterministic() {
        let nodes = NodeCatalog::with_builtins().expect("node catalog");
        let modules = SubgraphCatalog::with_builtins().expect("module catalog");
        let cfg = config(17);
        let ctx = PresetContext {
            config: &cfg,
            nodes: &nodes,
            modules: &modules,
        };
        let a = build_td_random_network(ctx).expect("graph a");
        let b = build_td_random_network(ctx).expect("graph b");
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }

    #[test]
    fn td_random_network_contains_chop_sop_and_top_camera() {
        let nodes = NodeCatalog::with_builtins().expect("node catalog");
        let modules = SubgraphCatalog::with_builtins().expect("module catalog");
        let cfg = config(33);
        let ctx = PresetContext {
            config: &cfg,
            nodes: &nodes,
            modules: &modules,
        };
        let graph = build_td_random_network(ctx).expect("graph");

        let mut has_chop = false;
        let mut has_sop = false;
        let mut has_camera = false;
        for node in &graph.nodes {
            match node.kind {
                NodeKind::ChopLfo(_) | NodeKind::ChopMath(_) | NodeKind::ChopRemap(_) => {
                    has_chop = true
                }
                NodeKind::SopCircle(_) | NodeKind::SopSphere(_) => has_sop = true,
                NodeKind::TopCameraRender(_) => has_camera = true,
                _ => {}
            }
        }

        assert!(has_chop);
        assert!(has_sop);
        assert!(has_camera);
    }
}

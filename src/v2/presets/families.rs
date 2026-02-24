//! Concrete graph-native preset family builders.

use crate::model::{LayerBlendMode, XorShift32};

use super::preset_catalog::PresetContext;
use super::primitives::{add_layers, random_blend, random_tonemap, random_warp, render_size};
use super::subgraph_catalog::{ModuleBuildContext, ModuleRequest};
use super::{node_catalog::NodePayload, subgraph_catalog::ModuleResult};
use crate::v2::graph::{GpuGraph, GraphBuildError, GraphBuilder, NodeId};

pub(super) fn build_hybrid_stack(ctx: PresetContext<'_>) -> Result<GpuGraph, GraphBuildError> {
    let (mut builder, mut rng) = builder_with_rng(ctx, 0x2F94_11D3, 0x771B_6A83);
    let layers = add_layers(
        &mut builder,
        ctx.nodes,
        ctx.config.layers.max(3),
        ctx.config.profile,
        &mut rng,
        true,
    )?;

    let warped = create_warp(&mut builder, ctx, &mut rng, 1.0, layers[0])?;
    let toned = create_tone(&mut builder, ctx, &mut rng, layers[1])?;

    let first_mask = module_noise_mask(&mut builder, ctx, &mut rng, 0x1111_0001)?.primary;
    let mix = create_blend(
        &mut builder,
        ctx,
        &mut rng,
        LayerBlendMode::Overlay,
        0.55,
        0.95,
    )?;
    builder.connect_luma_input(warped, mix, 0);
    builder.connect_luma_input(toned, mix, 1);
    builder.connect_mask_input(first_mask, mix, 2);

    let mut current = mix;
    for (stage, &layer) in layers.iter().enumerate().skip(2) {
        let processed = if stage % 2 == 0 {
            create_warp(&mut builder, ctx, &mut rng, 0.85, current)?
        } else {
            create_tone(&mut builder, ctx, &mut rng, current)?
        };

        let merge = create_blend(
            &mut builder,
            ctx,
            &mut rng,
            LayerBlendMode::Screen,
            0.35,
            0.88,
        )?;
        builder.connect_luma_input(processed, merge, 0);
        builder.connect_luma_input(layer, merge, 1);
        if stage % 2 == 1 {
            let mask = module_noise_mask(&mut builder, ctx, &mut rng, stage as u32)?.primary;
            builder.connect_mask_input(mask, merge, 2);
        }
        current = merge;
    }

    wire_output(&mut builder, ctx, current)?;
    builder.build()
}

pub(super) fn build_field_weave(ctx: PresetContext<'_>) -> Result<GpuGraph, GraphBuildError> {
    let (mut builder, mut rng) = builder_with_rng(ctx, 0x6D20_4E5B, 0x109A_AA37);
    let layers = add_layers(
        &mut builder,
        ctx.nodes,
        ctx.config.layers.max(3),
        ctx.config.profile,
        &mut rng,
        false,
    )?;

    let warp_a = create_warp(&mut builder, ctx, &mut rng, 0.75, layers[0])?;
    let warp_b = create_warp(&mut builder, ctx, &mut rng, 1.1, layers[1])?;
    let mask_a = module_noise_mask(&mut builder, ctx, &mut rng, 0xA001)?.primary;

    let blend_ab = create_blend(
        &mut builder,
        ctx,
        &mut rng,
        LayerBlendMode::Glow,
        0.45,
        0.90,
    )?;
    builder.connect_luma_input(warp_a, blend_ab, 0);
    builder.connect_luma_input(warp_b, blend_ab, 1);
    builder.connect_mask_input(mask_a, blend_ab, 2);

    let tone = create_tone(&mut builder, ctx, &mut rng, blend_ab)?;
    let mask_b = module_noise_mask(&mut builder, ctx, &mut rng, 0xA002)?.primary;
    let blend_final = create_blend(
        &mut builder,
        ctx,
        &mut rng,
        LayerBlendMode::Difference,
        0.35,
        0.80,
    )?;
    builder.connect_luma_input(tone, blend_final, 0);
    builder.connect_luma_input(layers[2], blend_final, 1);
    builder.connect_mask_input(mask_b, blend_final, 2);

    let mut current = blend_final;
    for &layer in layers.iter().skip(3) {
        let module_seed = rng.next_u32();
        current = module_masked_blend(
            &mut builder,
            ctx,
            &mut rng,
            vec![current, layer],
            module_seed,
        )?
        .primary;
    }

    wire_output(&mut builder, ctx, current)?;
    builder.build()
}

pub(super) fn build_node_weave(ctx: PresetContext<'_>) -> Result<GpuGraph, GraphBuildError> {
    let (mut builder, mut rng) = builder_with_rng(ctx, 0xA511_2F03, 0xB76D_5E29);
    let layers = add_layers(
        &mut builder,
        ctx.nodes,
        ctx.config.layers.max(2),
        ctx.config.profile,
        &mut rng,
        true,
    )?;

    let warp = create_warp(&mut builder, ctx, &mut rng, 1.0, layers[0])?;
    let tone = create_tone(&mut builder, ctx, &mut rng, layers[1])?;
    let first = module_masked_blend(&mut builder, ctx, &mut rng, vec![warp, tone], 0xC001)?.primary;

    let mut current = first;
    for (index, &layer) in layers.iter().enumerate().skip(2) {
        let processed = if index % 2 == 0 {
            create_warp(&mut builder, ctx, &mut rng, 0.8, current)?
        } else {
            create_tone(&mut builder, ctx, &mut rng, current)?
        };
        let merge = create_blend(
            &mut builder,
            ctx,
            &mut rng,
            LayerBlendMode::Lighten,
            0.28,
            0.68,
        )?;
        builder.connect_luma_input(processed, merge, 0);
        builder.connect_luma_input(layer, merge, 1);
        current = merge;
    }

    wire_output(&mut builder, ctx, current)?;
    builder.build()
}

pub(super) fn build_mask_atlas(ctx: PresetContext<'_>) -> Result<GpuGraph, GraphBuildError> {
    let (mut builder, mut rng) = builder_with_rng(ctx, 0xC7E6_00D1, 0x9F0C_B8A5);
    let layers = add_layers(
        &mut builder,
        ctx.nodes,
        ctx.config.layers.max(4),
        ctx.config.profile,
        &mut rng,
        false,
    )?;

    let mask_a = module_noise_mask(&mut builder, ctx, &mut rng, 0xD001)?.primary;
    let mask_b = module_noise_mask(&mut builder, ctx, &mut rng, 0xD002)?.primary;
    let mask_c = module_noise_mask(&mut builder, ctx, &mut rng, 0xD003)?.primary;

    let blend_a = create_blend(&mut builder, ctx, &mut rng, LayerBlendMode::Add, 0.30, 0.80)?;
    builder.connect_luma_input(layers[0], blend_a, 0);
    builder.connect_luma_input(layers[1], blend_a, 1);
    builder.connect_mask_input(mask_a, blend_a, 2);

    let blend_b = create_blend(
        &mut builder,
        ctx,
        &mut rng,
        LayerBlendMode::Multiply,
        0.35,
        0.82,
    )?;
    builder.connect_luma_input(layers[2], blend_b, 0);
    builder.connect_luma_input(layers[3], blend_b, 1);
    builder.connect_mask_input(mask_b, blend_b, 2);

    let warp = create_warp(&mut builder, ctx, &mut rng, 1.2, blend_a)?;
    let tone = create_tone(&mut builder, ctx, &mut rng, blend_b)?;

    let final_mix = create_blend(
        &mut builder,
        ctx,
        &mut rng,
        LayerBlendMode::Overlay,
        0.45,
        0.92,
    )?;
    builder.connect_luma_input(warp, final_mix, 0);
    builder.connect_luma_input(tone, final_mix, 1);
    builder.connect_mask_input(mask_c, final_mix, 2);

    wire_output(&mut builder, ctx, final_mix)?;
    builder.build()
}

pub(super) fn build_warp_grid(ctx: PresetContext<'_>) -> Result<GpuGraph, GraphBuildError> {
    let (mut builder, mut rng) = builder_with_rng(ctx, 0x0F51_1109, 0x6CA3_5D11);
    let layers = add_layers(
        &mut builder,
        ctx.nodes,
        ctx.config.layers.max(3),
        ctx.config.profile,
        &mut rng,
        true,
    )?;

    let warp_a = create_warp(&mut builder, ctx, &mut rng, 0.8, layers[0])?;
    let warp_b = create_warp(&mut builder, ctx, &mut rng, 1.1, warp_a)?;
    let tone = create_tone(&mut builder, ctx, &mut rng, layers[1])?;

    let first_mix = create_blend(
        &mut builder,
        ctx,
        &mut rng,
        LayerBlendMode::Screen,
        0.4,
        0.86,
    )?;
    builder.connect_luma_input(warp_b, first_mix, 0);
    builder.connect_luma_input(tone, first_mix, 1);

    let warp_c = create_warp(&mut builder, ctx, &mut rng, 1.3, layers[2])?;
    let final_mix =
        module_masked_blend(&mut builder, ctx, &mut rng, vec![first_mix, warp_c], 0xE001)?.primary;

    wire_output(&mut builder, ctx, final_mix)?;
    builder.build()
}

fn builder_with_rng(
    ctx: PresetContext<'_>,
    graph_salt: u32,
    rng_salt: u32,
) -> (GraphBuilder, XorShift32) {
    let (render_width, render_height) = render_size(ctx.config);
    (
        GraphBuilder::new(render_width, render_height, ctx.config.seed ^ graph_salt),
        XorShift32::new(ctx.config.seed ^ rng_salt),
    )
}

fn create_warp(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    strength_scale: f32,
    input: NodeId,
) -> Result<NodeId, GraphBuildError> {
    let node = ctx.nodes.create(
        builder,
        "warp",
        NodePayload::WarpTransform(random_warp(rng, strength_scale)),
    )?;
    builder.connect_luma(input, node);
    Ok(node)
}

fn create_tone(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    input: NodeId,
) -> Result<NodeId, GraphBuildError> {
    let node = ctx
        .nodes
        .create(builder, "tone", NodePayload::ToneMap(random_tonemap(rng)))?;
    builder.connect_luma(input, node);
    Ok(node)
}

fn create_blend(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    fallback: LayerBlendMode,
    min: f32,
    max: f32,
) -> Result<NodeId, GraphBuildError> {
    ctx.nodes.create(
        builder,
        "blend",
        NodePayload::Blend(random_blend(rng, fallback, min, max)),
    )
}

fn wire_output(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    input: NodeId,
) -> Result<(), GraphBuildError> {
    let output = ctx.nodes.create(builder, "output", NodePayload::Output)?;
    builder.connect_luma(input, output);
    Ok(())
}

fn module_noise_mask(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    seed: u32,
) -> Result<ModuleResult, GraphBuildError> {
    let mut module_ctx = ModuleBuildContext {
        builder,
        nodes: ctx.nodes,
        rng,
    };
    let result = ctx.modules.execute(
        "noise-mask",
        &mut module_ctx,
        ModuleRequest {
            seed,
            profile: ctx.config.profile,
            inputs: Vec::new(),
        },
    )?;
    let _extra_count = result.extra_outputs.len();
    Ok(result)
}

fn module_masked_blend(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    inputs: Vec<NodeId>,
    seed: u32,
) -> Result<ModuleResult, GraphBuildError> {
    let mut module_ctx = ModuleBuildContext {
        builder,
        nodes: ctx.nodes,
        rng,
    };
    let result = ctx.modules.execute(
        "masked-blend",
        &mut module_ctx,
        ModuleRequest {
            seed,
            profile: ctx.config.profile,
            inputs,
        },
    )?;
    let _extra_count = result.extra_outputs.len();
    Ok(result)
}

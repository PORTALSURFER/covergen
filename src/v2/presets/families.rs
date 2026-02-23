//! Concrete graph-native preset family builders.

use crate::model::{LayerBlendMode, XorShift32};

use super::super::cli::V2Config;
use super::super::graph::{GpuGraph, GraphBuildError, GraphBuilder};
use super::primitives::{
    add_layers, add_noise_mask, random_blend, random_tonemap, random_warp, render_size,
};

pub(super) fn build_hybrid_stack(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    let (render_width, render_height) = render_size(config);
    let mut builder = GraphBuilder::new(render_width, render_height, config.seed ^ 0x2F94_11D3);
    let mut rng = XorShift32::new(config.seed ^ 0x771B_6A83);
    let layers = add_layers(
        &mut builder,
        config.layers.max(3),
        config.profile,
        &mut rng,
        true,
    );

    let warped = builder.add_warp_transform(random_warp(&mut rng, 1.0));
    builder.connect_luma(layers[0], warped);
    let toned = builder.add_tonemap(random_tonemap(&mut rng));
    builder.connect_luma(layers[1], toned);

    let first_mask = add_noise_mask(&mut builder, &mut rng, false);
    let mix = builder.add_blend(random_blend(&mut rng, LayerBlendMode::Overlay, 0.55, 0.95));
    builder.connect_luma_input(warped, mix, 0);
    builder.connect_luma_input(toned, mix, 1);
    builder.connect_mask_input(first_mask, mix, 2);

    let mut current = mix;
    for (stage, &layer) in layers.iter().enumerate().skip(2) {
        let preprocess = if stage % 2 == 0 {
            let node = builder.add_warp_transform(random_warp(&mut rng, 0.85));
            builder.connect_luma(current, node);
            node
        } else {
            let node = builder.add_tonemap(random_tonemap(&mut rng));
            builder.connect_luma(current, node);
            node
        };

        let blend = builder.add_blend(random_blend(&mut rng, LayerBlendMode::Screen, 0.35, 0.88));
        builder.connect_luma_input(preprocess, blend, 0);
        builder.connect_luma_input(layer, blend, 1);
        if stage % 2 == 1 {
            let invert = stage % 3 == 0;
            let mask = add_noise_mask(&mut builder, &mut rng, invert);
            builder.connect_mask_input(mask, blend, 2);
        }
        current = blend;
    }

    let output = builder.add_output();
    builder.connect_luma(current, output);
    builder.build()
}

pub(super) fn build_field_weave(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    let (render_width, render_height) = render_size(config);
    let mut builder = GraphBuilder::new(render_width, render_height, config.seed ^ 0x6D20_4E5B);
    let mut rng = XorShift32::new(config.seed ^ 0x109A_AA37);
    let layers = add_layers(
        &mut builder,
        config.layers.max(3),
        config.profile,
        &mut rng,
        false,
    );

    let warp_a = builder.add_warp_transform(random_warp(&mut rng, 0.75));
    builder.connect_luma(layers[0], warp_a);
    let warp_b = builder.add_warp_transform(random_warp(&mut rng, 1.1));
    builder.connect_luma(layers[1], warp_b);

    let first_mask = add_noise_mask(&mut builder, &mut rng, false);
    let blend_ab = builder.add_blend(random_blend(&mut rng, LayerBlendMode::Glow, 0.45, 0.9));
    builder.connect_luma_input(warp_a, blend_ab, 0);
    builder.connect_luma_input(warp_b, blend_ab, 1);
    builder.connect_mask_input(first_mask, blend_ab, 2);

    let tone = builder.add_tonemap(random_tonemap(&mut rng));
    builder.connect_luma(blend_ab, tone);

    let second_mask = add_noise_mask(&mut builder, &mut rng, true);
    let blend_final = builder.add_blend(random_blend(
        &mut rng,
        LayerBlendMode::Difference,
        0.35,
        0.8,
    ));
    builder.connect_luma_input(tone, blend_final, 0);
    builder.connect_luma_input(layers[2], blend_final, 1);
    builder.connect_mask_input(second_mask, blend_final, 2);

    let mut current = blend_final;
    for &layer in layers.iter().skip(3) {
        let invert = rng.next_f32() < 0.45;
        let mask = add_noise_mask(&mut builder, &mut rng, invert);
        let blend = builder.add_blend(random_blend(&mut rng, LayerBlendMode::Overlay, 0.3, 0.72));
        builder.connect_luma_input(current, blend, 0);
        builder.connect_luma_input(layer, blend, 1);
        builder.connect_mask_input(mask, blend, 2);
        current = blend;
    }

    let output = builder.add_output();
    builder.connect_luma(current, output);
    builder.build()
}

pub(super) fn build_node_weave(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    let (render_width, render_height) = render_size(config);
    let mut builder = GraphBuilder::new(render_width, render_height, config.seed ^ 0xA511_2F03);
    let mut rng = XorShift32::new(config.seed ^ 0xB76D_5E29);
    let layers = add_layers(
        &mut builder,
        config.layers.max(2),
        config.profile,
        &mut rng,
        true,
    );

    let warp = builder.add_warp_transform(random_warp(&mut rng, 1.0));
    builder.connect_luma(layers[0], warp);
    let tone = builder.add_tonemap(random_tonemap(&mut rng));
    builder.connect_luma(layers[1], tone);

    let mask = add_noise_mask(&mut builder, &mut rng, false);
    let blend = builder.add_blend(random_blend(&mut rng, LayerBlendMode::Overlay, 0.45, 0.9));
    builder.connect_luma_input(warp, blend, 0);
    builder.connect_luma_input(tone, blend, 1);
    builder.connect_mask_input(mask, blend, 2);

    let mut current = blend;
    for (index, &layer) in layers.iter().enumerate().skip(2) {
        let preprocess = if index % 2 == 0 {
            let node = builder.add_warp_transform(random_warp(&mut rng, 0.8));
            builder.connect_luma(current, node);
            node
        } else {
            let node = builder.add_tonemap(random_tonemap(&mut rng));
            builder.connect_luma(current, node);
            node
        };

        let merge = builder.add_blend(random_blend(&mut rng, LayerBlendMode::Lighten, 0.28, 0.68));
        builder.connect_luma_input(preprocess, merge, 0);
        builder.connect_luma_input(layer, merge, 1);
        current = merge;
    }

    let output = builder.add_output();
    builder.connect_luma(current, output);
    builder.build()
}

pub(super) fn build_mask_atlas(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    let (render_width, render_height) = render_size(config);
    let mut builder = GraphBuilder::new(render_width, render_height, config.seed ^ 0xC7E6_00D1);
    let mut rng = XorShift32::new(config.seed ^ 0x9F0C_B8A5);
    let layers = add_layers(
        &mut builder,
        config.layers.max(4),
        config.profile,
        &mut rng,
        false,
    );

    let mask_a = add_noise_mask(&mut builder, &mut rng, false);
    let mask_b = add_noise_mask(&mut builder, &mut rng, true);
    let invert_c = rng.next_f32() < 0.5;
    let mask_c = add_noise_mask(&mut builder, &mut rng, invert_c);

    let blend_a = builder.add_blend(random_blend(&mut rng, LayerBlendMode::Add, 0.3, 0.8));
    builder.connect_luma_input(layers[0], blend_a, 0);
    builder.connect_luma_input(layers[1], blend_a, 1);
    builder.connect_mask_input(mask_a, blend_a, 2);

    let blend_b = builder.add_blend(random_blend(&mut rng, LayerBlendMode::Multiply, 0.35, 0.82));
    builder.connect_luma_input(layers[2], blend_b, 0);
    builder.connect_luma_input(layers[3], blend_b, 1);
    builder.connect_mask_input(mask_b, blend_b, 2);

    let warp = builder.add_warp_transform(random_warp(&mut rng, 1.2));
    builder.connect_luma(blend_a, warp);
    let tone = builder.add_tonemap(random_tonemap(&mut rng));
    builder.connect_luma(blend_b, tone);

    let final_mix = builder.add_blend(random_blend(&mut rng, LayerBlendMode::Overlay, 0.45, 0.92));
    builder.connect_luma_input(warp, final_mix, 0);
    builder.connect_luma_input(tone, final_mix, 1);
    builder.connect_mask_input(mask_c, final_mix, 2);

    let output = builder.add_output();
    builder.connect_luma(final_mix, output);
    builder.build()
}

pub(super) fn build_warp_grid(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    let (render_width, render_height) = render_size(config);
    let mut builder = GraphBuilder::new(render_width, render_height, config.seed ^ 0x0F51_1109);
    let mut rng = XorShift32::new(config.seed ^ 0x6CA3_5D11);
    let layers = add_layers(
        &mut builder,
        config.layers.max(3),
        config.profile,
        &mut rng,
        true,
    );

    let warp_a = builder.add_warp_transform(random_warp(&mut rng, 0.8));
    builder.connect_luma(layers[0], warp_a);
    let warp_b = builder.add_warp_transform(random_warp(&mut rng, 1.1));
    builder.connect_luma(warp_a, warp_b);

    let tone = builder.add_tonemap(random_tonemap(&mut rng));
    builder.connect_luma(layers[1], tone);

    let first_mix = builder.add_blend(random_blend(&mut rng, LayerBlendMode::Screen, 0.4, 0.86));
    builder.connect_luma_input(warp_b, first_mix, 0);
    builder.connect_luma_input(tone, first_mix, 1);

    let warp_c = builder.add_warp_transform(random_warp(&mut rng, 1.3));
    builder.connect_luma(layers[2], warp_c);

    let mask = add_noise_mask(&mut builder, &mut rng, false);
    let final_mix = builder.add_blend(random_blend(
        &mut rng,
        LayerBlendMode::Difference,
        0.25,
        0.7,
    ));
    builder.connect_luma_input(first_mix, final_mix, 0);
    builder.connect_luma_input(warp_c, final_mix, 1);
    builder.connect_mask_input(mask, final_mix, 2);

    let output = builder.add_output();
    builder.connect_luma(final_mix, output);
    builder.build()
}

//! Programmatic graph preset generation for V2.

use crate::model::{ArtStyle, LayerBlendMode, SymmetryStyle, XorShift32};

use super::cli::{V2Config, V2Profile};
use super::graph::{GenerateLayerNode, GpuGraph, GraphBuildError, GraphBuilder};
use super::node::{BlendNode, MaskNode, PortType, SourceNoiseNode, ToneMapNode, WarpTransformNode};

/// Build a deterministic graph from the selected V2 preset and CLI config.
pub fn build_preset_graph(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    match config.preset.as_str() {
        "hybrid-stack" => build_hybrid_stack(config),
        "field-weave" => build_field_weave(config),
        "node-weave" => build_node_weave(config),
        other => Err(GraphBuildError::new(format!(
            "unknown v2 preset '{other}', expected hybrid-stack|field-weave|node-weave"
        ))),
    }
}

fn build_hybrid_stack(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    let render_width = config.width.saturating_mul(config.antialias);
    let render_height = config.height.saturating_mul(config.antialias);
    let mut builder = GraphBuilder::new(render_width, render_height, config.seed);
    let mut rng = XorShift32::new(config.seed);

    let mut previous = None;
    for layer_index in 0..config.layers {
        let layer = generate_layer_node(layer_index, config.layers, config.profile, &mut rng, true);
        let node = builder.add_generate_layer(layer);
        if let Some(prev) = previous {
            builder.connect_luma(prev, node);
        }
        previous = Some(node);
    }

    let output = builder.add_output();
    if let Some(last) = previous {
        builder.connect_luma(last, output);
    }

    builder.build()
}

fn build_field_weave(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    let render_width = config.width.saturating_mul(config.antialias);
    let render_height = config.height.saturating_mul(config.antialias);
    let mut builder = GraphBuilder::new(render_width, render_height, config.seed ^ 0x51A7_0D93);
    let mut rng = XorShift32::new(config.seed ^ 0x7A31_29C5);

    let mut previous = None;
    for layer_index in 0..config.layers {
        let layer =
            generate_layer_node(layer_index, config.layers, config.profile, &mut rng, false);
        let node = builder.add_generate_layer(layer);
        if let Some(prev) = previous {
            builder.connect_luma(prev, node);
        }
        previous = Some(node);
    }

    let output = builder.add_output();
    if let Some(last) = previous {
        builder.connect_luma(last, output);
    }

    builder.build()
}

fn build_node_weave(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    let render_width = config.width.saturating_mul(config.antialias);
    let render_height = config.height.saturating_mul(config.antialias);
    let mut builder = GraphBuilder::new(render_width, render_height, config.seed ^ 0xA511_2F03);
    let mut rng = XorShift32::new(config.seed ^ 0xB76D_5E29);

    let layer_a = builder.add_generate_layer(generate_layer_node(
        0,
        config.layers.max(2),
        config.profile,
        &mut rng,
        true,
    ));
    let layer_b = builder.add_generate_layer(generate_layer_node(
        1,
        config.layers.max(2),
        config.profile,
        &mut rng,
        false,
    ));

    let warp = builder.add_warp_transform(WarpTransformNode {
        strength: 0.55 + rng.next_f32() * 0.65,
        frequency: 0.8 + rng.next_f32() * 3.8,
        phase: rng.next_f32(),
    });
    builder.connect_luma(layer_a, warp);

    let tone = builder.add_tonemap(ToneMapNode {
        contrast: 1.1 + rng.next_f32() * 0.5,
        low_pct: 0.01 + rng.next_f32() * 0.03,
        high_pct: 0.96 + rng.next_f32() * 0.03,
    });
    builder.connect_luma(layer_b, tone);

    let noise = builder.add_source_noise(SourceNoiseNode {
        seed: rng.next_u32(),
        scale: 1.6 + rng.next_f32() * 6.0,
        octaves: 3 + (rng.next_u32() % 3),
        amplitude: 0.7 + rng.next_f32() * 0.45,
        output_port: PortType::LumaTexture,
    });
    let mask = builder.add_mask(MaskNode {
        threshold: 0.42 + rng.next_f32() * 0.2,
        softness: 0.06 + rng.next_f32() * 0.2,
        invert: rng.next_f32() < 0.35,
    });
    builder.connect_luma(noise, mask);

    let blend = builder.add_blend(BlendNode {
        mode: LayerBlendMode::Overlay,
        opacity: 0.45 + rng.next_f32() * 0.45,
    });
    builder.connect_luma_input(warp, blend, 0);
    builder.connect_luma_input(tone, blend, 1);
    builder.connect_mask_input(mask, blend, 2);

    let output = builder.add_output();
    builder.connect_luma(blend, output);
    builder.build()
}

fn generate_layer_node(
    layer_index: u32,
    total_layers: u32,
    profile: V2Profile,
    rng: &mut XorShift32,
    emphasize_fractal: bool,
) -> GenerateLayerNode {
    let fast = matches!(profile, V2Profile::Performance);
    let style = if emphasize_fractal {
        ArtStyle::from_u32(rng.next_u32())
    } else {
        ArtStyle::from_u32((rng.next_u32() % 6) + 11)
    };
    let secondary = ArtStyle::from_u32(style.as_u32() + 1 + (rng.next_u32() % 3));
    let base_opacity = if layer_index == 0 {
        1.0
    } else if fast {
        0.28 + rng.next_f32() * 0.40
    } else {
        0.35 + rng.next_f32() * 0.45
    };

    GenerateLayerNode {
        symmetry: 2 + (rng.next_u32() % 8),
        symmetry_style: if rng.next_f32() < 0.15 {
            SymmetryStyle::Mirror.as_u32()
        } else {
            SymmetryStyle::Radial.as_u32()
        },
        iterations: if fast {
            96 + (rng.next_u32() % 180)
        } else {
            180 + (rng.next_u32() % 320)
        },
        seed: rng.next_u32() ^ (layer_index.wrapping_mul(0x9E37_79B9)),
        fill_scale: (1.0 + rng.next_f32() * 1.1).clamp(0.7, 2.4),
        fractal_zoom: (0.45 + rng.next_f32() * 0.9).clamp(0.35, 1.8),
        art_style: style.as_u32(),
        art_style_secondary: secondary.as_u32(),
        art_style_mix: (0.18 + rng.next_f32() * 0.70).clamp(0.0, 1.0),
        bend_strength: rng.next_f32() * if fast { 0.9 } else { 1.35 },
        warp_strength: rng.next_f32() * if fast { 0.9 } else { 1.35 },
        warp_frequency: 0.5 + rng.next_f32() * 5.2,
        tile_scale: 0.35 + rng.next_f32() * 1.1,
        tile_phase: rng.next_f32(),
        center_x: (rng.next_f32() * 2.0 - 1.0) * if fast { 0.15 } else { 0.28 },
        center_y: (rng.next_f32() * 2.0 - 1.0) * if fast { 0.15 } else { 0.28 },
        shader_layer_count: (2 + (rng.next_u32() % 6)).min(1 + total_layers),
        blend_mode: if layer_index == 0 {
            LayerBlendMode::Normal
        } else {
            LayerBlendMode::from_u32(rng.next_u32())
        },
        opacity: base_opacity.clamp(0.0, 1.0),
        contrast: if fast {
            1.05 + rng.next_f32() * 0.35
        } else {
            1.15 + rng.next_f32() * 0.55
        },
    }
}

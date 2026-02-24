//! Shared node construction primitives for V2 presets.

use crate::model::{ArtStyle, LayerBlendMode, SymmetryStyle, XorShift32};

use super::super::cli::{V2Config, V2Profile};
use super::super::graph::{GenerateLayerNode, GraphBuildError, GraphBuilder, NodeId};
use super::super::node::{
    BlendNode, BlendTemporal, GenerateLayerTemporal, TemporalCurve, TemporalModulation,
    ToneMapNode, ToneMapTemporal, WarpTransformNode, WarpTransformTemporal,
};
use super::node_catalog::{NodeCatalog, NodePayload};

pub(super) fn render_size(config: &V2Config) -> (u32, u32) {
    (
        config.width.saturating_mul(config.antialias),
        config.height.saturating_mul(config.antialias),
    )
}

pub(super) fn add_layers(
    builder: &mut GraphBuilder,
    nodes: &NodeCatalog,
    count: u32,
    profile: V2Profile,
    rng: &mut XorShift32,
    emphasize_fractal: bool,
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut layers = Vec::with_capacity(count as usize);
    for layer_index in 0..count {
        let node = generate_layer_node(layer_index, count, profile, rng, emphasize_fractal);
        layers.push(nodes.create(builder, "generate-layer", NodePayload::GenerateLayer(node))?);
    }
    Ok(layers)
}

pub(super) fn random_blend(
    rng: &mut XorShift32,
    fallback: LayerBlendMode,
    opacity_min: f32,
    opacity_max: f32,
) -> BlendNode {
    let mode = if rng.next_f32() < 0.5 {
        fallback
    } else {
        LayerBlendMode::from_u32(rng.next_u32())
    };

    BlendNode {
        mode,
        opacity: opacity_min + (rng.next_f32() * (opacity_max - opacity_min)),
        temporal: BlendTemporal {
            opacity_mul: Some(expr_sine(0.24, 0.55 + rng.next_f32() * 0.6, rng.next_f32())),
        },
    }
}

pub(super) fn random_tonemap(rng: &mut XorShift32) -> ToneMapNode {
    ToneMapNode {
        contrast: 1.05 + rng.next_f32() * 0.55,
        low_pct: 0.005 + rng.next_f32() * 0.035,
        high_pct: 0.94 + rng.next_f32() * 0.05,
        temporal: ToneMapTemporal {
            contrast_mul: Some(expr_sine(
                0.14,
                0.65 + rng.next_f32() * 0.55,
                rng.next_f32(),
            )),
            low_pct_add: Some(curve(0.012, 0.6 + rng.next_f32() * 0.5, rng.next_f32())),
            high_pct_add: Some(curve(0.012, 0.8 + rng.next_f32() * 0.5, rng.next_f32())),
        },
    }
}

pub(super) fn random_warp(rng: &mut XorShift32, strength_scale: f32) -> WarpTransformNode {
    WarpTransformNode {
        strength: (0.4 + rng.next_f32() * 0.95) * strength_scale,
        frequency: 0.55 + rng.next_f32() * 4.8,
        phase: rng.next_f32(),
        temporal: WarpTransformTemporal {
            strength_mul: Some(curve(0.18, 0.75 + rng.next_f32() * 0.5, rng.next_f32())),
            frequency_mul: Some(expr_sine(0.15, 0.6 + rng.next_f32() * 0.55, rng.next_f32())),
            phase_add: Some(curve(0.24, 0.8 + rng.next_f32() * 0.7, rng.next_f32())),
        },
    }
}

pub(super) fn generate_layer_node(
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
        temporal: GenerateLayerTemporal {
            iterations_scale: Some(curve(0.12, 1.0 + rng.next_f32() * 0.35, rng.next_f32())),
            fill_scale_mul: Some(curve(0.08, 0.6 + rng.next_f32() * 0.3, rng.next_f32())),
            fractal_zoom_mul: Some(curve(0.08, 0.8 + rng.next_f32() * 0.4, rng.next_f32())),
            art_style_mix_add: Some(curve(0.10, 0.7 + rng.next_f32() * 0.5, rng.next_f32())),
            warp_strength_mul: Some(curve(0.14, 0.9 + rng.next_f32() * 0.5, rng.next_f32())),
            warp_frequency_add: Some(curve(0.35, 0.75 + rng.next_f32() * 0.45, rng.next_f32())),
            tile_phase_add: Some(curve(0.08, 1.1 + rng.next_f32() * 0.3, rng.next_f32())),
            center_x_add: Some(curve(0.05, 0.65 + rng.next_f32() * 0.35, rng.next_f32())),
            center_y_add: Some(curve(0.05, 0.65 + rng.next_f32() * 0.35, rng.next_f32())),
            opacity_mul: Some(curve(0.12, 0.9 + rng.next_f32() * 0.4, rng.next_f32())),
            contrast_mul: Some(curve(0.10, 0.85 + rng.next_f32() * 0.35, rng.next_f32())),
        },
    }
}

fn curve(amplitude: f32, frequency: f32, phase: f32) -> TemporalModulation {
    TemporalCurve::sine(amplitude, frequency, phase, 0.0).into()
}

fn expr_sine(amplitude: f32, frequency: f32, phase: f32) -> TemporalModulation {
    let expression = format!("{amplitude} * sin((t * {frequency} + {phase}) * tau) * i");
    TemporalModulation::parse(&expression).unwrap_or_else(|_| curve(amplitude, frequency, phase))
}

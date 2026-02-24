//! Post-build graph tuning from high-level art-direction controls.
//!
//! Presets generate a base deterministic graph. This pass applies user-facing
//! creative intent knobs without requiring each preset to duplicate the same
//! tuning logic.

use crate::art_direction::{ArtDirectionConfig, MoodDirection, SymmetryDirection};
use crate::graph::GpuGraph;
use crate::node::{BlendNode, GenerateLayerNode, MaskNode, NodeKind, SourceNoiseNode, ToneMapNode};

/// Apply high-level art-direction controls to a generated graph in place.
pub(super) fn apply_graph_art_direction(graph: &mut GpuGraph, art: ArtDirectionConfig) {
    for spec in &mut graph.nodes {
        match &mut spec.kind {
            NodeKind::GenerateLayer(node) => apply_generate_layer(node, art),
            NodeKind::SourceNoise(node) => apply_source_noise(node, art),
            NodeKind::Mask(node) => apply_mask(node, art),
            NodeKind::Blend(node) => apply_blend(node, art),
            NodeKind::ToneMap(node) => apply_tonemap(node, art),
            NodeKind::WarpTransform(node) => apply_warp(node, art),
            NodeKind::StatefulFeedback(node) => {
                node.mix = (node.mix * art.chaos_gain()).clamp(0.05, 0.95);
            }
            _ => {}
        }
    }
}

fn apply_generate_layer(node: &mut GenerateLayerNode, art: ArtDirectionConfig) {
    let energy = art.energy_gain();
    let chaos = art.chaos_gain();
    let texture_gain = (energy * chaos).clamp(0.5, 1.8);
    node.iterations = scaled_u32(node.iterations, (0.85 * energy) + (0.15 * chaos), 48, 2800);
    node.bend_strength = (node.bend_strength * texture_gain).clamp(0.0, 2.4);
    node.warp_strength = (node.warp_strength * texture_gain).clamp(0.0, 2.4);
    node.warp_frequency = (node.warp_frequency * (0.88 + 0.22 * chaos)).clamp(0.08, 9.5);
    node.center_x = (node.center_x * chaos).clamp(-0.65, 0.65);
    node.center_y = (node.center_y * chaos).clamp(-0.65, 0.65);
    node.opacity = (node.opacity * art.mood_opacity_gain()).clamp(0.05, 1.0);
    node.contrast = (node.contrast * art.mood_contrast_gain()).clamp(0.85, 3.0);
    apply_symmetry(node, art);
    node.art_style = palette_style(node.art_style, node.seed, art);
    node.art_style_secondary =
        palette_style(node.art_style_secondary, node.seed ^ 0x9E37_79B9, art);
    node.art_style_mix = toward(node.art_style_mix, art.palette_mix_target(), 0.55).clamp(0.0, 1.0);
}

fn apply_symmetry(node: &mut GenerateLayerNode, art: ArtDirectionConfig) {
    let (min_symmetry, max_symmetry) = art.symmetry_range();
    node.symmetry = node.symmetry.clamp(min_symmetry, max_symmetry);
    match art.symmetry {
        SymmetryDirection::Low => {
            if node.symmetry > 3 {
                node.symmetry = 3;
            }
            if node.symmetry_style == 1 {
                node.symmetry_style = 0;
            }
        }
        SymmetryDirection::Medium => {}
        SymmetryDirection::High => {
            if node.symmetry < 6 {
                node.symmetry = 6;
            }
            if node.symmetry_style == 0 {
                node.symmetry_style = 1;
            }
        }
    }
}

fn apply_source_noise(node: &mut SourceNoiseNode, art: ArtDirectionConfig) {
    let chaos = art.chaos_gain();
    node.scale = (node.scale * (0.84 + 0.32 * chaos)).clamp(0.06, 36.0);
    node.amplitude = (node.amplitude * (0.80 + 0.30 * chaos)).clamp(0.04, 2.0);
    let octave_gain = if chaos >= 1.2 {
        1
    } else if chaos <= 0.8 {
        -1
    } else {
        0
    };
    node.octaves = node.octaves.saturating_add_signed(octave_gain).clamp(1, 8);
}

fn apply_mask(node: &mut MaskNode, art: ArtDirectionConfig) {
    node.threshold = match art.mood {
        MoodDirection::Moody => (node.threshold + 0.06).clamp(0.0, 1.0),
        MoodDirection::Bright => (node.threshold - 0.05).clamp(0.0, 1.0),
        MoodDirection::Dreamy => (node.threshold - 0.02).clamp(0.0, 1.0),
        MoodDirection::Balanced => node.threshold,
    };
    node.softness = (node.softness * (1.0 / art.chaos_gain()).clamp(0.6, 1.5)).clamp(0.0, 1.0);
}

fn apply_blend(node: &mut BlendNode, art: ArtDirectionConfig) {
    let scale = (art.mood_opacity_gain() * (0.92 + 0.14 * art.chaos_gain())).clamp(0.7, 1.3);
    node.opacity = (node.opacity * scale).clamp(0.05, 1.0);
}

fn apply_tonemap(node: &mut ToneMapNode, art: ArtDirectionConfig) {
    node.contrast = (node.contrast * art.mood_contrast_gain()).clamp(0.8, 3.2);
    match art.mood {
        MoodDirection::Moody => {
            node.low_pct = (node.low_pct + 0.008).clamp(0.0, 0.9);
            node.high_pct = (node.high_pct - 0.012).clamp(node.low_pct + 0.01, 1.0);
        }
        MoodDirection::Bright => {
            node.low_pct = (node.low_pct - 0.006).clamp(0.0, 0.9);
            node.high_pct = (node.high_pct + 0.008).clamp(node.low_pct + 0.01, 1.0);
        }
        MoodDirection::Dreamy => {
            node.low_pct = (node.low_pct + 0.003).clamp(0.0, 0.9);
            node.high_pct = (node.high_pct + 0.012).clamp(node.low_pct + 0.01, 1.0);
        }
        MoodDirection::Balanced => {}
    }
}

fn apply_warp(node: &mut crate::node::WarpTransformNode, art: ArtDirectionConfig) {
    let energy = art.energy_gain();
    let chaos = art.chaos_gain();
    node.strength = (node.strength * (0.82 + 0.24 * energy) * chaos).clamp(0.0, 2.6);
    node.frequency = (node.frequency * (0.86 + 0.28 * chaos)).clamp(0.05, 14.0);
}

fn scaled_u32(value: u32, gain: f32, min: u32, max: u32) -> u32 {
    ((value as f32 * gain).round() as u32).clamp(min, max)
}

fn toward(current: f32, target: f32, amount: f32) -> f32 {
    current + (target - current) * amount.clamp(0.0, 1.0)
}

fn palette_style(current: u32, seed: u32, art: ArtDirectionConfig) -> u32 {
    let pool = art.palette_style_pool();
    if pool.is_empty() {
        return current;
    }
    let index = ((seed ^ current.wrapping_mul(0x85EB_CA6B)) as usize) % pool.len();
    pool[index]
}

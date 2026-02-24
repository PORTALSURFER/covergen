//! CPU fallback kernels for V2 node execution.
//!
//! These operators are used for non-layer graph nodes and for mixed graphs
//! that combine GPU-generated layers with explicit post-processing nodes.

use rayon::prelude::*;

use crate::image_ops::clamp01;
use crate::model::LayerBlendMode;

use super::node::WarpTransformNode;

/// Fill `out` with deterministic multi-octave value noise in [0, 1].
pub(crate) fn generate_source_noise(
    width: u32,
    height: u32,
    seed: u32,
    scale: f32,
    octaves: u32,
    amplitude: f32,
    out: &mut [f32],
) {
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    let octaves = octaves.max(1).min(8);
    let scale = scale.max(0.001);
    let amp = amplitude.clamp(0.0, 2.0);

    out.par_iter_mut().enumerate().for_each(|(idx, value)| {
        let x = (idx as u32 % width) as f32;
        let y = (idx as u32 / width) as f32;
        let nx = (x / width_f) * scale;
        let ny = (y / height_f) * scale;

        let mut sum = 0.0f32;
        let mut norm = 0.0f32;
        let mut frequency = 1.0f32;
        let mut octave_amp = 1.0f32;

        for octave in 0..octaves {
            let sample_seed = seed.wrapping_add(octave.wrapping_mul(0x9E37_79B9));
            let noise = value_noise_2d(nx * frequency, ny * frequency, sample_seed);
            sum += noise * octave_amp;
            norm += octave_amp;
            frequency *= 2.0;
            octave_amp *= 0.5;
        }

        let normalized = if norm > 0.0 { sum / norm } else { 0.0 };
        *value = clamp01(normalized * amp);
    });
}

/// Convert luma values into a soft threshold mask in [0, 1].
pub(crate) fn build_mask(
    input: &[f32],
    threshold: f32,
    softness: f32,
    invert: bool,
    out: &mut [f32],
) {
    let threshold = threshold.clamp(0.0, 1.0);
    let softness = softness.max(0.0);
    let half = softness * 0.5;
    let edge_min = threshold - half;
    let edge_max = threshold + half;

    out.par_iter_mut().enumerate().for_each(|(idx, target)| {
        let source = input[idx];
        let mut value = if softness <= f32::EPSILON {
            if source >= threshold {
                1.0
            } else {
                0.0
            }
        } else {
            smoothstep(edge_min, edge_max, source)
        };

        if invert {
            value = 1.0 - value;
        }

        *target = clamp01(value);
    });
}

/// Blend `top` into `dst` using mode, opacity, and optional per-pixel mask.
pub(crate) fn blend_with_mask(
    dst: &mut [f32],
    top: &[f32],
    mode: LayerBlendMode,
    opacity: f32,
    mask: Option<&[f32]>,
) {
    let alpha = opacity.clamp(0.0, 1.0);

    match mask {
        Some(mask_values) => {
            dst.par_iter_mut().enumerate().for_each(|(idx, base)| {
                let mixed = blend_formula(*base, top[idx], mode);
                let layer_alpha = alpha * mask_values[idx].clamp(0.0, 1.0);
                *base = clamp01((1.0 - layer_alpha) * *base + layer_alpha * mixed);
            });
        }
        None => {
            dst.par_iter_mut().enumerate().for_each(|(idx, base)| {
                let mixed = blend_formula(*base, top[idx], mode);
                *base = clamp01((1.0 - alpha) * *base + alpha * mixed);
            });
        }
    }
}

/// Apply a deterministic warp transform from `src` into `out`.
pub(crate) fn warp_luma(
    src: &[f32],
    width: u32,
    height: u32,
    spec: WarpTransformNode,
    out: &mut [f32],
) {
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    let frequency = spec.frequency.max(0.01);
    let strength = spec.strength.clamp(0.0, 2.0) * 0.02;
    let phase = spec.phase;

    out.par_iter_mut().enumerate().for_each(|(idx, value)| {
        let x = (idx as u32 % width) as f32;
        let y = (idx as u32 / width) as f32;
        let u = x / width_f;
        let v = y / height_f;

        let dx = ((v * frequency + phase) * std::f32::consts::TAU).sin() * strength;
        let dy = ((u * frequency * 0.87 + phase * 1.13) * std::f32::consts::TAU).cos() * strength;

        let sample_x = x + dx * width_f;
        let sample_y = y + dy * height_f;
        *value = sample_bilinear(src, width, height, sample_x, sample_y);
    });
}

fn blend_formula(base: f32, top: f32, mode: LayerBlendMode) -> f32 {
    match mode {
        LayerBlendMode::Normal => top,
        LayerBlendMode::Add => clamp01(base + top),
        LayerBlendMode::Multiply => clamp01(base * top),
        LayerBlendMode::Screen => 1.0 - ((1.0 - base) * (1.0 - top)),
        LayerBlendMode::Overlay => {
            if base < 0.5 {
                2.0 * base * top
            } else {
                1.0 - (2.0 * (1.0 - base) * (1.0 - top))
            }
        }
        LayerBlendMode::Difference => (base - top).abs(),
        LayerBlendMode::Lighten => base.max(top),
        LayerBlendMode::Darken => base.min(top),
        LayerBlendMode::Glow => base + (1.0 - base) * (top * top),
        LayerBlendMode::Shadow => base * top,
    }
}

fn sample_bilinear(src: &[f32], width: u32, height: u32, x: f32, y: f32) -> f32 {
    let x0 = x.floor();
    let y0 = y.floor();
    let x1 = x0 + 1.0;
    let y1 = y0 + 1.0;

    let tx = x - x0;
    let ty = y - y0;

    let p00 = sample_clamped(src, width, height, x0 as i32, y0 as i32);
    let p10 = sample_clamped(src, width, height, x1 as i32, y0 as i32);
    let p01 = sample_clamped(src, width, height, x0 as i32, y1 as i32);
    let p11 = sample_clamped(src, width, height, x1 as i32, y1 as i32);

    let top = p00 + (p10 - p00) * tx;
    let bottom = p01 + (p11 - p01) * tx;
    clamp01(top + (bottom - top) * ty)
}

fn sample_clamped(src: &[f32], width: u32, height: u32, x: i32, y: i32) -> f32 {
    let max_x = width.saturating_sub(1) as i32;
    let max_y = height.saturating_sub(1) as i32;
    let cx = x.clamp(0, max_x) as usize;
    let cy = y.clamp(0, max_y) as usize;
    src[cy * width as usize + cx]
}

fn value_noise_2d(x: f32, y: f32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;

    let fx = x - x.floor();
    let fy = y - y.floor();
    let sx = smoothstep01(fx);
    let sy = smoothstep01(fy);

    let n00 = hash_to_unit(x0, y0, seed);
    let n10 = hash_to_unit(x1, y0, seed);
    let n01 = hash_to_unit(x0, y1, seed);
    let n11 = hash_to_unit(x1, y1, seed);

    let ix0 = n00 + (n10 - n00) * sx;
    let ix1 = n01 + (n11 - n01) * sx;
    clamp01(ix0 + (ix1 - ix0) * sy)
}

fn hash_to_unit(x: i32, y: i32, seed: u32) -> f32 {
    let mut v = seed ^ (x as u32).wrapping_mul(0x27D4_EB2D) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    v ^= v >> 15;
    v = v.wrapping_mul(0x2C1B_3C6D);
    v ^= v >> 12;
    v = v.wrapping_mul(0x297A_2D39);
    v ^= v >> 15;
    (v as f32) / (u32::MAX as f32)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge0 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    smoothstep01(t)
}

fn smoothstep01(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

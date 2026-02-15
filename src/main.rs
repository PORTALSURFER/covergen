use std::cmp::Ordering as CmpOrdering;
use std::io::{self, Cursor, Write};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use bytemuck::{Pod, Zeroable};
use image::{
    ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use rayon::prelude::*;

mod analysis;
mod blending;
mod config;
mod gpu_render;
mod render_workspace;
mod strategies;

use crate::analysis::{LumaStats, collect_luma_metrics, needs_complexity_fix};
use crate::blending::strategy_name;
use crate::config::{
    Config, MAX_OUTPUT_BYTES, MIN_IMAGE_DIMENSION, clamp_iteration_count, clamp_layer_count,
    resolve_fast_profile, resolve_fast_resolution, resolve_render_resolution,
};
use crate::gpu_render::GpuLayerRenderer;
use crate::render_workspace::RenderWorkspace;
use crate::strategies::{
    RenderStrategy, pick_render_strategy_near_family_with_preferences,
    pick_render_strategy_with_preferences, render_cpu_strategy, strategy_profile,
};

const SHADER: &str = r#"
struct Params {
    width: u32,
    height: u32,
    symmetry: u32,
    symmetry_style: u32,
    iterations: u32,
    seed: u32,
    fill_scale: f32,
    fractal_zoom: f32,
    art_style: u32,
    art_style_secondary: u32,
    art_style_mix: f32,
    bend_strength: f32,
    warp_strength: f32,
    warp_frequency: f32,
    tile_scale: f32,
    tile_phase: f32,
    center_x: f32,
    center_y: f32,
    layer_count: u32,
}

@group(0) @binding(0)
var<storage, read_write> out_pixels: array<u32>;

@group(0) @binding(1)
var<uniform> params: Params;

fn hash01(x: f32, y: f32, seed: u32) -> f32 {
    let s = f32(seed) * 0.00000011920928955;
    return fract(sin(dot(vec2<f32>(x, y), vec2<f32>(12.9898, 78.233)) + s) * 43758.5453123);
}

fn apply_domain_warp(px: f32, py: f32) -> vec2<f32> {
    var p = vec2<f32>(px, py);
    let seed = f32(params.seed) * 0.00000011920928955;
    let bend = clamp(params.bend_strength, 0.0, 1.8);
    let warp = clamp(params.warp_strength, 0.0, 1.8);
    let freq = clamp(params.warp_frequency, 0.25, 6.0);

    let radius = max(length(p), 0.0001);
    let angle = atan2(p.y, p.x);

    let radial = 1.0
        + (bend * (0.16 + 0.05 * freq) * cos(radius * (2.8 + freq) + seed + angle * 0.75))
        + (warp * (0.10 + 0.03 * freq) * sin(radius * 4.0 + angle + seed));
    let angular = bend * (0.32 + 0.07 * freq) * sin(angle * (2.2 + freq) + seed * 2.0);
    let warped_angle = angle + angular;
    let warped_radius = radius * radial;

    var wx = warped_radius * cos(warped_angle);
    var wy = warped_radius * sin(warped_angle);
    let wave = (sin((wx * (4.0 + 0.9 * freq)) + (seed * 7.0)) + 1.0) * 0.5;
    let wave2 = (cos((wy * (3.8 + 0.6 * freq)) + (seed * 5.0)) + 1.0) * 0.5;
    wx = wx + (wave - 0.5) * (0.5 * warp);
    wy = wy + (wave2 - 0.5) * (0.5 * warp);

    p = vec2<f32>(wx, wy);

    let swirl = sin((wx + wy + seed * 13.0) * (1.8 + 0.6 * freq)) * (bend * 0.08);
    return vec2<f32>(
        p.x * cos(swirl) - p.y * sin(swirl),
        p.x * sin(swirl) + p.y * cos(swirl),
    );
}

fn fold_for_symmetry(px: f32, py: f32, symmetry: u32, style: u32) -> vec2<f32> {
    var repeats = f32(symmetry) * 0.55 * clamp(params.tile_scale, 0.25, 1.8);
    repeats = repeats + sin((params.tile_phase * 6.283185307179586) * 0.22);
    repeats = clamp(repeats, 1.2, 3.5);
    if (style == 0u) {
        return vec2<f32>(px, py);
    }

    if (style == 2u) {
        return fold_symmetry_mirror(px, py, symmetry);
    }

    if (style == 3u) {
        return fold_symmetry_mirror_x(px, py, symmetry);
    }

    if (style == 4u) {
        return fold_symmetry_mirror_y(px, py, symmetry);
    }

    if (style == 5u) {
        return fold_symmetry_mirror_diagonal(px, py, symmetry);
    }

    if (style == 6u) {
        return fold_symmetry_mirror_cross(px, py, symmetry);
    }

    if (style == 7u) {
        return fold_symmetry_grid(px, py, symmetry, repeats, params.tile_phase);
    }

    return fold_symmetry_radial(px, py, symmetry);
}

fn fold_symmetry_radial(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    let radius = length(vec2<f32>(px, py));
    if (radius <= 0.0 || symmetry <= 1u) {
        return vec2<f32>(px, py);
    }

    let angle = atan2(py, px);
    let sector = 6.283185307179586 / f32(symmetry);
    let folded = fract((angle / sector) + 0.5) - 0.5;
    let folded_angle = folded * sector;

    let corner = 2.0 + (f32(symmetry) * 0.2);
    let edge = pow(abs(cos(folded_angle)), corner) + pow(abs(sin(folded_angle)), corner);
    let squircle = pow(edge, -1.0 / corner);
    let petals = abs(cos(folded_angle * f32(symmetry)));
    let folded_radius = radius * (0.75 + (petals * 0.45)) * squircle * 0.72;

    return vec2<f32>(folded_radius * cos(folded_angle), folded_radius * sin(folded_angle));
}

fn fold_symmetry_mirror(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    if (symmetry <= 1u) {
        return vec2<f32>(px, py);
    }

    var sx = px;
    var sy = py;
    sx = abs(sx);
    if (symmetry > 2u) {
        sy = abs(sy);
    }
    if (symmetry > 3u) {
        if (abs(sy) > abs(sx)) {
            let t = sx;
            sx = sy;
            sy = t;
        }
    }

    return vec2<f32>(sx * 1.0, sy * 1.0);
}

fn fold_symmetry_mirror_x(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    if (symmetry <= 1u) {
        return vec2<f32>(px, py);
    }
    if (symmetry > 3u) {
        return vec2<f32>(abs(px), abs(py));
    }

    return vec2<f32>(abs(px), py);
}

fn fold_symmetry_mirror_y(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    if (symmetry <= 1u) {
        return vec2<f32>(px, py);
    }
    if (symmetry > 3u) {
        return vec2<f32>(abs(px), abs(py));
    }

    return vec2<f32>(px, abs(py));
}

fn fold_symmetry_mirror_diagonal(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    let c = 0.70710678;
    let s = 0.70710678;
    let rotated_x = (px * c) + (py * s);
    let rotated_y = (-px * s) + (py * c);
    let folded = fold_symmetry_mirror(rotated_x, rotated_y, symmetry);
    let unrotated_x = (folded.x * c) - (folded.y * s);
    let unrotated_y = (folded.x * s) + (folded.y * c);
    return vec2<f32>(unrotated_x, unrotated_y);
}

fn fold_symmetry_mirror_cross(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    let first = fold_symmetry_mirror(abs(px), abs(py), max(symmetry, 2u));
    let second = fold_symmetry_mirror(first.x, first.y, max(symmetry, 2u));
    return vec2<f32>(abs(second.x), abs(second.y));
}

fn fold_symmetry_grid(
    px: f32,
    py: f32,
    symmetry: u32,
    repeats: f32,
    phase: f32,
) -> vec2<f32> {
    let skew = (phase * 6.283185307179586) + f32(symmetry) * 0.1;
    let rot = sin(skew) * 0.4;
    let c = cos(skew);
    let s = sin(skew);
    let rpx = px * c - py * s + (phase * 0.3);
    let rpy = px * s + py * c;

    let x_grid = fract((rpx * repeats) + (phase * 2.0) + sin(rpy * 2.0) * 0.25) - 0.5;
    let y_scale = 1.0 + sin(phase * 4.0) * 0.25;
    let y_grid = fract((rpy * repeats * y_scale) + (phase * 3.0) + cos(rpx * 3.0) * 0.2) - 0.5;

    let sx = abs(x_grid) * 2.0 - 0.5;
    let sy = abs(y_grid) * 2.0 - 0.5;
    let pulse = 0.03 * sin((sx * 12.0) + (sy * 9.0) + skew);
    return vec2<f32>(sx * (2.0 / repeats) + sy * 0.08 + rot * pulse, (sy * (2.0 / repeats)) + pulse * 1.8);
}

fn pack_gray(level: f32) -> u32 {
    let c = u32(clamp(level, 0.0, 1.0) * 255.0 + 0.5);
    return (255u << 24u) | (c << 16u) | (c << 8u) | c;
}

fn hybrid_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let jitter = hash01(x + base, y - base, params.seed + layer);
    let layer_shift = f32(layer) * 0.24 + (jitter - 0.5) * 0.35;
    let angle = layer_shift;
    let phase = hash01(x * 3.1, y * 4.7, params.seed + layer + 1u);
    let radius = 0.42 + 0.06 * f32(layer) + (jitter - 0.5) * 0.08;
    let cx = (radius * x * (3.2 + 0.2 * jitter)) + 0.16 * (sin(angle) + cos(angle * 0.7 + base));
    let cy = (radius * y * (3.0 + 0.2 * (1.0 - jitter))) + 0.16 * (cos(angle) + sin(angle * 0.9 - base));

    let rot_x = x * cos(angle) - y * sin(angle);
    let rot_y = x * sin(angle) + y * cos(angle);
    let orbit_scale = 1.62 + (f32(layer) * 0.02) + (phase - 0.5) * 0.15;
    var zx = rot_x * orbit_scale;
    var zy = rot_y * orbit_scale;
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var orbit_depth = 0.0;
    var z_angle = 0.0;
    var c_angle = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 4.0) {
            escaped = true;
            break;
        }

        let radius2 = max(mag2, 0.0001);
        let z_radius = sqrt(radius2);
        z_angle = atan2(zy, zx);
        let mode = hash01(f32(i) * 0.17 + base, f32(layer) * 0.11 + base, params.seed ^ (i + layer));
        let power = 2.0 + (phase * 0.6) + (mode * 0.7);
        let twist = 0.12 * sin(z_angle * 5.0 + phase * 6.28318530718 + f32(i) * 0.03);
        let scaled = pow(z_radius, power) * 0.55;
        c_angle = z_angle * power + twist;
        let polar_x = scaled * cos(c_angle);
        let polar_y = scaled * sin(c_angle);
        let folded_x = abs(polar_x) + 0.18 * (0.5 - mode);
        let folded_y = abs(polar_y) + 0.12 * (0.5 - phase);
        let p1 = folded_x - folded_y * 0.35;
        let p2 = folded_y + folded_x * 0.28;

        var x2 = p1 + cx;
        var y2 = 1.5 * p2 + cy + cos(twist * 2.0);
        if (mode >= 0.33 && mode < 0.66) {
            x2 = (p1 * p1 - p2 * p2) * 0.5 + cx;
            y2 = (2.0 * p1 * p2) + cy;
        } else if (mode >= 0.66) {
            x2 = polar_x + p2 * 0.24 + cx + sin(twist * 3.0);
            y2 = polar_y + cx * 0.18 + cos(polar_x * 1.4);
        }

        zx = x2 + (jitter - 0.5) * 0.09;
        zy = y2 + (1.0 - (f32(layer) + 1.0) * 0.06) * (mode - 0.5) * 0.22;
        mag2 = zx * zx + zy * zy;
        orbit_depth = orbit_depth + abs(z_radius - 1.0);
        i = i + 1u;
    }

    let iter_ratio = f32(i) / max(f32(params.iterations), 1.0);
    let escape_term = 1.0 - iter_ratio;
    let trap_term = 1.0 - clamp(orbit_depth / (f32(params.iterations) * 0.65 + 1.0), 0.0, 1.0);
    let angle_term = 0.5 + 0.5 * sin(z_angle + c_angle + base);
    let detail_term = hash01(zx, zy, params.seed + layer + i);
    let warp_term = 0.5 + 0.5 * sin(zx * 2.3 + zy * 1.9 + base);
    let value = (0.20 * angle_term) + (0.20 * trap_term) + (0.20 * escape_term) + (0.20 * detail_term) + (0.20 * warp_term);

    return select(0.0, clamp(value, 0.0, 1.0), escaped);
}

fn julia_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = (hash01(x * 1.7, y * 1.3, params.seed + layer + 11u) - 0.5) * 1.5;
    let cy = (hash01(x * 2.1, y * 1.9, params.seed + layer + 17u) - 0.5) * 1.5;
    let phase = hash01(x * 5.1, y * 4.7, params.seed + layer) * 6.28318530718;
    var zx = x * 0.95 + cos(phase) * 0.23;
    var zy = y * 0.95 + sin(phase) * 0.23;
    var i: u32 = 0u;
    var orbit = 0.0;
    var mag2 = 0.0;
    var escaped = false;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 16.0) {
            escaped = true;
            break;
        }

        let x2 = zx * zx - zy * zy + cx;
        let y2 = 2.0 * zx * zy + cy;
        zx = x2;
        zy = y2;
        mag2 = zx * zx + zy * zy;
        orbit = orbit + (1.0 - exp(-mag2 * 0.2));
        i = i + 1u;
    }

    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let orbit_term = clamp(orbit / max(f32(params.iterations), 1.0), 0.0, 1.0);
    let swirl = 0.5 + 0.5 * sin(orbit_term * 6.28318530718 + base);
    return select(0.0, clamp((0.60 * iter_term) + (0.30 * orbit_term) + (0.10 * swirl), 0.0, 1.0), escaped);
}

fn burning_ship_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = (hash01(x * 2.2, y * 1.4, params.seed + layer + 23u) - 0.5) * 1.35;
    let cy = (hash01(x * 1.6, y * 2.4, params.seed + layer + 29u) - 0.5) * 1.35;
    var zx = x + sin(base) * 0.2;
    var zy = y + cos(base) * 0.2;
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var distance = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 16.0) {
            escaped = true;
            break;
        }

        let zx_abs = abs(zx);
        let zy_abs = abs(zy);
        let x2 = zx_abs * zx_abs - zy_abs * zy_abs + cx;
        let y2 = 2.0 * zx_abs * zy_abs + cy;
        zx = x2;
        zy = y2;
        mag2 = zx * zx + zy * zy;
        distance = distance + (1.0 - exp(-mag2 * 0.18));
        i = i + 1u;
    }

    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let shape = clamp(distance / max(f32(params.iterations), 1.0), 0.0, 1.0);
    let warp = 0.5 + 0.5 * cos(iter_term * 4.0 + shape * 3.0);
    return select(0.0, clamp((0.65 * iter_term) + (0.25 * shape) + (0.10 * warp), 0.0, 1.0), escaped);
}

fn tricorn_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = (hash01(x * 1.9, y * 2.3, params.seed + layer + 31u) - 0.5) * 1.2;
    let cy = (hash01(x * 2.7, y * 1.8, params.seed + layer + 37u) - 0.5) * 1.2;
    var zx = x;
    var zy = y;
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var trail = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 12.0) {
            escaped = true;
            break;
        }

        let x2 = zx * zx - zy * zy + cx;
        let y2 = -2.0 * zx * zy + cy;
        zx = x2;
        zy = y2;
        mag2 = x2 * x2 + y2 * y2;
        trail = trail + (0.5 * hash01(x2, y2, params.seed + layer + i));
        i = i + 1u;
    }

    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let trail_term = clamp(trail / max(f32(params.iterations), 1.0), 0.0, 1.0);
    let pulse = 0.5 + 0.5 * sin(base * 7.0 + trail_term * 9.0);
    return select(0.0, clamp((0.66 * iter_term) + (0.24 * trail_term) + (0.10 * pulse), 0.0, 1.0), escaped);
}

fn phoenix_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = (hash01(x * 1.7, y * 2.1, params.seed + layer + 41u) - 0.5) * 1.1;
    let cy = (hash01(x * 2.9, y * 1.1, params.seed + layer + 47u) - 0.5) * 1.1;
    let p = (hash01(x * 0.8, y * 0.6, params.seed + layer + 53u) - 0.5) * 0.6;
    var zx = x;
    var zy = y;
    var px = 0.0;
    var py = 0.0;
    var i: u32 = 0u;
    var mag = 0.0;
    var escaped = false;
    var phase = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag > 10.0) {
            escaped = true;
            break;
        }

        let x2 = zx * zx - zy * zy + cx + p * px;
        let y2 = 2.0 * zx * zy + cy + p * py;
        px = zx;
        py = zy;
        zx = x2;
        zy = y2;
        mag = zx * zx + zy * zy;
        phase = phase + (mag * 0.05);
        i = i + 1u;
    }

    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let phase_term = fract(phase * 0.1);
    let wave = 0.5 + 0.5 * sin(phase_term * 12.0 + base);
    return select(0.0, clamp((0.70 * iter_term) + (0.30 * wave), 0.0, 1.0), escaped);
}

fn orbit_trap_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = (hash01(x * 3.3, y * 1.8, params.seed + layer + 59u) - 0.5) * 0.9;
    let cy = (hash01(x * 1.8, y * 3.1, params.seed + layer + 61u) - 0.5) * 0.9;
    var zx = x * 0.98;
    var zy = y * 0.98;
    let ring = 0.25 + 0.35 * (hash01(x, y, params.seed + layer) * 0.5);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var trap = 1e9;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 20.0) {
            escaped = true;
            break;
        }

        let n = zx * zx - zy * zy + cx;
        let m = 2.0 * zx * zy + cy;
        zx = n;
        zy = m;
        mag2 = zx * zx + zy * zy;
        let radius = sqrt(mag2);
        trap = min(trap, abs(radius - ring));
        i = i + 1u;
    }

    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let trap_term = 1.0 - clamp(trap * 1.2, 0.0, 1.0);
    let fold = 0.5 + 0.5 * cos(iter_term * 7.2 + base);
    return select(0.0, clamp((0.55 * iter_term) + (0.35 * trap_term) + (0.10 * fold), 0.0, 1.0), escaped);
}

fn field_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    var octave = 0u;
    var frequency = 1.5;
    var amplitude = 1.0;
    var total = 0.0;
    var denom = 0.0;
    let base = f32(params.seed) * 0.00000011928955;
    let drift = f32(layer) * 0.19 + base;
    let cx = hash01(x, y, params.seed + layer + 71u);
    let cy = hash01(y, x, params.seed + layer + 79u);

    loop {
        if (octave >= 5u) {
            break;
        }

        let sx = (x * frequency) + (cx * 3.0);
        let sy = (y * frequency) + (cy * 3.0);
        let wave = 0.5 + 0.5 * sin((sx + drift) * 1.5 + cos(sy * 1.3));
        let l1 = 0.5 + 0.5 * cos((sx * 1.9 - sy * 2.1) + drift);
        let ridge = hash01(sx, sy, params.seed + octave + layer) * 0.6;
        total = total + (wave * l1 * ridge) * amplitude;
        denom = denom + amplitude;
        frequency = frequency * 2.0;
        amplitude = amplitude * 0.58;
        octave = octave + 1u;
    }

    return clamp(total / max(denom, 1.0), 0.0, 1.0);
}

fn mandelbrot_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = x * 1.25 + (hash01(x * 1.1, y * 1.2, params.seed + layer + 101u) - 0.5) * 0.14;
    let cy = y * 1.25 + (hash01(x * 1.3, y * 0.9, params.seed + layer + 103u) - 0.5) * 0.14;
    var zx = 0.0;
    var zy = 0.0;
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var phase = 0.0;
    let jitter = hash01(x + base, y - base, params.seed + layer + 109u);

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 24.0) {
            escaped = true;
            break;
        }

        let x2 = zx * zx - zy * zy + cx + (jitter - 0.5) * 0.06;
        let y2 = 2.0 * zx * zy + cy + (jitter - 0.5) * 0.05;
        zx = x2;
        zy = y2;
        mag2 = x2 * x2 + y2 * y2;
        phase = phase + sin(mag2 + f32(i) * 0.03 + base);
        i = i + 1u;
    }

    escaped = escaped || (i >= params.iterations);
    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let orbit_term = (sin(phase + base + f32(layer) * 0.11) + 1.0) * 0.5;
    let radial_term = 0.5 + 0.5 * cos(sqrt(max(mag2, 0.0001)) * 0.85 + base);
    return select(
        0.0,
        clamp((0.5 * iter_term) + (0.30 * orbit_term) + (0.20 * radial_term), 0.0, 1.0),
        escaped,
    );
}

fn nova_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let rx = hash01(x * 1.4, y * 1.4, params.seed + layer + 131u) * 0.22;
    let ry = hash01(y * 1.8, x * 1.6, params.seed + layer + 137u) * 0.22;
    var zx = x * 0.96 + (rx - 0.11);
    var zy = y * 0.96 + (ry - 0.11);
    var angle = atan2(zy, zx);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var glow = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 80.0) {
            escaped = true;
            break;
        }

        let r = sqrt(max(mag2, 1e-6));
        let spin = sin(r * 2.7 + angle * 1.9 + base) * 0.55;
        let scale = 0.62 + 0.33 * sin((angle + base) * 2.4 + f32(i) * 0.03);
        let nx = (r * scale * cos(angle + spin * 0.24) * 1.1) + x * 0.22;
        let ny = (r * scale * sin(angle + spin * 0.24) * 1.1) + y * 0.22;
        let warp = 0.05 * (hash01(nx, ny, params.seed + layer + i) - 0.5);
        zx = nx + warp;
        zy = ny - warp;
        mag2 = zx * zx + zy * zy;
        angle = atan2(zy, zx);
        glow = glow + (0.5 + 0.5 * sin(nx * 1.6 + ny * 2.1 + f32(i) * 0.08));
        i = i + 1u;
    }

    escaped = escaped || (i >= params.iterations);
    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let iters = max(f32(params.iterations), 1.0);
    let glow_term = glow / iters;
    let edge_term = 1.0 - clamp(sqrt(max(mag2, 0.0001)) * 0.06, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.42 * iter_term) + (0.38 * glow_term) + (0.20 * edge_term), 0.0, 1.0),
        escaped,
    );
}

fn vortex_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let r_offset = hash01(x * 1.1, y * 0.9, params.seed + layer + 149u);
    let mut_x = hash01(x * 1.7, y * 0.7, params.seed + layer + 151u) - 0.5;
    let mut_y = hash01(y * 1.2, x * 1.4, params.seed + layer + 157u) - 0.5;
    var zx = x + (mut_x * 0.18);
    var zy = y + (mut_y * 0.18);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var orbit = 0.0;
    let twist = 0.15 + r_offset * 0.5;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 50.0) {
            escaped = true;
            break;
        }

        let r = sqrt(max(mag2, 1e-6));
        let theta = atan2(zy, zx) + twist + (1.0 / (r + 0.2)) * (0.35 + 0.15 * sin(f32(i) * 0.05 + base));
        let gain = (0.8 + 0.25 * sin(theta * 1.6 + base + f32(layer)));
        let nx = (gain * r + 0.03 * r * r) * cos(theta);
        let ny = (gain * r + 0.03 * r * r) * sin(theta);
        let spiral = 0.22 * sin((r + base) * 3.0 + f32(i) * 0.07);
        zx = nx + x * 0.26 + (spiral * (mut_x + mut_y * 0.5));
        zy = ny + y * 0.26 + (spiral * (mut_y - mut_x * 0.5));
        mag2 = zx * zx + zy * zy;
        orbit = orbit + (0.5 + 0.5 * cos(theta * 2.1 + f32(i) * 0.12));
        i = i + 1u;
    }

    escaped = escaped || (i >= params.iterations);
    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let orbit_term = 0.5 + (0.5 * sin(orbit * 0.02 + base + f32(layer) * 0.1));
    let edge_term = 1.0 - clamp(sqrt(max(mag2, 0.0001)) * 0.07, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.46 * iter_term) + (0.34 * orbit_term) + (0.20 * edge_term), 0.0, 1.0),
        escaped,
    );
}

fn dragon_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let jitter_x = hash01(x * 1.8, y * 1.3, params.seed + layer + 181u);
    let jitter_y = hash01(y * 1.4, x * 1.9, params.seed + layer + 191u);
    var zx = x * (1.15 + (jitter_x - 0.5) * 0.35);
    var zy = y * (1.15 + (jitter_y - 0.5) * 0.35);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var fold = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 30.0) {
            escaped = true;
            break;
        }

        let r = sqrt(max(mag2, 1e-6));
        let angle = atan2(zy, zx);
        let t = 0.5 + 0.5 * sin(base * 8.0 + f32(i) * 0.11 + f32(layer) * 0.09);
        let fold_term = 0.85 * t + 0.15 * t * sin(angle * 2.8);
        let abs_x = abs(zx);
        let abs_y = abs(zy);
        let nx = fold_term * (abs_x * abs_x - abs_y * abs_y)
            + 0.22 * r * cos(r * 1.1 + f32(layer) * 0.3);
        let ny = fold_term * (2.0 * abs_x * abs_y)
            + 0.22 * r * sin(r * 1.1 + f32(i) * 0.13);
        zx = nx + x * 0.26;
        zy = ny + y * 0.26;
        mag2 = zx * zx + zy * zy;
        fold = fold + (0.5 + 0.5 * sin(nx * 1.8 + ny * 2.6 + base));
        i = i + 1u;
    }

    escaped = escaped || (i >= params.iterations);
    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let fold_term = 0.5 + (0.5 * sin(fold * 0.03 + f32(layer) * 0.17));
    let edge_term = 1.0 - clamp(sqrt(max(mag2, 0.0001)) * 0.08, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.48 * iter_term) + (0.32 * fold_term) + (0.20 * edge_term), 0.0, 1.0),
        escaped,
    );
}

fn ifs_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let j1 = hash01(x, y, params.seed + layer + 211u) - 0.5;
    let j2 = hash01(y, x, params.seed + layer + 223u) - 0.5;
    var zx = x + (j1 * 0.2);
    var zy = y + (j2 * 0.2);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var attract = 0.0;
    var closest = 1e9;

    loop {
        if (i >= min(params.iterations, 200u)) {
            break;
        }
        if (mag2 > 60.0) {
            escaped = true;
            break;
        }

        let selector = hash01(zx + base, zy - base, params.seed + layer + i);
        if (selector < 0.33) {
            let nx = 0.86 * zx - 0.2 * zy + 0.02 + j1 * 0.1;
            let ny = 0.2 * zx + 0.86 * zy - 0.03 + j2 * 0.1;
            zx = nx;
            zy = ny;
        } else if (selector < 0.66) {
            let nx = -0.3 * zx + 0.9 * zy + 0.31;
            let ny = -0.9 * zx - 0.3 * zy - 0.09;
            zx = nx;
            zy = ny;
        } else {
            let nx = 0.4 * zx - 0.9 * zy + 0.24;
            let ny = 0.9 * zx + 0.4 * zy + 0.07;
            zx = nx;
            zy = ny;
        }

        let distance = abs(sqrt(max(mag2, 0.0001)) - (0.55 + 0.2 * j1));
        closest = min(closest, distance);
        attract = attract + (0.5 + 0.5 * cos(f32(i) * 0.16 + distance * 12.0 + base));
        mag2 = zx * zx + zy * zy;
        i = i + 1u;
    }

    escaped = escaped || (i >= min(params.iterations, 200u));
    let iters = max(f32(min(params.iterations, 200u)), 1.0);
    let iter_term = 1.0 - f32(i) / iters;
    let attract_term = 1.0 - (closest * 0.8);
    let orbit_term = 0.5 + 0.5 * sin(attract + base + f32(layer) * 0.2);
    return select(
        0.0,
        clamp((0.40 * iter_term) + (0.35 * attract_term) + (0.25 * orbit_term), 0.0, 1.0),
        escaped,
    );
}

fn moire_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let a_seed = hash01(x * 2.1, y * 1.6, params.seed + layer + 261u);
    let b_seed = hash01(y * 2.3, x * 1.8, params.seed + layer + 263u);
    var phase = (a_seed - 0.5) * 3.2;
    var px = x * (1.4 + a_seed * 0.25);
    var py = y * (1.35 + b_seed * 0.25);
    var mag2 = 0.0;
    var escaped = false;
    var energy = 0.0;
    var i: u32 = 0u;

    loop {
        if (i >= 12u) {
            break;
        }
        if (mag2 > 24.0) {
            escaped = true;
            break;
        }

        let freq = 2.4 + a_seed * 3.7 + f32(i) * 0.11;
        let pulse = 0.5 + 0.5 * sin((px * freq + phase) * 6.28318530718 + b_seed * 5.0);
        let wave = 0.5 + 0.5 * cos((py * (freq + 0.8) - phase) * 6.28318530718 - base);
        let stripe = 0.5 + 0.5 * sin((pulse + wave + sin((px + py + phase) * 2.2)) * 7.2);
        let jitter = (hash01(px, py, params.seed + layer + i + 271u) - 0.5) * 0.8;
        phase = phase + stripe + jitter;
        energy = energy + stripe;

        let c = cos(phase);
        let s = sin(phase);
        let nx = (px * c - py * s) + (pulse - 0.5) * 0.16;
        let ny = (px * s + py * c) + (wave - 0.5) * 0.16;
        px = nx * 0.86;
        py = ny * 0.86;
        mag2 = px * px + py * py;
        i = i + 1u;
    }

    let iter_term = energy / 12.0;
    let wave_term = 0.5 + 0.5 * sin(energy * 2.4 + phase + base);
    let radial_term = 1.0 - clamp(sqrt(max(mag2, 0.0001)) * 0.24, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.34 * iter_term) + (0.36 * wave_term) + (0.30 * radial_term), 0.0, 1.0),
        escaped,
    );
}

fn knot_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let knot_seed = hash01(x * 1.4, y * 1.9, params.seed + layer + 293u);
    let twist_seed = hash01(y * 1.1, x * 2.2, params.seed + layer + 307u);
    var px = x * (1.08 + knot_seed * 0.35);
    var py = y * (1.08 + twist_seed * 0.35);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var knot = 0.0;
    var ring = 0.0;
    let max_iter = min(params.iterations, 220u);

    loop {
        if (i >= max_iter) {
            break;
        }
        if (mag2 > 40.0) {
            escaped = true;
            break;
        }

        let radius = max(sqrt(max(mag2, 1e-6)), 0.08);
        let angle = atan2(py, px) + (base * 12.0);
        let spin = (1.0 / (radius + 0.2)) + (twist_seed * 0.45);
        let cx = cos(angle + spin + f32(i) * 0.06);
        let sx = sin(angle + spin + base);
        let fold = hash01(cx, sx, params.seed + layer + i + 311u);
        let rx = 0.9 * px + (fold - 0.5) * 0.24 + cos(radius * 3.4 + base + f32(i) * 0.13) * 0.04;
        let ry = 0.9 * py + (fold - 0.5) * 0.24 + sin(radius * 3.9 + f32(i) * 0.11 + base) * 0.04;
        let knot_line = abs(sin(radius * 2.2 + angle * 1.6 + fold * 2.4));
        knot = knot + knot_line;
        ring = ring + (1.0 - clamp(abs(radius - (0.28 + 0.15 * fold)), 0.0, 1.0));
        px = rx + cx * 0.18 * knot_line;
        py = ry + sx * 0.18 * knot_line;
        mag2 = rx * rx + ry * ry;
        i = i + 1u;
    }

    let i_term = f32(i) / max(f32(max_iter), 1.0);
    let knot_term = clamp(knot / max(f32(max_iter), 1.0), 0.0, 1.0);
    let ring_term = clamp(ring / max(f32(max_iter), 1.0), 0.0, 1.0);
    let escape_term = 1.0 - i_term;
    return select(
        0.0,
        clamp((0.36 * knot_term) + (0.30 * ring_term) + (0.34 * escape_term), 0.0, 1.0),
        escaped,
    );
}

fn radial_wave_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let ring_seed = hash01(x * 2.1, y * 1.7, params.seed + layer + 317u);
    let drift_seed = hash01(x * 1.4, y * 2.2, params.seed + layer + 331u);
    var px = x * (1.10 + (ring_seed * 0.25));
    var py = y * (1.10 + ((1.0 - ring_seed) * 0.25));
    let max_iter = min(params.iterations, 180u);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var envelope = 0.0;
    var phase = base * 2.5 + drift_seed * 2.0;
    var radius = max(length(vec2<f32>(px, py)), 0.0001);
    let base_angle = atan2(py, px);

    loop {
        if (i >= max_iter) {
            break;
        }
        if (mag2 > 40.0) {
            escaped = true;
            break;
        }

        let ring = (f32(i) * 0.12) + phase;
        let harmonic = (f32(((i + layer) % 4u) + 1u) * 0.75) + (drift_seed * 1.2);
        let pulse = 0.25 + 0.15 * sin(radius * harmonic + base_angle * 2.2 + ring);
        let twist = 0.15 + (0.18 * sin(base + ring * 0.4));
        let nr = radius + (0.22 * pulse) + (0.08 * cos(ring + phase));
        let local_angle = (base_angle * twist) + (ring * 0.24) + (pulse * 0.9);

        var wx = nr * cos(local_angle);
        var wy = nr * sin(local_angle);
        let wave = sin(wx * 3.1 + wy * 1.7 + phase);
        let blend = 0.55 + (0.45 * (wave * 0.5 + 0.5));
        wx = mix(wx, px, 0.38);
        wy = mix(wy, py, 0.38);

        px = (wx * 0.68) + (0.16 * (wave + 1.0) * cos(local_angle + base_angle));
        py = (wy * 0.68) + (0.16 * (wave + 1.0) * sin(local_angle - base_angle));
        px = px * (1.0 - (0.06 * blend));
        py = py * (1.0 + (0.06 * (1.0 - blend)));

        radius = max(length(vec2<f32>(px, py)), 0.0001);
        mag2 = radius * radius;
        envelope = envelope + (1.0 - clamp(abs(radius - (0.18 + pulse * 0.28)), 0.0, 1.0));
        phase = phase + (wave * 0.07) + (f32(i) * 0.002);
        i = i + 1u;
    }

    let iters = max(f32(max_iter), 1.0);
    let iter_term = 1.0 - (f32(i) / iters);
    let ring_term = clamp(envelope / iters, 0.0, 1.0);
    let orbit_term = 1.0 - clamp(radius * 0.4, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.38 * iter_term) + (0.38 * ring_term) + (0.24 * orbit_term), 0.0, 1.0),
        escaped,
    );
}

fn recursive_fold_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let k1 = hash01(x * 1.3, y * 1.8, params.seed + layer + 347u);
    let k2 = hash01(x * 1.9, y * 1.6, params.seed + layer + 359u);
    let k3 = hash01(x * 2.4, y * 1.1, params.seed + layer + 367u);
    var px = x * (1.0 + (k1 - 0.5) * 0.45);
    var py = y * (1.0 + (k2 - 0.5) * 0.45);
    let max_iter = min(params.iterations, 240u);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var fold_score = 0.0;
    let seed_angle = base * 4.0;

    loop {
        if (i >= max_iter) {
            break;
        }
        if (mag2 > 70.0) {
            escaped = true;
            break;
        }

        let a = sin(seed_angle + f32(i) * 0.13);
        let b = cos(seed_angle * 1.2 + f32(i) * 0.17);
        let fold_angle = atan2(py, px) + a * 1.6;
        let c = cos(fold_angle);
        let s = sin(fold_angle);
        let rx = px * c - py * s;
        let ry = px * s + py * c;
        let fold_x = abs(rx) - (0.26 + k1 * 0.24);
        let fold_y = abs(ry * 0.95) - (0.18 + k2 * 0.28);
        let twist = 0.34 + (0.26 * k3);
        let nx = (fold_x * (1.0 + a * 0.35)) * twist;
        let ny = (fold_y * (1.0 - b * 0.35)) * twist;
        px = nx + b * 0.09;
        py = ny - a * 0.09;
        px = px + (0.15 * sin(f32(i) * 0.21 + base + k1));
        py = py + (0.15 * cos(f32(i) * 0.23 + base + k2));
        mag2 = px * px + py * py;
        let fold_density = 1.0 - clamp(abs(fold_x * fold_y + 0.1), 0.0, 1.0);
        fold_score = fold_score + fold_density * (1.0 - clamp(mag2 * 0.02, 0.0, 1.0));
        i = i + 1u;
    }

    let iters = max(f32(max_iter), 1.0);
    let iter_term = 1.0 - f32(i) / iters;
    let fold_term = clamp(fold_score / iters, 0.0, 1.0);
    let orbit_term = 1.0 - clamp((sqrt(max(mag2, 0.0001)) * 0.28), 0.0, 1.0);
    return select(
        0.0,
        clamp((0.34 * iter_term) + (0.44 * fold_term) + (0.22 * orbit_term), 0.0, 1.0),
        escaped,
    );
}

fn attractor_hybrid_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let k1 = hash01(x * 1.9, y * 1.1, params.seed + layer + 379u);
    let k2 = hash01(x * 2.5, y * 2.2, params.seed + layer + 389u);
    let k3 = hash01(x * 1.4, y * 2.7, params.seed + layer + 397u);
    let a = 1.2 + k1 * 1.1;
    let b = -0.4 + k2 * 1.6;
    let c = -1.6 + k3 * 1.2;
    let d = 0.7 + hash01(x * 3.1, y * 1.9, params.seed + layer + 401u) * 0.9;
    var zx = x * (1.3 + (k1 - 0.5) * 0.2);
    var zy = y * (1.3 + (k2 - 0.5) * 0.2);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var attract_term = 0.0;
    var orbit_term = 0.0;
    let max_iter = min(params.iterations, 240u);

    loop {
        if (i >= max_iter) {
            break;
        }
        if (mag2 > 120.0) {
            escaped = true;
            break;
        }

        let attract_mix = fract(hash01(zx + base, zy - base, params.seed + layer + i) + (0.15 * k3));
        let attractor_x = sin(a * zy) + c * cos(b * zx);
        let attractor_y = sin(d * zx) + b * cos(c * zy);
        let fallback_x = 1.4 - (1.2 * zx * zx) + (0.3 * zy);
        let fallback_y = -0.6 * zy + 0.8 * sin(zx + base);
        let nx = mix(attractor_x, fallback_x, attract_mix);
        let ny = mix(attractor_y, fallback_y, attract_mix);
        zx = nx * 0.48 + (0.52 * zx);
        zy = ny * 0.48 + (0.52 * zy);
        mag2 = zx * zx + zy * zy;
        let trap = 1.0 - clamp(abs(sqrt(max(mag2, 0.000001)) - (0.30 + 0.20 * k2)), 0.0, 1.0);
        attract_term = attract_term + (0.5 + 0.5 * sin(trap * 9.0 + base));
        orbit_term = orbit_term + (1.0 - clamp(mag2 * 0.02, 0.0, 1.0));
        i = i + 1u;
    }

    let iters = max(f32(max_iter), 1.0);
    let iter_term = 1.0 - f32(i) / iters;
    let attract_norm = clamp(attract_term / iters, 0.0, 1.0);
    let orbit_norm = clamp(orbit_term / iters, 0.0, 1.0);
    let radius_term = 1.0 - clamp(sqrt(max(mag2, 0.0001)) * 0.09, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.30 * iter_term) + (0.40 * attract_norm) + (0.20 * orbit_norm) + (0.10 * radius_term), 0.0, 1.0),
        escaped,
    );
}

fn style_value(x: f32, y: f32, params: Params, layer: u32, style_selector: u32) -> f32 {
    let style = style_selector % 17u;
    switch (style) {
        case 0u: {
            return hybrid_style_value(x, y, params, layer);
        }
        case 1u: {
            return julia_style_value(x, y, params, layer);
        }
        case 2u: {
            return burning_ship_style_value(x, y, params, layer);
        }
        case 3u: {
            return tricorn_style_value(x, y, params, layer);
        }
        case 4u: {
            return phoenix_style_value(x, y, params, layer);
        }
        case 5u: {
            return orbit_trap_style_value(x, y, params, layer);
        }
        case 6u: {
            return field_style_value(x, y, params, layer);
        }
        case 7u: {
            return mandelbrot_style_value(x, y, params, layer);
        }
        case 8u: {
            return nova_style_value(x, y, params, layer);
        }
        case 9u: {
            return vortex_style_value(x, y, params, layer);
        }
        case 10u: {
            return dragon_style_value(x, y, params, layer);
        }
        case 11u: {
            return ifs_style_value(x, y, params, layer);
        }
        case 12u: {
            return moire_style_value(x, y, params, layer);
        }
        case 13u: {
            return knot_style_value(x, y, params, layer);
        }
        case 14u: {
            return radial_wave_style_value(x, y, params, layer);
        }
        case 15u: {
            return recursive_fold_style_value(x, y, params, layer);
        }
        case 16u: {
            return attractor_hybrid_style_value(x, y, params, layer);
        }
        default: {
            return field_style_value(x, y, params, layer);
        }
    }
}

fn layer_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let primary_style = style_value(x, y, params, layer, params.art_style);
    let secondary_style = style_value(x, y, params, layer, params.art_style_secondary);
    let blend = clamp(params.art_style_mix, 0.0, 1.0);
    return (1.0 - blend) * primary_style + blend * secondary_style;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }

    var px = ((f32(id.x) + 0.5) / f32(params.width)) * 2.0 - 1.0;
    var py = ((f32(id.y) + 0.5) / f32(params.height)) * 2.0 - 1.0;
    let zoom = max(0.2, params.fractal_zoom);
    px = px * params.fill_scale * zoom;
    py = py * params.fill_scale * zoom;
    let warped = apply_domain_warp(px, py);
    px = warped.x;
    py = warped.y;
    px = px + params.center_x;
    py = py + params.center_y;
    var value = 0.0;
    let layer_count = max(1u, min(14u, params.layer_count));

    if (params.symmetry > 1u) {
        let folded = fold_for_symmetry(px, py, params.symmetry, params.symmetry_style);
        px = folded.x;
        py = folded.y;
    }

    let swirl = hash01(px * 11.0, py * 13.0, params.seed);
    let sx = px + (swirl - 0.5) * 0.08;
    let sy = py + (swirl - 0.5) * 0.08;

    var layer: u32 = 0u;
    loop {
        if (layer >= layer_count) {
            break;
        }
        let layer_brightness = layer_value(sx, sy, params, layer);
        let layer_factor = f32(layer) / f32(layer_count);
        let weight = pow(1.0 - layer_factor, 1.2) * 1.15;
        value = value + (layer_brightness * layer_brightness * weight);
        layer = layer + 1u;
    }

    out_pixels[id.x + id.y * params.width] = pack_gray(value);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    width: u32,
    height: u32,
    symmetry: u32,
    symmetry_style: u32,
    iterations: u32,
    seed: u32,
    fill_scale: f32,
    fractal_zoom: f32,
    art_style: u32,
    art_style_secondary: u32,
    art_style_mix: f32,
    bend_strength: f32,
    warp_strength: f32,
    warp_frequency: f32,
    tile_scale: f32,
    tile_phase: f32,
    center_x: f32,
    center_y: f32,
    layer_count: u32,
}

#[derive(Clone, Copy)]
enum FilterMode {
    Motion,
    Gaussian,
    Median,
    Bilateral,
}

#[derive(Clone, Copy)]
enum ArtStyle {
    Hybrid,
    Julia,
    BurningShip,
    Tricorn,
    Phoenix,
    OrbitTrap,
    Field,
    Mandelbrot,
    Nova,
    Vortex,
    Dragon,
    Ifs,
    Moire,
    Knot,
    RadialWave,
    RecursiveFold,
    AttractorHybrid,
}

impl ArtStyle {
    fn from_u32(value: u32) -> Self {
        match value % 17 {
            0 => Self::Hybrid,
            1 => Self::Julia,
            2 => Self::BurningShip,
            3 => Self::Tricorn,
            4 => Self::Phoenix,
            5 => Self::OrbitTrap,
            6 => Self::Field,
            7 => Self::Mandelbrot,
            8 => Self::Nova,
            9 => Self::Vortex,
            10 => Self::Dragon,
            11 => Self::Ifs,
            12 => Self::Moire,
            13 => Self::Knot,
            14 => Self::RadialWave,
            15 => Self::RecursiveFold,
            _ => Self::AttractorHybrid,
        }
    }

    fn as_u32(self) -> u32 {
        match self {
            Self::Hybrid => 0,
            Self::Julia => 1,
            Self::BurningShip => 2,
            Self::Tricorn => 3,
            Self::Phoenix => 4,
            Self::OrbitTrap => 5,
            Self::Field => 6,
            Self::Mandelbrot => 7,
            Self::Nova => 8,
            Self::Vortex => 9,
            Self::Dragon => 10,
            Self::Ifs => 11,
            Self::Moire => 12,
            Self::Knot => 13,
            Self::RadialWave => 14,
            Self::RecursiveFold => 15,
            Self::AttractorHybrid => 16,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Hybrid => "hybrid",
            Self::Julia => "julia",
            Self::BurningShip => "burning",
            Self::Tricorn => "tricorn",
            Self::Phoenix => "phoenix",
            Self::OrbitTrap => "orbit",
            Self::Field => "field",
            Self::Mandelbrot => "mandelbrot",
            Self::Nova => "nova",
            Self::Vortex => "vortex",
            Self::Dragon => "dragon",
            Self::Ifs => "ifs",
            Self::Moire => "moire",
            Self::Knot => "knot",
            Self::RadialWave => "radial-wave",
            Self::RecursiveFold => "recursive-fold",
            Self::AttractorHybrid => "attractor-hybrid",
        }
    }

    fn total() -> u32 {
        17
    }

    fn is_tiling_like(self) -> bool {
        matches!(
            self,
            Self::Field | Self::RadialWave | Self::Knot | Self::RecursiveFold | Self::Moire
        )
    }

    fn next_non_tiling_from(rng: &mut XorShift32) -> Self {
        let mut candidate = Self::from_u32(rng.next_u32());
        if !candidate.is_tiling_like() {
            return candidate;
        }

        // Keep trying until we hit a non-tiling style.
        for _ in 0..Self::total() {
            candidate = Self::from_u32(candidate.as_u32() + 1);
            if !candidate.is_tiling_like() {
                break;
            }
        }

        candidate
    }
}

impl FilterMode {
    fn from_u32(value: u32) -> Self {
        match value % 4 {
            0 => Self::Motion,
            1 => Self::Gaussian,
            2 => Self::Median,
            _ => Self::Bilateral,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Motion => "motion",
            Self::Gaussian => "gaussian",
            Self::Median => "median",
            Self::Bilateral => "bilateral",
        }
    }
}

#[derive(Clone, Copy)]
enum GradientMode {
    Linear,
    Contrast,
    Gamma,
    Sine,
    Sigmoid,
    Posterize,
}

impl GradientMode {
    fn from_u32(value: u32) -> Self {
        match value % 6 {
            0 => Self::Linear,
            1 => Self::Contrast,
            2 => Self::Gamma,
            3 => Self::Sine,
            4 => Self::Sigmoid,
            _ => Self::Posterize,
        }
    }
}

#[derive(Clone, Copy)]
struct BlurConfig {
    mode: FilterMode,
    max_radius: u32,
    axis_x: i32,
    axis_y: i32,
    softness: u32,
}

#[derive(Clone, Copy)]
struct GradientConfig {
    mode: GradientMode,
    gamma: f32,
    contrast: f32,
    pivot: f32,
    invert: bool,
    frequency: f32,
    phase: f32,
    bands: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum SymmetryStyle {
    None,
    Radial,
    Mirror,
    MirrorX,
    MirrorY,
    MirrorDiagonal,
    MirrorCross,
    Grid,
}

#[derive(Default)]
struct SpinnerState {
    total_images: usize,
    current_image: AtomicUsize,
    current_layer: AtomicUsize,
    total_layers: AtomicUsize,
}

impl SpinnerState {
    fn new(total_images: usize) -> Self {
        Self {
            total_images,
            ..Self::default()
        }
    }

    fn set_image(&self, image_index: usize, layer_total: usize) {
        self.current_image.store(image_index, Ordering::Relaxed);
        self.total_layers.store(layer_total, Ordering::Relaxed);
        self.current_layer.store(0, Ordering::Relaxed);
    }

    fn set_layer(&self, layer_index: usize) {
        self.current_layer.store(layer_index, Ordering::Relaxed);
    }
}

fn start_spinner(state: Arc<SpinnerState>) -> (Arc<AtomicBool>, thread::JoinHandle<()>) {
    let running = Arc::new(AtomicBool::new(true));
    let thread_state = state.clone();
    let running_thread = running.clone();
    let frames = ["|", "/", "-", "\\"];

    let handle = thread::spawn(move || {
        let mut tick = 0usize;
        while running_thread.load(Ordering::Acquire) {
            let image = thread_state.current_image.load(Ordering::Relaxed);
            let layer = thread_state.current_layer.load(Ordering::Relaxed);
            let total_layers = thread_state.total_layers.load(Ordering::Relaxed);
            let layer_text = if total_layers == 0 {
                "starting".to_string()
            } else {
                format!("layer {}/{}", layer, total_layers)
            };
            let _ = write!(
                io::stderr(),
                "\r{} image {}/{} {}",
                frames[tick % frames.len()],
                image,
                thread_state.total_images,
                layer_text,
            );
            let _ = io::stdout().flush();
            tick = tick.wrapping_add(1);
            thread::sleep(Duration::from_millis(90));
        }
    });

    (running, handle)
}

impl SymmetryStyle {
    fn from_u32(value: u32) -> Self {
        match value % 8 {
            0 => Self::None,
            1 => Self::Radial,
            2 => Self::Mirror,
            3 => Self::MirrorX,
            4 => Self::MirrorY,
            5 => Self::MirrorDiagonal,
            6 => Self::MirrorCross,
            _ => Self::Grid,
        }
    }

    fn as_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Radial => 1,
            Self::Mirror => 2,
            Self::MirrorX => 3,
            Self::MirrorY => 4,
            Self::MirrorDiagonal => 5,
            Self::MirrorCross => 6,
            Self::Grid => 7,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Radial => "radial",
            Self::Mirror => "mirror",
            Self::MirrorX => "mirror-x",
            Self::MirrorY => "mirror-y",
            Self::MirrorDiagonal => "mirror-diagonal",
            Self::MirrorCross => "mirror-cross",
            Self::Grid => "grid",
        }
    }
}

#[derive(Clone, Copy)]
enum LayerBlendMode {
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Difference,
    Lighten,
    Darken,
    Glow,
    Shadow,
}

impl LayerBlendMode {
    fn from_u32(value: u32) -> Self {
        match value % 10 {
            0 => Self::Normal,
            1 => Self::Add,
            2 => Self::Multiply,
            3 => Self::Screen,
            4 => Self::Overlay,
            5 => Self::Difference,
            6 => Self::Lighten,
            7 => Self::Darken,
            8 => Self::Glow,
            _ => Self::Shadow,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Add => "add",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Difference => "difference",
            Self::Lighten => "lighten",
            Self::Darken => "darken",
            Self::Glow => "glow",
            Self::Shadow => "shadow",
        }
    }
}

struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    fn new(seed: u32) -> Self {
        let state = if seed == 0 { 0x9e3779b9 } else { seed };
        Self { state }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }
}

fn randomize_symmetry(base: u32, rng: &mut XorShift32) -> u32 {
    if rng.next_f32() < 0.34 {
        return 1;
    }

    if rng.next_f32() < 0.56 {
        return 2 + (rng.next_u32() % 8);
    }

    if base <= 1 {
        return 2 + (rng.next_u32() % 5);
    }

    let full_range: u32 = 12;
    if rng.next_f32() < 0.45 {
        return 2 + (rng.next_u32() % 7);
    }

    let spread = (base as f32 * 0.65).round() as u32;
    let low = base.saturating_sub(spread).max(1);
    let high = (base.saturating_add(spread)).max(low + 1).min(full_range);
    low + (rng.next_u32() % (high - low + 1))
}

fn randomize_iterations(base: u32, rng: &mut XorShift32) -> u32 {
    let low = (base as f32 * 0.28).floor().max(96.0) as u32;
    let high = (base as f32 * 3.2).ceil().max(300.0) as u32;
    low + (rng.next_u32() % (high - low + 1))
}

fn randomize_fill_scale(base: f32, rng: &mut XorShift32) -> f32 {
    let jitter = 0.65 + (rng.next_f32() * 1.2);
    (base * jitter).clamp(0.6, 2.4)
}

fn randomize_zoom(base: f32, rng: &mut XorShift32) -> f32 {
    let jitter = 0.42 + (rng.next_f32() * 1.18);
    (base * jitter).clamp(0.35, 1.6)
}

fn randomize_center_offset(rng: &mut XorShift32, fast: bool) -> (f32, f32) {
    let center_lock = if fast { 0.12 } else { 0.18 };
    if rng.next_f32() < center_lock {
        return (0.0, 0.0);
    }

    let max_shift = if fast { 0.24 } else { 0.44 };
    let radius = max_shift * rng.next_f32().sqrt();
    let angle = rng.next_f32() * std::f32::consts::TAU;
    (radius * angle.cos(), radius * angle.sin())
}

fn modulate_center_offset(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let jitter = (rng.next_f32() * 2.0 - 1.0) * if fast { 0.12 } else { 0.20 };
    (base + jitter).clamp(-0.5, 0.5)
}

fn pick_bend_strength(rng: &mut XorShift32) -> f32 {
    1.5 * rng.next_f32()
}

fn pick_warp_strength(rng: &mut XorShift32) -> f32 {
    1.5 * rng.next_f32()
}

fn pick_warp_frequency(rng: &mut XorShift32) -> f32 {
    0.6 + (rng.next_f32() * 5.2)
}

fn pick_tile_scale(rng: &mut XorShift32) -> f32 {
    0.45 + (rng.next_f32() * 1.0)
}

fn pick_tile_phase(rng: &mut XorShift32) -> f32 {
    rng.next_f32()
}

fn pick_art_style(rng: &mut XorShift32) -> ArtStyle {
    ArtStyle::from_u32(rng.next_u32())
}

fn modulate_art_style(base: ArtStyle, rng: &mut XorShift32, fast: bool) -> ArtStyle {
    let roll = rng.next_f32();
    let stride = 1 + (rng.next_u32() % (ArtStyle::total() - 1));
    if fast && base.is_tiling_like() && rng.next_f32() < 0.80 {
        return ArtStyle::next_non_tiling_from(rng);
    }

    if fast {
        if roll < 0.22 {
            return base;
        }
        if roll < 0.44 {
            return ArtStyle::from_u32(base.as_u32() + stride);
        }
        if roll < 0.58 {
            return ArtStyle::from_u32(base.as_u32() + ArtStyle::total() - 1);
        }
        return pick_art_style(rng);
    }

    if roll < 0.20 {
        return base;
    }
    if roll < 0.50 {
        return ArtStyle::from_u32(base.as_u32() + stride);
    }
    if roll < 0.74 {
        return ArtStyle::from_u32(base.as_u32() + ArtStyle::total() - 1);
    }
    pick_art_style(rng)
}

fn pick_art_style_secondary(base: ArtStyle, rng: &mut XorShift32) -> ArtStyle {
    let secondary = pick_art_style(rng);
    if secondary.as_u32() == base.as_u32() {
        ArtStyle::from_u32(base.as_u32() + 1)
    } else {
        secondary
    }
}

fn modulate_style_mix(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.15 } else { 0.28 };
    let jitter = (rng.next_f32() * 2.0 - 1.0) * spread;
    (base + jitter).clamp(0.0, 1.0)
}

fn pick_layer_count(rng: &mut XorShift32, user_count: Option<u32>, fast: bool) -> u32 {
    if let Some(fixed) = user_count {
        return fixed;
    }

    if fast {
        2 + (rng.next_u32() % 5)
    } else {
        2 + (rng.next_u32() % 7)
    }
}

fn pick_shader_layer_count(base_layer_count: u32, rng: &mut XorShift32, fast: bool) -> u32 {
    let base = (base_layer_count.clamp(1, 14)) as f32;
    let spread = if fast { 0.25 } else { 0.45 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    (base * factor).clamp(1.0, 14.0).round() as u32
}

fn modulate_shader_layer_count(
    base_layer_count: u32,
    rng: &mut XorShift32,
    fast: bool,
    structural: bool,
) -> u32 {
    let mut base = base_layer_count.clamp(1, 14) as f32;
    let spread = if structural {
        0.12
    } else if fast {
        0.20
    } else {
        0.34
    };
    let drift = (rng.next_f32() * 2.0 - 1.0) * spread;
    base *= 1.0 + drift;
    if structural {
        base = base.max(2.0);
    }
    base.clamp(1.0, 14.0).round() as u32
}

fn modulate_symmetry(base: u32, rng: &mut XorShift32, fast: bool) -> u32 {
    if rng.next_f32() < if fast { 0.18 } else { 0.10 } {
        return 1;
    }

    if base <= 1 {
        return 2;
    }

    let jitter = if fast { 4 } else { 6 };
    if rng.next_f32() < 0.30 {
        return 2 + (rng.next_u32() % 15);
    }

    let jitter_range = jitter.min(base - 1);
    let shift = (rng.next_u32() % (jitter_range * 2 + 1)) as i32 - jitter_range as i32;
    ((base as i32 + shift).clamp(1, 16)) as u32
}

fn modulate_symmetry_style(base: u32, rng: &mut XorShift32, fast: bool, allow_grid: bool) -> u32 {
    let keep_base = if fast { 0.24 } else { 0.30 };
    let roll = rng.next_f32();
    let mut style = if roll < keep_base {
        SymmetryStyle::from_u32(base)
    } else {
        let sampled = SymmetryStyle::from_u32(base + rng.next_u32());
        if sampled.as_u32() == base {
            SymmetryStyle::from_u32(pick_symmetry_style(rng))
        } else {
            sampled
        }
    };

    if allow_grid {
        if style == SymmetryStyle::Grid && rng.next_f32() > 0.01 {
            return pick_non_grid_symmetry_style(rng).as_u32();
        }
    } else if style == SymmetryStyle::Grid {
        style = pick_non_grid_symmetry_style(rng);
    }

    style.as_u32()
}

fn should_apply_grid_across_layers(
    base_style: SymmetryStyle,
    layer_count: u32,
    rng: &mut XorShift32,
    fast: bool,
) -> bool {
    if base_style != SymmetryStyle::Grid || layer_count <= 1 {
        return false;
    }

    let base_chance = if fast { 0.001 } else { 0.004 };
    let layer_scale = ((layer_count as f32) / 8.0).clamp(0.45, 1.0);
    rng.next_f32() < (base_chance * layer_scale)
}

fn resolve_symmetry_style(
    base_style: SymmetryStyle,
    apply_to_all_layers: bool,
    rng: &mut XorShift32,
) -> SymmetryStyle {
    if base_style == SymmetryStyle::Grid && !apply_to_all_layers {
        return pick_non_grid_symmetry_style(rng);
    }

    base_style
}

fn modulate_iterations(base: u32, rng: &mut XorShift32, fast: bool) -> u32 {
    let spread = if fast { 0.18 } else { 0.42 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    let value = (base as f32 * factor).max(64.0).round() as u32;
    value.max(1)
}

fn modulate_fill_scale(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.12 } else { 0.24 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    (base * factor).clamp(0.80, 2.4)
}

fn modulate_bend_strength(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.30 } else { 0.65 };
    (base + ((rng.next_f32() * 2.0 - 1.0) * spread)).clamp(0.0, 1.9)
}

fn modulate_warp_strength(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.30 } else { 0.65 };
    (base + ((rng.next_f32() * 2.0 - 1.0) * spread)).clamp(0.0, 1.9)
}

fn modulate_warp_frequency(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.15 } else { 0.35 };
    (base + ((rng.next_f32() * 2.0 - 1.0) * spread)).clamp(0.2, 6.2)
}

fn modulate_tile_scale(base: f32, for_grid: bool, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.18 } else { 0.33 };
    let clamp_max = if for_grid { 1.2 } else { 1.7 };
    (base + ((rng.next_f32() * 2.0 - 1.0) * spread)).clamp(0.22, clamp_max)
}

fn modulate_tile_phase(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.30 } else { 0.62 };
    (base + ((rng.next_f32() * 2.0 - 1.0) * spread)).rem_euclid(1.0)
}

fn modulate_zoom(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.18 } else { 0.30 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    (base * factor).clamp(0.35, 1.65)
}

fn bias_layer_strategy(
    current: RenderStrategy,
    rng: &mut XorShift32,
    fast: bool,
    prefer_gpu: bool,
) -> RenderStrategy {
    let switch_prob = if fast { 0.06 } else { 0.04 };
    if rng.next_f32() < switch_prob {
        let family_bias = if fast { 0.9 } else { 0.86 };
        pick_render_strategy_near_family_with_preferences(
            rng,
            fast,
            current,
            family_bias,
            prefer_gpu,
        )
    } else {
        current
    }
}

fn pick_layer_blend(rng: &mut XorShift32) -> LayerBlendMode {
    LayerBlendMode::from_u32(rng.next_u32())
}

fn pick_layer_contrast(rng: &mut XorShift32, fast: bool) -> f32 {
    let low = if fast { 1.18 } else { 1.35 };
    let high = if fast { 1.58 } else { 1.95 };
    low + (rng.next_f32() * (high - low))
}

fn layer_opacity(rng: &mut XorShift32) -> f32 {
    0.30 + (rng.next_f32() * 0.55)
}

fn pick_symmetry_style(rng: &mut XorShift32) -> u32 {
    let roll = rng.next_f32();
    if roll < 0.02 {
        SymmetryStyle::Grid.as_u32()
    } else if roll < 0.52 {
        SymmetryStyle::Radial.as_u32()
    } else if roll < 0.80 {
        SymmetryStyle::None.as_u32()
    } else if roll < 0.88 {
        SymmetryStyle::Mirror.as_u32()
    } else if roll < 0.93 {
        SymmetryStyle::MirrorX.as_u32()
    } else if roll < 0.97 {
        SymmetryStyle::MirrorY.as_u32()
    } else if roll < 0.99 {
        SymmetryStyle::MirrorDiagonal.as_u32()
    } else {
        SymmetryStyle::MirrorCross.as_u32()
    }
}

fn pick_non_grid_symmetry_style(rng: &mut XorShift32) -> SymmetryStyle {
    let roll = rng.next_f32();
    if roll < 0.20 {
        SymmetryStyle::None
    } else if roll < 0.52 {
        SymmetryStyle::Radial
    } else if roll < 0.72 {
        SymmetryStyle::Mirror
    } else if roll < 0.82 {
        SymmetryStyle::MirrorX
    } else if roll < 0.92 {
        SymmetryStyle::MirrorY
    } else if roll < 0.96 {
        SymmetryStyle::MirrorDiagonal
    } else {
        SymmetryStyle::MirrorCross
    }
}

fn pick_filter_from_rng(rng: &mut XorShift32) -> BlurConfig {
    let mode = FilterMode::from_u32(rng.next_u32());
    let mut axis_x = (rng.next_u32() % 5) as i32 - 2;
    let mut axis_y = (rng.next_u32() % 5) as i32 - 2;
    if axis_x == 0 && axis_y == 0 {
        axis_x = 1;
        axis_y = 0;
    }
    BlurConfig {
        mode,
        max_radius: 2 + (rng.next_u32() % 8),
        axis_x,
        axis_y,
        softness: 1 + (rng.next_u32() % 4),
    }
}

fn should_apply_dynamic_filter(
    layer_index: u32,
    rng: &mut XorShift32,
    fast: bool,
    structural_profile: bool,
    strategy_bias: f32,
) -> bool {
    let base: f32 = if fast { 0.26 } else { 0.20 };
    let layer_bias: f32 = if layer_index == 0 { -0.20 } else { 0.12 };
    let strategy_bias = strategy_bias.clamp(0.0, 1.5);
    let threshold = ((base + layer_bias) * strategy_bias).clamp(0.02, 0.95);
    let adjusted = if structural_profile {
        threshold * 0.35
    } else {
        threshold
    };
    rng.next_f32() < adjusted
}

fn should_apply_gradient_map(
    layer_index: u32,
    rng: &mut XorShift32,
    fast: bool,
    structural_profile: bool,
    strategy_bias: f32,
) -> bool {
    let base: f32 = if fast { 0.34 } else { 0.26 };
    let layer_bias: f32 = if layer_index == 0 { -0.25 } else { 0.00 };
    let strategy_bias = strategy_bias.clamp(0.0, 1.5);
    let threshold = ((base + layer_bias) * strategy_bias).clamp(0.05, 0.95);
    let adjusted = if structural_profile {
        threshold * 0.33
    } else {
        threshold
    };
    rng.next_f32() < adjusted
}

fn should_use_structural_profile(fast: bool, rng: &mut XorShift32) -> bool {
    let threshold = if fast { 0.55 } else { 0.38 };
    rng.next_f32() < threshold
}

fn tune_filter_for_speed(cfg: BlurConfig, fast: bool) -> BlurConfig {
    if !fast {
        return cfg;
    }

    BlurConfig {
        mode: cfg.mode,
        max_radius: (cfg.max_radius / 2).clamp(1, 4),
        axis_x: cfg.axis_x.signum(),
        axis_y: cfg.axis_y.signum(),
        softness: cfg.softness.min(2),
    }
}

fn pick_gradient_from_rng(rng: &mut XorShift32) -> GradientConfig {
    let mode = GradientMode::from_u32(rng.next_u32());
    let gamma = 0.45 + (rng.next_u32() % 160) as f32 * 0.01;
    let contrast = 0.6 + (rng.next_u32() % 240) as f32 * 0.01;
    let pivot = 0.25 + (rng.next_u32() % 70) as f32 * 0.01;
    let invert = rng.next_u32().is_multiple_of(2);
    let frequency = 0.5 + (rng.next_u32() % 250) as f32 * 0.02;
    let phase = (rng.next_u32() % 360) as f32 * 0.0174533;
    let bands = (rng.next_u32() % 6) + 1;

    GradientConfig {
        mode,
        gamma,
        contrast,
        pivot,
        invert,
        frequency,
        phase,
        bands,
    }
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn apply_posterize(mut value: f32, bands: u32) -> f32 {
    if bands <= 1 {
        return clamp01(value);
    }
    let levels = bands as f32;
    value = clamp01(value) * levels;
    (value.floor() / levels).min(1.0)
}

fn apply_posterize_buffer(src: &mut [f32], bands: u32) {
    for value in src.iter_mut() {
        *value = apply_posterize(*value, bands);
    }
}

fn apply_gradient_map(src: &mut [f32], cfg: GradientConfig) {
    for value in src {
        let mut mapped = clamp01(*value);
        match cfg.mode {
            GradientMode::Linear => {}
            GradientMode::Contrast => {
                mapped = mapped.powf(1.0 + cfg.contrast * 0.05) * cfg.pivot;
                mapped = mapped.clamp(0.0, 1.0);
            }
            GradientMode::Gamma => {
                mapped = mapped.powf(cfg.gamma);
            }
            GradientMode::Sine => {
                mapped = 0.5
                    + (0.5
                        * (cfg.frequency * mapped * std::f32::consts::PI * 2.0 + cfg.phase).sin());
            }
            GradientMode::Sigmoid => {
                let x = cfg.contrast * 0.1 * (mapped - cfg.pivot);
                mapped = 1.0 / (1.0 + (-x).exp());
            }
            GradientMode::Posterize => {}
        }

        if cfg.invert {
            mapped = 1.0 - mapped;
        }

        mapped = (mapped * cfg.contrast.recip()).clamp(0.0, 1.0);
        mapped = apply_posterize(mapped, cfg.bands);
        *value = clamp01(mapped);
    }
}

fn pixel_index(x: i32, y: i32, width: i32) -> usize {
    (y * width + x) as usize
}

fn sample_luma(src: &[f32], width: i32, height: i32, x: i32, y: i32) -> f32 {
    let clamped_x = x.clamp(0, width - 1);
    let clamped_y = y.clamp(0, height - 1);
    let idx = pixel_index(clamped_x, clamped_y, width);
    src[idx]
}

fn decode_luma(raw: &[u8], out: &mut [f32]) {
    debug_assert_eq!(out.len() * 4, raw.len());

    for (i, px) in raw.chunks_exact(4).enumerate() {
        out[i] = px[0] as f32 / 255.0;
    }
}

fn encode_gray(dst: &mut [u8], luma: &[f32]) {
    debug_assert_eq!(luma.len(), dst.len());

    for (i, &v) in luma.iter().enumerate() {
        dst[i] = (clamp01(v) * 255.0).round() as u8;
    }
}

fn downsample_luma<'a>(
    source: &[f32],
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
    output: &'a mut Vec<f32>,
) -> Result<&'a [f32], Box<dyn Error>> {
    let target_len = (target_width as usize) * (target_height as usize);
    if output.len() != target_len {
        output.resize(target_len, 0.0f32);
    }

    if source.is_empty() {
        output.fill(0.0);
        return Ok(&output[..target_len]);
    }

    if source.len() != (source_width as usize) * (source_height as usize) {
        return Err("invalid source luma size for downsample".into());
    }

    if source_width == target_width && source_height == target_height {
        output[..source.len()].copy_from_slice(source);
        return Ok(&output[..source.len()]);
    }

    let source_bytes = source
        .iter()
        .map(|value| (clamp01(*value) * 255.0).round() as u8)
        .collect::<Vec<u8>>();

    let source_image = image::GrayImage::from_raw(source_width, source_height, source_bytes)
        .ok_or("invalid source image buffer during downsample")?;

    let resized = image::imageops::resize(
        &source_image,
        target_width,
        target_height,
        image::imageops::FilterType::Lanczos3,
    );

    let resized_values = resized.into_raw();
    if resized_values.len() != target_len {
        return Err("downsample output size mismatch".into());
    }

    for (out, value) in output.iter_mut().zip(resized_values.into_iter()) {
        *out = (value as f32) / 255.0;
    }

    Ok(&output[..target_len])
}

fn stretch_to_percentile(
    src: &mut [f32],
    scratch: &mut [f32],
    low_pct: f32,
    high_pct: f32,
    fast_mode: bool,
) {
    if src.is_empty() {
        return;
    }

    debug_assert_eq!(src.len(), scratch.len());

    let sample_limit = if fast_mode { 8_192usize } else { src.len() }
        .min(src.len())
        .max(2);

    if sample_limit == src.len() {
        scratch.copy_from_slice(src);
    } else {
        let step = src.len() as f32 / sample_limit as f32;
        for (idx, sample_target) in scratch[..sample_limit].iter_mut().enumerate() {
            let source_idx = (idx as f32 * step).floor() as usize;
            *sample_target = src[source_idx.min(src.len() - 1)];
        }
    }

    let sample = &mut scratch[..sample_limit];
    let len_minus_1 = sample_limit - 1;
    let low = (len_minus_1 as f32 * low_pct.clamp(0.0, 1.0)).round() as usize;
    let high = (len_minus_1 as f32 * high_pct.clamp(0.0, 1.0)).round() as usize;
    sample.select_nth_unstable_by(low, |a, b| a.partial_cmp(b).unwrap_or(CmpOrdering::Equal));
    let in_min = sample[low];
    sample.select_nth_unstable_by(high, |a, b| a.partial_cmp(b).unwrap_or(CmpOrdering::Equal));
    let in_max = sample[high];
    let span = in_max - in_min;

    if span <= f32::EPSILON {
        for value in src.iter_mut() {
            *value = 0.5;
        }
        return;
    }

    for value in src.iter_mut() {
        *value = ((*value - in_min) / span).clamp(0.0, 1.0);
    }
}

fn inject_noise(src: &mut [f32], seed: u32, strength: f32) {
    let mut rng = XorShift32::new(seed);
    let gain = strength * 0.5;
    for value in src.iter_mut() {
        let noise = ((rng.next_u32() as f32) / (u32::MAX as f32) - 0.5) * 2.0;
        *value = clamp01(*value + noise * gain);
    }
}

fn create_soft_background(width: u32, height: u32, seed: u32, out: &mut [f32]) {
    debug_assert_eq!(out.len(), (width as usize) * (height as usize));

    let mut rng = XorShift32::new(seed ^ 0x9e37_79b9);
    let freq_x = 0.25 + (rng.next_f32() * 1.8);
    let freq_y = 0.25 + (rng.next_f32() * 1.8);
    let phase_a = rng.next_f32() * std::f32::consts::TAU;
    let phase_b = rng.next_f32() * std::f32::consts::TAU;
    let noise_strength = 0.08 + (rng.next_f32() * 0.1);
    let mut jitter_rng = XorShift32::new(seed ^ 0xA53F_12B1);

    let mut iter = out.iter_mut();
    let width_f = width as f32;
    let height_f = height as f32;
    for y in 0..height {
        let v = (y as f32 / height_f) * 2.0 - 1.0;
        let v_l1 = v.abs();
        for x in 0..width {
            let u = (x as f32 / width_f) * 2.0 - 1.0;
            let u_l1 = u.abs();
            let wave_x = (u * std::f32::consts::TAU * freq_x + phase_a).sin() * 0.3;
            let wave_y = (v * std::f32::consts::TAU * freq_y + phase_b).cos() * 0.3;
            let cross = ((u - v) * 1.5).sin() * 0.2;
            let jitter = (jitter_rng.next_f32() - 0.5) * 2.0;
            let l1_falloff = 0.82 - (u_l1 + v_l1) * 0.22;
            let value = clamp01(
                0.46 + (wave_x * 0.24)
                    + (wave_y * 0.24)
                    + (cross * 0.16)
                    + (l1_falloff * 0.24)
                    + (jitter - 0.5) * noise_strength,
            );
            if let Some(px) = iter.next() {
                *px = value;
            }
        }
    }
}

fn blend_background(src: &mut [f32], bg: &[f32], strength: f32) {
    debug_assert_eq!(src.len(), bg.len());

    src.par_iter_mut()
        .zip(bg.par_iter())
        .for_each(|(value, bg_value)| {
            *value = clamp01((*value * (1.0 - strength)) + (*bg_value * strength));
        });
}

fn blend_layer_stack(dst: &mut [f32], layer: &[f32], strength: f32, mode: LayerBlendMode) {
    debug_assert_eq!(dst.len(), layer.len());

    let alpha = strength.clamp(0.0, 1.0);
    dst.par_iter_mut()
        .zip(layer.par_iter())
        .for_each(|(base, top)| {
            let mixed = match mode {
                LayerBlendMode::Normal => *top,
                LayerBlendMode::Add => clamp01(*base + *top),
                LayerBlendMode::Multiply => clamp01(*base * *top),
                LayerBlendMode::Screen => 1.0 - ((1.0 - *base) * (1.0 - *top)),
                LayerBlendMode::Overlay => {
                    if *base < 0.5 {
                        2.0 * *base * *top
                    } else {
                        1.0 - (2.0 * (1.0 - *base) * (1.0 - *top))
                    }
                }
                LayerBlendMode::Difference => (*base - *top).abs(),
                LayerBlendMode::Lighten => (*base).max(*top),
                LayerBlendMode::Darken => (*base).min(*top),
                LayerBlendMode::Glow => *base + (1.0 - *base) * (*top * *top),
                LayerBlendMode::Shadow => *base * *top,
            };

            *base = clamp01((1.0 - alpha) * *base + alpha * mixed);
        });
}

fn apply_contrast(src: &mut [f32], strength: f32) {
    let clamped = strength.clamp(1.0, 3.0);
    let midpoint = 0.5;
    for value in src.iter_mut() {
        *value = clamp01(((*value - midpoint) * clamped) + midpoint);
    }
}

fn apply_motion_blur(width: u32, height: u32, src: &[f32], dst: &mut [f32], cfg: &BlurConfig) {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let width_usize = width as usize;
    let cfg = *cfg;

    dst.par_iter_mut().enumerate().for_each(|(idx, out)| {
        let y = (idx / width_usize) as i32;
        let x = (idx % width_usize) as i32;
        let center = sample_luma(src, width_i32, height_i32, x, y);
        let local_blur = (1.0 - center).powi(2);
        let radius = (1.0 + (cfg.max_radius as f32 * (0.2 + 0.8 * local_blur))).round() as i32;

        let mut numerator = 0.0;
        let mut denominator = 0.0;
        let mut step = -radius;
        while step <= radius {
            let t = 1.0 - (step.abs() as f32 / (radius as f32 + 1.0));
            let sx = x + step * cfg.axis_x;
            let sy = y + step * cfg.axis_y;
            let sample = sample_luma(src, width_i32, height_i32, sx, sy);
            numerator += sample * t;
            denominator += t;
            step += 1;
        }

        if denominator > 0.0 {
            *out = numerator / denominator;
        } else {
            *out = center;
        }
    });
}

fn apply_gaussian_blur(width: u32, height: u32, src: &[f32], dst: &mut [f32], cfg: &BlurConfig) {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let width_usize = width as usize;
    let cfg = *cfg;

    dst.par_iter_mut().enumerate().for_each(|(idx, out)| {
        let y = (idx / width_usize) as i32;
        let x = (idx % width_usize) as i32;
        let center = sample_luma(src, width_i32, height_i32, x, y);
        let local_blur = (1.0 - center).powf(1.5);
        let radius = (1.0 + (cfg.max_radius as f32 * (0.2 + 0.8 * local_blur))).round() as i32;
        let sigma = (radius as f32 + 1.0) * 0.5;
        let sigma2 = sigma * sigma * 2.0;

        let mut num = 0.0;
        let mut den = 0.0;
        let mut dy = -radius;
        while dy <= radius {
            let mut dx = -radius;
            while dx <= radius {
                let sx = x + dx;
                let sy = y + dy;
                let d2 = (dx * dx + dy * dy) as f32;
                let spatial = (-d2 / sigma2).exp();
                let sample = sample_luma(src, width_i32, height_i32, sx, sy);
                num += sample * spatial;
                den += spatial;
                dx += 1;
            }
            dy += 1;
        }

        if den > 0.0 {
            *out = num / den;
        } else {
            *out = center;
        }
    });
}

fn apply_median_blur(width: u32, height: u32, src: &[f32], dst: &mut [f32], cfg: &BlurConfig) {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let width_usize = width as usize;
    let cfg = *cfg;

    dst.par_iter_mut().enumerate().for_each(|(idx, out)| {
        let y = (idx / width_usize) as i32;
        let x = (idx % width_usize) as i32;
        let center = sample_luma(src, width_i32, height_i32, x, y);
        let local_blur = (1.0 - center).powi(2);
        let base = 1 + ((cfg.max_radius as f32 * (0.4 + 0.6 * local_blur)).floor() as i32);
        let radius = base.clamp(1, 2);
        let mut values = [0f32; 25];
        let mut count = 0usize;

        let mut dy = -radius;
        while dy <= radius {
            let mut dx = -radius;
            while dx <= radius {
                let sample = sample_luma(src, width_i32, height_i32, x + dx, y + dy);
                values[count] = sample;
                count += 1;
                dx += 1;
            }
            dy += 1;
        }

        values[..count].sort_by(|a, b| a.partial_cmp(b).unwrap_or(CmpOrdering::Equal));
        *out = values[count / 2];
    });
}

fn apply_bilateral_blur(width: u32, height: u32, src: &[f32], dst: &mut [f32], cfg: &BlurConfig) {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let width_usize = width as usize;
    let sigma_r = 0.1 + (cfg.softness as f32 * 0.03);
    let cfg = *cfg;
    let radius_limit = cfg.max_radius as f32;

    dst.par_iter_mut().enumerate().for_each(|(idx, out)| {
        let y = (idx / width_usize) as i32;
        let x = (idx % width_usize) as i32;
        let center = sample_luma(src, width_i32, height_i32, x, y);
        let local_blur = (1.0 - center).powi(2);
        let radius = (1 + ((radius_limit * (0.2 + 0.8 * local_blur)).round() as i32)).clamp(1, 2);

        let mut num = 0.0;
        let mut den = 0.0;
        let mut dy = -radius;
        while dy <= radius {
            let mut dx = -radius;
            while dx <= radius {
                let sample = sample_luma(src, width_i32, height_i32, x + dx, y + dy);
                let d = sample - center;
                let range = (-(d * d / (2.0 * sigma_r * sigma_r))).exp();
                let weight = (-((dx * dx + dy * dy) as f32) / 16.0).exp() * range;
                num += sample * weight;
                den += weight;
                dx += 1;
            }
            dy += 1;
        }

        if den > 0.0 {
            *out = num / den;
        } else {
            *out = center;
        }
    });
}

fn apply_dynamic_filter(width: u32, height: u32, luma: &[f32], dst: &mut [f32], cfg: &BlurConfig) {
    match cfg.mode {
        FilterMode::Motion => apply_motion_blur(width, height, luma, dst, cfg),
        FilterMode::Gaussian => apply_gaussian_blur(width, height, luma, dst, cfg),
        FilterMode::Median => apply_median_blur(width, height, luma, dst, cfg),
        FilterMode::Bilateral => apply_bilateral_blur(width, height, luma, dst, cfg),
    }
}

fn apply_sharpen(width: u32, height: u32, src: &[f32], dst: &mut [f32], strength: f32) {
    let width_usize = width as usize;
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let strength = strength.clamp(0.2, 2.0);
    let inv_count = 1.0 / 9.0;

    dst.par_iter_mut().enumerate().for_each(|(idx, out)| {
        let y = (idx / width_usize) as i32;
        let x = (idx % width_usize) as i32;
        let mut sum = 0.0;
        let mut count = 0.0;
        let mut dy = -1;
        while dy <= 1 {
            let mut dx = -1;
            while dx <= 1 {
                sum += sample_luma(src, width_i32, height_i32, x + dx, y + dy);
                count += 1.0;
                dx += 1;
            }
            dy += 1;
        }
        let center = src[idx];
        let local_mean = sum * (inv_count / (count / 9.0));
        *out = clamp01(center + (center - local_mean) * strength);
    });
}

fn apply_detail_waves(src: &mut [f32], width: u32, height: u32, seed: u32, strength: f32) {
    let mut rng = XorShift32::new(seed);
    let strength = strength.clamp(0.0, 0.25);
    let freq = 4.0 + (rng.next_f32() * 16.0);
    let freq_y = 3.0 + (rng.next_f32() * 14.0);
    let phase_a = rng.next_f32() * std::f32::consts::TAU;
    let phase_b = rng.next_f32() * std::f32::consts::TAU;
    let phase_c = rng.next_f32() * std::f32::consts::TAU;

    let width_f = width as f32;
    let height_f = height as f32;
    let two_pi = std::f32::consts::TAU;

    for (idx, value) in src.iter_mut().enumerate() {
        let y = (idx as u32 / width) as f32;
        let x = (idx as u32 % width) as f32;
        let u = (x / width_f * two_pi * freq) + phase_a;
        let v = (y / height_f * two_pi * freq_y) + phase_b;
        let mix = 0.5 + 0.5 * (u.sin() * 0.55 + v.cos() * 0.45 + ((u + v + phase_c).sin() * 0.35));
        *value = clamp01((*value * (1.0 - strength)) + (mix * strength));
    }
}

fn resolve_output_path(output: &str) -> PathBuf {
    let base_path = Path::new(output);
    if !base_path.exists() {
        return base_path.to_path_buf();
    }

    let parent = base_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = base_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");
    let extension = base_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    let mut index = 1u32;

    loop {
        let candidate_name = if extension.is_empty() {
            format!("{stem}_{index}")
        } else {
            format!("{stem}_{index}.{extension}")
        };

        let candidate = if parent.as_os_str().is_empty() {
            PathBuf::from(candidate_name)
        } else {
            parent.join(candidate_name)
        };

        if !candidate.exists() {
            return candidate;
        }

        index += 1;
    }
}

fn encode_png_bytes(width: u32, height: u32, data: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    if data.len() != width as usize * height as usize {
        return Err("invalid buffer size for grayscale image".into());
    }

    let mut cursor = Cursor::new(Vec::new());
    {
        let encoder =
            PngEncoder::new_with_quality(&mut cursor, CompressionType::Best, FilterType::Adaptive);
        encoder.write_image(data, width, height, image::ColorType::L8)?;
    }

    Ok(cursor.into_inner())
}

fn save_png_under_10mb(
    output: &Path,
    mut width: u32,
    mut height: u32,
    gray: &[u8],
) -> Result<(u32, u32, usize), Box<dyn Error>> {
    let mut working = gray.to_vec();
    let mut encoded = encode_png_bytes(width, height, &working)?;
    let mut shrink_passes = 0u32;

    while encoded.len() > MAX_OUTPUT_BYTES
        && width > MIN_IMAGE_DIMENSION
        && height > MIN_IMAGE_DIMENSION
    {
        let next_width = ((width as f32) * 0.9)
            .round()
            .max(MIN_IMAGE_DIMENSION as f32)
            .min(width as f32) as u32;
        let next_height = ((height as f32) * 0.9)
            .round()
            .max(MIN_IMAGE_DIMENSION as f32)
            .min(height as f32) as u32;

        if next_width == width && next_height == height {
            break;
        }

        let source = image::GrayImage::from_raw(width, height, working)
            .ok_or("invalid working image buffer during resize")?;
        let resized = image::imageops::resize(
            &source,
            next_width,
            next_height,
            image::imageops::FilterType::Lanczos3,
        );
        width = next_width;
        height = next_height;
        working = resized.into_raw();
        encoded = encode_png_bytes(width, height, &working)?;
        shrink_passes += 1;

        if shrink_passes > 48 {
            break;
        }
    }

    if encoded.len() > MAX_OUTPUT_BYTES {
        // Final safety pass: progressively force down to minimum dimension in bigger steps.
        while encoded.len() > MAX_OUTPUT_BYTES
            && width > MIN_IMAGE_DIMENSION
            && height > MIN_IMAGE_DIMENSION
        {
            let width_scale = (MAX_OUTPUT_BYTES as f32 / encoded.len() as f32).sqrt() * 0.95;
            let next_width = ((width as f32) * width_scale)
                .floor()
                .max(MIN_IMAGE_DIMENSION as f32) as u32;
            let next_height = ((height as f32) * width_scale)
                .floor()
                .max(MIN_IMAGE_DIMENSION as f32) as u32;
            let target_width = next_width.max(1).min(width);
            let target_height = next_height.max(1).min(height);

            if target_width == width && target_height == height {
                break;
            }

            let source = image::GrayImage::from_raw(width, height, working)
                .ok_or("invalid working image buffer during final resize")?;
            let resized = image::imageops::resize(
                &source,
                target_width,
                target_height,
                image::imageops::FilterType::Lanczos3,
            );
            width = target_width;
            height = target_height;
            working = resized.into_raw();
            encoded = encode_png_bytes(width, height, &working)?;
        }
    }

    let final_size = encoded.len();
    fs::write(output, encoded)?;
    Ok((width, height, final_size))
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    pollster::block_on(run(config))?;
    Ok(())
}

async fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .ok_or("no compatible GPU adapter found")?;
    let adapter_info = adapter.get_info();
    let can_use_gpu = !matches!(adapter_info.device_type, wgpu::DeviceType::Cpu);
    if can_use_gpu {
        eprintln!(
            "Using adapter: {} ({:?})",
            adapter_info.name, adapter_info.device_type
        );
    } else {
        eprintln!(
            "Using adapter: {} ({:?}) - GPU-accelerated strategies unavailable",
            adapter_info.name, adapter_info.device_type
        );
    }

    let (render_width, render_height, resolved_antialias) =
        resolve_render_resolution(config.width, config.height, config.antialias);
    let fast = config.fast || render_width >= 1536;
    if fast && !config.fast {
        eprintln!(
            "High-resolution run ({render_width}x{render_height}) detected, enabling fast profile for responsiveness."
        );
    }
    let fast_profile = resolve_fast_profile(render_width, config.count, fast);
    let (render_width, render_height, resolved_antialias, render_scaled) = resolve_fast_resolution(
        render_width,
        render_height,
        resolved_antialias,
        fast_profile,
    );
    if fast
        && (fast_profile.iteration_cap != u32::MAX
            || fast_profile.layer_cap != u32::MAX
            || fast_profile.render_side_cap != u32::MAX)
        && (config.count > 1 || render_width >= 2048)
    {
        eprintln!(
            "Fast profile caps: max iterations {}, max layers {}, render side {}{}.",
            fast_profile.iteration_cap,
            fast_profile.layer_cap,
            fast_profile.render_side_cap,
            if render_scaled {
                " (render capped for safety)"
            } else {
                ""
            }
        );
    }

    let mut gpu = GpuLayerRenderer::new(&adapter, SHADER, render_width, render_height).await?;

    let pixel_count = (render_width as usize) * (render_height as usize);
    let final_pixel_count = (config.width as usize) * (config.height as usize);
    let mut workspace = RenderWorkspace::new(pixel_count, final_pixel_count);

    let mut image_rng = XorShift32::new(config.seed);
    let spinner_state = Arc::new(SpinnerState::new(config.count as usize));
    let user_set_layer_count = config.layers.is_some();
    let (spinner_running, _spinner_handle) = start_spinner(spinner_state.clone());

    let mut render_strategy_layer = |strategy: RenderStrategy,
                                     strategy_params: &Params,
                                     out: &mut [f32]|
     -> Result<(), Box<dyn Error>> {
        match strategy {
            RenderStrategy::Gpu(_) => {
                gpu.render_layer(strategy_params, out)?;
                Ok(())
            }
            RenderStrategy::Cpu(cpu_strategy) => {
                let generated = render_cpu_strategy(
                    cpu_strategy,
                    render_width,
                    render_height,
                    strategy_params.seed,
                    fast,
                );
                out.copy_from_slice(&generated);
                Ok(())
            }
        }
    };

    for i in 0..config.count {
        spinner_state.set_image((i + 1) as usize, 0);
        let mut image_seed = image_rng.next_u32();
        if image_seed == 0 {
            image_seed = 0x9e3779b9;
        }
        let base_seed = image_seed;
        let base_symmetry = randomize_symmetry(config.symmetry, &mut image_rng);
        let mut base_iterations = randomize_iterations(config.iterations, &mut image_rng);
        base_iterations = clamp_iteration_count(base_iterations, fast_profile.iteration_cap);
        let base_fill_scale = randomize_fill_scale(config.fill_scale, &mut image_rng);
        let mut base_symmetry_style = pick_symmetry_style(&mut image_rng);
        if image_rng.next_f32() > (if fast { 0.02 } else { 0.03 }) {
            base_symmetry_style = pick_non_grid_symmetry_style(&mut image_rng).as_u32();
        }
        let base_zoom = randomize_zoom(config.fractal_zoom, &mut image_rng);
        let base_bend_strength = pick_bend_strength(&mut image_rng);
        let base_warp_strength = pick_warp_strength(&mut image_rng);
        let base_warp_frequency = pick_warp_frequency(&mut image_rng);
        let base_tile_scale = pick_tile_scale(&mut image_rng);
        let base_tile_phase = pick_tile_phase(&mut image_rng);
        let (base_center_x, base_center_y) = randomize_center_offset(&mut image_rng, fast);
        let mut layer_count = pick_layer_count(&mut image_rng, config.layers, fast);
        if !user_set_layer_count {
            layer_count = clamp_layer_count(layer_count, fast_profile.layer_cap);
        }
        let mut shader_layer_count = pick_shader_layer_count(layer_count, &mut image_rng, fast);
        spinner_state.set_image((i + 1) as usize, layer_count as usize);
        let base_symmetry_style = SymmetryStyle::from_u32(base_symmetry_style);
        let grid_on_all_layers =
            should_apply_grid_across_layers(base_symmetry_style, layer_count, &mut image_rng, fast);
        let base_symmetry_style =
            resolve_symmetry_style(base_symmetry_style, grid_on_all_layers, &mut image_rng)
                .as_u32();
        let base_art_style = pick_art_style(&mut image_rng);
        let base_art_style_secondary = pick_art_style_secondary(base_art_style, &mut image_rng);
        let base_art_mix = image_rng.next_f32();
        let mut base_strategy =
            pick_render_strategy_with_preferences(&mut image_rng, fast, can_use_gpu);
        if can_use_gpu
            && fast
            && render_width >= 1536
            && let RenderStrategy::Cpu(_) = base_strategy
        {
            base_strategy = RenderStrategy::Gpu(ArtStyle::from_u32(image_rng.next_u32()).as_u32());
        }
        let base_strategy_name = base_strategy.label();
        let base_profile = strategy_profile(base_strategy);
        let mut structural_profile =
            should_use_structural_profile(fast, &mut image_rng) || base_profile.force_detail;
        let mut layer_steps = Vec::new();
        let mut active_strategy = base_strategy;

        create_soft_background(
            render_width,
            render_height,
            base_seed ^ (i + 0x0BADC0DEu32),
            &mut workspace.background,
        );
        let background_strength = 0.2 + (image_rng.next_f32() * 0.14);
        let mut pre_filter_stats = LumaStats::default();
        workspace.reset_layered();

        for layer_index in 0..layer_count {
            spinner_state.set_layer((layer_index + 1) as usize);
            let layer_seed = base_seed.wrapping_add((layer_index + 1).wrapping_mul(0x9e3779b9));
            if layer_index > 0 {
                active_strategy =
                    bias_layer_strategy(active_strategy, &mut image_rng, fast, can_use_gpu);
            }
            let layer_strategy = if layer_index == 0 {
                base_strategy
            } else {
                active_strategy
            };
            if render_width >= 1536 && i == 0 && layer_index == 0 {
                let strategy_desc = match layer_strategy {
                    RenderStrategy::Gpu(style) => {
                        format!("gpu:{}", ArtStyle::from_u32(style).label())
                    }
                    RenderStrategy::Cpu(cpu) => format!("cpu:{}", cpu.label()),
                };
                eprintln!(
                    "Image 1/{} layer 1/{} start: {}",
                    config.count, layer_count, strategy_desc
                );
            }
            let strategy_profile = strategy_profile(layer_strategy);
            let layer_force_detail = structural_profile || strategy_profile.force_detail;
            structural_profile = layer_force_detail;

            let mut layer_style = modulate_art_style(base_art_style, &mut image_rng, fast);
            let mut layer_style_secondary =
                modulate_art_style(base_art_style_secondary, &mut image_rng, fast);
            shader_layer_count = pick_shader_layer_count(shader_layer_count, &mut image_rng, fast)
                .max(1 + (layer_index > 0) as u32);
            let symmetry_style = SymmetryStyle::from_u32(modulate_symmetry_style(
                base_symmetry_style,
                &mut image_rng,
                fast,
                grid_on_all_layers,
            ));

            if let RenderStrategy::Gpu(style) = layer_strategy {
                layer_style = ArtStyle::from_u32(style);
                layer_style_secondary = modulate_art_style(
                    ArtStyle::from_u32((style + 1) % ArtStyle::total()),
                    &mut image_rng,
                    fast,
                );
            }

            let params = Params {
                width: render_width,
                height: render_height,
                symmetry: modulate_symmetry(base_symmetry, &mut image_rng, fast),
                symmetry_style: symmetry_style.as_u32(),
                iterations: clamp_iteration_count(
                    modulate_iterations(base_iterations, &mut image_rng, fast),
                    fast_profile.iteration_cap,
                ),
                seed: layer_seed,
                fill_scale: modulate_fill_scale(base_fill_scale, &mut image_rng, fast),
                fractal_zoom: modulate_zoom(base_zoom, &mut image_rng, fast),
                bend_strength: modulate_bend_strength(base_bend_strength, &mut image_rng, fast),
                warp_strength: modulate_warp_strength(base_warp_strength, &mut image_rng, fast),
                warp_frequency: modulate_warp_frequency(base_warp_frequency, &mut image_rng, fast),
                tile_scale: modulate_tile_scale(
                    base_tile_scale,
                    symmetry_style == SymmetryStyle::Grid,
                    &mut image_rng,
                    fast,
                ),
                tile_phase: modulate_tile_phase(base_tile_phase, &mut image_rng, fast),
                center_x: modulate_center_offset(base_center_x, &mut image_rng, fast),
                center_y: modulate_center_offset(base_center_y, &mut image_rng, fast),
                art_style: layer_style.as_u32(),
                art_style_secondary: layer_style_secondary.as_u32(),
                art_style_mix: modulate_style_mix(base_art_mix, &mut image_rng, fast),
                layer_count: modulate_shader_layer_count(
                    shader_layer_count,
                    &mut image_rng,
                    fast,
                    layer_force_detail,
                ),
            };

            render_strategy_layer(layer_strategy, &params, &mut workspace.luma)?;
            if layer_index == 0 {
                pre_filter_stats =
                    collect_luma_metrics(&workspace.luma, render_width, render_height).stats;
            }

            let filter = tune_filter_for_speed(pick_filter_from_rng(&mut image_rng), fast);
            let gradient = pick_gradient_from_rng(&mut image_rng);
            let overlay = pick_layer_blend(&mut image_rng);
            let layer_contrast = pick_layer_contrast(&mut image_rng, fast);
            let apply_filter = should_apply_dynamic_filter(
                layer_index,
                &mut image_rng,
                fast,
                layer_force_detail,
                strategy_profile.filter_bias,
            );
            let apply_gradient = should_apply_gradient_map(
                layer_index,
                &mut image_rng,
                fast,
                layer_force_detail,
                strategy_profile.gradient_bias,
            );
            let opacity = if layer_index == 0 {
                1.0
            } else {
                layer_opacity(&mut image_rng)
            };
            let mut complexity_fixed = false;
            let mut layer_mix_desc = String::new();
            let apply_strategy_mix = blending::should_mix_strategies(
                layer_index,
                &mut image_rng,
                fast,
                structural_profile,
                strategy_profile.filter_bias.max(0.5),
            );
            if apply_strategy_mix {
                let secondary_strategy = blending::pick_blended_strategy(
                    layer_strategy,
                    &mut image_rng,
                    fast,
                    can_use_gpu,
                );
                let secondary_seed = layer_seed ^ 0x91A5_FD3Bu32;
                let mut secondary_params = params;
                secondary_params.seed = secondary_seed;
                secondary_params.iterations = clamp_iteration_count(
                    modulate_iterations(params.iterations, &mut image_rng, fast),
                    fast_profile.iteration_cap,
                );
                if let RenderStrategy::Gpu(style) = secondary_strategy {
                    secondary_params.art_style = style;
                    secondary_params.art_style_secondary = modulate_art_style(
                        ArtStyle::from_u32((style + 1) % ArtStyle::total()),
                        &mut image_rng,
                        fast,
                    )
                    .as_u32();
                    secondary_params.art_style_mix =
                        modulate_style_mix(params.art_style_mix, &mut image_rng, fast);
                }
                render_strategy_layer(
                    secondary_strategy,
                    &secondary_params,
                    &mut workspace.blend_secondary,
                )?;
                let mask_kind = blending::pick_layer_mask_kind(&mut image_rng, structural_profile);
                let mut mask_request = blending::LayerMaskBuildRequest {
                    primary: &workspace.luma,
                    width: render_width,
                    height: render_height,
                    source_seed: secondary_seed,
                    kind: mask_kind,
                    out: &mut workspace.mix_mask,
                    blur_work: &mut workspace.mask_workspace,
                    fast,
                };
                blending::build_layer_mask(&mut mask_request, &mut image_rng);
                blending::blend_with_mask(
                    &mut workspace.luma,
                    &workspace.blend_secondary,
                    &workspace.mix_mask,
                    image_rng.next_f32() < 0.2,
                );
                layer_mix_desc = format!(
                    " mix:{}:{}({})",
                    strategy_name(layer_strategy),
                    strategy_name(secondary_strategy),
                    mask_kind.label()
                );
            }

            if apply_filter {
                apply_dynamic_filter(
                    render_width,
                    render_height,
                    &workspace.luma,
                    &mut workspace.filtered,
                    &filter,
                );
                let low_stretch = if fast { 0.03 } else { 0.04 };
                let high_stretch = if fast { 0.97 } else { 0.96 };
                stretch_to_percentile(
                    &mut workspace.filtered,
                    &mut workspace.percentile,
                    low_stretch,
                    high_stretch,
                    fast,
                );
            } else {
                workspace.filtered.copy_from_slice(&workspace.luma);
                stretch_to_percentile(
                    &mut workspace.filtered,
                    &mut workspace.percentile,
                    if fast { 0.02 } else { 0.03 },
                    if fast { 0.98 } else { 0.97 },
                    fast,
                );
            }

            if apply_filter && layer_force_detail && image_rng.next_f32() < 0.5 {
                apply_detail_waves(
                    &mut workspace.filtered,
                    render_width,
                    render_height,
                    layer_seed ^ 0x4D4E_4446,
                    if fast { 0.03 } else { 0.05 },
                );
                apply_sharpen(
                    render_width,
                    render_height,
                    &workspace.filtered,
                    &mut workspace.detail,
                    if fast { 0.32 } else { 0.58 },
                );
                std::mem::swap(&mut workspace.filtered, &mut workspace.detail);
            }

            if apply_gradient {
                apply_gradient_map(&mut workspace.filtered, gradient);
                stretch_to_percentile(
                    &mut workspace.filtered,
                    &mut workspace.percentile,
                    if fast { 0.01 } else { 0.02 },
                    if fast { 0.99 } else { 0.98 },
                    fast,
                );
            }
            if !apply_filter && !apply_gradient {
                if structural_profile {
                    apply_detail_waves(
                        &mut workspace.filtered,
                        render_width,
                        render_height,
                        layer_seed ^ 0x2f7f_8d3d,
                        if fast { 0.05 } else { 0.09 },
                    );
                } else if image_rng.next_f32() < 0.35 {
                    apply_detail_waves(
                        &mut workspace.filtered,
                        render_width,
                        render_height,
                        layer_seed ^ 0x9d7e_4f2a,
                        if fast { 0.04 } else { 0.07 },
                    );
                }

                apply_sharpen(
                    render_width,
                    render_height,
                    &workspace.filtered,
                    &mut workspace.detail,
                    if structural_profile {
                        if fast { 0.72 } else { 1.12 }
                    } else if fast {
                        0.45
                    } else {
                        0.75
                    },
                );
                std::mem::swap(&mut workspace.filtered, &mut workspace.detail);
                apply_posterize_buffer(
                    &mut workspace.filtered,
                    2 + (image_rng.next_u32() % if structural_profile { 7 } else { 5 }),
                );
            }
            let layer_contrast = if apply_filter || apply_gradient {
                layer_contrast
            } else {
                layer_contrast * 0.75
            };
            apply_contrast(&mut workspace.filtered, layer_contrast.max(1.0));
            let layer_metrics =
                collect_luma_metrics(&workspace.filtered, render_width, render_height);
            if needs_complexity_fix(&layer_metrics.stats, layer_metrics.edge_energy) {
                complexity_fixed = true;
                apply_detail_waves(
                    &mut workspace.filtered,
                    render_width,
                    render_height,
                    layer_seed ^ 0x4445_6d63,
                    if fast { 0.10 } else { 0.18 },
                );
                apply_sharpen(
                    render_width,
                    render_height,
                    &workspace.filtered,
                    &mut workspace.detail,
                    if fast { 0.55 } else { 0.9 },
                );
                std::mem::swap(&mut workspace.filtered, &mut workspace.detail);
                apply_posterize_buffer(&mut workspace.filtered, 2 + (image_rng.next_u32() % 6));
                apply_contrast(
                    &mut workspace.filtered,
                    1.25 + (image_rng.next_f32() * 0.45),
                );
            }

            if layer_index == 0 {
                workspace.layered.copy_from_slice(&workspace.filtered);
            } else {
                blend_layer_stack(
                    &mut workspace.layered,
                    &workspace.filtered,
                    opacity,
                    overlay,
                );
            }

            let layer_strategy_name = match layer_strategy {
                RenderStrategy::Cpu(cpu) => format!("[{}]", cpu.label()),
                RenderStrategy::Gpu(_) => String::new(),
            };
            let filter_name = if apply_filter {
                filter.mode.label()
            } else {
                "none"
            };

            layer_steps.push(format!(
                "L{}:{}({:.2}, f{}{}, g{}, d{}, c{:.2}) S{}+{}:{:.2}",
                layer_index + 1,
                overlay.label(),
                opacity,
                filter_name,
                layer_strategy_name,
                if apply_gradient { "on" } else { "off" },
                if complexity_fixed { "on" } else { "off" },
                layer_contrast,
                ArtStyle::from_u32(params.art_style).label(),
                ArtStyle::from_u32(params.art_style_secondary).label(),
                params.art_style_mix,
            ));
            if !layer_mix_desc.is_empty() {
                layer_steps.push(format!("M{}", layer_mix_desc));
            }
        }

        blend_background(
            &mut workspace.layered,
            &workspace.background,
            background_strength,
        );
        let final_contrast = if fast { 1.45 } else { 1.8 };
        apply_contrast(&mut workspace.layered, final_contrast);
        stretch_to_percentile(
            &mut workspace.layered,
            &mut workspace.percentile,
            0.01,
            0.99,
            fast,
        );

        let mut final_metrics =
            collect_luma_metrics(&workspace.layered, render_width, render_height);
        let mut final_complexity_fixed = false;
        if needs_complexity_fix(&final_metrics.stats, final_metrics.edge_energy) {
            final_complexity_fixed = true;
            apply_detail_waves(
                &mut workspace.layered,
                render_width,
                render_height,
                base_seed ^ (i + 0x445f_6e65),
                if fast { 0.08 } else { 0.14 },
            );
            apply_sharpen(
                render_width,
                render_height,
                &workspace.layered,
                &mut workspace.detail,
                if fast { 0.45 } else { 0.75 },
            );
            std::mem::swap(&mut workspace.layered, &mut workspace.detail);
            apply_posterize_buffer(&mut workspace.layered, if fast { 4 } else { 5 });
            apply_contrast(&mut workspace.layered, if fast { 1.2 } else { 1.4 });
            final_metrics = collect_luma_metrics(&workspace.layered, render_width, render_height);
        }
        if final_metrics.stats.std < 0.09
            || (final_metrics.stats.max - final_metrics.stats.min) < 0.23
        {
            inject_noise(
                &mut workspace.layered,
                base_seed ^ (i + 1),
                if fast { 0.04 } else { 0.06 },
            );
            stretch_to_percentile(
                &mut workspace.layered,
                &mut workspace.percentile,
                0.01,
                0.99,
                fast,
            );
            final_metrics = collect_luma_metrics(&workspace.layered, render_width, render_height);
        }

        let output_luma = if resolved_antialias == 1
            && render_width == config.width
            && render_height == config.height
        {
            &workspace.layered
        } else {
            downsample_luma(
                &workspace.layered,
                render_width,
                render_height,
                config.width,
                config.height,
                &mut workspace.final_luma,
            )?;
            workspace.final_luma.as_slice()
        };
        encode_gray(&mut workspace.final_pixels, output_luma);
        let final_output = resolve_output_path(&config.output);
        let (final_width, final_height, final_bytes) = save_png_under_10mb(
            &final_output,
            config.width,
            config.height,
            &workspace.final_pixels,
        )?;
        let scale = format!(
            "{:.2}",
            if final_width == config.width {
                1.0
            } else {
                (final_width as f32) / (config.width as f32)
            }
        );

        let layer_summary = if layer_steps.is_empty() {
            "none".to_string()
        } else {
            layer_steps.join(", ")
        };

        println!(
            "Generated {} | index {} | seed {} | fill {:.2} | zoom {:.2} | symmetry {} [{}] center({:.2},{:.2}) | iterations {} | strategy {} | final d{} | layers {} | layers [{}] | image {}x{} (aa {}) (scale {} / {:.2}MB) | pre({:.2}-{:.2},{:.2}) post({:.2}-{:.2},{:.2})",
            final_output.display(),
            i,
            base_seed,
            base_fill_scale,
            base_zoom,
            base_symmetry,
            SymmetryStyle::from_u32(base_symmetry_style).label(),
            base_center_x,
            base_center_y,
            base_iterations,
            base_strategy_name,
            if final_complexity_fixed { "on" } else { "off" },
            layer_count,
            layer_summary,
            final_width,
            final_height,
            resolved_antialias,
            scale,
            final_bytes as f64 / (1024.0 * 1024.0),
            pre_filter_stats.min,
            pre_filter_stats.max,
            pre_filter_stats.mean,
            final_metrics.stats.min,
            final_metrics.stats.max,
            final_metrics.stats.mean
        );
    }
    spinner_running.store(false, Ordering::Release);
    let _ = write!(io::stderr(), "\r{:<120}\r", "");
    let _ = io::stderr().flush();
    Ok(())
}

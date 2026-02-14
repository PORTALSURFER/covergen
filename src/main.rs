use std::cmp::Ordering;
use std::io::Cursor;
use std::sync::mpsc::channel;
use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use bytemuck::{Pod, Zeroable};
use image::{
    ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use rayon::prelude::*;
use wgpu::util::DeviceExt;

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
}

@group(0) @binding(0)
var<storage, read_write> out_pixels: array<u32>;

@group(0) @binding(1)
var<uniform> params: Params;

fn hash01(x: f32, y: f32, seed: u32) -> f32 {
    let s = f32(seed) * 0.00000011920928955;
    return fract(sin(dot(vec2<f32>(x, y), vec2<f32>(12.9898, 78.233)) + s) * 43758.5453123);
}

fn fold_for_symmetry(px: f32, py: f32, symmetry: u32, style: u32) -> vec2<f32> {
    if (style == 1u) {
        return fold_symmetry_mirror(px, py, symmetry);
    }

    if (style == 2u) {
        return fold_symmetry_grid(px, py, symmetry);
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

fn fold_symmetry_grid(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    let repeats = max(2.0, f32(symmetry) * 0.7);
    let gx = fract((px * repeats) + 0.5) - 0.5;
    let gy = fract((py * repeats * 1.15) + 0.5) - 0.5;
    let mx = abs(gx) * 2.0 - 0.5;
    let my = abs(gy) * 2.0 - 0.5;
    let skew = sin(px * 8.0 + py * 4.0) * 0.15;
    return vec2<f32>(mx * (2.0 / repeats) + (my * 0.08), (my * (2.0 / repeats)) + skew);
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

fn style_value(x: f32, y: f32, params: Params, layer: u32, style_selector: u32) -> f32 {
    let style = style_selector % 12u;
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
    var value = 0.0;
    let layer_count = 10u;

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

const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;
const MIN_IMAGE_DIMENSION: u32 = 64;

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
    IFS,
}

impl ArtStyle {
    fn from_u32(value: u32) -> Self {
        match value % 12 {
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
            _ => Self::IFS,
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
            Self::IFS => 11,
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
            Self::IFS => "ifs",
        }
    }

    fn total() -> u32 {
        12
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

#[derive(Clone, Copy)]
enum SymmetryStyle {
    Radial,
    Mirror,
    Grid,
}

impl SymmetryStyle {
    fn from_u32(value: u32) -> Self {
        match value % 3 {
            0 => Self::Radial,
            1 => Self::Mirror,
            _ => Self::Grid,
        }
    }

    fn as_u32(self) -> u32 {
        match self {
            Self::Radial => 0,
            Self::Mirror => 1,
            Self::Grid => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Radial => "radial",
            Self::Mirror => "mirror",
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

struct Config {
    width: u32,
    height: u32,
    symmetry: u32,
    iterations: u32,
    seed: u32,
    fill_scale: f32,
    fractal_zoom: f32,
    fast: bool,
    layers: Option<u32>,
    count: u32,
    output: String,
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

impl Config {
    fn from_env() -> Result<Self, Box<dyn Error>> {
        let mut args = env::args().skip(1);
        let mut cfg = Config {
            width: 1024,
            height: 1024,
            symmetry: 4,
            iterations: 320,
            seed: random_seed(),
            fill_scale: 1.35,
            fractal_zoom: 0.72,
            fast: false,
            layers: None,
            count: 1,
            output: "fractal.png".to_string(),
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--size" => {
                    let value = args.next().ok_or("missing size value, pass --size <u32>")?;
                    let size = value.parse::<u32>()?;
                    cfg.width = size;
                    cfg.height = size;
                }
                "--symmetry" => {
                    let value = args
                        .next()
                        .ok_or("missing symmetry value, pass --symmetry <1-8>")?;
                    cfg.symmetry = value.parse()?;
                }
                "--iterations" => {
                    let value = args
                        .next()
                        .ok_or("missing iterations value, pass --iterations <u32>")?;
                    cfg.iterations = value.parse()?;
                }
                "--seed" => {
                    let value = args.next().ok_or("missing seed value, pass --seed <u32>")?;
                    cfg.seed = value.parse()?;
                }
                "--fill" => {
                    let value = args.next().ok_or("missing fill value, pass --fill <f32>")?;
                    cfg.fill_scale = value.parse()?;
                }
                "--zoom" => {
                    let value = args.next().ok_or("missing zoom value, pass --zoom <f32>")?;
                    cfg.fractal_zoom = value.parse()?;
                }
                "--fast" => {
                    cfg.fast = true;
                }
                "--layers" => {
                    cfg.layers = Some(
                        args.next()
                            .ok_or("missing layers value, pass --layers <u32>")?
                            .parse()?,
                    );
                }
                "--count" | "-n" => {
                    let value = args
                        .next()
                        .ok_or("missing count value, pass --count <u32>")?;
                    cfg.count = value.parse()?;
                }
                "--output" | "-o" => {
                    cfg.output = args
                        .next()
                        .ok_or("missing output file name, pass --output <path>")?
                        .to_string();
                }
                _ => return Err(format!("unknown argument: {arg}").into()),
            }
        }

        if cfg.width == 0 || cfg.height == 0 {
            return Err("width and height must be greater than zero".into());
        }
        if cfg.symmetry == 0 {
            return Err("symmetry must be at least 1".into());
        }
        if cfg.iterations == 0 {
            return Err("iterations must be at least 1".into());
        }
        if cfg.fill_scale <= 0.0 {
            return Err("fill scale must be greater than 0".into());
        }
        if cfg.fractal_zoom <= 0.0 {
            return Err("zoom must be greater than 0".into());
        }
        if let Some(0) = cfg.layers {
            return Err("layers must be greater than 0".into());
        }
        if cfg.count == 0 {
            return Err("count must be at least 1".into());
        }

        Ok(cfg)
    }
}

fn random_seed() -> u32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_nanos() as u64)
        .unwrap_or(0);
    let mut seed = now as u32 ^ (now >> 32) as u32;
    seed ^= seed << 13;
    seed ^= seed >> 17;
    seed ^= seed << 5;
    seed
}

fn randomize_symmetry(base: u32, rng: &mut XorShift32) -> u32 {
    let spread = (base as f32 * 0.2).round() as u32;
    let low = base.saturating_sub(spread).max(1);
    let high = (base.saturating_add(spread)).max(low);
    let value = low + (rng.next_u32() % (high - low + 1));
    value
}

fn randomize_iterations(base: u32, rng: &mut XorShift32) -> u32 {
    let low = (base as f32 * 0.5).floor().max(140.0) as u32;
    let high = (base as f32 * 2.5).ceil().max(220.0) as u32;
    low + (rng.next_u32() % (high - low + 1))
}

fn randomize_fill_scale(base: f32, rng: &mut XorShift32) -> f32 {
    let jitter = 0.8 + (rng.next_f32() * 0.5);
    (base * jitter).clamp(0.95, 1.75)
}

fn randomize_zoom(base: f32, rng: &mut XorShift32) -> f32 {
    let jitter = 0.3 + (rng.next_f32() * 0.55);
    (base * jitter).clamp(0.20, 0.95)
}

fn pick_art_style(rng: &mut XorShift32) -> ArtStyle {
    ArtStyle::from_u32(rng.next_u32())
}

fn modulate_art_style(base: ArtStyle, rng: &mut XorShift32, fast: bool) -> ArtStyle {
    let roll = rng.next_f32();
    if fast {
        if roll < 0.70 {
            return base;
        }
        if roll < 0.82 {
            return ArtStyle::from_u32(base.as_u32() + 1);
        }
        if roll < 0.94 {
            return ArtStyle::from_u32(base.as_u32() + ArtStyle::total() - 1);
        }
        return pick_art_style(rng);
    }

    if roll < 0.45 {
        return base;
    }
    if roll < 0.70 {
        return ArtStyle::from_u32(base.as_u32() + 1);
    }
    if roll < 0.95 {
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
        2 + (rng.next_u32() % 3)
    } else {
        2 + (rng.next_u32() % 5)
    }
}

fn modulate_symmetry(base: u32, rng: &mut XorShift32, fast: bool) -> u32 {
    let jitter = if fast { 1 } else { 2 };
    if base <= 1 {
        return 1;
    }

    let jitter_range = jitter.min(base - 1);
    let shift = (rng.next_u32() % (jitter_range * 2 + 1) as u32) as i32 - jitter_range as i32;
    ((base as i32 + shift).max(1)) as u32
}

fn modulate_iterations(base: u32, rng: &mut XorShift32, fast: bool) -> u32 {
    let spread = if fast { 0.18 } else { 0.42 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    let value = (base as f32 * factor).max(64.0).round() as u32;
    value.max(1)
}

fn modulate_fill_scale(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.08 } else { 0.20 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    (base * factor).clamp(0.85, 1.7)
}

fn modulate_zoom(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.08 } else { 0.20 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    (base * factor).clamp(0.35, 0.95)
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
    SymmetryStyle::from_u32(rng.next_u32()).as_u32()
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
) -> bool {
    let base: f32 = if fast { 0.34 } else { 0.24 };
    let layer_bias: f32 = if layer_index == 0 { -0.20 } else { 0.12 };
    let threshold = (base + layer_bias).clamp(0.05, 0.8);
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
) -> bool {
    let base: f32 = if fast { 0.42 } else { 0.30 };
    let layer_bias: f32 = if layer_index == 0 { -0.25 } else { 0.00 };
    let threshold = (base + layer_bias).clamp(0.15, 0.95);
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
        max_radius: ((cfg.max_radius / 2).max(1)).min(4),
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
    let invert = (rng.next_u32() % 2) == 0;
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

#[derive(Clone, Copy)]
struct LumaStats {
    min: f32,
    max: f32,
    mean: f32,
    std: f32,
}

fn luma_stats(luma: &[f32]) -> LumaStats {
    let mut min: f32 = 1.0;
    let mut max: f32 = 0.0;
    let mut sum = 0.0;
    for &value in luma {
        min = min.min(value);
        max = max.max(value);
        sum += value;
    }
    let mean = if luma.is_empty() {
        0.0
    } else {
        sum / (luma.len() as f32)
    };

    let mut variance = 0.0;
    for &value in luma {
        let delta = value - mean;
        variance += delta * delta;
    }
    let std = if luma.is_empty() {
        0.0
    } else {
        (variance / (luma.len() as f32)).sqrt()
    };

    LumaStats {
        min,
        max,
        mean,
        std,
    }
}

fn stretch_to_percentile(src: &mut [f32], scratch: &mut [f32], low_pct: f32, high_pct: f32) {
    if src.is_empty() {
        return;
    }

    debug_assert_eq!(src.len(), scratch.len());

    scratch.copy_from_slice(src);
    let len_minus_1 = src.len() - 1;
    let low = (len_minus_1 as f32 * low_pct.clamp(0.0, 1.0)).round() as usize;
    let high = (len_minus_1 as f32 * high_pct.clamp(0.0, 1.0)).round() as usize;
    scratch.select_nth_unstable_by(low, |a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let in_min = scratch[low];
    scratch.select_nth_unstable_by(high, |a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let in_max = scratch[high];
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
    let clamped = strength.max(1.0).min(3.0);
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

        values[..count].sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
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
        let radius = (1 + ((radius_limit * (0.2 + 0.8 * local_blur)).round() as i32))
            .max(1)
            .min(2);

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

fn apply_dynamic_filter(width: u32, height: u32, luma: &[f32], dst: &mut [f32], cfg: BlurConfig) {
    match cfg.mode {
        FilterMode::Motion => apply_motion_blur(width, height, luma, dst, &cfg),
        FilterMode::Gaussian => apply_gaussian_blur(width, height, luma, dst, &cfg),
        FilterMode::Median => apply_median_blur(width, height, luma, dst, &cfg),
        FilterMode::Bilateral => apply_bilateral_blur(width, height, luma, dst, &cfg),
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

fn needs_complexity_fix(stats: LumaStats) -> bool {
    let span = stats.max - stats.min;
    stats.std < 0.12 || span < 0.26
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
    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("fractal shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    };

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .ok_or("no compatible GPU adapter found")?;

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        )
        .await?;

    let shader_module = device.create_shader_module(shader);
    let mut params = Params {
        width: config.width,
        height: config.height,
        symmetry: config.symmetry,
        symmetry_style: 0,
        iterations: config.iterations,
        seed: config.seed,
        fill_scale: config.fill_scale,
        fractal_zoom: config.fractal_zoom,
        art_style: ArtStyle::Hybrid.as_u32(),
        art_style_secondary: ArtStyle::Field.as_u32(),
        art_style_mix: 0.0,
    };

    let output_size =
        (config.width as usize * config.height as usize * std::mem::size_of::<u32>()) as u64;
    let pixel_count = (config.width as usize) * (config.height as usize);
    let mut filtered = vec![0.0f32; pixel_count];
    let mut detail = vec![0.0f32; pixel_count];
    let mut layered = vec![0.0f32; pixel_count];
    let mut final_pixels = vec![0u8; pixel_count];

    let out_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("output storage"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("uniforms"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("staging"),
        size: output_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bind group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: out_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("compute pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: "main",
        compilation_options: wgpu::PipelineCompilationOptions::default(),
    });
    let mut image_rng = XorShift32::new(config.seed);
    let mut luma = vec![0.0f32; pixel_count];
    let mut background = vec![0.0f32; pixel_count];
    let mut percentile = vec![0.0f32; pixel_count];

    let render_layer = |layer_params: &Params, out: &mut [f32]| -> Result<(), Box<dyn Error>> {
        queue.write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(layer_params));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("command encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let work_x = (config.width + 15) / 16;
            let work_y = (config.height + 15) / 16;
            pass.dispatch_workgroups(work_x, work_y, 1);
        }
        encoder.copy_buffer_to_buffer(&out_buffer, 0, &staging_buffer, 0, output_size);
        queue.submit(Some(encoder.finish()));

        let slice = staging_buffer.slice(..);
        let (sender, receiver) = channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).expect("map callback dropped");
        });
        device.poll(wgpu::Maintain::Wait);
        match receiver.recv()? {
            Ok(()) => {}
            Err(err) => return Err(format!("buffer map failed: {err:?}").into()),
        }

        {
            let raw = slice.get_mapped_range();
            decode_luma(&raw, out);
        }
        staging_buffer.unmap();
        Ok(())
    };

    for i in 0..config.count {
        let mut image_seed = image_rng.next_u32();
        if image_seed == 0 {
            image_seed = 0x9e3779b9;
        }
        let base_seed = image_seed;
        let base_symmetry = randomize_symmetry(config.symmetry, &mut image_rng);
        let base_iterations = randomize_iterations(config.iterations, &mut image_rng);
        let base_fill_scale = randomize_fill_scale(config.fill_scale, &mut image_rng);
        let base_symmetry_style = pick_symmetry_style(&mut image_rng);
        let base_zoom = randomize_zoom(config.fractal_zoom, &mut image_rng);
        let layer_count = pick_layer_count(&mut image_rng, config.layers, config.fast);
        let base_art_style = pick_art_style(&mut image_rng);
        let base_art_style_secondary = pick_art_style_secondary(base_art_style, &mut image_rng);
        let base_art_mix = image_rng.next_f32();
        let structural_profile = should_use_structural_profile(config.fast, &mut image_rng);
        let mut layer_steps = Vec::new();

        create_soft_background(
            config.width,
            config.height,
            base_seed ^ (i + 0x0BADC0DEu32),
            &mut background,
        );
        let background_strength = 0.2 + (image_rng.next_f32() * 0.14);
        let mut pre_filter_stats = LumaStats {
            min: 1.0,
            max: 0.0,
            mean: 0.0,
            std: 0.0,
        };
        layered.fill(0.0);

        for layer_index in 0..layer_count {
            let layer_seed = base_seed.wrapping_add((layer_index + 1).wrapping_mul(0x9e3779b9));
            params = Params {
                width: config.width,
                height: config.height,
                symmetry: modulate_symmetry(base_symmetry, &mut image_rng, config.fast),
                symmetry_style: base_symmetry_style,
                iterations: modulate_iterations(base_iterations, &mut image_rng, config.fast),
                seed: layer_seed,
                fill_scale: modulate_fill_scale(base_fill_scale, &mut image_rng, config.fast),
                fractal_zoom: modulate_zoom(base_zoom, &mut image_rng, config.fast),
                art_style: modulate_art_style(base_art_style, &mut image_rng, config.fast).as_u32(),
                art_style_secondary: modulate_art_style(
                    base_art_style_secondary,
                    &mut image_rng,
                    config.fast,
                )
                .as_u32(),
                art_style_mix: modulate_style_mix(base_art_mix, &mut image_rng, config.fast),
            };

            render_layer(&params, &mut luma)?;
            if layer_index == 0 {
                pre_filter_stats = luma_stats(&luma);
            }

            let filter = tune_filter_for_speed(pick_filter_from_rng(&mut image_rng), config.fast);
            let gradient = pick_gradient_from_rng(&mut image_rng);
            let overlay = pick_layer_blend(&mut image_rng);
            let layer_contrast = pick_layer_contrast(&mut image_rng, config.fast);
            let apply_filter = should_apply_dynamic_filter(
                layer_index,
                &mut image_rng,
                config.fast,
                structural_profile,
            );
            let apply_gradient = should_apply_gradient_map(
                layer_index,
                &mut image_rng,
                config.fast,
                structural_profile,
            );
            let opacity = if layer_index == 0 {
                1.0
            } else {
                layer_opacity(&mut image_rng)
            };
            let mut complexity_fixed = false;

            if apply_filter {
                apply_dynamic_filter(config.width, config.height, &luma, &mut filtered, filter);
                let low_stretch = if config.fast { 0.03 } else { 0.04 };
                let high_stretch = if config.fast { 0.97 } else { 0.96 };
                stretch_to_percentile(&mut filtered, &mut percentile, low_stretch, high_stretch);
            } else {
                filtered.copy_from_slice(&luma);
                stretch_to_percentile(
                    &mut filtered,
                    &mut percentile,
                    if config.fast { 0.02 } else { 0.03 },
                    if config.fast { 0.98 } else { 0.97 },
                );
            }

            if apply_gradient {
                apply_gradient_map(&mut filtered, gradient);
                stretch_to_percentile(
                    &mut filtered,
                    &mut percentile,
                    if config.fast { 0.01 } else { 0.02 },
                    if config.fast { 0.99 } else { 0.98 },
                );
            }
            if !apply_filter && !apply_gradient {
                if structural_profile {
                    apply_detail_waves(
                        &mut filtered,
                        config.width,
                        config.height,
                        layer_seed ^ 0x2f7f_8d3d,
                        if config.fast { 0.05 } else { 0.09 },
                    );
                } else if image_rng.next_f32() < 0.35 {
                    apply_detail_waves(
                        &mut filtered,
                        config.width,
                        config.height,
                        layer_seed ^ 0x9d7e_4f2a,
                        if config.fast { 0.04 } else { 0.07 },
                    );
                }

                apply_sharpen(
                    config.width,
                    config.height,
                    &filtered,
                    &mut detail,
                    if structural_profile {
                        if config.fast { 0.72 } else { 1.12 }
                    } else {
                        if config.fast { 0.45 } else { 0.75 }
                    },
                );
                std::mem::swap(&mut filtered, &mut detail);
                apply_posterize_buffer(
                    &mut filtered,
                    2 + (image_rng.next_u32() % if structural_profile { 7 } else { 5 }),
                );
            }
            let layer_contrast = if apply_filter || apply_gradient {
                layer_contrast
            } else {
                layer_contrast * 0.75
            };
            apply_contrast(&mut filtered, layer_contrast.max(1.0));
            let layer_stats = luma_stats(&filtered);
            if needs_complexity_fix(layer_stats) {
                complexity_fixed = true;
                apply_detail_waves(
                    &mut filtered,
                    config.width,
                    config.height,
                    layer_seed ^ 0x4445_6d63,
                    if config.fast { 0.10 } else { 0.18 },
                );
                apply_sharpen(
                    config.width,
                    config.height,
                    &filtered,
                    &mut detail,
                    if config.fast { 0.55 } else { 0.9 },
                );
                std::mem::swap(&mut filtered, &mut detail);
                apply_posterize_buffer(&mut filtered, 2 + (image_rng.next_u32() % 6));
                apply_contrast(&mut filtered, 1.25 + (image_rng.next_f32() * 0.45));
            }

            if layer_index == 0 {
                layered.copy_from_slice(&filtered);
            } else {
                blend_layer_stack(&mut layered, &filtered, opacity, overlay);
            }

            layer_steps.push(format!(
                "L{}:{}({:.2}, f{}, g{}, d{}, c{:.2}) S{}+{}:{:.2}",
                layer_index + 1,
                overlay.label(),
                opacity,
                if apply_filter {
                    filter.mode.label()
                } else {
                    "none"
                },
                if apply_gradient { "on" } else { "off" },
                if complexity_fixed { "on" } else { "off" },
                layer_contrast,
                ArtStyle::from_u32(params.art_style).label(),
                ArtStyle::from_u32(params.art_style_secondary).label(),
                params.art_style_mix,
            ));
        }

        blend_background(&mut layered, &background, background_strength);
        let final_contrast = if config.fast { 1.45 } else { 1.8 };
        apply_contrast(&mut layered, final_contrast);
        stretch_to_percentile(
            &mut layered,
            &mut percentile,
            if config.fast { 0.01 } else { 0.01 },
            if config.fast { 0.99 } else { 0.99 },
        );

        let mut final_stats = luma_stats(&layered);
        let mut final_complexity_fixed = false;
        if needs_complexity_fix(final_stats) {
            final_complexity_fixed = true;
            apply_detail_waves(
                &mut layered,
                config.width,
                config.height,
                base_seed ^ (i + 0x445f_6e65),
                if config.fast { 0.08 } else { 0.14 },
            );
            apply_sharpen(
                config.width,
                config.height,
                &layered,
                &mut detail,
                if config.fast { 0.45 } else { 0.75 },
            );
            std::mem::swap(&mut layered, &mut detail);
            apply_posterize_buffer(&mut layered, if config.fast { 4 } else { 5 });
            apply_contrast(&mut layered, if config.fast { 1.2 } else { 1.4 });
            final_stats = luma_stats(&layered);
        }
        if final_stats.std < 0.06 || (final_stats.max - final_stats.min) < 0.18 {
            inject_noise(
                &mut layered,
                base_seed ^ (i + 1),
                if config.fast { 0.04 } else { 0.06 },
            );
            stretch_to_percentile(
                &mut layered,
                &mut percentile,
                if config.fast { 0.01 } else { 0.01 },
                if config.fast { 0.99 } else { 0.99 },
            );
            final_stats = luma_stats(&layered);
        }

        encode_gray(&mut final_pixels, &layered);
        let final_output = resolve_output_path(&config.output);
        let (final_width, final_height, final_bytes) =
            save_png_under_10mb(&final_output, config.width, config.height, &final_pixels)?;
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
            "Generated {} | index {} | seed {} | fill {:.2} | zoom {:.2} | symmetry {} [{}] | iterations {} | styles {}+{}:{:.2} | final d{} | layers {} | layers [{}] | image {}x{} (scale {} / {:.2}MB) | pre({:.2}-{:.2},{:.2}) post({:.2}-{:.2},{:.2})",
            final_output.display(),
            i,
            base_seed,
            base_fill_scale,
            base_zoom,
            base_symmetry,
            SymmetryStyle::from_u32(base_symmetry_style).label(),
            base_iterations,
            base_art_style.label(),
            base_art_style_secondary.label(),
            base_art_mix,
            if final_complexity_fixed { "on" } else { "off" },
            layer_count,
            layer_summary,
            final_width,
            final_height,
            scale,
            final_bytes as f64 / (1024.0 * 1024.0),
            pre_filter_stats.min,
            pre_filter_stats.max,
            pre_filter_stats.mean,
            final_stats.min,
            final_stats.max,
            final_stats.mean
        );
    }
    Ok(())
}

#![cfg_attr(target_arch = "spirv", no_std)]

use spirv_std::{
    glam::{UVec3, Vec2},
    num_traits::Float,
    spirv,
};

/// Uniform payload for fractal layer generation.
#[repr(C)]
pub struct Params {
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

/// Clamp to normalized grayscale range.
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// Return the larger of two `u32` values.
fn max_u32(a: u32, b: u32) -> u32 {
    if a > b {
        a
    } else {
        b
    }
}

/// Return the smaller of two `u32` values.
fn min_u32(a: u32, b: u32) -> u32 {
    if a < b {
        a
    } else {
        b
    }
}

/// Clamp `value` into inclusive `[lo, hi]`.
fn clamp_u32(value: u32, lo: u32, hi: u32) -> u32 {
    if value < lo {
        lo
    } else if value > hi {
        hi
    } else {
        value
    }
}

/// Fractional-part helper.
fn fract(value: f32) -> f32 {
    value - value.floor()
}

/// Deterministic scalar hash used for coordinate jitter.
fn hash01(x: f32, y: f32, seed: u32) -> f32 {
    let s = seed as f32 * 0.000_000_119_209_29;
    fract(((x * 12.9898 + y * 78.233 + s).sin()) * 43_758.547)
}

/// Convert normalized grayscale value to packed `0xFFRRGGBB`.
fn pack_gray(level: f32) -> u32 {
    let c = (clamp01(level) * 255.0 + 0.5) as u32;
    (255 << 24) | (c << 16) | (c << 8) | c
}

/// Apply domain warp before symmetry folding and style sampling.
fn apply_domain_warp(p: Vec2, params: &Params) -> Vec2 {
    let seed = params.seed as f32 * 0.000_000_119_209_29;
    let bend = params.bend_strength.clamp(0.0, 1.8);
    let warp = params.warp_strength.clamp(0.0, 1.8);
    let freq = params.warp_frequency.clamp(0.25, 6.0);

    let radius = p.length().max(0.0001);
    let angle = p.y.atan2(p.x);
    let radial = 1.0
        + (bend * (0.16 + 0.05 * freq) * (radius * (2.8 + freq) + seed + angle * 0.75).cos())
        + (warp * (0.10 + 0.03 * freq) * (radius * 4.0 + angle + seed).sin());
    let angular = bend * (0.32 + 0.07 * freq) * (angle * (2.2 + freq) + seed * 2.0).sin();
    let warped_angle = angle + angular;
    let warped_radius = radius * radial;

    let mut q = Vec2::new(
        warped_radius * warped_angle.cos(),
        warped_radius * warped_angle.sin(),
    );
    let wave = ((q.x * (4.0 + 0.9 * freq) + seed * 7.0).sin() + 1.0) * 0.5;
    let wave2 = ((q.y * (3.8 + 0.6 * freq) + seed * 5.0).cos() + 1.0) * 0.5;
    q.x += (wave - 0.5) * (0.5 * warp);
    q.y += (wave2 - 0.5) * (0.5 * warp);

    let swirl = (q.x + q.y + seed * 13.0).sin() * (bend * 0.08);
    Vec2::new(
        q.x * swirl.cos() - q.y * swirl.sin(),
        q.x * swirl.sin() + q.y * swirl.cos(),
    )
}

/// Fold coordinates according to configured symmetry style.
fn fold_for_symmetry(mut p: Vec2, symmetry: u32, style: u32) -> Vec2 {
    if symmetry <= 1 {
        return p;
    }

    match style {
        2 => {
            p.x = p.x.abs();
            p.y = p.y.abs();
        }
        3 => p.x = p.x.abs(),
        4 => p.y = p.y.abs(),
        5 => {
            let c = 0.707_106_77;
            let s = 0.707_106_77;
            let rx = p.x * c + p.y * s;
            let ry = -p.x * s + p.y * c;
            p = Vec2::new(rx.abs(), ry.abs());
            let ux = p.x * c - p.y * s;
            let uy = p.x * s + p.y * c;
            p = Vec2::new(ux, uy);
        }
        6 => {
            p.x = p.x.abs();
            p.y = p.y.abs();
            if p.y > p.x {
                p = Vec2::new(p.y, p.x);
            }
        }
        7 => {
            let repeats = (symmetry as f32 * 0.55).clamp(1.2, 3.5);
            let gx = fract(p.x * repeats + 0.5) - 0.5;
            let gy = fract(p.y * repeats + 0.5) - 0.5;
            p = Vec2::new(gx.abs() * 2.0 - 0.5, gy.abs() * 2.0 - 0.5) * (2.0 / repeats);
        }
        _ => {
            let angle = p.y.atan2(p.x);
            let radius = p.length();
            let sector = core::f32::consts::TAU / max_u32(symmetry, 2) as f32;
            let folded = fract(angle / sector + 0.5) - 0.5;
            let folded_angle = folded * sector;
            p = Vec2::new(radius * folded_angle.cos(), radius * folded_angle.sin());
        }
    }
    p
}

/// Mandelbrot-style escape metric.
fn style_mandelbrot(mut p: Vec2, params: &Params, layer: u32) -> f32 {
    let jitter = hash01(p.x * 2.1, p.y * 3.7, params.seed.wrapping_add(layer));
    let c = Vec2::new(p.x + (jitter - 0.5) * 0.22, p.y - (jitter - 0.5) * 0.22);
    p *= 1.6 + layer as f32 * 0.035;

    let max_iter = max_u32(min_u32(params.iterations, 240), 1);
    let mut i = 0;
    while i < max_iter {
        if p.dot(p) > 4.0 {
            break;
        }
        p = Vec2::new(p.x * p.x - p.y * p.y, 2.0 * p.x * p.y) + c;
        i += 1;
    }
    1.0 - i as f32 / max_iter as f32
}

/// Julia-style field with deterministic seed-derived constants.
fn style_julia(mut p: Vec2, params: &Params, layer: u32) -> f32 {
    let base = params.seed.wrapping_add(layer.wrapping_mul(0x9E37_79B9));
    let c = Vec2::new(
        hash01(1.3, 4.7, base) * 1.6 - 0.8,
        hash01(8.5, 2.9, base ^ 0xA5A5_5A5A) * 1.6 - 0.8,
    );
    p *= 1.8;

    let max_iter = max_u32(min_u32(params.iterations, 220), 1);
    let mut i = 0;
    while i < max_iter {
        if p.dot(p) > 6.0 {
            break;
        }
        p = Vec2::new(p.x * p.x - p.y * p.y, 2.0 * p.x * p.y) + c;
        i += 1;
    }
    1.0 - i as f32 / max_iter as f32
}

/// Burning-ship style variant for sharper ridge structure.
fn style_burning_ship(mut p: Vec2, params: &Params, layer: u32) -> f32 {
    let c = p * (1.25 + layer as f32 * 0.025);
    let max_iter = max_u32(min_u32(params.iterations, 220), 1);
    let mut i = 0;
    while i < max_iter {
        if p.dot(p) > 10.0 {
            break;
        }
        p = Vec2::new(p.x.abs(), p.y.abs());
        p = Vec2::new(p.x * p.x - p.y * p.y, 2.0 * p.x * p.y) + c;
        i += 1;
    }
    1.0 - i as f32 / max_iter as f32
}

/// Trig/noise field style used for non-escape-map families.
fn style_field(p: Vec2, params: &Params, layer: u32) -> f32 {
    let s = params.seed as f32 * 0.001 + layer as f32 * 0.13;
    let a = (p.x * (3.1 + s) + p.y * (2.2 - s)).sin();
    let b = (p.y * (4.2 + s * 0.7) - p.x * (1.7 + s * 0.3)).cos();
    let h = hash01(p.x * 6.1 + s, p.y * 5.9 - s, params.seed ^ layer);
    clamp01(((a * 0.5 + 0.5) * 0.45) + ((b * 0.5 + 0.5) * 0.45) + h * 0.1)
}

/// Multi-wave style for periodic families.
fn style_wave(p: Vec2, params: &Params, layer: u32) -> f32 {
    let t = params.tile_phase + layer as f32 * 0.17;
    let k = params.tile_scale.clamp(0.25, 2.5);
    let v = (p.x * k * 4.0 + t * 6.0).sin() * (p.y * k * 3.0 - t * 4.0).cos();
    let w = (p.length() * (5.0 + k) + t * 3.0).sin();
    clamp01(v * 0.35 + w * 0.35 + 0.5)
}

/// Dispatch art style selector to one of the shader style families.
fn style_value(p: Vec2, params: &Params, layer: u32, style_selector: u32) -> f32 {
    match style_selector % 17 {
        0 | 7 => style_mandelbrot(p, params, layer),
        1 | 8 => style_julia(p, params, layer),
        2 | 3 | 4 => style_burning_ship(p, params, layer),
        5 | 6 | 9 | 10 | 16 => style_field(p, params, layer),
        11 | 12 | 13 | 14 | 15 => style_wave(p, params, layer),
        _ => style_field(p, params, layer),
    }
}

/// Blend primary and secondary style families for one layer sample.
fn layer_value(p: Vec2, params: &Params, layer: u32) -> f32 {
    let primary = style_value(p, params, layer, params.art_style);
    let secondary = style_value(p, params, layer, params.art_style_secondary);
    let mix = params.art_style_mix.clamp(0.0, 1.0);
    ((1.0 - mix) * primary) + (mix * secondary)
}

/// Generate one grayscale layer in packed `u32` output.
#[spirv(compute(threads(16, 16, 1)))]
pub fn main(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] out_pixels: &mut [u32],
    #[spirv(uniform, descriptor_set = 0, binding = 1)] params: &Params,
) {
    if id.x >= params.width || id.y >= params.height {
        return;
    }

    let mut p = Vec2::new(
        ((id.x as f32 + 0.5) / max_u32(params.width, 1) as f32) * 2.0 - 1.0,
        ((id.y as f32 + 0.5) / max_u32(params.height, 1) as f32) * 2.0 - 1.0,
    );
    let zoom = params.fractal_zoom.max(0.2);
    p *= params.fill_scale * zoom;
    p = apply_domain_warp(p, params);
    p += Vec2::new(params.center_x, params.center_y);

    if params.symmetry > 1 {
        p = fold_for_symmetry(p, params.symmetry, params.symmetry_style);
    }

    let swirl = hash01(p.x * 11.0, p.y * 13.0, params.seed);
    let sample = Vec2::new(p.x + (swirl - 0.5) * 0.08, p.y + (swirl - 0.5) * 0.08);

    let layer_count = clamp_u32(params.layer_count, 1, 14);
    let mut value = 0.0;
    let mut layer = 0;
    while layer < layer_count {
        let layer_brightness = layer_value(sample, params, layer);
        let layer_factor = layer as f32 / layer_count as f32;
        let weight = (1.0 - layer_factor).powf(1.2) * 1.15;
        value += layer_brightness * layer_brightness * weight;
        layer += 1;
    }

    let index = (id.x + id.y * params.width) as usize;
    out_pixels[index] = pack_gray(value);
}

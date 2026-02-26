#![cfg_attr(target_arch = "spirv", no_std)]

use spirv_std::{glam::UVec3, num_traits::Float, spirv};

/// Uniform payload shared by all graph-op compute kernels.
#[repr(C)]
pub struct GraphOpUniforms {
    width: u32,
    height: u32,
    mode: u32,
    flags: u32,
    seed: u32,
    octaves: u32,
    _pad0: u32,
    _pad1: u32,
    p0: f32,
    p1: f32,
    p2: f32,
    p3: f32,
    p4: f32,
    p5: f32,
    p6: f32,
    p7: f32,
    p8: f32,
    p9: f32,
    p10: f32,
    p11: f32,
    p12: f32,
    p13: f32,
    p14: f32,
    p15: f32,
}

/// Clamp a scalar to normalized grayscale range.
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

/// Apply contrast around midpoint while preserving output bounds.
fn apply_contrast(value: f32, contrast: f32) -> f32 {
    clamp01(((value - 0.5) * contrast.max(1.0)) + 0.5)
}

/// Convert 2D coordinates into a flat pixel index.
fn idx(cfg: &GraphOpUniforms, x: u32, y: u32) -> usize {
    (x + y * cfg.width) as usize
}

/// Smooth interpolation curve for value-noise blending.
fn smoothstep01(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Fractional part helper valid in `no_std` shader code.
fn fract(value: f32) -> f32 {
    value - value.floor()
}

/// Integer hash that maps a lattice coordinate into [0, 1].
fn hash_to_unit(x: i32, y: i32, seed: u32) -> f32 {
    let mut v =
        seed ^ ((x as u32).wrapping_mul(0x27D4_EB2D)) ^ ((y as u32).wrapping_mul(0x85EB_CA77));
    v ^= v >> 15;
    v = v.wrapping_mul(0x2C1B_3C6D);
    v ^= v >> 12;
    v = v.wrapping_mul(0x297A_2D39);
    v ^= v >> 15;
    v as f32 / 4_294_967_295.0
}

/// Deterministic smoothed value noise used by source nodes.
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

    let ix0 = n00 + ((n10 - n00) * sx);
    let ix1 = n01 + ((n11 - n01) * sx);
    clamp01(ix0 + ((ix1 - ix0) * sy))
}

/// Smoothstep with explicit degenerate-range behavior.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        if x >= edge0 {
            return 1.0;
        }
        return 0.0;
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    smoothstep01(t)
}

/// Blend two grayscale values using the configured mode index.
fn blend_mode(base: f32, top: f32, mode: u32) -> f32 {
    match mode {
        0 => top,
        1 => clamp01(base + top),
        2 => clamp01(base * top),
        3 => 1.0 - ((1.0 - base) * (1.0 - top)),
        4 => {
            if base < 0.5 {
                2.0 * base * top
            } else {
                1.0 - (2.0 * (1.0 - base) * (1.0 - top))
            }
        }
        5 => (base - top).abs(),
        6 => base.max(top),
        7 => base.min(top),
        8 => base + ((1.0 - base) * (top * top)),
        _ => base * top,
    }
}

/// Read from src0 with edge clamping.
fn sample_clamped_src0(src0_f32: &[f32], cfg: &GraphOpUniforms, x: i32, y: i32) -> f32 {
    let max_x = max_u32(cfg.width, 1) as i32 - 1;
    let max_y = max_u32(cfg.height, 1) as i32 - 1;
    let cx = x.clamp(0, max_x) as u32;
    let cy = y.clamp(0, max_y) as u32;
    src0_f32[idx(cfg, cx, cy)]
}

/// Bilinear sampling from src0 used by warp transform nodes.
fn sample_bilinear_src0(src0_f32: &[f32], cfg: &GraphOpUniforms, x: f32, y: f32) -> f32 {
    let x0 = x.floor();
    let y0 = y.floor();
    let x1 = x0 + 1.0;
    let y1 = y0 + 1.0;
    let tx = fract(x);
    let ty = fract(y);

    let p00 = sample_clamped_src0(src0_f32, cfg, x0 as i32, y0 as i32);
    let p10 = sample_clamped_src0(src0_f32, cfg, x1 as i32, y0 as i32);
    let p01 = sample_clamped_src0(src0_f32, cfg, x0 as i32, y1 as i32);
    let p11 = sample_clamped_src0(src0_f32, cfg, x1 as i32, y1 as i32);

    let top = p00 + ((p10 - p00) * tx);
    let bottom = p01 + ((p11 - p01) * tx);
    clamp01(top + ((bottom - top) * ty))
}

/// Copy one luma buffer into another.
#[spirv(compute(threads(16, 16, 1)))]
pub fn copy_luma(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] src0_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] _src1_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] _src2_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] dst_f32: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 4)] cfg: &GraphOpUniforms,
) {
    if id.x >= cfg.width || id.y >= cfg.height {
        return;
    }
    let i = idx(cfg, id.x, id.y);
    dst_f32[i] = src0_f32[i];
}

/// Generate deterministic procedural source noise.
#[spirv(compute(threads(16, 16, 1)))]
pub fn source_noise(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] _src0_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] _src1_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] _src2_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] dst_f32: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 4)] cfg: &GraphOpUniforms,
) {
    if id.x >= cfg.width || id.y >= cfg.height {
        return;
    }

    let width_f = max_u32(cfg.width, 1) as f32;
    let height_f = max_u32(cfg.height, 1) as f32;
    let scale = cfg.p0.max(0.001);
    let nx = (id.x as f32 / width_f) * scale;
    let ny = (id.y as f32 / height_f) * scale;
    let octave_count = clamp_u32(cfg.octaves, 1, 8);

    let mut sum = 0.0;
    let mut norm = 0.0;
    let mut frequency = 1.0;
    let mut octave_amp = 1.0;
    let mut octave = 0;
    while octave < octave_count {
        let sample_seed = cfg.seed.wrapping_add(octave.wrapping_mul(0x9E37_79B9));
        let noise = value_noise_2d(nx * frequency, ny * frequency, sample_seed);
        sum += noise * octave_amp;
        norm += octave_amp;
        frequency *= 2.0;
        octave_amp *= 0.5;
        octave += 1;
    }

    let normalized = if norm > 0.0 { sum / norm } else { 0.0 };
    dst_f32[idx(cfg, id.x, id.y)] = clamp01(normalized * cfg.p1.clamp(0.0, 2.0));
}

/// Build a binary/soft mask from one luma source.
#[spirv(compute(threads(16, 16, 1)))]
pub fn build_mask(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] src0_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] _src1_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] _src2_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] dst_f32: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 4)] cfg: &GraphOpUniforms,
) {
    if id.x >= cfg.width || id.y >= cfg.height {
        return;
    }

    let source = src0_f32[idx(cfg, id.x, id.y)];
    let threshold = cfg.p0.clamp(0.0, 1.0);
    let softness = cfg.p1.max(0.0);
    let half = softness * 0.5;
    let edge_min = threshold - half;
    let edge_max = threshold + half;

    let mut value = if softness <= 0.000_001 {
        if source >= threshold {
            1.0
        } else {
            0.0
        }
    } else {
        smoothstep(edge_min, edge_max, source)
    };

    if (cfg.flags & 0x2) != 0 {
        value = 1.0 - value;
    }
    dst_f32[idx(cfg, id.x, id.y)] = clamp01(value);
}

/// Blend base/top sources with optional mask.
#[spirv(compute(threads(16, 16, 1)))]
pub fn blend_luma(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] src0_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] src1_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] src2_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] dst_f32: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 4)] cfg: &GraphOpUniforms,
) {
    if id.x >= cfg.width || id.y >= cfg.height {
        return;
    }

    let i = idx(cfg, id.x, id.y);
    let base = src0_f32[i];
    let top = src1_f32[i];
    let mixed = blend_mode(base, top, cfg.mode % 10);
    let mask = if (cfg.flags & 0x1) != 0 {
        src2_f32[i].clamp(0.0, 1.0)
    } else {
        1.0
    };
    let alpha = cfg.p0.clamp(0.0, 1.0) * mask;
    dst_f32[i] = clamp01(((1.0 - alpha) * base) + (alpha * mixed));
}

/// Blend current-frame input with persistent prior-frame feedback state.
#[spirv(compute(threads(16, 16, 1)))]
pub fn feedback_mix(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] src0_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] src1_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] _src2_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] dst_f32: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 4)] cfg: &GraphOpUniforms,
) {
    if id.x >= cfg.width || id.y >= cfg.height {
        return;
    }

    let i = idx(cfg, id.x, id.y);
    let current = src0_f32[i];
    let prior = src1_f32[i];
    let mix = cfg.p0.clamp(0.0, 1.0);
    dst_f32[i] = clamp01(((1.0 - mix) * current) + (mix * prior));
}

/// Circle primitive sampling in camera-normalized space.
fn sample_circle_camera(radius: f32, _feather: f32, center_x: f32, center_y: f32, x: f32, y: f32) -> f32 {
    let dx = x - center_x;
    let dy = y - center_y;
    let distance = (dx * dx + dy * dy).sqrt();
    let r = radius.max(0.01);
    if distance <= r { 1.0 } else { 0.0 }
}

/// Sphere primitive sampling with simple diffuse shading in camera-normalized space.
fn sample_sphere_camera(
    radius: f32,
    center_x: f32,
    center_y: f32,
    light_x: f32,
    light_y: f32,
    ambient: f32,
    deform: f32,
    deform_freq: f32,
    deform_phase: f32,
    x: f32,
    y: f32,
) -> f32 {
    let dx = x - center_x;
    let dy = y - center_y;
    let r = radius.max(0.01);
    let angle = dy.atan2(dx);
    let band = (dy / r).clamp(-1.0, 1.0);
    let lump = (angle * deform_freq + deform_phase).sin()
        * (band * deform_freq * 0.7 + deform_phase * 0.6).cos();
    let local_r = (r * (1.0 + deform.clamp(0.0, 1.0) * 0.34 * lump)).clamp(r * 0.55, r * 1.45);
    let rr = local_r * local_r;
    let dist2 = dx * dx + dy * dy;
    if dist2 > rr {
        return 0.0;
    }

    let z = (rr - dist2).sqrt();
    let inv_r = 1.0 / local_r.max(1e-5);
    let nx = dx * inv_r;
    let ny = dy * inv_r;
    let nz = z * inv_r;

    let mut lx = light_x;
    let mut ly = light_y;
    let mut lz = 1.0;
    let len = (lx * lx + ly * ly + lz * lz).sqrt().max(1e-6);
    lx /= len;
    ly /= len;
    lz /= len;

    let diffuse = (nx * lx + ny * ly + nz * lz).max(0.0);
    let a = ambient.clamp(0.0, 1.0);
    clamp01(a + (1.0 - a) * diffuse)
}

/// Render SOP primitive + camera parameters directly on GPU into luma output.
#[spirv(compute(threads(16, 16, 1)))]
pub fn top_camera_render(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] _src0_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] _src1_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] _src2_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] dst_f32: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 4)] cfg: &GraphOpUniforms,
) {
    if id.x >= cfg.width || id.y >= cfg.height {
        return;
    }

    let width_f = max_u32(cfg.width, 1) as f32;
    let height_f = max_u32(cfg.height, 1) as f32;
    let modulation = cfg.p6.clamp(0.2, 3.0);
    let zoom = (cfg.p2 * modulation).clamp(0.2, 4.0);
    let cos_r = cfg.p5.cos();
    let sin_r = cfg.p5.sin();
    let ux = id.x as f32 / width_f - 0.5;
    let uy = id.y as f32 / height_f - 0.5;
    let px = (ux - cfg.p3) / zoom;
    let py = (uy - cfg.p4) / zoom;
    let rx = px * cos_r - py * sin_r;
    let ry = px * sin_r + py * cos_r;

    let mut value = if cfg.mode == 0 {
        sample_circle_camera(cfg.p7, cfg.p8, cfg.p9, cfg.p10, rx, ry)
    } else {
        let ambient = f32::from_bits(cfg.octaves);
        sample_sphere_camera(
            cfg.p7,
            cfg.p8,
            cfg.p9,
            cfg.p10,
            cfg.p11,
            ambient,
            cfg.p12,
            cfg.p13.max(0.8),
            cfg.p14,
            rx,
            ry,
        )
    };

    value = (value * cfg.p0.max(0.0))
        .max(0.0)
        .powf(1.0 / cfg.p1.max(0.2));
    if (cfg.flags & 0x1) != 0 {
        value = 1.0 - value;
    }
    dst_f32[idx(cfg, id.x, id.y)] = clamp01(value);
}

/// Apply contrast and percentile stretch.
#[spirv(compute(threads(16, 16, 1)))]
pub fn tone_map(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] src0_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] _src1_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] _src2_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] dst_f32: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 4)] cfg: &GraphOpUniforms,
) {
    if id.x >= cfg.width || id.y >= cfg.height {
        return;
    }

    let i = idx(cfg, id.x, id.y);
    let contrasted = apply_contrast(src0_f32[i], cfg.p0);
    let low = cfg.p1.clamp(0.0, 1.0);
    let high = cfg.p2.clamp(0.0, 1.0).max(low + (1.0 / 255.0));
    dst_f32[i] = clamp01((contrasted - low) / (high - low));
}

/// Apply coordinate warp with bilinear resampling.
#[spirv(compute(threads(16, 16, 1)))]
pub fn warp_luma(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] src0_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] _src1_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] _src2_f32: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] dst_f32: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 4)] cfg: &GraphOpUniforms,
) {
    if id.x >= cfg.width || id.y >= cfg.height {
        return;
    }

    let width_f = max_u32(cfg.width, 1) as f32;
    let height_f = max_u32(cfg.height, 1) as f32;
    let x = id.x as f32;
    let y = id.y as f32;
    let u = x / width_f;
    let v = y / height_f;
    let strength = cfg.p0.clamp(0.0, 2.0) * 0.02;
    let frequency = cfg.p1.max(0.01);
    let phase = cfg.p2;

    let dx = ((v * frequency + phase) * core::f32::consts::TAU).sin() * strength;
    let dy = ((u * frequency * 0.87 + phase * 1.13) * core::f32::consts::TAU).cos() * strength;
    let sample_x = x + (dx * width_f);
    let sample_y = y + (dy * height_f);

    dst_f32[idx(cfg, id.x, id.y)] = sample_bilinear_src0(src0_f32, cfg, sample_x, sample_y);
}

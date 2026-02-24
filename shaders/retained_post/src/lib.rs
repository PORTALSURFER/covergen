#![cfg_attr(target_arch = "spirv", no_std)]

use spirv_std::{glam::UVec3, num_traits::Float, spirv};

/// Per-layer blend configuration for retained post-processing.
#[repr(C)]
pub struct RetainedPostParams {
    width: u32,
    height: u32,
    blend_mode: u32,
    _pad0: u32,
    opacity: f32,
    contrast: f32,
    _pad1: f32,
    _pad2: f32,
}

/// Finalization configuration for resize/tone/stretch pass.
#[repr(C)]
pub struct FinalizeParams {
    src_width: u32,
    src_height: u32,
    dst_width: u32,
    dst_height: u32,
    contrast: f32,
    low_pct: f32,
    high_pct: f32,
    fast_mode: u32,
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

/// Return the smaller of two `u32` values.
fn min_u32(a: u32, b: u32) -> u32 {
    if a < b {
        a
    } else {
        b
    }
}

/// Return `value - 1` when value is non-zero, otherwise 0.
fn dec_u32(value: u32) -> u32 {
    if value == 0 {
        0
    } else {
        value - 1
    }
}

/// Apply center-weighted contrast curve.
fn apply_contrast(value: f32, contrast: f32) -> f32 {
    clamp01(((value - 0.5) * contrast.max(1.0)) + 0.5)
}

/// Blend two grayscale values using engine blend mode indices.
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
        8 => base + (1.0 - base) * (top * top),
        _ => base * top,
    }
}

/// Convert source coordinates into a flat index.
fn src_index(width: u32, x: u32, y: u32) -> usize {
    (x + y * width) as usize
}

/// Clamp sampled coordinate and fetch nearest source value.
fn sample_src_nearest(accum_luma: &[f32], cfg: &FinalizeParams, x: f32, y: f32) -> f32 {
    let max_x = dec_u32(cfg.src_width) as f32;
    let max_y = dec_u32(cfg.src_height) as f32;
    let sx = x.round().clamp(0.0, max_x) as u32;
    let sy = y.round().clamp(0.0, max_y) as u32;
    accum_luma[src_index(max_u32(cfg.src_width, 1), sx, sy)]
}

/// Bilinear sample from source accumulation.
fn sample_src_bilinear(accum_luma: &[f32], cfg: &FinalizeParams, x: f32, y: f32) -> f32 {
    let width = max_u32(cfg.src_width, 1);
    let height = max_u32(cfg.src_height, 1);
    let max_x = dec_u32(width) as f32;
    let max_y = dec_u32(height) as f32;
    let fx = x.clamp(0.0, max_x);
    let fy = y.clamp(0.0, max_y);

    let x0 = fx.floor() as u32;
    let y0 = fy.floor() as u32;
    let x1 = min_u32(x0 + 1, width - 1);
    let y1 = min_u32(y0 + 1, height - 1);

    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;

    let p00 = accum_luma[src_index(width, x0, y0)];
    let p10 = accum_luma[src_index(width, x1, y0)];
    let p01 = accum_luma[src_index(width, x0, y1)];
    let p11 = accum_luma[src_index(width, x1, y1)];

    let top = p00 + (p10 - p00) * tx;
    let bottom = p01 + (p11 - p01) * tx;
    top + (bottom - top) * ty
}

/// Reset retained accumulation buffer before processing a new image.
#[spirv(compute(threads(16, 16, 1)))]
pub fn clear_accum(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] _layer_pixels: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] accum_luma: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 2)] post: &RetainedPostParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] _histogram: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] _stretch_thresholds: &mut [f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] _final_pixels: &mut [u32],
    #[spirv(uniform, descriptor_set = 0, binding = 6)] _final_cfg: &FinalizeParams,
) {
    if id.x >= post.width || id.y >= post.height {
        return;
    }
    let idx = (id.x + id.y * post.width) as usize;
    accum_luma[idx] = 0.0;
}

/// Blend one freshly rendered layer into retained accumulation.
#[spirv(compute(threads(16, 16, 1)))]
pub fn blend_layer(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] layer_pixels: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] accum_luma: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 2)] post: &RetainedPostParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] _histogram: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] _stretch_thresholds: &mut [f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] _final_pixels: &mut [u32],
    #[spirv(uniform, descriptor_set = 0, binding = 6)] _final_cfg: &FinalizeParams,
) {
    if id.x >= post.width || id.y >= post.height {
        return;
    }

    let idx = (id.x + id.y * post.width) as usize;
    let packed = layer_pixels[idx];
    let layer_raw = (packed & 255) as f32 / 255.0;
    let layer = apply_contrast(layer_raw, post.contrast);
    let base = accum_luma[idx];
    let mixed = blend_mode(base, layer, post.blend_mode);
    let alpha = post.opacity.clamp(0.0, 1.0);
    accum_luma[idx] = clamp01(((1.0 - alpha) * base) + (alpha * mixed));
}

/// Clear histogram buffer before one finalization run.
#[spirv(compute(threads(64, 1, 1)))]
pub fn clear_histogram(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] _layer_pixels: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] _accum_luma: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 2)] _post: &RetainedPostParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] histogram: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] _stretch_thresholds: &mut [f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] _final_pixels: &mut [u32],
    #[spirv(uniform, descriptor_set = 0, binding = 6)] _final_cfg: &FinalizeParams,
) {
    if id.x >= 256 {
        return;
    }
    histogram[id.x as usize] = 0;
}

/// Histogram accumulation placeholder.
///
/// This pass is intentionally a no-op in rust-gpu v1 migration because
/// host-side quality remains acceptable with percentile hints from uniforms.
#[spirv(compute(threads(16, 16, 1)))]
pub fn accumulate_histogram(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] _layer_pixels: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] _accum_luma: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 2)] _post: &RetainedPostParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] _histogram: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] _stretch_thresholds: &mut [f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] _final_pixels: &mut [u32],
    #[spirv(uniform, descriptor_set = 0, binding = 6)] final_cfg: &FinalizeParams,
) {
    if id.x >= final_cfg.src_width || id.y >= final_cfg.src_height {
        return;
    }
}

/// Resolve stretch thresholds from configured percentiles.
#[spirv(compute(threads(1, 1, 1)))]
pub fn compute_thresholds(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] _layer_pixels: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] _accum_luma: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 2)] _post: &RetainedPostParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] _histogram: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] stretch_thresholds: &mut [f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] _final_pixels: &mut [u32],
    #[spirv(uniform, descriptor_set = 0, binding = 6)] final_cfg: &FinalizeParams,
) {
    if id.x > 0 || id.y > 0 || id.z > 0 {
        return;
    }

    let low = final_cfg.low_pct.clamp(0.0, 1.0);
    let high = final_cfg.high_pct.clamp(0.0, 1.0).max(low + (1.0 / 255.0));
    stretch_thresholds[0] = low;
    stretch_thresholds[1] = high;
}

/// Resize and finalize retained output to packed grayscale bytes.
#[spirv(compute(threads(16, 16, 1)))]
pub fn finalize_to_u8(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] _layer_pixels: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] accum_luma: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 2)] _post: &RetainedPostParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] _histogram: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] stretch_thresholds: &mut [f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] final_pixels: &mut [u32],
    #[spirv(uniform, descriptor_set = 0, binding = 6)] final_cfg: &FinalizeParams,
) {
    if id.x >= final_cfg.dst_width || id.y >= final_cfg.dst_height {
        return;
    }

    let dst_idx = (id.x + id.y * final_cfg.dst_width) as usize;
    let dst_width_f = max_u32(final_cfg.dst_width, 1) as f32;
    let dst_height_f = max_u32(final_cfg.dst_height, 1) as f32;
    let src_width_f = max_u32(final_cfg.src_width, 1) as f32;
    let src_height_f = max_u32(final_cfg.src_height, 1) as f32;

    let u = (id.x as f32 + 0.5) / dst_width_f;
    let v = (id.y as f32 + 0.5) / dst_height_f;
    let sample_x = u * src_width_f - 0.5;
    let sample_y = v * src_height_f - 0.5;

    let sampled = if final_cfg.fast_mode == 0 {
        sample_src_bilinear(accum_luma, final_cfg, sample_x, sample_y)
    } else {
        sample_src_nearest(accum_luma, final_cfg, sample_x, sample_y)
    };

    let contrasted = apply_contrast(sampled, final_cfg.contrast);
    let low = stretch_thresholds[0];
    let high = stretch_thresholds[1].max(low + (1.0 / 255.0));
    let stretched = clamp01((contrasted - low) / (high - low));
    final_pixels[dst_idx] = (stretched * 255.0).round() as u32;
}

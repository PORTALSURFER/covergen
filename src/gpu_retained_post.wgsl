struct RetainedPostParams {
    width: u32,
    height: u32,
    blend_mode: u32,
    _pad0: u32,
    opacity: f32,
    contrast: f32,
    _pad1: vec2<f32>,
}

struct FinalizeParams {
    src_width: u32,
    src_height: u32,
    dst_width: u32,
    dst_height: u32,
    contrast: f32,
    low_pct: f32,
    high_pct: f32,
    fast_mode: u32,
}

@group(0) @binding(0)
var<storage, read> layer_pixels: array<u32>;

@group(0) @binding(1)
var<storage, read_write> accum_luma: array<f32>;

@group(0) @binding(2)
var<uniform> post: RetainedPostParams;

@group(0) @binding(3)
var<storage, read_write> histogram: array<atomic<u32>, 256>;

@group(0) @binding(4)
var<storage, read_write> stretch_thresholds: array<f32, 2>;

@group(0) @binding(5)
var<storage, read_write> final_pixels: array<u32>;

@group(0) @binding(6)
var<uniform> final_cfg: FinalizeParams;

fn clamp01(value: f32) -> f32 {
    return clamp(value, 0.0, 1.0);
}

fn apply_contrast_value(value: f32, contrast: f32) -> f32 {
    return clamp01(((value - 0.5) * max(contrast, 1.0)) + 0.5);
}

fn blend_mode(base: f32, top: f32, mode: u32) -> f32 {
    switch mode {
        case 0u: {
            return top;
        }
        case 1u: {
            return clamp01(base + top);
        }
        case 2u: {
            return clamp01(base * top);
        }
        case 3u: {
            return 1.0 - ((1.0 - base) * (1.0 - top));
        }
        case 4u: {
            if (base < 0.5) {
                return 2.0 * base * top;
            }
            return 1.0 - (2.0 * (1.0 - base) * (1.0 - top));
        }
        case 5u: {
            return abs(base - top);
        }
        case 6u: {
            return max(base, top);
        }
        case 7u: {
            return min(base, top);
        }
        case 8u: {
            return base + (1.0 - base) * (top * top);
        }
        default: {
            return base * top;
        }
    }
}

fn src_index(x: u32, y: u32) -> u32 {
    return x + y * final_cfg.src_width;
}

fn sample_src_nearest(x: f32, y: f32) -> f32 {
    let sx = u32(clamp(round(x), 0.0, f32(final_cfg.src_width - 1u)));
    let sy = u32(clamp(round(y), 0.0, f32(final_cfg.src_height - 1u)));
    return accum_luma[src_index(sx, sy)];
}

fn sample_src_bilinear(x: f32, y: f32) -> f32 {
    let max_x = f32(final_cfg.src_width - 1u);
    let max_y = f32(final_cfg.src_height - 1u);
    let fx = clamp(x, 0.0, max_x);
    let fy = clamp(y, 0.0, max_y);

    let x0 = u32(floor(fx));
    let y0 = u32(floor(fy));
    let x1 = min(x0 + 1u, final_cfg.src_width - 1u);
    let y1 = min(y0 + 1u, final_cfg.src_height - 1u);

    let tx = fx - f32(x0);
    let ty = fy - f32(y0);

    let p00 = accum_luma[src_index(x0, y0)];
    let p10 = accum_luma[src_index(x1, y0)];
    let p01 = accum_luma[src_index(x0, y1)];
    let p11 = accum_luma[src_index(x1, y1)];

    let top = p00 + (p10 - p00) * tx;
    let bottom = p01 + (p11 - p01) * tx;
    return top + (bottom - top) * ty;
}

@compute @workgroup_size(16, 16)
fn clear_accum(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= post.width || id.y >= post.height) {
        return;
    }
    let idx = id.x + id.y * post.width;
    accum_luma[idx] = 0.0;
}

@compute @workgroup_size(16, 16)
fn blend_layer(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= post.width || id.y >= post.height) {
        return;
    }

    let idx = id.x + id.y * post.width;
    let packed = layer_pixels[idx];
    let layer_u8 = packed & 255u;
    let layer_raw = f32(layer_u8) / 255.0;
    let layer = apply_contrast_value(layer_raw, post.contrast);
    let base = accum_luma[idx];
    let mixed = blend_mode(base, layer, post.blend_mode);
    let alpha = clamp(post.opacity, 0.0, 1.0);
    accum_luma[idx] = clamp01(((1.0 - alpha) * base) + (alpha * mixed));
}

@compute @workgroup_size(64, 1, 1)
fn clear_histogram(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= 256u) {
        return;
    }
    atomicStore(&histogram[id.x], 0u);
}

@compute @workgroup_size(16, 16)
fn accumulate_histogram(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= final_cfg.src_width || id.y >= final_cfg.src_height) {
        return;
    }
    let idx = src_index(id.x, id.y);
    let contrasted = apply_contrast_value(accum_luma[idx], final_cfg.contrast);
    let bin = u32(clamp(round(contrasted * 255.0), 0.0, 255.0));
    atomicAdd(&histogram[bin], 1u);
}

@compute @workgroup_size(1, 1, 1)
fn compute_thresholds(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x > 0u || id.y > 0u || id.z > 0u) {
        return;
    }

    let total_pixels = max(final_cfg.src_width * final_cfg.src_height, 1u);
    let low_target = u32(f32(total_pixels - 1u) * clamp(final_cfg.low_pct, 0.0, 1.0));
    let high_target = u32(f32(total_pixels - 1u) * clamp(final_cfg.high_pct, 0.0, 1.0));

    var cumulative = 0u;
    var low_bin = 0u;
    var high_bin = 255u;
    var found_low = false;

    for (var bin = 0u; bin < 256u; bin = bin + 1u) {
        cumulative = cumulative + atomicLoad(&histogram[bin]);
        if (!found_low && cumulative > low_target) {
            low_bin = bin;
            found_low = true;
        }
        if (cumulative > high_target) {
            high_bin = bin;
            break;
        }
    }

    if (high_bin <= low_bin) {
        high_bin = min(low_bin + 1u, 255u);
    }

    stretch_thresholds[0] = f32(low_bin) / 255.0;
    stretch_thresholds[1] = f32(high_bin) / 255.0;
}

@compute @workgroup_size(16, 16)
fn finalize_to_u8(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= final_cfg.dst_width || id.y >= final_cfg.dst_height) {
        return;
    }

    let dst_idx = id.x + id.y * final_cfg.dst_width;
    let u = (f32(id.x) + 0.5) / f32(final_cfg.dst_width);
    let v = (f32(id.y) + 0.5) / f32(final_cfg.dst_height);
    let sample_x = u * f32(final_cfg.src_width) - 0.5;
    let sample_y = v * f32(final_cfg.src_height) - 0.5;

    let sampled = if (final_cfg.fast_mode == 0u) {
        sample_src_bilinear(sample_x, sample_y)
    } else {
        sample_src_nearest(sample_x, sample_y)
    };

    let contrasted = apply_contrast_value(sampled, final_cfg.contrast);
    let low = stretch_thresholds[0];
    let high = max(stretch_thresholds[1], low + (1.0 / 255.0));
    let stretched = clamp01((contrasted - low) / (high - low));
    final_pixels[dst_idx] = u32(round(stretched * 255.0));
}

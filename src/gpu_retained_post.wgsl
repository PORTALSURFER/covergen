struct RetainedPostParams {
    width: u32,
    height: u32,
    blend_mode: u32,
    _pad0: u32,
    opacity: f32,
    contrast: f32,
    _pad1: vec2<f32>,
}

@group(0) @binding(0)
var<storage, read> layer_pixels: array<u32>;

@group(0) @binding(1)
var<storage, read_write> accum_luma: array<f32>;

@group(0) @binding(2)
var<uniform> post: RetainedPostParams;

fn clamp01(value: f32) -> f32 {
    return clamp(value, 0.0, 1.0);
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
    let layer = clamp01(((layer_raw - 0.5) * max(post.contrast, 1.0)) + 0.5);
    let base = accum_luma[idx];
    let mixed = blend_mode(base, layer, post.blend_mode);
    let alpha = clamp(post.opacity, 0.0, 1.0);
    accum_luma[idx] = clamp01(((1.0 - alpha) * base) + (alpha * mixed));
}

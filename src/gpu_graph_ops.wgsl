struct GraphOpUniforms {
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
}

@group(0) @binding(0)
var<storage, read> src0_f32: array<f32>;

@group(0) @binding(1)
var<storage, read> src1_f32: array<f32>;

@group(0) @binding(2)
var<storage, read> src2_f32: array<f32>;

@group(0) @binding(3)
var<storage, read_write> dst_f32: array<f32>;

@group(0) @binding(4)
var<uniform> cfg: GraphOpUniforms;

fn clamp01(value: f32) -> f32 {
    return clamp(value, 0.0, 1.0);
}

fn apply_contrast(value: f32, contrast: f32) -> f32 {
    return clamp01(((value - 0.5) * max(contrast, 1.0)) + 0.5);
}

fn idx(x: u32, y: u32) -> u32 {
    return x + y * cfg.width;
}

fn smoothstep01(t: f32) -> f32 {
    return t * t * (3.0 - 2.0 * t);
}

fn hash_to_unit(x: i32, y: i32, seed: u32) -> f32 {
    var v = seed ^ (u32(x) * 0x27D4EB2Du) ^ (u32(y) * 0x85EBCA77u);
    v = v ^ (v >> 15u);
    v = v * 0x2C1B3C6Du;
    v = v ^ (v >> 12u);
    v = v * 0x297A2D39u;
    v = v ^ (v >> 15u);
    return f32(v) / 4294967295.0;
}

fn value_noise_2d(x: f32, y: f32, seed: u32) -> f32 {
    let x0 = i32(floor(x));
    let y0 = i32(floor(y));
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let fx = x - floor(x);
    let fy = y - floor(y);
    let sx = smoothstep01(fx);
    let sy = smoothstep01(fy);

    let n00 = hash_to_unit(x0, y0, seed);
    let n10 = hash_to_unit(x1, y0, seed);
    let n01 = hash_to_unit(x0, y1, seed);
    let n11 = hash_to_unit(x1, y1, seed);

    let ix0 = n00 + ((n10 - n00) * sx);
    let ix1 = n01 + ((n11 - n01) * sx);
    return clamp01(ix0 + ((ix1 - ix0) * sy));
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 <= edge0) {
        if (x >= edge0) {
            return 1.0;
        }
        return 0.0;
    }
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return smoothstep01(t);
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
            return base + ((1.0 - base) * (top * top));
        }
        default: {
            return base * top;
        }
    }
}

fn sample_clamped_src0(x: i32, y: i32) -> f32 {
    let max_x = i32(max(cfg.width, 1u) - 1);
    let max_y = i32(max(cfg.height, 1u) - 1);
    let cx = u32(clamp(x, 0, max_x));
    let cy = u32(clamp(y, 0, max_y));
    return src0_f32[idx(cx, cy)];
}

fn sample_bilinear_src0(x: f32, y: f32) -> f32 {
    let x0 = floor(x);
    let y0 = floor(y);
    let x1 = x0 + 1.0;
    let y1 = y0 + 1.0;
    let tx = x - x0;
    let ty = y - y0;

    let p00 = sample_clamped_src0(i32(x0), i32(y0));
    let p10 = sample_clamped_src0(i32(x1), i32(y0));
    let p01 = sample_clamped_src0(i32(x0), i32(y1));
    let p11 = sample_clamped_src0(i32(x1), i32(y1));

    let top = p00 + ((p10 - p00) * tx);
    let bottom = p01 + ((p11 - p01) * tx);
    return clamp01(top + ((bottom - top) * ty));
}

@compute @workgroup_size(16, 16)
fn copy_luma(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= cfg.width || id.y >= cfg.height) {
        return;
    }
    let i = idx(id.x, id.y);
    dst_f32[i] = src0_f32[i];
}

@compute @workgroup_size(16, 16)
fn source_noise(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= cfg.width || id.y >= cfg.height) {
        return;
    }

    let width_f = f32(max(cfg.width, 1u));
    let height_f = f32(max(cfg.height, 1u));
    let nx = (f32(id.x) / width_f) * max(cfg.p0, 0.001);
    let ny = (f32(id.y) / height_f) * max(cfg.p0, 0.001);
    let octave_count = clamp(cfg.octaves, 1u, 8u);

    var sum = 0.0;
    var norm = 0.0;
    var frequency = 1.0;
    var octave_amp = 1.0;

    for (var octave = 0u; octave < octave_count; octave = octave + 1u) {
        let sample_seed = cfg.seed + (octave * 0x9E3779B9u);
        let noise = value_noise_2d(nx * frequency, ny * frequency, sample_seed);
        sum = sum + (noise * octave_amp);
        norm = norm + octave_amp;
        frequency = frequency * 2.0;
        octave_amp = octave_amp * 0.5;
    }

    let normalized = select(0.0, sum / norm, norm > 0.0);
    dst_f32[idx(id.x, id.y)] = clamp01(normalized * clamp(cfg.p1, 0.0, 2.0));
}

@compute @workgroup_size(16, 16)
fn build_mask(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= cfg.width || id.y >= cfg.height) {
        return;
    }
    let source = src0_f32[idx(id.x, id.y)];
    let threshold = clamp(cfg.p0, 0.0, 1.0);
    let softness = max(cfg.p1, 0.0);
    let half = softness * 0.5;
    let edge_min = threshold - half;
    let edge_max = threshold + half;

    var value = 0.0;
    if (softness <= 0.000001) {
        value = select(0.0, 1.0, source >= threshold);
    } else {
        value = smoothstep(edge_min, edge_max, source);
    }
    if ((cfg.flags & 0x2u) != 0u) {
        value = 1.0 - value;
    }
    dst_f32[idx(id.x, id.y)] = clamp01(value);
}

@compute @workgroup_size(16, 16)
fn blend_luma(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= cfg.width || id.y >= cfg.height) {
        return;
    }
    let i = idx(id.x, id.y);
    let base = src0_f32[i];
    let top = src1_f32[i];
    let mixed = blend_mode(base, top, cfg.mode % 10u);
    let mask = select(1.0, clamp(src2_f32[i], 0.0, 1.0), (cfg.flags & 0x1u) != 0u);
    let alpha = clamp(cfg.p0, 0.0, 1.0) * mask;
    dst_f32[i] = clamp01(((1.0 - alpha) * base) + (alpha * mixed));
}

@compute @workgroup_size(16, 16)
fn tone_map(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= cfg.width || id.y >= cfg.height) {
        return;
    }
    let i = idx(id.x, id.y);
    let contrasted = apply_contrast(src0_f32[i], cfg.p0);
    let low = clamp(cfg.p1, 0.0, 1.0);
    let high = max(clamp(cfg.p2, 0.0, 1.0), low + (1.0 / 255.0));
    dst_f32[i] = clamp01((contrasted - low) / (high - low));
}

@compute @workgroup_size(16, 16)
fn warp_luma(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= cfg.width || id.y >= cfg.height) {
        return;
    }

    let width_f = f32(max(cfg.width, 1u));
    let height_f = f32(max(cfg.height, 1u));
    let x = f32(id.x);
    let y = f32(id.y);
    let u = x / width_f;
    let v = y / height_f;
    let strength = clamp(cfg.p0, 0.0, 2.0) * 0.02;
    let frequency = max(cfg.p1, 0.01);
    let phase = cfg.p2;

    let dx = sin((v * frequency + phase) * 6.28318530718) * strength;
    let dy = cos((u * frequency * 0.87 + phase * 1.13) * 6.28318530718) * strength;
    let sample_x = x + (dx * width_f);
    let sample_y = y + (dy * height_f);

    dst_f32[idx(id.x, id.y)] = sample_bilinear_src0(sample_x, sample_y);
}

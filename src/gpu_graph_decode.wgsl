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
var<storage, read> src_u32: array<u32>;

@group(0) @binding(1)
var<storage, read_write> dst_f32: array<f32>;

@group(0) @binding(2)
var<uniform> cfg: GraphOpUniforms;

fn clamp01(value: f32) -> f32 {
    return clamp(value, 0.0, 1.0);
}

fn apply_contrast(value: f32, contrast: f32) -> f32 {
    return clamp01(((value - 0.5) * max(contrast, 1.0)) + 0.5);
}

@compute @workgroup_size(16, 16)
fn decode_layer_u32(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= cfg.width || id.y >= cfg.height) {
        return;
    }
    let i = id.x + id.y * cfg.width;
    let raw_u8 = src_u32[i] & 255u;
    let raw = f32(raw_u8) / 255.0;
    dst_f32[i] = apply_contrast(raw, cfg.p0);
}

#![cfg_attr(target_arch = "spirv", no_std)]

use spirv_std::{glam::UVec3, spirv};

/// Uniforms shared with V2 graph decode compute entrypoints.
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
}

/// Clamp grayscale value into the normalized [0,1] range.
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// Apply center-weighted contrast curve.
fn apply_contrast(value: f32, contrast: f32) -> f32 {
    clamp01(((value - 0.5) * contrast.max(1.0)) + 0.5)
}

/// Decode packed u32 layer values into normalized float luma with contrast.
#[spirv(compute(threads(16, 16, 1)))]
pub fn decode_layer_u32(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] src_u32: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] dst_f32: &mut [f32],
    #[spirv(uniform, descriptor_set = 0, binding = 2)] cfg: &GraphOpUniforms,
) {
    if id.x >= cfg.width || id.y >= cfg.height {
        return;
    }
    let idx = (id.x + id.y * cfg.width) as usize;
    let raw_u8 = src_u32[idx] & 255;
    let raw = raw_u8 as f32 / 255.0;
    dst_f32[idx] = apply_contrast(raw, cfg.p0);
}

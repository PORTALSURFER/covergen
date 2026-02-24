//! Shared graph fixtures and helpers for V2 visual regression tests.

use std::error::Error;

use crate::model::LayerBlendMode;
use crate::v2::cli::{AnimationConfig, AnimationMotion, V2Config, V2Profile};
use crate::v2::compiler::{CompiledGraph, compile_graph};
use crate::v2::graph::{GpuGraph, GraphBuilder, NodeId};
use crate::v2::node::{
    BlendNode, BlendTemporal, MaskNode, MaskTemporal, PortType, SourceNoiseNode,
    SourceNoiseTemporal, TemporalCurve, ToneMapNode, ToneMapTemporal, WarpTransformNode,
    WarpTransformTemporal,
};
use crate::v2::runtime::RuntimeBuffers;

#[derive(Clone, Copy, Debug)]
pub(super) enum SnapshotGraphKind {
    Weave,
    MaskAtlas,
    WarpGrid,
    ToneCascade,
    BranchMosaic,
}

pub(super) fn build_cpu_only_compiled(
    seed: u32,
    width: u32,
    height: u32,
    kind: SnapshotGraphKind,
) -> Result<CompiledGraph, Box<dyn Error>> {
    let graph = match kind {
        SnapshotGraphKind::Weave => build_weave_graph(seed, width, height)?,
        SnapshotGraphKind::MaskAtlas => build_mask_atlas_graph(seed, width, height)?,
        SnapshotGraphKind::WarpGrid => build_warp_grid_graph(seed, width, height)?,
        SnapshotGraphKind::ToneCascade => build_tone_cascade_graph(seed, width, height)?,
        SnapshotGraphKind::BranchMosaic => build_branch_mosaic_graph(seed, width, height)?,
    };
    compile_graph(&graph).map_err(Into::into)
}

fn build_weave_graph(seed: u32, width: u32, height: u32) -> Result<GpuGraph, Box<dyn Error>> {
    let mut builder = GraphBuilder::new(width, height, seed ^ 0xC0DE_FEED);
    let noise_a = add_luma_source(&mut builder, seed ^ 0x1001, 3.2, 4, 1.0);
    let noise_b = add_luma_source(&mut builder, seed ^ 0x2002, 6.0, 3, 0.85);
    let noise_c = add_luma_source(&mut builder, seed ^ 0x3003, 4.7, 5, 0.95);

    let mask = add_mask_from(&mut builder, noise_a, 0.48, 0.17, false);
    let warp = builder.add_warp_transform(warp_node(0.95, 1.8, 0.2));
    builder.connect_luma(noise_b, warp);
    let tone = builder.add_tonemap(tone_node(1.35, 0.02, 0.98));
    builder.connect_luma(noise_c, tone);

    let blend = builder.add_blend(blend_node(LayerBlendMode::Overlay, 0.72));
    builder.connect_luma_input(warp, blend, 0);
    builder.connect_luma_input(tone, blend, 1);
    builder.connect_mask_input(mask, blend, 2);

    connect_output(&mut builder, blend);
    builder.build().map_err(Into::into)
}

fn build_mask_atlas_graph(seed: u32, width: u32, height: u32) -> Result<GpuGraph, Box<dyn Error>> {
    let mut builder = GraphBuilder::new(width, height, seed ^ 0xBAD5_EED1);
    let a = add_luma_source(&mut builder, seed ^ 0x4010, 2.2, 4, 1.0);
    let b = add_luma_source(&mut builder, seed ^ 0x5020, 5.8, 3, 0.82);
    let c = add_luma_source(&mut builder, seed ^ 0x6030, 3.7, 5, 0.94);
    let d = add_luma_source(&mut builder, seed ^ 0x7040, 7.1, 2, 0.76);

    let mask_a = add_mask_from(&mut builder, c, 0.36, 0.22, false);
    let mask_b = add_mask_from(&mut builder, d, 0.62, 0.18, true);

    let blend_a = builder.add_blend(blend_node(LayerBlendMode::Add, 0.54));
    builder.connect_luma_input(a, blend_a, 0);
    builder.connect_luma_input(b, blend_a, 1);
    builder.connect_mask_input(mask_a, blend_a, 2);

    let warp = builder.add_warp_transform(warp_node(1.2, 2.2, 0.37));
    builder.connect_luma(blend_a, warp);

    let tone = builder.add_tonemap(tone_node(1.24, 0.015, 0.97));
    builder.connect_luma(warp, tone);

    let final_mix = builder.add_blend(blend_node(LayerBlendMode::Overlay, 0.62));
    builder.connect_luma_input(tone, final_mix, 0);
    builder.connect_luma_input(d, final_mix, 1);
    builder.connect_mask_input(mask_b, final_mix, 2);

    connect_output(&mut builder, final_mix);
    builder.build().map_err(Into::into)
}

fn build_warp_grid_graph(seed: u32, width: u32, height: u32) -> Result<GpuGraph, Box<dyn Error>> {
    let mut builder = GraphBuilder::new(width, height, seed ^ 0x0F51_1109);
    let a = add_luma_source(&mut builder, seed ^ 0x8111, 2.8, 4, 1.0);
    let b = add_luma_source(&mut builder, seed ^ 0x9222, 4.2, 4, 0.9);
    let c = add_luma_source(&mut builder, seed ^ 0xA333, 6.4, 3, 0.8);
    let d = add_luma_source(&mut builder, seed ^ 0xB444, 8.1, 2, 0.74);

    let warp_a = builder.add_warp_transform(warp_node(0.86, 1.5, 0.12));
    builder.connect_luma(a, warp_a);
    let warp_b = builder.add_warp_transform(warp_node(1.06, 2.4, 0.27));
    builder.connect_luma(warp_a, warp_b);

    let tone = builder.add_tonemap(tone_node(1.42, 0.02, 0.98));
    builder.connect_luma(b, tone);

    let first_mix = builder.add_blend(blend_node(LayerBlendMode::Screen, 0.58));
    builder.connect_luma_input(warp_b, first_mix, 0);
    builder.connect_luma_input(tone, first_mix, 1);

    let warp_c = builder.add_warp_transform(warp_node(1.32, 3.1, 0.43));
    builder.connect_luma(c, warp_c);

    let mask = add_mask_from(&mut builder, d, 0.41, 0.15, false);
    let final_mix = builder.add_blend(blend_node(LayerBlendMode::Difference, 0.51));
    builder.connect_luma_input(first_mix, final_mix, 0);
    builder.connect_luma_input(warp_c, final_mix, 1);
    builder.connect_mask_input(mask, final_mix, 2);

    connect_output(&mut builder, final_mix);
    builder.build().map_err(Into::into)
}

fn build_tone_cascade_graph(
    seed: u32,
    width: u32,
    height: u32,
) -> Result<GpuGraph, Box<dyn Error>> {
    let mut builder = GraphBuilder::new(width, height, seed ^ 0x7134_11A5);
    let a = add_luma_source(&mut builder, seed ^ 0xC101, 2.9, 4, 1.0);
    let b = add_luma_source(&mut builder, seed ^ 0xC202, 5.1, 3, 0.86);
    let c = add_luma_source(&mut builder, seed ^ 0xC303, 7.3, 2, 0.78);

    let tone_a = builder.add_tonemap(tone_node(1.52, 0.018, 0.985));
    builder.connect_luma(a, tone_a);
    let warp_a = builder.add_warp_transform(warp_node(0.91, 2.0, 0.22));
    builder.connect_luma(tone_a, warp_a);

    let tone_b = builder.add_tonemap(tone_node(1.26, 0.012, 0.972));
    builder.connect_luma(b, tone_b);
    let warp_b = builder.add_warp_transform(warp_node(1.13, 2.8, 0.43));
    builder.connect_luma(tone_b, warp_b);

    let mask = add_mask_from(&mut builder, c, 0.47, 0.16, false);
    let blend = builder.add_blend(blend_node(LayerBlendMode::Overlay, 0.63));
    builder.connect_luma_input(warp_a, blend, 0);
    builder.connect_luma_input(warp_b, blend, 1);
    builder.connect_mask_input(mask, blend, 2);

    let final_tone = builder.add_tonemap(tone_node(1.18, 0.01, 0.98));
    builder.connect_luma(blend, final_tone);
    connect_output(&mut builder, final_tone);
    builder.build().map_err(Into::into)
}

fn build_branch_mosaic_graph(
    seed: u32,
    width: u32,
    height: u32,
) -> Result<GpuGraph, Box<dyn Error>> {
    let mut builder = GraphBuilder::new(width, height, seed ^ 0xE44B_A551);
    let a = add_luma_source(&mut builder, seed ^ 0xD101, 2.4, 5, 1.0);
    let b = add_luma_source(&mut builder, seed ^ 0xD202, 4.8, 4, 0.92);
    let c = add_luma_source(&mut builder, seed ^ 0xD303, 6.7, 3, 0.82);
    let d = add_luma_source(&mut builder, seed ^ 0xD404, 8.6, 2, 0.74);

    let mask_a = add_mask_from(&mut builder, c, 0.39, 0.21, false);
    let mask_b = add_mask_from(&mut builder, d, 0.58, 0.18, true);

    let left_warp = builder.add_warp_transform(warp_node(0.84, 1.7, 0.14));
    builder.connect_luma(a, left_warp);
    let right_warp = builder.add_warp_transform(warp_node(1.28, 3.0, 0.51));
    builder.connect_luma(b, right_warp);

    let left_mix = builder.add_blend(blend_node(LayerBlendMode::Lighten, 0.56));
    builder.connect_luma_input(left_warp, left_mix, 0);
    builder.connect_luma_input(c, left_mix, 1);
    builder.connect_mask_input(mask_a, left_mix, 2);

    let right_mix = builder.add_blend(blend_node(LayerBlendMode::Difference, 0.49));
    builder.connect_luma_input(right_warp, right_mix, 0);
    builder.connect_luma_input(d, right_mix, 1);
    builder.connect_mask_input(mask_b, right_mix, 2);

    let union = builder.add_blend(blend_node(LayerBlendMode::Screen, 0.61));
    builder.connect_luma_input(left_mix, union, 0);
    builder.connect_luma_input(right_mix, union, 1);

    let final_warp = builder.add_warp_transform(warp_node(0.73, 1.2, 0.08));
    builder.connect_luma(union, final_warp);
    connect_output(&mut builder, final_warp);
    builder.build().map_err(Into::into)
}

fn add_luma_source(
    builder: &mut GraphBuilder,
    seed: u32,
    scale: f32,
    octaves: u32,
    amplitude: f32,
) -> NodeId {
    builder.add_source_noise(SourceNoiseNode {
        seed,
        scale,
        octaves,
        amplitude,
        output_port: PortType::LumaTexture,
        temporal: SourceNoiseTemporal {
            scale_mul: Some(TemporalCurve::sine(0.11, 0.8, 0.2, 0.0)),
            amplitude_mul: Some(TemporalCurve::sine(0.09, 1.1, 0.4, 0.0)),
        },
    })
}

fn add_mask_from(
    builder: &mut GraphBuilder,
    luma: NodeId,
    threshold: f32,
    softness: f32,
    invert: bool,
) -> NodeId {
    let mask = builder.add_mask(MaskNode {
        threshold,
        softness,
        invert,
        temporal: MaskTemporal {
            threshold_add: Some(TemporalCurve::sine(0.05, 0.9, 0.1, 0.0)),
            softness_mul: Some(TemporalCurve::sine(0.12, 1.2, 0.3, 0.0)),
        },
    });
    builder.connect_luma(luma, mask);
    mask
}

fn blend_node(mode: LayerBlendMode, opacity: f32) -> BlendNode {
    BlendNode {
        mode,
        opacity,
        temporal: BlendTemporal {
            opacity_mul: Some(TemporalCurve::sine(0.18, 0.7, 0.0, 0.0)),
        },
    }
}

fn tone_node(contrast: f32, low_pct: f32, high_pct: f32) -> ToneMapNode {
    ToneMapNode {
        contrast,
        low_pct,
        high_pct,
        temporal: ToneMapTemporal {
            contrast_mul: Some(TemporalCurve::sine(0.09, 0.8, 0.0, 0.0)),
            low_pct_add: Some(TemporalCurve::sine(0.008, 0.6, 0.25, 0.0)),
            high_pct_add: Some(TemporalCurve::sine(0.008, 1.0, 0.6, 0.0)),
        },
    }
}

fn warp_node(strength: f32, frequency: f32, phase: f32) -> WarpTransformNode {
    WarpTransformNode {
        strength,
        frequency,
        phase,
        temporal: WarpTransformTemporal {
            strength_mul: Some(TemporalCurve::sine(0.14, 0.7, 0.3, 0.0)),
            frequency_mul: Some(TemporalCurve::sine(0.10, 0.9, 0.1, 0.0)),
            phase_add: Some(TemporalCurve::sine(0.22, 1.0, 0.0, 0.0)),
        },
    }
}

fn connect_output(builder: &mut GraphBuilder, source: NodeId) {
    let output = builder.add_output();
    builder.connect_luma(source, output);
}

pub(super) fn runtime_buffers(
    config: &V2Config,
    compiled: &CompiledGraph,
) -> Result<RuntimeBuffers, Box<dyn Error>> {
    Ok(RuntimeBuffers {
        layered: vec![0.0f32; pixel_count(compiled.width, compiled.height)?],
        percentile: vec![0.0f32; pixel_count(compiled.width, compiled.height)?],
        layer_scratch: vec![0.0f32; pixel_count(compiled.width, compiled.height)?],
        final_luma: vec![0.0f32; pixel_count(config.width, config.height)?],
        downsample_scratch: Vec::new(),
        output_gray: vec![0u8; pixel_count(config.width, config.height)?],
    })
}

fn pixel_count(width: u32, height: u32) -> Result<usize, Box<dyn Error>> {
    width
        .checked_mul(height)
        .map(|count| count as usize)
        .ok_or("invalid test dimensions".into())
}

pub(super) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(super) fn snapshot_config(seed: u32, width: u32, height: u32, profile: V2Profile) -> V2Config {
    V2Config {
        width,
        height,
        seed,
        count: 1,
        output: "snapshot.png".to_string(),
        layers: 4,
        antialias: 1,
        preset: "hybrid-stack".to_string(),
        profile,
        animation: AnimationConfig {
            enabled: false,
            seconds: 2,
            fps: 12,
            keep_frames: false,
            reels: false,
            motion: AnimationMotion::Normal,
        },
    }
}

//! Deterministic visual regression tests for V2 fixed-seed renders.
//!
//! These tests snapshot hashes of CPU-only V2 graph output for still images and
//! sampled animation frames. Using CPU-only node kinds keeps results portable
//! and deterministic across hosts without requiring a hardware GPU.

use std::error::Error;

use crate::model::LayerBlendMode;

use super::cli::{AnimationConfig, V2Config, V2Profile};
use super::compiler::{CompiledGraph, compile_graph};
use super::graph::GraphBuilder;
use super::node::{
    BlendNode, BlendTemporal, GraphTimeInput, MaskNode, MaskTemporal, PortType, SourceNoiseNode,
    SourceNoiseTemporal, TemporalCurve, ToneMapNode, ToneMapTemporal, WarpTransformNode,
    WarpTransformTemporal,
};
use super::runtime::{RuntimeBuffers, finalize_luma_for_output_for_test};
use super::runtime_eval::render_graph_luma;

#[derive(Clone, Copy)]
struct StillSnapshotCase {
    name: &'static str,
    seed: u32,
    expected_hash: u64,
}

#[derive(Clone, Copy)]
struct AnimationSnapshotCase {
    name: &'static str,
    seed: u32,
    frame_total: u32,
    frame_indices: [u32; 4],
    expected_hashes: [u64; 4],
}

const STILL_SNAPSHOTS: &[StillSnapshotCase] = &[
    StillSnapshotCase {
        name: "cpu-weave-still-a",
        seed: 0x1357_9BDF,
        expected_hash: 0x7bce_fca6_cc4c_b01c,
    },
    StillSnapshotCase {
        name: "cpu-weave-still-b",
        seed: 0x2468_ACE0,
        expected_hash: 0x383b_4f68_7fdf_d848,
    },
];

const ANIMATION_SNAPSHOT: AnimationSnapshotCase = AnimationSnapshotCase {
    name: "cpu-weave-animation",
    seed: 0xA5A5_1F1F,
    frame_total: 24,
    frame_indices: [0, 7, 15, 23],
    expected_hashes: [
        0x2f36_25f6_910c_ab69,
        0xba57_f0a0_cb28_3d37,
        0xecc8_bf45_2f2d_7ff0,
        0x45b2_c056_4c21_ff71,
    ],
};

#[test]
fn v2_still_fixed_seed_snapshots_match() {
    for case in STILL_SNAPSHOTS {
        let actual_hash = render_still_hash(*case)
            .unwrap_or_else(|err| panic!("failed to render still snapshot '{}': {err}", case.name));
        assert_eq!(
            actual_hash, case.expected_hash,
            "snapshot '{}' drifted: expected {:#018x}, got {:#018x}",
            case.name, case.expected_hash, actual_hash
        );
    }
}

#[test]
fn v2_animation_fixed_seed_sampled_frames_match() {
    let actual_hashes = render_animation_hashes(ANIMATION_SNAPSHOT).unwrap_or_else(|err| {
        panic!(
            "failed to render animation snapshot '{}': {err}",
            ANIMATION_SNAPSHOT.name
        )
    });

    for (index, actual_hash) in actual_hashes.into_iter().enumerate() {
        let expected_hash = ANIMATION_SNAPSHOT.expected_hashes[index];
        assert_eq!(
            actual_hash,
            expected_hash,
            "animation snapshot '{}' frame {} drifted: expected {:#018x}, got {:#018x}",
            ANIMATION_SNAPSHOT.name,
            ANIMATION_SNAPSHOT.frame_indices[index],
            expected_hash,
            actual_hash
        );
    }
}

fn render_still_hash(case: StillSnapshotCase) -> Result<u64, Box<dyn Error>> {
    let config = base_config(case.seed);
    let compiled = build_cpu_only_compiled(case.seed, config.width, config.height)?;
    let mut buffers = runtime_buffers(&config, &compiled)?;

    render_graph_luma(
        &compiled,
        None,
        &mut buffers,
        config.seed.wrapping_add(compiled.seed),
        None,
    )?;
    finalize_luma_for_output_for_test(&config, &compiled, None, &mut buffers)?;
    Ok(fnv1a64(&buffers.output_gray))
}

fn render_animation_hashes(case: AnimationSnapshotCase) -> Result<[u64; 4], Box<dyn Error>> {
    let config = base_config(case.seed);
    let compiled = build_cpu_only_compiled(case.seed, config.width, config.height)?;
    let mut buffers = runtime_buffers(&config, &compiled)?;
    let mut hashes = [0u64; 4];

    for (slot, frame_index) in case.frame_indices.into_iter().enumerate() {
        if frame_index >= case.frame_total {
            return Err(format!(
                "invalid frame index {} for total frame count {}",
                frame_index, case.frame_total
            )
            .into());
        }

        let graph_time = GraphTimeInput::from_frame(frame_index, case.frame_total);
        let seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(frame_index.wrapping_mul(0x9E37_79B9));

        render_graph_luma(&compiled, None, &mut buffers, seed_offset, Some(graph_time))?;
        finalize_luma_for_output_for_test(&config, &compiled, None, &mut buffers)?;
        hashes[slot] = fnv1a64(&buffers.output_gray);
    }

    Ok(hashes)
}

fn build_cpu_only_compiled(
    seed: u32,
    width: u32,
    height: u32,
) -> Result<CompiledGraph, Box<dyn Error>> {
    let mut builder = GraphBuilder::new(width, height, seed ^ 0xC0DE_FEED);

    let noise_a = builder.add_source_noise(SourceNoiseNode {
        seed: seed ^ 0x1001,
        scale: 3.2,
        octaves: 4,
        amplitude: 1.0,
        output_port: PortType::LumaTexture,
        temporal: SourceNoiseTemporal {
            scale_mul: Some(TemporalCurve::sine(0.12, 0.8, 0.0, 0.0)),
            amplitude_mul: Some(TemporalCurve::sine(0.10, 1.1, 0.25, 0.0)),
        },
    });
    let noise_b = builder.add_source_noise(SourceNoiseNode {
        seed: seed ^ 0x2002,
        scale: 6.0,
        octaves: 3,
        amplitude: 0.85,
        output_port: PortType::LumaTexture,
        temporal: SourceNoiseTemporal {
            scale_mul: Some(TemporalCurve::sine(0.09, 0.7, 0.5, 0.0)),
            amplitude_mul: Some(TemporalCurve::sine(0.11, 1.0, 0.0, 0.0)),
        },
    });
    let noise_c = builder.add_source_noise(SourceNoiseNode {
        seed: seed ^ 0x3003,
        scale: 4.7,
        octaves: 5,
        amplitude: 0.95,
        output_port: PortType::LumaTexture,
        temporal: SourceNoiseTemporal {
            scale_mul: Some(TemporalCurve::sine(0.10, 1.2, 0.75, 0.0)),
            amplitude_mul: Some(TemporalCurve::sine(0.08, 0.9, 0.4, 0.0)),
        },
    });

    let mask = builder.add_mask(MaskNode {
        threshold: 0.48,
        softness: 0.17,
        invert: false,
        temporal: MaskTemporal {
            threshold_add: Some(TemporalCurve::sine(0.06, 0.85, 0.0, 0.0)),
            softness_mul: Some(TemporalCurve::sine(0.14, 1.05, 0.2, 0.0)),
        },
    });
    builder.connect_luma(noise_a, mask);

    let warp = builder.add_warp_transform(WarpTransformNode {
        strength: 0.95,
        frequency: 1.8,
        phase: 0.2,
        temporal: WarpTransformTemporal {
            strength_mul: Some(TemporalCurve::sine(0.15, 0.7, 0.3, 0.0)),
            frequency_mul: Some(TemporalCurve::sine(0.12, 0.9, 0.1, 0.0)),
            phase_add: Some(TemporalCurve::sine(0.25, 1.0, 0.0, 0.0)),
        },
    });
    builder.connect_luma(noise_b, warp);

    let tone = builder.add_tonemap(ToneMapNode {
        contrast: 1.35,
        low_pct: 0.02,
        high_pct: 0.98,
        temporal: ToneMapTemporal {
            contrast_mul: Some(TemporalCurve::sine(0.09, 0.8, 0.0, 0.0)),
            low_pct_add: Some(TemporalCurve::sine(0.01, 0.6, 0.25, 0.0)),
            high_pct_add: Some(TemporalCurve::sine(0.01, 1.0, 0.6, 0.0)),
        },
    });
    builder.connect_luma(noise_c, tone);

    let blend = builder.add_blend(BlendNode {
        mode: LayerBlendMode::Overlay,
        opacity: 0.72,
        temporal: BlendTemporal {
            opacity_mul: Some(TemporalCurve::sine(0.2, 0.75, 0.0, 0.0)),
        },
    });
    builder.connect_luma_input(warp, blend, 0);
    builder.connect_luma_input(tone, blend, 1);
    builder.connect_mask_input(mask, blend, 2);

    let output = builder.add_output();
    builder.connect_luma(blend, output);

    let graph = builder.build()?;
    compile_graph(&graph).map_err(Into::into)
}

fn runtime_buffers(
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

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn base_config(seed: u32) -> V2Config {
    V2Config {
        width: 192,
        height: 192,
        seed,
        count: 1,
        output: "snapshot.png".to_string(),
        layers: 4,
        antialias: 1,
        preset: "hybrid-stack".to_string(),
        profile: V2Profile::Performance,
        animation: AnimationConfig {
            enabled: false,
            seconds: 2,
            fps: 12,
            keep_frames: false,
            reels: false,
        },
    }
}

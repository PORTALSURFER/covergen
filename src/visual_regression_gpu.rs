//! GPU confidence regression tests for V2 fixed-seed renders.
//!
//! These checks run only when a hardware GPU is available. They verify
//! deterministic still rendering and temporal variation across sampled
//! animation frames on the GPU path.

use std::collections::HashSet;
use std::error::Error;

use super::node::GraphTimeInput;
use super::runtime_config::V2Profile;
use super::runtime_gpu::render_graph_luma_gpu;
use super::runtime_test_support::finalize_luma_for_output_for_test;
use super::visual_regression_fixtures as fixtures;
use crate::gpu_render::GpuLayerRenderer;

#[derive(Clone, Copy, Debug)]
struct GpuStillCase {
    seed: u32,
    width: u32,
    height: u32,
    profile: V2Profile,
    graph: fixtures::SnapshotGraphKind,
}

#[derive(Clone, Copy, Debug)]
struct GpuAnimationCase {
    seed: u32,
    width: u32,
    height: u32,
    profile: V2Profile,
    graph: fixtures::SnapshotGraphKind,
    frame_total: u32,
    frame_indices: &'static [u32],
}

const GPU_STILL_CASES: &[GpuStillCase] = &[
    GpuStillCase {
        seed: 0x44AA_9911,
        width: 320,
        height: 320,
        profile: V2Profile::Performance,
        graph: fixtures::SnapshotGraphKind::MaskAtlas,
    },
    GpuStillCase {
        seed: 0x11DD_7139,
        width: 448,
        height: 448,
        profile: V2Profile::Quality,
        graph: fixtures::SnapshotGraphKind::ToneCascade,
    },
    GpuStillCase {
        seed: 0x0F0F_C0DE,
        width: 512,
        height: 512,
        profile: V2Profile::Performance,
        graph: fixtures::SnapshotGraphKind::WarpGrid,
    },
];

const GPU_ANIMATION_CASES: &[GpuAnimationCase] = &[
    GpuAnimationCase {
        seed: 0x5599_AA33,
        width: 256,
        height: 256,
        profile: V2Profile::Quality,
        graph: fixtures::SnapshotGraphKind::Weave,
        frame_total: 24,
        frame_indices: &[0, 6, 12, 18, 23],
    },
    GpuAnimationCase {
        seed: 0x8081_2299,
        width: 384,
        height: 384,
        profile: V2Profile::Performance,
        graph: fixtures::SnapshotGraphKind::BranchMosaic,
        frame_total: 30,
        frame_indices: &[0, 5, 10, 15, 20, 25, 29],
    },
];

#[test]
fn v2_gpu_still_fixed_seed_is_deterministic_when_hardware_available() {
    for case in GPU_STILL_CASES {
        let Ok(mut renderer) = try_create_hardware_gpu_renderer(case.width, case.height) else {
            return;
        };
        let Some(renderer) = renderer.as_mut() else {
            return;
        };

        let first = render_still_hash_gpu(*case, renderer)
            .unwrap_or_else(|err| panic!("gpu deterministic first render failed: {err}"));
        let second = render_still_hash_gpu(*case, renderer)
            .unwrap_or_else(|err| panic!("gpu deterministic second render failed: {err}"));
        assert_eq!(
            first, second,
            "gpu still output should be deterministic for fixed seed"
        );
    }
}

#[test]
fn v2_gpu_animation_sampled_frames_change_when_hardware_available() {
    for case in GPU_ANIMATION_CASES {
        let Ok(mut renderer) = try_create_hardware_gpu_renderer(case.width, case.height) else {
            return;
        };
        let Some(renderer) = renderer.as_mut() else {
            return;
        };

        let frame_hashes = render_animation_hashes_gpu(*case, renderer)
            .unwrap_or_else(|err| panic!("gpu animation sample render failed: {err}"));
        assert!(
            !frame_hashes.is_empty(),
            "gpu sampled animation frame set should not be empty"
        );
        let unique: HashSet<u64> = frame_hashes.into_iter().collect();
        assert!(
            unique.len() > 1,
            "gpu sampled animation frames should vary over clip time"
        );
    }
}

fn render_still_hash_gpu(
    case: GpuStillCase,
    renderer: &mut GpuLayerRenderer,
) -> Result<u64, Box<dyn Error>> {
    let config = fixtures::snapshot_config(case.seed, case.width, case.height, case.profile);
    let compiled =
        fixtures::build_cpu_only_compiled(case.seed, config.width, config.height, case.graph)?;
    renderer.ensure_node_alias_buffers(
        compiled.resource_plan.gpu_peak_luma_slots,
        compiled.resource_plan.gpu_peak_mask_slots,
    )?;
    let mut buffers = fixtures::runtime_buffers(&config, &compiled)?;

    render_graph_luma_gpu(
        &compiled,
        renderer,
        config.seed.wrapping_add(compiled.seed),
        None,
    )?;
    finalize_luma_for_output_for_test(&config, &compiled, Some(renderer), &mut buffers)?;
    Ok(fixtures::fnv1a64(&buffers.output_gray))
}

fn render_animation_hashes_gpu(
    case: GpuAnimationCase,
    renderer: &mut GpuLayerRenderer,
) -> Result<Vec<u64>, Box<dyn Error>> {
    let config = fixtures::snapshot_config(case.seed, case.width, case.height, case.profile);
    let compiled =
        fixtures::build_cpu_only_compiled(case.seed, config.width, config.height, case.graph)?;
    renderer.ensure_node_alias_buffers(
        compiled.resource_plan.gpu_peak_luma_slots,
        compiled.resource_plan.gpu_peak_mask_slots,
    )?;
    let mut buffers = fixtures::runtime_buffers(&config, &compiled)?;
    let mut hashes = Vec::with_capacity(case.frame_indices.len());

    for &frame_index in case.frame_indices {
        let graph_time = GraphTimeInput::from_frame(frame_index, case.frame_total);
        let seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(frame_index.wrapping_mul(0x9E37_79B9));

        render_graph_luma_gpu(&compiled, renderer, seed_offset, Some(graph_time))?;
        finalize_luma_for_output_for_test(&config, &compiled, Some(renderer), &mut buffers)?;
        hashes.push(fixtures::fnv1a64(&buffers.output_gray));
    }

    Ok(hashes)
}

fn try_create_hardware_gpu_renderer(
    width: u32,
    height: u32,
) -> Result<Option<GpuLayerRenderer>, Box<dyn Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }));
    let Some(adapter) = adapter else {
        return Ok(None);
    };

    let info = adapter.get_info();
    if matches!(
        info.device_type,
        wgpu::DeviceType::Cpu | wgpu::DeviceType::VirtualGpu
    ) {
        return Ok(None);
    }
    let name = info.name.to_ascii_lowercase();
    if [
        "swiftshader",
        "llvmpipe",
        "lavapipe",
        "softpipe",
        "software rasterizer",
        "microsoft basic render driver",
        "warp",
    ]
    .iter()
    .any(|needle| name.contains(needle))
    {
        return Ok(None);
    }

    let renderer = pollster::block_on(GpuLayerRenderer::new_with_output(
        &adapter, width, height, width, height,
    ))?;
    Ok(Some(renderer))
}

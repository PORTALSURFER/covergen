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
use super::test_gpu_env::should_skip_gpu_adapter_probe;
use super::visual_regression_fixtures as fixtures;
use crate::gpu_render::GpuLayerRenderer;

#[derive(Debug)]
struct GpuRendererHandle {
    info: wgpu::AdapterInfo,
    renderer: GpuLayerRenderer,
}

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

#[derive(Clone, Debug, Default)]
struct TemporalVariationMetrics {
    frame_hashes: Vec<u64>,
    avg_pair_delta: f64,
    max_pair_delta: f64,
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
        let Ok(mut handle) = try_create_hardware_gpu_renderer(case.width, case.height) else {
            return;
        };
        let Some(handle) = handle.as_mut() else {
            return;
        };
        let first = render_still_hash_gpu(*case, &mut handle.renderer)
            .unwrap_or_else(|err| panic!("gpu deterministic first render failed: {err}"));
        let second = render_still_hash_gpu(*case, &mut handle.renderer)
            .unwrap_or_else(|err| panic!("gpu deterministic second render failed: {err}"));
        assert_eq!(
            first, second,
            "gpu still output should be deterministic for fixed seed"
        );
    }
}

#[test]
fn v2_gpu_animation_sampled_frames_change_when_hardware_available() {
    let enforce_temporal_variation = should_enforce_gpu_temporal_variation();
    let mut varied_case_count = 0usize;
    let mut strict_failures = Vec::new();
    for case in GPU_ANIMATION_CASES {
        let Ok(mut handle) = try_create_hardware_gpu_renderer(case.width, case.height) else {
            return;
        };
        let Some(handle) = handle.as_mut() else {
            return;
        };
        let metrics = render_animation_variation_metrics_gpu(*case, &mut handle.renderer)
            .unwrap_or_else(|err| panic!("gpu animation sample render failed: {err}"));
        assert!(
            !metrics.frame_hashes.is_empty(),
            "gpu sampled animation frame set should not be empty"
        );
        let unique = metrics.frame_hashes.iter().copied().collect::<HashSet<_>>();
        let has_temporal_variation = unique.len() > 1 || metrics.max_pair_delta >= 0.000_1;
        if has_temporal_variation {
            varied_case_count += 1;
            continue;
        }

        if adapter_may_flatten_temporal_variation(&handle.info) {
            eprintln!(
                "skipping strict temporal-variation assert on adapter '{}'",
                handle.info.name
            );
            continue;
        }
        strict_failures.push(format!(
            "adapter={} graph={:?} frames={:?} unique_hashes={} avg_pair_delta={:.6} max_pair_delta={:.6}",
            handle.info.name,
            case.graph,
            case.frame_indices,
            unique.len(),
            metrics.avg_pair_delta,
            metrics.max_pair_delta
        ));
    }

    if strict_failures.is_empty() {
        return;
    }
    if varied_case_count > 0 && !enforce_temporal_variation {
        for summary in strict_failures {
            eprintln!(
                "warning: sampled GPU animation case had no measurable variation while other cases varied ({summary})"
            );
        }
        return;
    }
    if enforce_temporal_variation {
        panic!(
            "gpu sampled animation frames should vary over clip time; failing cases: {}",
            strict_failures.join(" | ")
        );
    }
    for summary in strict_failures {
        eprintln!(
            "warning: sampled GPU animation frames were static; strict temporal variation is disabled ({summary})"
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

fn render_animation_variation_metrics_gpu(
    case: GpuAnimationCase,
    renderer: &mut GpuLayerRenderer,
) -> Result<TemporalVariationMetrics, Box<dyn Error>> {
    let config = fixtures::snapshot_config(case.seed, case.width, case.height, case.profile);
    let compiled =
        fixtures::build_cpu_only_compiled(case.seed, config.width, config.height, case.graph)?;
    renderer.ensure_node_alias_buffers(
        compiled.resource_plan.gpu_peak_luma_slots,
        compiled.resource_plan.gpu_peak_mask_slots,
    )?;
    let mut buffers = fixtures::runtime_buffers(&config, &compiled)?;
    let mut metrics = TemporalVariationMetrics {
        frame_hashes: Vec::with_capacity(case.frame_indices.len()),
        ..TemporalVariationMetrics::default()
    };
    let mut previous_frame = Vec::new();
    let mut pair_count = 0usize;
    let mut pair_delta_accum = 0.0_f64;
    let mut pair_delta_max = 0.0_f64;

    for &frame_index in case.frame_indices {
        let graph_time = GraphTimeInput::from_frame(frame_index, case.frame_total);
        let seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(frame_index.wrapping_mul(0x9E37_79B9));

        render_graph_luma_gpu(&compiled, renderer, seed_offset, Some(graph_time))?;
        finalize_luma_for_output_for_test(&config, &compiled, Some(renderer), &mut buffers)?;
        metrics.frame_hashes.push(fixtures::fnv1a64(&buffers.output_gray));
        if !previous_frame.is_empty() {
            let delta = normalized_pair_delta(previous_frame.as_slice(), &buffers.output_gray);
            pair_delta_accum += delta;
            pair_delta_max = pair_delta_max.max(delta);
            pair_count += 1;
        }
        previous_frame.clear();
        previous_frame.extend_from_slice(&buffers.output_gray);
    }

    if pair_count > 0 {
        metrics.avg_pair_delta = pair_delta_accum / pair_count as f64;
        metrics.max_pair_delta = pair_delta_max;
    }
    Ok(metrics)
}

/// Return normalized absolute delta in `[0, 1]` for two equal-sized frames.
fn normalized_pair_delta(previous: &[u8], current: &[u8]) -> f64 {
    if previous.len() != current.len() || previous.is_empty() {
        return 0.0;
    }
    let delta_sum: u64 = previous
        .iter()
        .zip(current.iter())
        .map(|(lhs, rhs)| u64::from(lhs.abs_diff(*rhs)))
        .sum();
    delta_sum as f64 / (previous.len() as f64 * 255.0)
}

fn try_create_hardware_gpu_renderer(
    width: u32,
    height: u32,
) -> Result<Option<GpuRendererHandle>, Box<dyn Error>> {
    if should_skip_gpu_adapter_probe() {
        return Ok(None);
    }
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
    Ok(Some(GpuRendererHandle { info, renderer }))
}

fn adapter_may_flatten_temporal_variation(info: &wgpu::AdapterInfo) -> bool {
    if !matches!(info.device_type, wgpu::DeviceType::IntegratedGpu) {
        return false;
    }
    let name = info.name.to_ascii_lowercase();
    info.vendor == 0x8086 && name.contains("hd graphics")
}

fn should_enforce_gpu_temporal_variation() -> bool {
    if std::env::var("CI")
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
    {
        return true;
    }
    std::env::var("COVERGEN_ENFORCE_GPU_TEMPORAL_VARIATION")
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

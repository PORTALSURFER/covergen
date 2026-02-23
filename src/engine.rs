//! Rendering orchestration and image generation workflow.

use std::error::Error;
use std::io::{self, Write};
use std::sync::{Arc, atomic::Ordering};
use std::time::Instant;

use crate::analysis::{
    LumaStats, collect_luma_metrics, collect_luma_metrics_sampled, needs_complexity_fix,
};
use crate::blending::{self, strategy_name};
use crate::config::{
    Config, apply_cpu_fallback_profile, clamp_iteration_count, clamp_layer_count,
    resolve_fast_profile, resolve_fast_resolution, resolve_render_resolution,
    resolve_strategy_budget,
};
use crate::gpu_render::GpuLayerRenderer;
use crate::image_ops::*;
use crate::model::{ArtStyle, Params, SymmetryStyle, XorShift32};
use crate::progress::{SpinnerState, log_progress_message, start_spinner};
use crate::randomization::*;
use crate::render_workspace::RenderWorkspace;
use crate::strategies::{
    RenderStrategy, StrategyScratch, fit_strategy_to_budget, pick_render_strategy_with_preferences,
    record_strategy_runtime, render_cpu_strategy_into, render_strategy_cost, strategy_profile,
};

/// WGSL compute shader source used by the GPU renderer.
const SHADER: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/shader.wgsl"));

/// Returns whether an adapter should be treated as software/CPU for performance safety.
fn is_software_adapter(device_type: wgpu::DeviceType, adapter_name: &str) -> bool {
    if matches!(
        device_type,
        wgpu::DeviceType::Cpu | wgpu::DeviceType::VirtualGpu
    ) {
        return true;
    }

    let adapter_name = adapter_name.to_ascii_lowercase();
    [
        "swiftshader",
        "llvmpipe",
        "lavapipe",
        "softpipe",
        "software rasterizer",
        "microsoft basic render driver",
        "warp",
    ]
    .iter()
    .any(|needle| adapter_name.contains(needle))
}

/// Parse an opt-in performance timing flag from a string value.
fn parse_perf_timing_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Returns true when detailed performance timing logs are enabled.
fn perf_timing_enabled_from_env() -> bool {
    std::env::var("COVERGEN_PERF_TIMING")
        .map(|value| parse_perf_timing_flag(&value))
        .unwrap_or(false)
}

/// Build the GPU layer renderer on first use so CPU-only runs skip startup cost.
async fn ensure_gpu_renderer(
    adapter: &wgpu::Adapter,
    gpu: &mut Option<GpuLayerRenderer>,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn Error>> {
    if gpu.is_none() {
        eprintln!("Initializing GPU renderer ({width}x{height}) on first GPU strategy use.");
        *gpu = Some(GpuLayerRenderer::new(adapter, SHADER, width, height).await?);
    }
    Ok(())
}

/// Render one strategy layer into `out`, dispatching to either GPU or CPU paths.
fn render_strategy_layer(
    strategy: RenderStrategy,
    strategy_params: &Params,
    out: &mut [f32],
    complexity_budget: u32,
    fast: bool,
    render_width: u32,
    render_height: u32,
    gpu: &mut Option<GpuLayerRenderer>,
    strategy_scratch: &mut StrategyScratch,
) -> Result<(), Box<dyn Error>> {
    match strategy {
        RenderStrategy::Gpu(_) => {
            let renderer = gpu
                .as_mut()
                .ok_or("gpu renderer unavailable; initialize before gpu layer render")?;
            renderer.render_layer(strategy_params, out)?;
            Ok(())
        }
        RenderStrategy::Cpu(cpu_strategy) => {
            render_cpu_strategy_into(
                cpu_strategy,
                render_width,
                render_height,
                strategy_params.seed,
                fast,
                complexity_budget,
                out,
                strategy_scratch,
            );
            Ok(())
        }
    }
}

/// Execute the image generation pipeline for one parsed CLI configuration.
pub(crate) async fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .ok_or("no compatible GPU adapter found")?;
    let adapter_info = adapter.get_info();
    let cpu_fallback_safe = is_software_adapter(adapter_info.device_type, &adapter_info.name);
    let can_use_gpu = !cpu_fallback_safe;
    if can_use_gpu {
        eprintln!(
            "Using adapter: {} ({:?})",
            adapter_info.name, adapter_info.device_type
        );
    } else {
        eprintln!(
            "Using adapter: {} ({:?}) - software/CPU adapter detected; GPU-accelerated strategies unavailable",
            adapter_info.name, adapter_info.device_type
        );
    }

    let (render_width, render_height, resolved_antialias) =
        resolve_render_resolution(config.width, config.height, config.antialias);
    let fast_due_to_resolution = render_width >= 1536;
    let fast = config.fast || fast_due_to_resolution || cpu_fallback_safe;
    if fast_due_to_resolution && !config.fast {
        eprintln!(
            "High-resolution run ({render_width}x{render_height}) detected, enabling fast profile for responsiveness."
        );
    }
    if cpu_fallback_safe {
        eprintln!(
            "CPU fallback adapter detected, enabling CPU-safe profile to limit tail latency."
        );
    }
    let fast_profile = apply_cpu_fallback_profile(
        resolve_fast_profile(render_width, config.count, fast),
        render_width,
        config.count,
        cpu_fallback_safe,
    );
    let (render_width, render_height, resolved_antialias, render_scaled) = resolve_fast_resolution(
        render_width,
        render_height,
        resolved_antialias,
        fast_profile,
    );
    if fast
        && (fast_profile.iteration_cap != u32::MAX
            || fast_profile.layer_cap != u32::MAX
            || fast_profile.render_side_cap != u32::MAX)
        && (config.count > 1 || render_width >= 2048 || cpu_fallback_safe)
    {
        eprintln!(
            "Fast profile caps: max iterations {}, max layers {}, render side {}{}.",
            fast_profile.iteration_cap,
            fast_profile.layer_cap,
            fast_profile.render_side_cap,
            if render_scaled {
                " (render capped for safety)"
            } else {
                ""
            }
        );
    }

    let mut gpu: Option<GpuLayerRenderer> = None;

    let pixel_count = (render_width as usize) * (render_height as usize);
    let final_pixel_count = (config.width as usize) * (config.height as usize);
    let mut workspace = RenderWorkspace::new(pixel_count, final_pixel_count);

    let mut image_rng = XorShift32::new(config.seed);
    let mut strategy_scratch = StrategyScratch::default();
    let complexity_metrics_sample_cap = if fast || cpu_fallback_safe {
        Some(16_384usize)
    } else {
        None
    };
    let collect_complexity_metrics = |src: &[f32]| {
        if let Some(sample_cap) = complexity_metrics_sample_cap {
            collect_luma_metrics_sampled(src, render_width, render_height, sample_cap)
        } else {
            collect_luma_metrics(src, render_width, render_height)
        }
    };
    let spinner_state = Arc::new(SpinnerState::new(config.count as usize));
    let user_set_layer_count = config.layers.is_some();
    let (spinner_running, _spinner_handle) = start_spinner(spinner_state.clone());
    let perf_timing_enabled = perf_timing_enabled_from_env();
    if perf_timing_enabled {
        log_progress_message("[perf] timing logs enabled (set COVERGEN_PERF_TIMING=0 to disable)");
    }

    for i in 0..config.count {
        let image_start = Instant::now();
        let image_setup_start = Instant::now();
        let mut layers_total_ms = 0.0f64;
        spinner_state.set_image((i + 1) as usize, 0);
        let mut image_seed = image_rng.next_u32();
        if image_seed == 0 {
            image_seed = 0x9e3779b9;
        }
        let base_seed = image_seed;
        let base_symmetry = randomize_symmetry(config.symmetry, &mut image_rng);
        let mut base_iterations = randomize_iterations(config.iterations, &mut image_rng);
        base_iterations = clamp_iteration_count(base_iterations, fast_profile.iteration_cap);
        let base_fill_scale = randomize_fill_scale(config.fill_scale, &mut image_rng);
        let mut base_symmetry_style = pick_symmetry_style(&mut image_rng);
        if image_rng.next_f32() > (if fast { 0.02 } else { 0.03 }) {
            base_symmetry_style = pick_non_grid_symmetry_style(&mut image_rng).as_u32();
        }
        let base_zoom = randomize_zoom(config.fractal_zoom, &mut image_rng);
        let base_bend_strength = pick_bend_strength(&mut image_rng);
        let base_warp_strength = pick_warp_strength(&mut image_rng);
        let base_warp_frequency = pick_warp_frequency(&mut image_rng);
        let base_tile_scale = pick_tile_scale(&mut image_rng);
        let base_tile_phase = pick_tile_phase(&mut image_rng);
        let (base_center_x, base_center_y) = randomize_center_offset(&mut image_rng, fast);
        let mut layer_count = pick_layer_count(&mut image_rng, config.layers, fast);
        if !user_set_layer_count {
            layer_count = clamp_layer_count(layer_count, fast_profile.layer_cap);
        }
        let mut shader_layer_count = pick_shader_layer_count(layer_count, &mut image_rng, fast);
        spinner_state.set_image((i + 1) as usize, layer_count as usize);
        let base_symmetry_style = SymmetryStyle::from_u32(base_symmetry_style);
        let grid_on_all_layers =
            should_apply_grid_across_layers(base_symmetry_style, layer_count, &mut image_rng, fast);
        let base_symmetry_style =
            resolve_symmetry_style(base_symmetry_style, grid_on_all_layers, &mut image_rng)
                .as_u32();
        let base_art_style = pick_art_style(&mut image_rng);
        let base_art_style_secondary = pick_art_style_secondary(base_art_style, &mut image_rng);
        let base_art_mix = image_rng.next_f32();
        let mut strategy_budget = resolve_strategy_budget(
            render_width,
            render_height,
            layer_count,
            base_iterations,
            fast,
            cpu_fallback_safe,
        );
        let mut base_strategy = pick_render_strategy_with_preferences(
            &mut image_rng,
            fast,
            can_use_gpu,
            strategy_budget,
        );
        if can_use_gpu
            && fast
            && render_width >= 1536
            && let RenderStrategy::Cpu(_) = base_strategy
        {
            base_strategy = RenderStrategy::Gpu(ArtStyle::from_u32(image_rng.next_u32()).as_u32());
        }
        base_strategy = fit_strategy_to_budget(
            &mut image_rng,
            base_strategy,
            strategy_budget,
            fast,
            can_use_gpu,
        );
        let base_strategy_name = base_strategy.label();
        let base_profile = strategy_profile(base_strategy);
        let mut structural_profile =
            should_use_structural_profile(fast, &mut image_rng) || base_profile.force_detail;
        let mut layer_steps = Vec::new();
        let mut active_strategy = base_strategy;

        create_soft_background(
            render_width,
            render_height,
            base_seed ^ (i + 0x0BADC0DEu32),
            &mut workspace.background,
        );
        let background_strength = 0.2 + (image_rng.next_f32() * 0.14);
        let mut pre_filter_stats = LumaStats::default();
        workspace.reset_layered();
        let image_setup_ms = image_setup_start.elapsed().as_secs_f64() * 1000.0;

        for layer_index in 0..layer_count {
            let layer_start = Instant::now();
            spinner_state.set_layer((layer_index + 1) as usize);
            let layer_seed = base_seed.wrapping_add((layer_index + 1).wrapping_mul(0x9e3779b9));
            if layer_index > 0 {
                active_strategy = bias_layer_strategy(
                    active_strategy,
                    &mut image_rng,
                    fast,
                    can_use_gpu,
                    strategy_budget,
                );
                active_strategy = fit_strategy_to_budget(
                    &mut image_rng,
                    active_strategy,
                    strategy_budget,
                    fast,
                    can_use_gpu,
                );
            }
            let layer_strategy = if layer_index == 0 {
                base_strategy
            } else {
                active_strategy
            };
            if render_width >= 1536 && i == 0 && layer_index == 0 {
                let strategy_desc = match layer_strategy {
                    RenderStrategy::Gpu(style) => {
                        format!("gpu:{}", ArtStyle::from_u32(style).label())
                    }
                    RenderStrategy::Cpu(cpu) => format!("cpu:{}", cpu.label()),
                };
                eprintln!(
                    "Image 1/{} layer 1/{} start: {}",
                    config.count, layer_count, strategy_desc
                );
            }
            let strategy_profile = strategy_profile(layer_strategy);
            let layer_force_detail = structural_profile || strategy_profile.force_detail;
            structural_profile = layer_force_detail;

            let mut layer_style = modulate_art_style(base_art_style, &mut image_rng, fast);
            let mut layer_style_secondary =
                modulate_art_style(base_art_style_secondary, &mut image_rng, fast);
            shader_layer_count = pick_shader_layer_count(shader_layer_count, &mut image_rng, fast)
                .max(1 + (layer_index > 0) as u32);
            let symmetry_style = SymmetryStyle::from_u32(modulate_symmetry_style(
                base_symmetry_style,
                &mut image_rng,
                fast,
                grid_on_all_layers,
            ));

            if let RenderStrategy::Gpu(style) = layer_strategy {
                layer_style = ArtStyle::from_u32(style);
                layer_style_secondary = modulate_art_style(
                    ArtStyle::from_u32((style + 1) % ArtStyle::total()),
                    &mut image_rng,
                    fast,
                );
            }

            let params = Params {
                width: render_width,
                height: render_height,
                symmetry: modulate_symmetry(base_symmetry, &mut image_rng, fast),
                symmetry_style: symmetry_style.as_u32(),
                iterations: clamp_iteration_count(
                    modulate_iterations(base_iterations, &mut image_rng, fast),
                    fast_profile.iteration_cap,
                ),
                seed: layer_seed,
                fill_scale: modulate_fill_scale(base_fill_scale, &mut image_rng, fast),
                fractal_zoom: modulate_zoom(base_zoom, &mut image_rng, fast),
                bend_strength: modulate_bend_strength(base_bend_strength, &mut image_rng, fast),
                warp_strength: modulate_warp_strength(base_warp_strength, &mut image_rng, fast),
                warp_frequency: modulate_warp_frequency(base_warp_frequency, &mut image_rng, fast),
                tile_scale: modulate_tile_scale(
                    base_tile_scale,
                    symmetry_style == SymmetryStyle::Grid,
                    &mut image_rng,
                    fast,
                ),
                tile_phase: modulate_tile_phase(base_tile_phase, &mut image_rng, fast),
                center_x: modulate_center_offset(base_center_x, &mut image_rng, fast),
                center_y: modulate_center_offset(base_center_y, &mut image_rng, fast),
                art_style: layer_style.as_u32(),
                art_style_secondary: layer_style_secondary.as_u32(),
                art_style_mix: modulate_style_mix(base_art_mix, &mut image_rng, fast),
                layer_count: modulate_shader_layer_count(
                    shader_layer_count,
                    &mut image_rng,
                    fast,
                    layer_force_detail,
                ),
            };

            let layer_cost = render_strategy_cost(layer_strategy);
            let layer_complexity_budget = params.iterations.min(strategy_budget.max(96));
            let layer_render_start = Instant::now();
            if matches!(layer_strategy, RenderStrategy::Gpu(_)) {
                ensure_gpu_renderer(&adapter, &mut gpu, render_width, render_height).await?;
            }
            render_strategy_layer(
                layer_strategy,
                &params,
                &mut workspace.luma,
                layer_complexity_budget,
                fast,
                render_width,
                render_height,
                &mut gpu,
                &mut strategy_scratch,
            )?;
            let layer_render_elapsed = layer_render_start.elapsed();
            let layer_render_ms = layer_render_elapsed.as_secs_f64() * 1000.0;
            record_strategy_runtime(layer_strategy, layer_render_elapsed);
            strategy_budget = strategy_budget.saturating_sub(layer_cost);
            if layer_index == 0 {
                pre_filter_stats =
                    collect_luma_metrics(&workspace.luma, render_width, render_height).stats;
            }

            let filter = tune_filter_for_speed(pick_filter_from_rng(&mut image_rng), fast);
            let gradient = pick_gradient_from_rng(&mut image_rng);
            let overlay = pick_layer_blend(&mut image_rng);
            let layer_contrast = pick_layer_contrast(&mut image_rng, fast);
            let apply_filter = should_apply_dynamic_filter(
                layer_index,
                &mut image_rng,
                fast,
                layer_force_detail,
                strategy_profile.filter_bias,
            );
            let apply_gradient = should_apply_gradient_map(
                layer_index,
                &mut image_rng,
                fast,
                layer_force_detail,
                strategy_profile.gradient_bias,
            );
            let opacity = if layer_index == 0 {
                1.0
            } else {
                layer_opacity(&mut image_rng)
            };
            let mut complexity_fixed = false;
            let mut layer_mix_desc = String::new();
            let mut layer_mix_ms = 0.0f64;
            let apply_strategy_mix = blending::should_mix_strategies(
                layer_index,
                &mut image_rng,
                fast,
                structural_profile,
                strategy_profile.filter_bias.max(0.5),
            );
            if apply_strategy_mix {
                let layer_mix_start = Instant::now();
                let secondary_strategy = blending::pick_blended_strategy(
                    layer_strategy,
                    &mut image_rng,
                    fast,
                    can_use_gpu,
                    strategy_budget,
                );
                let secondary_strategy = fit_strategy_to_budget(
                    &mut image_rng,
                    secondary_strategy,
                    strategy_budget,
                    fast,
                    can_use_gpu,
                );
                let secondary_seed = layer_seed ^ 0x91A5_FD3Bu32;
                let mut secondary_params = params;
                secondary_params.seed = secondary_seed;
                secondary_params.iterations = clamp_iteration_count(
                    modulate_iterations(params.iterations, &mut image_rng, fast),
                    fast_profile.iteration_cap,
                );
                if let RenderStrategy::Gpu(style) = secondary_strategy {
                    secondary_params.art_style = style;
                    secondary_params.art_style_secondary = modulate_art_style(
                        ArtStyle::from_u32((style + 1) % ArtStyle::total()),
                        &mut image_rng,
                        fast,
                    )
                    .as_u32();
                    secondary_params.art_style_mix =
                        modulate_style_mix(params.art_style_mix, &mut image_rng, fast);
                }
                let secondary_cost = render_strategy_cost(secondary_strategy);
                let secondary_budget = secondary_params.iterations.min(strategy_budget.max(96));
                let secondary_is_gpu = matches!(secondary_strategy, RenderStrategy::Gpu(_));
                let mut secondary_submit_elapsed = None;
                if secondary_is_gpu {
                    let secondary_submit_start = Instant::now();
                    ensure_gpu_renderer(&adapter, &mut gpu, render_width, render_height).await?;
                    let renderer = gpu
                        .as_mut()
                        .ok_or("gpu renderer unavailable for secondary layer submission")?;
                    renderer.submit_layer(&secondary_params)?;
                    secondary_submit_elapsed = Some(secondary_submit_start.elapsed());
                } else {
                    let secondary_render_start = Instant::now();
                    render_strategy_layer(
                        secondary_strategy,
                        &secondary_params,
                        &mut workspace.blend_secondary,
                        secondary_budget,
                        fast,
                        render_width,
                        render_height,
                        &mut gpu,
                        &mut strategy_scratch,
                    )?;
                    record_strategy_runtime(secondary_strategy, secondary_render_start.elapsed());
                }
                strategy_budget = strategy_budget.saturating_sub(secondary_cost);
                let mask_kind = blending::pick_layer_mask_kind(&mut image_rng, structural_profile);
                let mut mask_request = blending::LayerMaskBuildRequest {
                    primary: &workspace.luma,
                    width: render_width,
                    height: render_height,
                    source_seed: secondary_seed,
                    kind: mask_kind,
                    out: &mut workspace.mix_mask,
                    blur_work: &mut workspace.mask_workspace,
                    fast,
                };
                blending::build_layer_mask(&mut mask_request, &mut image_rng);
                if secondary_is_gpu {
                    let secondary_collect_start = Instant::now();
                    let renderer = gpu
                        .as_mut()
                        .ok_or("gpu renderer unavailable for secondary layer readback")?;
                    renderer.collect_layer(&mut workspace.blend_secondary)?;
                    if let Some(submit_elapsed) = secondary_submit_elapsed {
                        record_strategy_runtime(
                            secondary_strategy,
                            submit_elapsed + secondary_collect_start.elapsed(),
                        );
                    }
                }
                blending::blend_with_mask(
                    &mut workspace.luma,
                    &workspace.blend_secondary,
                    &workspace.mix_mask,
                    image_rng.next_f32() < 0.2,
                );
                layer_mix_ms = layer_mix_start.elapsed().as_secs_f64() * 1000.0;
                layer_mix_desc = format!(
                    " mix:{}:{}({})",
                    strategy_name(layer_strategy),
                    strategy_name(secondary_strategy),
                    mask_kind.label()
                );
            }
            let layer_post_start = Instant::now();

            if apply_filter {
                apply_dynamic_filter(
                    render_width,
                    render_height,
                    &workspace.luma,
                    &mut workspace.filtered,
                    &filter,
                );
                let low_stretch = if fast { 0.03 } else { 0.04 };
                let high_stretch = if fast { 0.97 } else { 0.96 };
                stretch_to_percentile(
                    &mut workspace.filtered,
                    &mut workspace.percentile,
                    low_stretch,
                    high_stretch,
                    fast,
                );
            } else {
                workspace.filtered.copy_from_slice(&workspace.luma);
                stretch_to_percentile(
                    &mut workspace.filtered,
                    &mut workspace.percentile,
                    if fast { 0.02 } else { 0.03 },
                    if fast { 0.98 } else { 0.97 },
                    fast,
                );
            }

            if apply_filter && layer_force_detail && image_rng.next_f32() < 0.5 {
                apply_detail_waves(
                    &mut workspace.filtered,
                    render_width,
                    render_height,
                    layer_seed ^ 0x4D4E_4446,
                    if fast { 0.03 } else { 0.05 },
                );
                apply_sharpen(
                    render_width,
                    render_height,
                    &workspace.filtered,
                    &mut workspace.detail,
                    if fast { 0.32 } else { 0.58 },
                );
                std::mem::swap(&mut workspace.filtered, &mut workspace.detail);
            }

            if apply_gradient {
                apply_gradient_map(&mut workspace.filtered, gradient);
                stretch_to_percentile(
                    &mut workspace.filtered,
                    &mut workspace.percentile,
                    if fast { 0.01 } else { 0.02 },
                    if fast { 0.99 } else { 0.98 },
                    fast,
                );
            }
            let mut no_filter_posterize_bands = None;
            if !apply_filter && !apply_gradient {
                if structural_profile {
                    apply_detail_waves(
                        &mut workspace.filtered,
                        render_width,
                        render_height,
                        layer_seed ^ 0x2f7f_8d3d,
                        if fast { 0.05 } else { 0.09 },
                    );
                } else if image_rng.next_f32() < 0.35 {
                    apply_detail_waves(
                        &mut workspace.filtered,
                        render_width,
                        render_height,
                        layer_seed ^ 0x9d7e_4f2a,
                        if fast { 0.04 } else { 0.07 },
                    );
                }

                apply_sharpen(
                    render_width,
                    render_height,
                    &workspace.filtered,
                    &mut workspace.detail,
                    if structural_profile {
                        if fast { 0.72 } else { 1.12 }
                    } else if fast {
                        0.45
                    } else {
                        0.75
                    },
                );
                std::mem::swap(&mut workspace.filtered, &mut workspace.detail);
                no_filter_posterize_bands =
                    Some(2 + (image_rng.next_u32() % if structural_profile { 7 } else { 5 }));
            }
            let layer_contrast = if apply_filter || apply_gradient {
                layer_contrast
            } else {
                layer_contrast * 0.75
            };
            if let Some(bands) = no_filter_posterize_bands {
                apply_posterize_and_contrast(
                    &mut workspace.filtered,
                    bands,
                    layer_contrast.max(1.0),
                );
            } else {
                apply_contrast(&mut workspace.filtered, layer_contrast.max(1.0));
            }
            let layer_metrics = collect_complexity_metrics(&workspace.filtered);
            if needs_complexity_fix(&layer_metrics.stats, layer_metrics.edge_energy) {
                complexity_fixed = true;
                apply_detail_waves(
                    &mut workspace.filtered,
                    render_width,
                    render_height,
                    layer_seed ^ 0x4445_6d63,
                    if fast { 0.10 } else { 0.18 },
                );
                apply_sharpen(
                    render_width,
                    render_height,
                    &workspace.filtered,
                    &mut workspace.detail,
                    if fast { 0.55 } else { 0.9 },
                );
                std::mem::swap(&mut workspace.filtered, &mut workspace.detail);
                apply_posterize_and_contrast(
                    &mut workspace.filtered,
                    2 + (image_rng.next_u32() % 6),
                    1.25 + (image_rng.next_f32() * 0.45),
                );
            }

            if layer_index == 0 {
                workspace.layered.copy_from_slice(&workspace.filtered);
            } else {
                blend_layer_stack(
                    &mut workspace.layered,
                    &workspace.filtered,
                    opacity,
                    overlay,
                );
            }

            let layer_strategy_name = match layer_strategy {
                RenderStrategy::Cpu(cpu) => format!("[{}]", cpu.label()),
                RenderStrategy::Gpu(_) => String::new(),
            };
            let filter_name = if apply_filter {
                filter.mode.label()
            } else {
                "none"
            };

            layer_steps.push(format!(
                "L{}:{}({:.2}, f{}{}, g{}, d{}, c{:.2}) S{}+{}:{:.2}",
                layer_index + 1,
                overlay.label(),
                opacity,
                filter_name,
                layer_strategy_name,
                if apply_gradient { "on" } else { "off" },
                if complexity_fixed { "on" } else { "off" },
                layer_contrast,
                ArtStyle::from_u32(params.art_style).label(),
                ArtStyle::from_u32(params.art_style_secondary).label(),
                params.art_style_mix,
            ));
            if !layer_mix_desc.is_empty() {
                layer_steps.push(format!("M{}", layer_mix_desc));
            }
            let layer_post_ms = layer_post_start.elapsed().as_secs_f64() * 1000.0;
            let layer_total_ms = layer_start.elapsed().as_secs_f64() * 1000.0;
            layers_total_ms += layer_total_ms;
            if perf_timing_enabled {
                log_progress_message(&format!(
                    "[perf] image {}/{} layer {}/{} | render {:.2}ms | mix {:.2}ms | post {:.2}ms | total {:.2}ms",
                    i + 1,
                    config.count,
                    layer_index + 1,
                    layer_count,
                    layer_render_ms,
                    layer_mix_ms,
                    layer_post_ms,
                    layer_total_ms
                ));
            }
        }

        let image_finalize_start = Instant::now();
        blend_background(
            &mut workspace.layered,
            &workspace.background,
            background_strength,
        );
        let final_contrast = if fast { 1.45 } else { 1.8 };
        apply_contrast(&mut workspace.layered, final_contrast);
        stretch_to_percentile(
            &mut workspace.layered,
            &mut workspace.percentile,
            0.01,
            0.99,
            fast,
        );

        let mut final_decision_metrics = collect_complexity_metrics(&workspace.layered);
        let mut final_complexity_fixed = false;
        if needs_complexity_fix(
            &final_decision_metrics.stats,
            final_decision_metrics.edge_energy,
        ) {
            final_complexity_fixed = true;
            apply_detail_waves(
                &mut workspace.layered,
                render_width,
                render_height,
                base_seed ^ (i + 0x445f_6e65),
                if fast { 0.08 } else { 0.14 },
            );
            apply_sharpen(
                render_width,
                render_height,
                &workspace.layered,
                &mut workspace.detail,
                if fast { 0.45 } else { 0.75 },
            );
            std::mem::swap(&mut workspace.layered, &mut workspace.detail);
            apply_posterize_and_contrast(
                &mut workspace.layered,
                if fast { 4 } else { 5 },
                if fast { 1.2 } else { 1.4 },
            );
            final_decision_metrics = collect_complexity_metrics(&workspace.layered);
        }
        if final_decision_metrics.stats.std < 0.09
            || (final_decision_metrics.stats.max - final_decision_metrics.stats.min) < 0.23
        {
            inject_noise(
                &mut workspace.layered,
                base_seed ^ (i + 1),
                if fast { 0.04 } else { 0.06 },
            );
            stretch_to_percentile(
                &mut workspace.layered,
                &mut workspace.percentile,
                0.01,
                0.99,
                fast,
            );
        }
        let final_metrics = collect_luma_metrics(&workspace.layered, render_width, render_height);
        let image_finalize_ms = image_finalize_start.elapsed().as_secs_f64() * 1000.0;

        let output_stage_start = Instant::now();
        let output_luma = if resolved_antialias == 1
            && render_width == config.width
            && render_height == config.height
        {
            &workspace.layered
        } else {
            downsample_luma(
                &workspace.layered,
                render_width,
                render_height,
                config.width,
                config.height,
                &mut workspace.final_luma,
                &mut workspace.downsample_source_u8,
            )?;
            workspace.final_luma.as_slice()
        };
        encode_gray(&mut workspace.final_pixels, output_luma);
        let final_output = resolve_output_path(&config.output);
        let (final_width, final_height, final_bytes) = save_png_under_10mb(
            &final_output,
            config.width,
            config.height,
            &workspace.final_pixels,
        )?;
        let scale = format!(
            "{:.2}",
            if final_width == config.width {
                1.0
            } else {
                (final_width as f32) / (config.width as f32)
            }
        );

        let layer_summary = if layer_steps.is_empty() {
            "none".to_string()
        } else {
            layer_steps.join(", ")
        };

        println!(
            "Generated {} | index {} | seed {} | fill {:.2} | zoom {:.2} | symmetry {} [{}] center({:.2},{:.2}) | iterations {} | strategy {} | final d{} | layers {} | layers [{}] | image {}x{} (aa {}) (scale {} / {:.2}MB) | pre({:.2}-{:.2},{:.2}) post({:.2}-{:.2},{:.2})",
            final_output.display(),
            i,
            base_seed,
            base_fill_scale,
            base_zoom,
            base_symmetry,
            SymmetryStyle::from_u32(base_symmetry_style).label(),
            base_center_x,
            base_center_y,
            base_iterations,
            base_strategy_name,
            if final_complexity_fixed { "on" } else { "off" },
            layer_count,
            layer_summary,
            final_width,
            final_height,
            resolved_antialias,
            scale,
            final_bytes as f64 / (1024.0 * 1024.0),
            pre_filter_stats.min,
            pre_filter_stats.max,
            pre_filter_stats.mean,
            final_metrics.stats.min,
            final_metrics.stats.max,
            final_metrics.stats.mean
        );
        let output_stage_ms = output_stage_start.elapsed().as_secs_f64() * 1000.0;
        if perf_timing_enabled {
            log_progress_message(&format!(
                "[perf] image {}/{} summary | setup {:.2}ms | layers {:.2}ms | finalize {:.2}ms | output {:.2}ms | total {:.2}ms",
                i + 1,
                config.count,
                image_setup_ms,
                layers_total_ms,
                image_finalize_ms,
                output_stage_ms,
                image_start.elapsed().as_secs_f64() * 1000.0
            ));
        }
    }
    spinner_running.store(false, Ordering::Release);
    let _ = write!(io::stderr(), "\r{:<120}\r", "");
    let _ = io::stderr().flush();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_software_adapter, parse_perf_timing_flag};

    #[test]
    fn classifies_cpu_and_virtual_as_software() {
        assert!(is_software_adapter(wgpu::DeviceType::Cpu, "cpu"));
        assert!(is_software_adapter(wgpu::DeviceType::VirtualGpu, "virtual"));
    }

    #[test]
    fn classifies_known_software_driver_names() {
        assert!(is_software_adapter(
            wgpu::DeviceType::Other,
            "llvmpipe (LLVM 16.0.6, 256 bits)"
        ));
        assert!(is_software_adapter(
            wgpu::DeviceType::Other,
            "Microsoft Basic Render Driver"
        ));
    }

    #[test]
    fn keeps_hardware_adapters_gpu_capable() {
        assert!(!is_software_adapter(
            wgpu::DeviceType::DiscreteGpu,
            "NVIDIA GeForce RTX"
        ));
        assert!(!is_software_adapter(
            wgpu::DeviceType::IntegratedGpu,
            "Intel Iris Xe"
        ));
    }

    #[test]
    fn parses_perf_timing_flag_values() {
        assert!(parse_perf_timing_flag("1"));
        assert!(parse_perf_timing_flag("true"));
        assert!(parse_perf_timing_flag("YES"));
        assert!(parse_perf_timing_flag("on"));
        assert!(!parse_perf_timing_flag("0"));
        assert!(!parse_perf_timing_flag("false"));
    }
}

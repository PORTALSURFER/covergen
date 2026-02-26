//! GPU executor for compiled V2 graphs.
//!
//! The runtime orchestrates per-image execution, output finalization, and
//! animation frame encoding. Node evaluation logic lives in `runtime_eval`
//! (test-only CPU validation path) and `runtime_gpu` (retained GPU path).

use std::error::Error;
use std::path::Path;
use std::time::Instant;

use crate::gpu_render::GpuLayerRenderer;
use crate::image_ops::{encode_png_bytes, resolve_output_path, save_png_under_10mb};
use crate::telemetry;
use image::codecs::png::CompressionType;

use super::animation::{
    clip_output_path, create_frame_dir, encode_frames_to_mp4, frame_filename, total_frames,
    RawVideoEncoder, StreamFrameFormat,
};
use super::compiler::CompiledGraph;
use super::node::GraphTimeInput;
use super::runtime_config::{AnimationMotion, V2Config, V2Profile};
use super::runtime_gpu::render_graph_luma_gpu;
use super::runtime_progress::{finish_animation_progress_line, print_animation_progress};
use super::runtime_selection::{execute_still_with_selection, should_use_selection};

/// Reusable image buffers for V2 execution.
pub(crate) struct RuntimeBuffers {
    #[cfg(test)]
    pub layered: Vec<f32>,
    #[cfg(test)]
    pub percentile: Vec<f32>,
    #[cfg(test)]
    pub layer_scratch: Vec<f32>,
    #[cfg(test)]
    pub final_luma: Vec<f32>,
    #[cfg(test)]
    pub downsample_scratch: Vec<u8>,
    pub output_gray: Vec<u8>,
    pub output_bgra: Vec<u8>,
}

/// Execute a compiled graph for all requested output images.
pub async fn execute_compiled(
    config: &V2Config,
    compiled: &CompiledGraph,
    low_res_explore: Option<(&V2Config, &CompiledGraph)>,
) -> Result<(), Box<dyn Error>> {
    telemetry::snapshot_memory("v2.run.start");
    let mut renderer = create_renderer(config, compiled).await?;
    let alias_start = Instant::now();
    renderer.ensure_node_alias_buffers(
        compiled.resource_plan.gpu_peak_luma_slots,
        compiled.resource_plan.gpu_peak_mask_slots,
    )?;
    renderer.ensure_node_feedback_buffers(compiled.feedback_slots.len())?;
    telemetry::record_timing("v2.gpu.alias_buffers.init", alias_start.elapsed());

    let mut buffers =
        create_runtime_buffers(compiled.width, compiled.height, config.width, config.height)?;

    if config.animation.enabled {
        return execute_animation(config, compiled, &mut renderer, &mut buffers);
    }

    if should_use_selection(&config.selection, low_res_explore) {
        if let Some((low_res_config, low_res_compiled)) = low_res_explore {
            return execute_still_with_selection(
                config,
                compiled,
                &mut renderer,
                &mut buffers,
                low_res_config,
                low_res_compiled,
            )
            .await;
        }
    }

    for image_index in 0..config.count {
        let image_start = Instant::now();
        telemetry::snapshot_memory(format!("v2.image.{image_index}.start"));
        let image_seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(image_index.wrapping_mul(0x9E37_79B9));
        renderer.reset_feedback_state()?;

        let render_start = Instant::now();
        render_graph_frame(compiled, &mut renderer, image_seed_offset, None)?;
        telemetry::record_timing("v2.image.render", render_start.elapsed());
        let finalize_start = Instant::now();
        finalize_luma_for_output(config, &mut renderer, &mut buffers)?;
        telemetry::record_timing("v2.image.finalize", finalize_start.elapsed());

        let output_start = Instant::now();
        let indexed_output = indexed_output(&config.output, image_index, config.count);
        let output_path = resolve_output_path(&indexed_output.to_string_lossy());
        let (w, h, bytes) = save_png_under_10mb(
            &output_path,
            config.width,
            config.height,
            &buffers.output_gray,
        )?;
        telemetry::record_timing("v2.image.output", output_start.elapsed());
        telemetry::record_timing("v2.image.total", image_start.elapsed());
        telemetry::snapshot_memory(format!("v2.image.{image_index}.end"));

        println!(
            "[v2] generated {} | graph {}x{} -> output {}x{} | nodes {} | outputs {} | {:.2}MB",
            output_path.display(),
            compiled.width,
            compiled.height,
            w,
            h,
            compiled.steps.len(),
            compiled.output_bindings.len(),
            bytes as f64 / (1024.0 * 1024.0)
        );
    }
    telemetry::snapshot_memory("v2.run.end");

    Ok(())
}

/// Allocate reusable runtime buffers for one graph/output shape.
pub(crate) fn create_runtime_buffers(
    _graph_width: u32,
    _graph_height: u32,
    output_width: u32,
    output_height: u32,
) -> Result<RuntimeBuffers, Box<dyn Error>> {
    Ok(RuntimeBuffers {
        #[cfg(test)]
        layered: vec![0.0f32; pixel_count(_graph_width, _graph_height)?],
        #[cfg(test)]
        percentile: vec![0.0f32; pixel_count(_graph_width, _graph_height)?],
        #[cfg(test)]
        layer_scratch: vec![0.0f32; pixel_count(_graph_width, _graph_height)?],
        #[cfg(test)]
        final_luma: vec![0.0f32; pixel_count(output_width, output_height)?],
        #[cfg(test)]
        downsample_scratch: Vec::new(),
        output_gray: vec![0u8; pixel_count(output_width, output_height)?],
        output_bgra: vec![0u8; pixel_count(output_width, output_height)?.saturating_mul(4)],
    })
}

fn execute_animation(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: &mut GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
) -> Result<(), Box<dyn Error>> {
    let frames = total_frames(&config.animation);
    print_animation_progress(0, frames, 0.0, config.count, 0);
    for clip_index in 0..config.count {
        let clip_start = Instant::now();
        telemetry::snapshot_memory(format!("v2.animation.clip.{clip_index}.start"));
        let frame_dir = if config.animation.keep_frames {
            Some(create_frame_dir(&config.output, clip_index)?)
        } else {
            None
        };
        let clip_path = clip_output_path(&config.output, clip_index, config.count);
        let mut stream_encoder = if frame_dir.is_none() {
            Some(RawVideoEncoder::spawn(
                config.width,
                config.height,
                config.animation.fps,
                &clip_path,
            )?)
        } else {
            None
        };
        if let Some(encoder) = stream_encoder.as_ref() {
            println!(
                "[v2] stream export frame format {:?} | data path {:?} | zero-readback active: false (planned target architecture)",
                encoder.frame_format(),
                encoder.data_path()
            );
        }
        let clip_seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(clip_index.wrapping_mul(0x6A09_E667));
        let motion = config.animation.motion;
        let modulation_intensity = motion.modulation_intensity();
        let use_seed_jitter = motion.use_seed_jitter();
        renderer.reset_feedback_state()?;

        for frame_index in 0..frames {
            let frame_start = Instant::now();
            let frame_seed_offset = if use_seed_jitter {
                clip_seed_offset.wrapping_add(frame_index.wrapping_mul(0x9E37_79B9))
            } else {
                clip_seed_offset
            };
            let graph_time = apply_motion_temporal_constraints(
                GraphTimeInput::from_frame(frame_index, frames)
                    .with_intensity(modulation_intensity),
                motion,
            );

            if let Some(encoder) = stream_encoder.as_mut() {
                render_graph_frame(compiled, renderer, frame_seed_offset, Some(graph_time))?;
                write_stream_frame(config, renderer, buffers, encoder)?;
            } else if let Some(dir) = frame_dir.as_ref() {
                render_graph_frame(compiled, renderer, frame_seed_offset, Some(graph_time))?;
                finalize_luma_for_output(config, renderer, buffers)?;
                let encoded = encode_png_bytes(
                    config.width,
                    config.height,
                    &buffers.output_gray,
                    CompressionType::Fast,
                )?;
                let frame_path = dir.join(frame_filename(frame_index));
                std::fs::write(frame_path, encoded)?;
            } else {
                return Err("animation encoder path is not configured".into());
            }
            let frame_elapsed = frame_start.elapsed();
            telemetry::record_timing("v2.animation.frame.total", frame_elapsed);
            telemetry::record_frame("v2.animation.frame.total", frame_elapsed);
            print_animation_progress(
                frame_index + 1,
                frames,
                clip_start.elapsed().as_secs_f64(),
                config.count,
                clip_index,
            );
        }
        finish_animation_progress_line();

        if let Some(encoder) = stream_encoder {
            encoder.finish()?;
        } else if let Some(dir) = frame_dir.as_ref() {
            encode_frames_to_mp4(dir, config.animation.fps, &clip_path)?;
            if !config.animation.keep_frames {
                std::fs::remove_dir_all(dir)?;
            }
        } else {
            return Err("animation finalize path is not configured".into());
        }

        println!(
            "[v2] animation {} | {}s @ {}fps | {} frames | {}",
            clip_index + 1,
            config.animation.seconds,
            config.animation.fps,
            frames,
            clip_path.display()
        );
        telemetry::record_timing("v2.animation.clip.total", clip_start.elapsed());
        telemetry::snapshot_memory(format!("v2.animation.clip.{clip_index}.end"));
    }
    Ok(())
}

fn write_stream_frame(
    config: &V2Config,
    renderer: &mut GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
    encoder: &mut RawVideoEncoder,
) -> Result<(), Box<dyn Error>> {
    match encoder.frame_format() {
        StreamFrameFormat::Gray8 => {
            finalize_luma_for_output(config, renderer, buffers)?;
            encoder.write_gray_frame(&buffers.output_gray)?;
        }
        StreamFrameFormat::Bgra8 => {
            finalize_bgra_for_output(config, renderer, buffers)?;
            encoder.write_bgra_frame(&buffers.output_bgra)?;
        }
    }
    Ok(())
}

/// Run final retained-output post-processing and copy grayscale output bytes.
pub(crate) fn finalize_luma_for_output(
    config: &V2Config,
    renderer: &mut GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
) -> Result<(), Box<dyn Error>> {
    let final_contrast = match config.profile {
        V2Profile::Quality => 1.45,
        V2Profile::Performance => 1.25,
    };
    let low_pct = if matches!(config.profile, V2Profile::Performance) {
        0.02
    } else {
        0.01
    };
    let fast_mode = matches!(config.profile, V2Profile::Performance);

    let retained_finalize_start = Instant::now();
    renderer.collect_retained_output_gray(
        &mut buffers.output_gray,
        final_contrast,
        low_pct,
        0.99,
        fast_mode,
    )?;
    telemetry::record_timing(
        "v2.gpu.node.finalize_retained_output",
        retained_finalize_start.elapsed(),
    );
    Ok(())
}

/// Run final retained-output post-processing and copy BGRA output bytes.
pub(crate) fn finalize_bgra_for_output(
    config: &V2Config,
    renderer: &mut GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
) -> Result<(), Box<dyn Error>> {
    let final_contrast = match config.profile {
        V2Profile::Quality => 1.45,
        V2Profile::Performance => 1.25,
    };
    let low_pct = if matches!(config.profile, V2Profile::Performance) {
        0.02
    } else {
        0.01
    };
    let fast_mode = matches!(config.profile, V2Profile::Performance);

    let retained_finalize_start = Instant::now();
    renderer.collect_retained_output_bgra(
        &mut buffers.output_bgra,
        final_contrast,
        low_pct,
        0.99,
        fast_mode,
    )?;
    telemetry::record_timing(
        "v2.gpu.node.finalize_retained_output",
        retained_finalize_start.elapsed(),
    );
    Ok(())
}

/// Render one graph frame/image into retained GPU buffers.
pub(crate) fn render_graph_frame(
    compiled: &CompiledGraph,
    renderer: &mut GpuLayerRenderer,
    seed_offset: u32,
    modulation: Option<GraphTimeInput>,
) -> Result<(), Box<dyn Error>> {
    render_graph_luma_gpu(compiled, renderer, seed_offset, modulation)
}

/// Create a hardware-GPU renderer for one compiled graph shape.
pub(crate) async fn create_renderer(
    config: &V2Config,
    compiled: &CompiledGraph,
) -> Result<GpuLayerRenderer, Box<dyn Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .ok_or({
            "covergen requires a hardware GPU adapter; no GPU adapter was detected. \
            install GPU drivers and run on a machine with an available hardware GPU."
        })?;

    let info = adapter.get_info();
    if is_software_adapter(info.device_type, &info.name) {
        return Err(format!(
            "covergen requires a hardware GPU adapter; software adapter '{} ({:?})' is not supported. \
            use a system with an active integrated/discrete GPU and current graphics drivers.",
            info.name, info.device_type
        )
        .into());
    }

    GpuLayerRenderer::new_with_output(
        &adapter,
        compiled.width,
        compiled.height,
        config.width,
        config.height,
    )
    .await
}

/// Resolve indexed output path for multi-image runs.
pub(crate) fn indexed_output(base: &str, index: u32, total: u32) -> std::path::PathBuf {
    if total <= 1 {
        return Path::new(base).to_path_buf();
    }
    clip_output_path(base, index, total)
}

fn pixel_count(width: u32, height: u32) -> Result<usize, Box<dyn Error>> {
    width
        .checked_mul(height)
        .map(|count| count as usize)
        .ok_or("invalid pixel dimensions".into())
}

fn is_software_adapter(device_type: wgpu::DeviceType, adapter_name: &str) -> bool {
    if matches!(
        device_type,
        wgpu::DeviceType::Cpu | wgpu::DeviceType::VirtualGpu
    ) {
        return true;
    }

    let name = adapter_name.to_ascii_lowercase();
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
    .any(|needle| name.contains(needle))
}

/// Apply motion-profile temporal constraints to one graph-time sample.
pub(crate) fn apply_motion_temporal_constraints(
    time: GraphTimeInput,
    motion: AnimationMotion,
) -> GraphTimeInput {
    let (envelope, slew_limit) = match motion {
        AnimationMotion::Gentle => ((-0.35, 0.35), Some(0.02)),
        AnimationMotion::Normal => ((-0.6, 0.6), Some(0.05)),
        AnimationMotion::Wild => ((-1.0, 1.0), Some(0.12)),
    };
    let constrained = time.with_envelope(envelope.0, envelope.1);
    if let Some(limit) = slew_limit {
        constrained.with_slew_limit(limit)
    } else {
        constrained
    }
}

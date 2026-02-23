//! GPU executor for compiled V2 graphs.
//!
//! The runtime orchestrates per-image execution, output finalization, and
//! animation frame encoding. Node evaluation logic lives in `runtime_eval`.

use std::error::Error;
use std::path::Path;
use std::time::Instant;

use crate::gpu_render::GpuLayerRenderer;
use crate::image_ops::{
    apply_contrast, downsample_luma, encode_gray, encode_png_bytes, resolve_output_path,
    save_png_under_10mb, stretch_to_percentile,
};
use crate::telemetry;
use image::codecs::png::CompressionType;

use super::animation::{
    RawVideoEncoder, clip_output_path, create_frame_dir, encode_frames_to_mp4, frame_filename,
    total_frames,
};
use super::cli::{V2Config, V2Profile};
use super::compiler::{CompiledGraph, CompiledOp};
use super::node::GraphTimeInput;
use super::runtime_eval::render_graph_luma;

/// Reusable image buffers for V2 execution.
pub(crate) struct RuntimeBuffers {
    pub layered: Vec<f32>,
    pub percentile: Vec<f32>,
    pub layer_scratch: Vec<f32>,
    pub final_luma: Vec<f32>,
    pub downsample_scratch: Vec<u8>,
    pub output_gray: Vec<u8>,
}

/// Execute a compiled graph for all requested output images.
pub async fn execute_compiled(
    config: &V2Config,
    compiled: &CompiledGraph,
) -> Result<(), Box<dyn Error>> {
    telemetry::snapshot_memory("v2.run.start");
    let needs_gpu = graph_uses_gpu_generation(compiled);
    let mut renderer = if needs_gpu {
        Some(create_renderer(config, compiled).await?)
    } else {
        None
    };

    let mut buffers = RuntimeBuffers {
        layered: vec![0.0f32; pixel_count(compiled.width, compiled.height)?],
        percentile: vec![0.0f32; pixel_count(compiled.width, compiled.height)?],
        layer_scratch: vec![0.0f32; pixel_count(compiled.width, compiled.height)?],
        final_luma: vec![0.0f32; pixel_count(config.width, config.height)?],
        downsample_scratch: Vec::new(),
        output_gray: vec![0u8; pixel_count(config.width, config.height)?],
    };

    if config.animation.enabled {
        return execute_animation(config, compiled, renderer.as_mut(), &mut buffers);
    }

    for image_index in 0..config.count {
        let image_start = Instant::now();
        telemetry::snapshot_memory(format!("v2.image.{image_index}.start"));
        let image_seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(image_index.wrapping_mul(0x9E37_79B9));

        let render_start = Instant::now();
        render_graph_luma(
            compiled,
            renderer.as_mut(),
            &mut buffers,
            image_seed_offset,
            None,
        )?;
        telemetry::record_timing("v2.image.render", render_start.elapsed());
        let finalize_start = Instant::now();
        finalize_luma_for_output(config, compiled, renderer.as_mut(), &mut buffers)?;
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
            "[v2] generated {} | graph {}x{} -> output {}x{} | nodes {} | {:.2}MB",
            output_path.display(),
            compiled.width,
            compiled.height,
            w,
            h,
            compiled.steps.len(),
            bytes as f64 / (1024.0 * 1024.0)
        );
    }
    telemetry::snapshot_memory("v2.run.end");

    Ok(())
}

fn execute_animation(
    config: &V2Config,
    compiled: &CompiledGraph,
    mut renderer: Option<&mut GpuLayerRenderer>,
    buffers: &mut RuntimeBuffers,
) -> Result<(), Box<dyn Error>> {
    let frames = total_frames(&config.animation);
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
        let clip_seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(clip_index.wrapping_mul(0x6A09_E667));

        for frame_index in 0..frames {
            let frame_start = Instant::now();
            let frame_seed_offset =
                clip_seed_offset.wrapping_add(frame_index.wrapping_mul(0x9E37_79B9));
            let graph_time = GraphTimeInput::from_frame(frame_index, frames);

            render_graph_luma(
                compiled,
                renderer.as_deref_mut(),
                buffers,
                frame_seed_offset,
                Some(graph_time),
            )?;
            finalize_luma_for_output(config, compiled, renderer.as_deref_mut(), buffers)?;
            if let Some(encoder) = stream_encoder.as_mut() {
                encoder.write_gray_frame(&buffers.output_gray)?;
            } else if let Some(dir) = frame_dir.as_ref() {
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
        }

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

fn finalize_luma_for_output(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: Option<&mut GpuLayerRenderer>,
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

    if compiled.can_use_retained_layer_path {
        let renderer = renderer.ok_or("retained finalization requires GPU renderer")?;
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
        return Ok(());
    }

    apply_contrast(&mut buffers.layered, final_contrast);
    stretch_to_percentile(
        &mut buffers.layered,
        &mut buffers.percentile,
        low_pct,
        0.99,
        fast_mode,
    );

    let output_luma = if compiled.width == config.width && compiled.height == config.height {
        buffers.layered.as_slice()
    } else {
        downsample_luma(
            &buffers.layered,
            compiled.width,
            compiled.height,
            config.width,
            config.height,
            &mut buffers.final_luma,
            &mut buffers.downsample_scratch,
        )?
    };
    encode_gray(&mut buffers.output_gray, output_luma);
    Ok(())
}

fn graph_uses_gpu_generation(compiled: &CompiledGraph) -> bool {
    compiled
        .steps
        .iter()
        .any(|step| matches!(step.op, CompiledOp::GenerateLayer(_)))
}

async fn create_renderer(
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
        .ok_or("no compatible GPU adapter found for v2")?;

    let info = adapter.get_info();
    if is_software_adapter(info.device_type, &info.name) {
        return Err(format!(
            "V2 requires a hardware GPU adapter, found software adapter '{} ({:?})'",
            info.name, info.device_type
        )
        .into());
    }

    GpuLayerRenderer::new_with_output(
        &adapter,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/shader.wgsl")),
        compiled.width,
        compiled.height,
        config.width,
        config.height,
    )
    .await
}

fn indexed_output(base: &str, index: u32, total: u32) -> std::path::PathBuf {
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

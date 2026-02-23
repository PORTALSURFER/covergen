//! GPU executor for compiled V2 graphs.
//!
//! The runtime keeps all layer composition on-device and performs one readback
//! at the output boundary per image.

use std::error::Error;
use std::path::Path;

use crate::gpu_render::GpuLayerRenderer;
use crate::image_ops::{
    apply_contrast, downsample_luma, encode_gray, encode_png_bytes, resolve_output_path,
    save_png_under_10mb, stretch_to_percentile,
};
use image::codecs::png::CompressionType;

use super::animation::{
    clip_output_path, create_frame_dir, encode_frames_to_mp4, frame_filename,
    modulate_layer_for_frame, total_frames,
};
use super::cli::{V2Config, V2Profile};
use super::compiler::CompiledGraph;

/// Reusable image buffers for V2 execution.
struct RuntimeBuffers {
    layered: Vec<f32>,
    percentile: Vec<f32>,
    final_luma: Vec<f32>,
    downsample_scratch: Vec<u8>,
    output_gray: Vec<u8>,
}

/// Execute a compiled graph for all requested output images.
pub async fn execute_compiled(
    config: &V2Config,
    compiled: &CompiledGraph,
) -> Result<(), Box<dyn Error>> {
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

    let mut renderer = GpuLayerRenderer::new(
        &adapter,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/shader.wgsl")),
        compiled.width,
        compiled.height,
    )
    .await?;

    let mut buffers = RuntimeBuffers {
        layered: vec![0.0f32; (compiled.width as usize) * (compiled.height as usize)],
        percentile: vec![0.0f32; (compiled.width as usize) * (compiled.height as usize)],
        final_luma: vec![0.0f32; (config.width as usize) * (config.height as usize)],
        downsample_scratch: Vec::new(),
        output_gray: vec![0u8; (config.width as usize) * (config.height as usize)],
    };

    if config.animation.enabled {
        return execute_animation(config, compiled, &mut renderer, &mut buffers);
    }

    for image_index in 0..config.count {
        renderer.begin_retained_image()?;
        let image_seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(image_index.wrapping_mul(0x9E37_79B9));

        for step in &compiled.steps {
            let params = step
                .layer
                .to_params(compiled.width, compiled.height, image_seed_offset);
            renderer.submit_retained_layer(
                &params,
                step.layer.opacity,
                step.layer.blend_mode.as_u32(),
                step.layer.contrast,
            )?;
        }

        finalize_luma_for_output(config, compiled, &mut renderer, &mut buffers)?;

        let indexed_output = indexed_output(&config.output, image_index, config.count);
        let output_path = resolve_output_path(&indexed_output.to_string_lossy());
        let (w, h, bytes) = save_png_under_10mb(
            &output_path,
            config.width,
            config.height,
            &buffers.output_gray,
        )?;

        println!(
            "[v2] generated {} | graph {}x{} -> output {}x{} | layers {} | {:.2}MB",
            output_path.display(),
            compiled.width,
            compiled.height,
            w,
            h,
            compiled.steps.len(),
            bytes as f64 / (1024.0 * 1024.0)
        );
    }

    Ok(())
}

fn execute_animation(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: &mut GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
) -> Result<(), Box<dyn Error>> {
    let frames = total_frames(&config.animation);
    for clip_index in 0..config.count {
        let frame_dir = create_frame_dir(&config.output, clip_index)?;
        let clip_seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(clip_index.wrapping_mul(0x6A09_E667));

        for frame_index in 0..frames {
            renderer.begin_retained_image()?;
            let frame_seed_offset =
                clip_seed_offset.wrapping_add(frame_index.wrapping_mul(0x9E37_79B9));
            for (layer_index, step) in compiled.steps.iter().enumerate() {
                let modulated =
                    modulate_layer_for_frame(step.layer, frame_index, frames, layer_index as u32);
                let params =
                    modulated.to_params(compiled.width, compiled.height, frame_seed_offset);
                renderer.submit_retained_layer(
                    &params,
                    modulated.opacity,
                    modulated.blend_mode.as_u32(),
                    modulated.contrast,
                )?;
            }

            finalize_luma_for_output(config, compiled, renderer, buffers)?;
            let encoded = encode_png_bytes(
                config.width,
                config.height,
                &buffers.output_gray,
                CompressionType::Fast,
            )?;
            let frame_path = frame_dir.join(frame_filename(frame_index));
            std::fs::write(frame_path, encoded)?;
        }

        let clip_path = clip_output_path(&config.output, clip_index, config.count);
        encode_frames_to_mp4(&frame_dir, config.animation.fps, &clip_path)?;
        if !config.animation.keep_frames {
            std::fs::remove_dir_all(&frame_dir)?;
        }

        println!(
            "[v2] animation {} | {}s @ {}fps | {} frames | {}",
            clip_index + 1,
            config.animation.seconds,
            config.animation.fps,
            frames,
            clip_path.display()
        );
    }
    Ok(())
}

fn finalize_luma_for_output(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: &mut GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
) -> Result<(), Box<dyn Error>> {
    renderer.collect_retained_image(&mut buffers.layered)?;
    let final_contrast = match config.profile {
        V2Profile::Quality => 1.45,
        V2Profile::Performance => 1.25,
    };
    apply_contrast(&mut buffers.layered, final_contrast);
    stretch_to_percentile(
        &mut buffers.layered,
        &mut buffers.percentile,
        if matches!(config.profile, V2Profile::Performance) {
            0.02
        } else {
            0.01
        },
        0.99,
        matches!(config.profile, V2Profile::Performance),
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

fn indexed_output(base: &str, index: u32, total: u32) -> std::path::PathBuf {
    if total <= 1 {
        return Path::new(base).to_path_buf();
    }
    clip_output_path(base, index, total)
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

//! GPU executor for compiled V2 graphs.
//!
//! The runtime keeps all layer composition on-device and performs one readback
//! at the output boundary per image.

use std::error::Error;
use std::path::{Path, PathBuf};

use crate::gpu_render::GpuLayerRenderer;
use crate::image_ops::{
    apply_contrast, downsample_luma, encode_gray, resolve_output_path, save_png_under_10mb,
    stretch_to_percentile,
};

use super::cli::{V2Config, V2Profile};
use super::compiler::CompiledGraph;

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

    let render_pixels = (compiled.width as usize) * (compiled.height as usize);
    let output_pixels = (config.width as usize) * (config.height as usize);
    let mut layered = vec![0.0f32; render_pixels];
    let mut percentile = vec![0.0f32; render_pixels];
    let mut final_luma = vec![0.0f32; output_pixels];
    let mut downsample_scratch = Vec::new();
    let mut output_gray = vec![0u8; output_pixels];

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

        renderer.collect_retained_image(&mut layered)?;
        let final_contrast = match config.profile {
            V2Profile::Quality => 1.45,
            V2Profile::Performance => 1.25,
        };
        apply_contrast(&mut layered, final_contrast);
        stretch_to_percentile(
            &mut layered,
            &mut percentile,
            if matches!(config.profile, V2Profile::Performance) {
                0.02
            } else {
                0.01
            },
            0.99,
            matches!(config.profile, V2Profile::Performance),
        );

        let output_luma = if compiled.width == config.width && compiled.height == config.height {
            layered.as_slice()
        } else {
            downsample_luma(
                &layered,
                compiled.width,
                compiled.height,
                config.width,
                config.height,
                &mut final_luma,
                &mut downsample_scratch,
            )?
        };
        encode_gray(&mut output_gray, output_luma);

        let indexed_output = indexed_output(&config.output, image_index, config.count);
        let output_path = resolve_output_path(&indexed_output.to_string_lossy());
        let (w, h, bytes) =
            save_png_under_10mb(&output_path, config.width, config.height, &output_gray)?;

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

fn indexed_output(base: &str, index: u32, total: u32) -> PathBuf {
    if total <= 1 {
        return Path::new(base).to_path_buf();
    }

    let base_path = Path::new(base);
    let parent = base_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = base_path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("covergen_v2");
    let ext = base_path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("png");
    let name = format!("{}_{}.{}", stem, index + 1, ext);
    if parent.as_os_str().is_empty() {
        PathBuf::from(name)
    } else {
        parent.join(name)
    }
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

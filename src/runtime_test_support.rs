//! Test-only helpers for V2 runtime output finalization.
//!
//! These utilities keep CPU snapshot regression checks deterministic while the
//! production runtime stays GPU-first.

use std::error::Error;

use crate::gpu_render::GpuLayerRenderer;
use crate::image_ops::{apply_contrast, downsample_luma, encode_gray, stretch_to_percentile};

use super::compiler::CompiledGraph;
use super::runtime::RuntimeBuffers;
use super::runtime_config::{V2Config, V2Profile};

/// Finalize one rendered frame into grayscale test output.
///
/// When `renderer` is present, this mirrors production retained-GPU finalization.
/// When it is absent, this applies deterministic CPU post-processing for
/// snapshot fixtures that render CPU-only buffers.
pub(super) fn finalize_luma_for_output_for_test(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: Option<&mut GpuLayerRenderer>,
    buffers: &mut RuntimeBuffers,
) -> Result<(), Box<dyn Error>> {
    match renderer {
        Some(renderer) => finalize_gpu_output(config, renderer, buffers),
        None => finalize_cpu_output(config, compiled, buffers),
    }
}

fn finalize_gpu_output(
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

    renderer.collect_retained_output_gray(
        &mut buffers.output_gray,
        final_contrast,
        low_pct,
        0.99,
        fast_mode,
    )
}

fn finalize_cpu_output(
    config: &V2Config,
    compiled: &CompiledGraph,
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

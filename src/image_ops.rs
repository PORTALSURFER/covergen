//! Image buffer manipulation and output helpers.
//!
//! Runtime-critical helpers in this module are used by the CPU execution path
//! and output writers. Legacy posterize/downsample helpers are test-only
//! reference utilities and are compiled only for test targets.

#[cfg(test)]
use std::cmp::Ordering as CmpOrdering;
use std::error::Error;
use std::io::Cursor;
use std::path::Path;

use image::{
    codecs::png::{CompressionType, FilterType, PngEncoder},
    ImageEncoder,
};
#[cfg(test)]
use rayon::prelude::*;

#[cfg(test)]
use crate::model::LayerBlendMode;

/// Absolute output size cap for generated PNG files.
pub(crate) const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;

/// Minimum allowed output dimension used when shrinking oversized outputs.
pub(crate) const MIN_IMAGE_DIMENSION: u32 = 64;

/// Clamp a normalized scalar to the `[0, 1]` range.
#[cfg(test)]
pub(crate) fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// Quantize one normalized scalar into the requested posterization band count.
#[cfg(test)]
pub(crate) fn apply_posterize(mut value: f32, bands: u32) -> f32 {
    if bands <= 1 {
        return clamp01(value);
    }
    let levels = bands as f32;
    value = clamp01(value) * levels;
    (value.floor() / levels).min(1.0)
}

/// Posterize an entire grayscale buffer in place.
#[cfg(test)]
pub(crate) fn apply_posterize_buffer(src: &mut [f32], bands: u32) {
    for value in src.iter_mut() {
        *value = apply_posterize(*value, bands);
    }
}

/// Apply posterization and contrast in a single pass to reduce memory traffic.
#[cfg(test)]
pub(crate) fn apply_posterize_and_contrast(src: &mut [f32], bands: u32, strength: f32) {
    let contrast = strength.clamp(1.0, 3.0);
    let midpoint = 0.5;
    for value in src.iter_mut() {
        let posterized = apply_posterize(*value, bands);
        *value = clamp01(((posterized - midpoint) * contrast) + midpoint);
    }
}

/// Decode BGRA bytes into normalized luma values using the blue channel.
#[cfg(test)]
pub(crate) fn decode_luma(raw: &[u8], out: &mut [f32]) {
    debug_assert_eq!(out.len() * 4, raw.len());

    for (i, px) in raw.chunks_exact(4).enumerate() {
        out[i] = px[0] as f32 / 255.0;
    }
}

/// Encode normalized luma values into grayscale bytes.
#[cfg(test)]
pub(crate) fn encode_gray(dst: &mut [u8], luma: &[f32]) {
    debug_assert_eq!(luma.len(), dst.len());

    for (i, &v) in luma.iter().enumerate() {
        dst[i] = (clamp01(v) * 255.0).round() as u8;
    }
}

/// Downsample one luma buffer and reuse caller-owned scratch allocations.
#[cfg(test)]
pub(crate) fn downsample_luma<'a>(
    source: &[f32],
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
    output: &'a mut Vec<f32>,
    source_u8_scratch: &mut Vec<u8>,
) -> Result<&'a [f32], Box<dyn Error>> {
    let target_len = (target_width as usize) * (target_height as usize);
    if output.len() != target_len {
        output.resize(target_len, 0.0f32);
    }

    if source.is_empty() {
        output.fill(0.0);
        return Ok(&output[..target_len]);
    }

    if source.len() != (source_width as usize) * (source_height as usize) {
        return Err("invalid source luma size for downsample".into());
    }

    if source_width == target_width && source_height == target_height {
        output[..source.len()].copy_from_slice(source);
        return Ok(&output[..source.len()]);
    }

    source_u8_scratch.resize(source.len(), 0u8);
    encode_gray(source_u8_scratch, source);

    let source_image = image::GrayImage::from_raw(
        source_width,
        source_height,
        std::mem::take(source_u8_scratch),
    )
    .ok_or("invalid source image buffer during downsample")?;

    let resized = image::imageops::resize(
        &source_image,
        target_width,
        target_height,
        image::imageops::FilterType::Lanczos3,
    );
    *source_u8_scratch = source_image.into_raw();

    let resized_values = resized.as_raw();
    if resized_values.len() != target_len {
        return Err("downsample output size mismatch".into());
    }

    for (out, value) in output.iter_mut().zip(resized_values.iter()) {
        *out = (*value as f32) / 255.0;
    }

    Ok(&output[..target_len])
}

/// Stretch in-place values to percentile-selected low/high anchors.
#[cfg(test)]
pub(crate) fn stretch_to_percentile(
    src: &mut [f32],
    scratch: &mut [f32],
    low_pct: f32,
    high_pct: f32,
    fast_mode: bool,
) {
    if src.is_empty() {
        return;
    }

    debug_assert_eq!(src.len(), scratch.len());

    let sample_limit = if fast_mode { 8_192usize } else { src.len() }
        .min(src.len())
        .max(2);

    if sample_limit == src.len() {
        scratch.copy_from_slice(src);
    } else {
        let step = src.len() as f32 / sample_limit as f32;
        for (idx, sample_target) in scratch[..sample_limit].iter_mut().enumerate() {
            let source_idx = (idx as f32 * step).floor() as usize;
            *sample_target = src[source_idx.min(src.len() - 1)];
        }
    }

    let sample = &mut scratch[..sample_limit];
    let len_minus_1 = sample_limit - 1;
    let low = (len_minus_1 as f32 * low_pct.clamp(0.0, 1.0)).round() as usize;
    let high = (len_minus_1 as f32 * high_pct.clamp(0.0, 1.0)).round() as usize;
    sample.select_nth_unstable_by(low, |a, b| a.partial_cmp(b).unwrap_or(CmpOrdering::Equal));
    let in_min = sample[low];
    sample.select_nth_unstable_by(high, |a, b| a.partial_cmp(b).unwrap_or(CmpOrdering::Equal));
    let in_max = sample[high];
    let span = in_max - in_min;

    if span <= f32::EPSILON {
        for value in src.iter_mut() {
            *value = 0.5;
        }
        return;
    }

    for value in src.iter_mut() {
        *value = ((*value - in_min) / span).clamp(0.0, 1.0);
    }
}

/// Blend a source layer into the destination buffer with the selected blend mode.
#[cfg(test)]
pub(crate) fn blend_layer_stack(
    dst: &mut [f32],
    layer: &[f32],
    strength: f32,
    mode: LayerBlendMode,
) {
    debug_assert_eq!(dst.len(), layer.len());

    let alpha = strength.clamp(0.0, 1.0);
    dst.par_iter_mut()
        .zip(layer.par_iter())
        .for_each(|(base, top)| {
            let mixed = match mode {
                LayerBlendMode::Normal => *top,
                LayerBlendMode::Add => clamp01(*base + *top),
                LayerBlendMode::Multiply => clamp01(*base * *top),
                LayerBlendMode::Screen => 1.0 - ((1.0 - *base) * (1.0 - *top)),
                LayerBlendMode::Overlay => {
                    if *base < 0.5 {
                        2.0 * *base * *top
                    } else {
                        1.0 - (2.0 * (1.0 - *base) * (1.0 - *top))
                    }
                }
                LayerBlendMode::Difference => (*base - *top).abs(),
                LayerBlendMode::Lighten => (*base).max(*top),
                LayerBlendMode::Darken => (*base).min(*top),
                LayerBlendMode::Glow => *base + (1.0 - *base) * (*top * *top),
                LayerBlendMode::Shadow => *base * *top,
            };

            *base = clamp01((1.0 - alpha) * *base + alpha * mixed);
        });
}

/// Apply a centered linear contrast curve in place.
#[cfg(test)]
pub(crate) fn apply_contrast(src: &mut [f32], strength: f32) {
    let clamped = strength.clamp(1.0, 3.0);
    let midpoint = 0.5;
    for value in src.iter_mut() {
        *value = clamp01(((*value - midpoint) * clamped) + midpoint);
    }
}

/// Return a unique output path by appending an incrementing suffix when needed.
pub(crate) fn resolve_output_path(output: &str) -> std::path::PathBuf {
    let base_path = Path::new(output);
    if !base_path.exists() {
        return base_path.to_path_buf();
    }

    let parent = base_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = base_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");
    let extension = base_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    let mut index = 1u32;

    loop {
        let candidate_name = if extension.is_empty() {
            format!("{stem}_{index}")
        } else {
            format!("{stem}_{index}.{extension}")
        };

        let candidate = if parent.as_os_str().is_empty() {
            std::path::PathBuf::from(candidate_name)
        } else {
            parent.join(candidate_name)
        };

        if !candidate.exists() {
            return candidate;
        }

        index += 1;
    }
}

/// Encode one grayscale image payload to PNG bytes at the requested compression level.
pub(crate) fn encode_png_bytes(
    width: u32,
    height: u32,
    data: &[u8],
    compression: CompressionType,
) -> Result<Vec<u8>, Box<dyn Error>> {
    if data.len() != width as usize * height as usize {
        return Err("invalid buffer size for grayscale image".into());
    }

    let mut cursor = Cursor::new(Vec::new());
    {
        let encoder = PngEncoder::new_with_quality(&mut cursor, compression, FilterType::Adaptive);
        encoder.write_image(data, width, height, image::ColorType::L8)?;
    }

    Ok(cursor.into_inner())
}

/// Resize a grayscale frame while preserving detail for final output.
fn resize_gray_frame(
    width: u32,
    height: u32,
    next_width: u32,
    next_height: u32,
    working: Vec<u8>,
    error_context: &'static str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let source = image::GrayImage::from_raw(width, height, working).ok_or(error_context)?;
    let resized = image::imageops::resize(
        &source,
        next_width,
        next_height,
        image::imageops::FilterType::Lanczos3,
    );
    Ok(resized.into_raw())
}

/// Persist a grayscale frame as PNG, shrinking dimensions until the output fits under 10MB.
pub(crate) fn save_png_under_10mb(
    output: &Path,
    mut width: u32,
    mut height: u32,
    gray: &[u8],
) -> Result<(u32, u32, usize), Box<dyn Error>> {
    if gray.len() != width as usize * height as usize {
        return Err("invalid buffer size for grayscale image".into());
    }

    // Keep the original borrowed buffer until a resize is actually required.
    let mut working_owned: Option<Vec<u8>> = None;
    let mut encoded_fast = encode_png_bytes(width, height, gray, CompressionType::Fast)?;
    let mut shrink_passes = 0u32;

    while encoded_fast.len() > MAX_OUTPUT_BYTES
        && width > MIN_IMAGE_DIMENSION
        && height > MIN_IMAGE_DIMENSION
    {
        let next_width = ((width as f32) * 0.9)
            .round()
            .max(MIN_IMAGE_DIMENSION as f32)
            .min(width as f32) as u32;
        let next_height = ((height as f32) * 0.9)
            .round()
            .max(MIN_IMAGE_DIMENSION as f32)
            .min(height as f32) as u32;

        if next_width == width && next_height == height {
            break;
        }

        let source = working_owned.take().unwrap_or_else(|| gray.to_vec());
        let resized = resize_gray_frame(
            width,
            height,
            next_width,
            next_height,
            source,
            "invalid working image buffer during resize",
        );
        width = next_width;
        height = next_height;
        working_owned = Some(resized?);
        encoded_fast = encode_png_bytes(
            width,
            height,
            working_owned
                .as_deref()
                .ok_or("missing working buffer after resize")?,
            CompressionType::Fast,
        )?;
        shrink_passes += 1;

        if shrink_passes > 48 {
            break;
        }
    }

    if encoded_fast.len() > MAX_OUTPUT_BYTES {
        while encoded_fast.len() > MAX_OUTPUT_BYTES
            && width > MIN_IMAGE_DIMENSION
            && height > MIN_IMAGE_DIMENSION
        {
            let width_scale = (MAX_OUTPUT_BYTES as f32 / encoded_fast.len() as f32).sqrt() * 0.95;
            let next_width = ((width as f32) * width_scale)
                .floor()
                .max(MIN_IMAGE_DIMENSION as f32) as u32;
            let next_height = ((height as f32) * width_scale)
                .floor()
                .max(MIN_IMAGE_DIMENSION as f32) as u32;
            let target_width = next_width.max(1).min(width);
            let target_height = next_height.max(1).min(height);

            if target_width == width && target_height == height {
                break;
            }

            let source = working_owned.take().unwrap_or_else(|| gray.to_vec());
            let resized = resize_gray_frame(
                width,
                height,
                target_width,
                target_height,
                source,
                "invalid working image buffer during final resize",
            );
            width = target_width;
            height = target_height;
            working_owned = Some(resized?);
            encoded_fast = encode_png_bytes(
                width,
                height,
                working_owned
                    .as_deref()
                    .ok_or("missing working buffer after final resize")?,
                CompressionType::Fast,
            )?;
        }
    }

    let working = working_owned.as_deref().unwrap_or(gray);
    let mut encoded_final = encode_png_bytes(width, height, working, CompressionType::Best)?;

    // Best compression should usually be smaller than fast compression, but if
    // a codec edge case regresses size, run one extra shrink pass.
    if encoded_final.len() > MAX_OUTPUT_BYTES
        && width > MIN_IMAGE_DIMENSION
        && height > MIN_IMAGE_DIMENSION
    {
        let width_scale = (MAX_OUTPUT_BYTES as f32 / encoded_final.len() as f32).sqrt() * 0.95;
        let next_width = ((width as f32) * width_scale)
            .floor()
            .max(MIN_IMAGE_DIMENSION as f32) as u32;
        let next_height = ((height as f32) * width_scale)
            .floor()
            .max(MIN_IMAGE_DIMENSION as f32) as u32;
        let target_width = next_width.max(1).min(width);
        let target_height = next_height.max(1).min(height);
        if target_width != width || target_height != height {
            let source = working_owned.take().unwrap_or_else(|| gray.to_vec());
            let resized = resize_gray_frame(
                width,
                height,
                target_width,
                target_height,
                source,
                "invalid working image buffer during best-compression resize",
            )?;
            width = target_width;
            height = target_height;
            working_owned = Some(resized);
            encoded_final = encode_png_bytes(
                width,
                height,
                working_owned
                    .as_deref()
                    .ok_or("missing working buffer after best-compression resize")?,
                CompressionType::Best,
            )?;
        }
    }

    let final_size = encoded_final.len();
    std::fs::write(output, encoded_final)?;
    Ok((width, height, final_size))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_contrast, apply_posterize_and_contrast, apply_posterize_buffer, downsample_luma,
    };

    #[test]
    fn fused_posterize_and_contrast_matches_split_passes() {
        let mut split = vec![0.05f32, 0.21, 0.37, 0.64, 0.81, 0.97];
        let mut fused = split.clone();
        let bands = 5;
        let contrast = 1.38;

        apply_posterize_buffer(&mut split, bands);
        apply_contrast(&mut split, contrast);
        apply_posterize_and_contrast(&mut fused, bands, contrast);

        for (a, b) in split.iter().zip(fused.iter()) {
            assert!((*a - *b).abs() < 1e-6);
        }
    }

    #[test]
    fn downsample_luma_reuses_source_byte_scratch() {
        let source = vec![
            0.0f32, 0.1, 0.2, 0.3, 0.1, 0.2, 0.3, 0.4, 0.2, 0.3, 0.4, 0.5, 0.3, 0.4, 0.5, 0.6,
        ];
        let mut output = Vec::new();
        let mut source_u8_scratch = Vec::new();

        let downsampled = downsample_luma(&source, 4, 4, 2, 2, &mut output, &mut source_u8_scratch)
            .expect("downsample should succeed");

        assert_eq!(downsampled.len(), 4);
        assert_eq!(source_u8_scratch.len(), source.len());
        assert!(downsampled.iter().all(|value| (0.0..=1.0).contains(value)));
    }
}

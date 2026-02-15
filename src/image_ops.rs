//! Image buffer manipulation and output helpers.

use std::cmp::Ordering as CmpOrdering;
use std::error::Error;
use std::io::Cursor;
use std::path::Path;

use image::{
    ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use rayon::prelude::*;

use crate::config::{MAX_OUTPUT_BYTES, MIN_IMAGE_DIMENSION};
use crate::model::{BlurConfig, FilterMode, GradientConfig, GradientMode, XorShift32};

pub(crate) fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

pub(crate) fn apply_posterize(mut value: f32, bands: u32) -> f32 {
    if bands <= 1 {
        return clamp01(value);
    }
    let levels = bands as f32;
    value = clamp01(value) * levels;
    (value.floor() / levels).min(1.0)
}

pub(crate) fn apply_posterize_buffer(src: &mut [f32], bands: u32) {
    for value in src.iter_mut() {
        *value = apply_posterize(*value, bands);
    }
}

pub(crate) fn apply_gradient_map(src: &mut [f32], cfg: GradientConfig) {
    for value in src {
        let mut mapped = clamp01(*value);
        match cfg.mode {
            GradientMode::Linear => {}
            GradientMode::Contrast => {
                mapped = mapped.powf(1.0 + cfg.contrast * 0.05) * cfg.pivot;
                mapped = mapped.clamp(0.0, 1.0);
            }
            GradientMode::Gamma => {
                mapped = mapped.powf(cfg.gamma);
            }
            GradientMode::Sine => {
                mapped = 0.5
                    + (0.5
                        * (cfg.frequency * mapped * std::f32::consts::PI * 2.0 + cfg.phase).sin());
            }
            GradientMode::Sigmoid => {
                let x = cfg.contrast * 0.1 * (mapped - cfg.pivot);
                mapped = 1.0 / (1.0 + (-x).exp());
            }
            GradientMode::Posterize => {}
        }

        if cfg.invert {
            mapped = 1.0 - mapped;
        }

        mapped = (mapped * cfg.contrast.recip()).clamp(0.0, 1.0);
        mapped = apply_posterize(mapped, cfg.bands);
        *value = clamp01(mapped);
    }
}

#[inline]
pub(crate) fn pixel_index(x: i32, y: i32, width: i32) -> usize {
    (y * width + x) as usize
}

#[inline]
pub(crate) fn sample_luma(src: &[f32], width: i32, height: i32, x: i32, y: i32) -> f32 {
    let clamped_x = x.clamp(0, width - 1);
    let clamped_y = y.clamp(0, height - 1);
    let idx = pixel_index(clamped_x, clamped_y, width);
    src[idx]
}

pub(crate) fn decode_luma(raw: &[u8], out: &mut [f32]) {
    debug_assert_eq!(out.len() * 4, raw.len());

    for (i, px) in raw.chunks_exact(4).enumerate() {
        out[i] = px[0] as f32 / 255.0;
    }
}

pub(crate) fn encode_gray(dst: &mut [u8], luma: &[f32]) {
    debug_assert_eq!(luma.len(), dst.len());

    for (i, &v) in luma.iter().enumerate() {
        dst[i] = (clamp01(v) * 255.0).round() as u8;
    }
}

pub(crate) fn downsample_luma<'a>(
    source: &[f32],
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
    output: &'a mut Vec<f32>,
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

    let source_bytes = source
        .iter()
        .map(|value| (clamp01(*value) * 255.0).round() as u8)
        .collect::<Vec<u8>>();

    let source_image = image::GrayImage::from_raw(source_width, source_height, source_bytes)
        .ok_or("invalid source image buffer during downsample")?;

    let resized = image::imageops::resize(
        &source_image,
        target_width,
        target_height,
        image::imageops::FilterType::Lanczos3,
    );

    let resized_values = resized.into_raw();
    if resized_values.len() != target_len {
        return Err("downsample output size mismatch".into());
    }

    for (out, value) in output.iter_mut().zip(resized_values.into_iter()) {
        *out = (value as f32) / 255.0;
    }

    Ok(&output[..target_len])
}

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

pub(crate) fn inject_noise(src: &mut [f32], seed: u32, strength: f32) {
    let mut rng = XorShift32::new(seed);
    let gain = strength * 0.5;
    for value in src.iter_mut() {
        let noise = ((rng.next_u32() as f32) / (u32::MAX as f32) - 0.5) * 2.0;
        *value = clamp01(*value + noise * gain);
    }
}

pub(crate) fn create_soft_background(width: u32, height: u32, seed: u32, out: &mut [f32]) {
    debug_assert_eq!(out.len(), (width as usize) * (height as usize));

    let mut rng = XorShift32::new(seed ^ 0x9e37_79b9);
    let freq_x = 0.25 + (rng.next_f32() * 1.8);
    let freq_y = 0.25 + (rng.next_f32() * 1.8);
    let phase_a = rng.next_f32() * std::f32::consts::TAU;
    let phase_b = rng.next_f32() * std::f32::consts::TAU;
    let noise_strength = 0.08 + (rng.next_f32() * 0.1);
    let mut jitter_rng = XorShift32::new(seed ^ 0xA53F_12B1);

    let mut iter = out.iter_mut();
    let width_f = width as f32;
    let height_f = height as f32;
    for y in 0..height {
        let v = (y as f32 / height_f) * 2.0 - 1.0;
        let v_l1 = v.abs();
        for x in 0..width {
            let u = (x as f32 / width_f) * 2.0 - 1.0;
            let u_l1 = u.abs();
            let wave_x = (u * std::f32::consts::TAU * freq_x + phase_a).sin() * 0.3;
            let wave_y = (v * std::f32::consts::TAU * freq_y + phase_b).cos() * 0.3;
            let cross = ((u - v) * 1.5).sin() * 0.2;
            let jitter = (jitter_rng.next_f32() - 0.5) * 2.0;
            let l1_falloff = 0.82 - (u_l1 + v_l1) * 0.22;
            let value = clamp01(
                0.46 + (wave_x * 0.24)
                    + (wave_y * 0.24)
                    + (cross * 0.16)
                    + (l1_falloff * 0.24)
                    + (jitter - 0.5) * noise_strength,
            );
            if let Some(px) = iter.next() {
                *px = value;
            }
        }
    }
}

pub(crate) fn blend_background(src: &mut [f32], bg: &[f32], strength: f32) {
    debug_assert_eq!(src.len(), bg.len());

    src.par_iter_mut()
        .zip(bg.par_iter())
        .for_each(|(value, bg_value)| {
            *value = clamp01((*value * (1.0 - strength)) + (*bg_value * strength));
        });
}

pub(crate) fn blend_layer_stack(
    dst: &mut [f32],
    layer: &[f32],
    strength: f32,
    mode: crate::model::LayerBlendMode,
) {
    debug_assert_eq!(dst.len(), layer.len());

    let alpha = strength.clamp(0.0, 1.0);
    dst.par_iter_mut()
        .zip(layer.par_iter())
        .for_each(|(base, top)| {
            let mixed = match mode {
                crate::model::LayerBlendMode::Normal => *top,
                crate::model::LayerBlendMode::Add => clamp01(*base + *top),
                crate::model::LayerBlendMode::Multiply => clamp01(*base * *top),
                crate::model::LayerBlendMode::Screen => 1.0 - ((1.0 - *base) * (1.0 - *top)),
                crate::model::LayerBlendMode::Overlay => {
                    if *base < 0.5 {
                        2.0 * *base * *top
                    } else {
                        1.0 - (2.0 * (1.0 - *base) * (1.0 - *top))
                    }
                }
                crate::model::LayerBlendMode::Difference => (*base - *top).abs(),
                crate::model::LayerBlendMode::Lighten => (*base).max(*top),
                crate::model::LayerBlendMode::Darken => (*base).min(*top),
                crate::model::LayerBlendMode::Glow => *base + (1.0 - *base) * (*top * *top),
                crate::model::LayerBlendMode::Shadow => *base * *top,
            };

            *base = clamp01((1.0 - alpha) * *base + alpha * mixed);
        });
}

pub(crate) fn apply_contrast(src: &mut [f32], strength: f32) {
    let clamped = strength.clamp(1.0, 3.0);
    let midpoint = 0.5;
    for value in src.iter_mut() {
        *value = clamp01(((*value - midpoint) * clamped) + midpoint);
    }
}

fn apply_motion_blur(width: u32, height: u32, src: &[f32], dst: &mut [f32], cfg: &BlurConfig) {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let width_usize = width as usize;
    let cfg = *cfg;

    dst.par_iter_mut().enumerate().for_each(|(idx, out)| {
        let y = (idx / width_usize) as i32;
        let x = (idx % width_usize) as i32;
        let center = sample_luma(src, width_i32, height_i32, x, y);
        let local_blur = (1.0 - center).powi(2);
        let radius = (1.0 + (cfg.max_radius as f32 * (0.2 + 0.8 * local_blur))).round() as i32;

        let mut numerator = 0.0;
        let mut denominator = 0.0;
        let mut step = -radius;
        while step <= radius {
            let t = 1.0 - (step.abs() as f32 / (radius as f32 + 1.0));
            let sx = x + step * cfg.axis_x;
            let sy = y + step * cfg.axis_y;
            let sample = sample_luma(src, width_i32, height_i32, sx, sy);
            numerator += sample * t;
            denominator += t;
            step += 1;
        }

        if denominator > 0.0 {
            *out = numerator / denominator;
        } else {
            *out = center;
        }
    });
}

fn apply_gaussian_blur(width: u32, height: u32, src: &[f32], dst: &mut [f32], cfg: &BlurConfig) {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let width_usize = width as usize;
    let cfg = *cfg;

    dst.par_iter_mut().enumerate().for_each(|(idx, out)| {
        let y = (idx / width_usize) as i32;
        let x = (idx % width_usize) as i32;
        let center = sample_luma(src, width_i32, height_i32, x, y);
        let local_blur = (1.0 - center).powf(1.5);
        let radius = (1.0 + (cfg.max_radius as f32 * (0.2 + 0.8 * local_blur))).round() as i32;
        let sigma = (radius as f32 + 1.0) * 0.5;
        let sigma2 = sigma * sigma * 2.0;

        let mut num = 0.0;
        let mut den = 0.0;
        let mut dy = -radius;
        while dy <= radius {
            let mut dx = -radius;
            while dx <= radius {
                let sx = x + dx;
                let sy = y + dy;
                let d2 = (dx * dx + dy * dy) as f32;
                let spatial = (-d2 / sigma2).exp();
                let sample = sample_luma(src, width_i32, height_i32, sx, sy);
                num += sample * spatial;
                den += spatial;
                dx += 1;
            }
            dy += 1;
        }

        if den > 0.0 {
            *out = num / den;
        } else {
            *out = center;
        }
    });
}

fn apply_median_blur(width: u32, height: u32, src: &[f32], dst: &mut [f32], cfg: &BlurConfig) {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let width_usize = width as usize;
    let cfg = *cfg;

    dst.par_iter_mut().enumerate().for_each(|(idx, out)| {
        let y = (idx / width_usize) as i32;
        let x = (idx % width_usize) as i32;
        let center = sample_luma(src, width_i32, height_i32, x, y);
        let local_blur = (1.0 - center).powi(2);
        let base = 1 + ((cfg.max_radius as f32 * (0.4 + 0.6 * local_blur)).floor() as i32);
        let radius = base.clamp(1, 2);
        let mut values = [0f32; 25];
        let mut count = 0usize;

        let mut dy = -radius;
        while dy <= radius {
            let mut dx = -radius;
            while dx <= radius {
                let sample = sample_luma(src, width_i32, height_i32, x + dx, y + dy);
                values[count] = sample;
                count += 1;
                dx += 1;
            }
            dy += 1;
        }

        values[..count].sort_by(|a, b| a.partial_cmp(b).unwrap_or(CmpOrdering::Equal));
        *out = values[count / 2];
    });
}

fn apply_bilateral_blur(width: u32, height: u32, src: &[f32], dst: &mut [f32], cfg: &BlurConfig) {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let width_usize = width as usize;
    let sigma_r = 0.1 + (cfg.softness as f32 * 0.03);
    let cfg = *cfg;
    let radius_limit = cfg.max_radius as f32;

    dst.par_iter_mut().enumerate().for_each(|(idx, out)| {
        let y = (idx / width_usize) as i32;
        let x = (idx % width_usize) as i32;
        let center = sample_luma(src, width_i32, height_i32, x, y);
        let local_blur = (1.0 - center).powi(2);
        let radius = (1 + ((radius_limit * (0.2 + 0.8 * local_blur)).round() as i32)).clamp(1, 2);

        let mut num = 0.0;
        let mut den = 0.0;
        let mut dy = -radius;
        while dy <= radius {
            let mut dx = -radius;
            while dx <= radius {
                let sample = sample_luma(src, width_i32, height_i32, x + dx, y + dy);
                let d = sample - center;
                let range = (-(d * d / (2.0 * sigma_r * sigma_r))).exp();
                let weight = (-((dx * dx + dy * dy) as f32) / 16.0).exp() * range;
                num += sample * weight;
                den += weight;
                dx += 1;
            }
            dy += 1;
        }

        if den > 0.0 {
            *out = num / den;
        } else {
            *out = center;
        }
    });
}

pub(crate) fn apply_dynamic_filter(
    width: u32,
    height: u32,
    luma: &[f32],
    dst: &mut [f32],
    cfg: &BlurConfig,
) {
    match cfg.mode {
        FilterMode::Motion => apply_motion_blur(width, height, luma, dst, cfg),
        FilterMode::Gaussian => apply_gaussian_blur(width, height, luma, dst, cfg),
        FilterMode::Median => apply_median_blur(width, height, luma, dst, cfg),
        FilterMode::Bilateral => apply_bilateral_blur(width, height, luma, dst, cfg),
    }
}

pub(crate) fn apply_sharpen(width: u32, height: u32, src: &[f32], dst: &mut [f32], strength: f32) {
    let width_usize = width as usize;
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let strength = strength.clamp(0.2, 2.0);
    let inv_count = 1.0 / 9.0;

    dst.par_iter_mut().enumerate().for_each(|(idx, out)| {
        let y = (idx / width_usize) as i32;
        let x = (idx % width_usize) as i32;
        let mut sum = 0.0;
        let mut count = 0.0;
        let mut dy = -1;
        while dy <= 1 {
            let mut dx = -1;
            while dx <= 1 {
                sum += sample_luma(src, width_i32, height_i32, x + dx, y + dy);
                count += 1.0;
                dx += 1;
            }
            dy += 1;
        }
        let center = src[idx];
        let local_mean = sum * (inv_count / (count / 9.0));
        *out = clamp01(center + (center - local_mean) * strength);
    });
}

pub(crate) fn apply_detail_waves(
    src: &mut [f32],
    width: u32,
    height: u32,
    seed: u32,
    strength: f32,
) {
    let mut rng = XorShift32::new(seed);
    let strength = strength.clamp(0.0, 0.25);
    let freq = 4.0 + (rng.next_f32() * 16.0);
    let freq_y = 3.0 + (rng.next_f32() * 14.0);
    let phase_a = rng.next_f32() * std::f32::consts::TAU;
    let phase_b = rng.next_f32() * std::f32::consts::TAU;
    let phase_c = rng.next_f32() * std::f32::consts::TAU;

    let width_f = width as f32;
    let height_f = height as f32;
    let two_pi = std::f32::consts::TAU;

    for (idx, value) in src.iter_mut().enumerate() {
        let y = (idx as u32 / width) as f32;
        let x = (idx as u32 % width) as f32;
        let u = (x / width_f * two_pi * freq) + phase_a;
        let v = (y / height_f * two_pi * freq_y) + phase_b;
        let mix = 0.5 + 0.5 * (u.sin() * 0.55 + v.cos() * 0.45 + ((u + v + phase_c).sin() * 0.35));
        *value = clamp01((*value * (1.0 - strength)) + (mix * strength));
    }
}

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

pub(crate) fn encode_png_bytes(
    width: u32,
    height: u32,
    data: &[u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    if data.len() != width as usize * height as usize {
        return Err("invalid buffer size for grayscale image".into());
    }

    let mut cursor = Cursor::new(Vec::new());
    {
        let encoder =
            PngEncoder::new_with_quality(&mut cursor, CompressionType::Best, FilterType::Adaptive);
        encoder.write_image(data, width, height, image::ColorType::L8)?;
    }

    Ok(cursor.into_inner())
}

pub(crate) fn save_png_under_10mb(
    output: &Path,
    mut width: u32,
    mut height: u32,
    gray: &[u8],
) -> Result<(u32, u32, usize), Box<dyn Error>> {
    let mut working = gray.to_vec();
    let mut encoded = encode_png_bytes(width, height, &working)?;
    let mut shrink_passes = 0u32;

    while encoded.len() > MAX_OUTPUT_BYTES
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

        let source = image::GrayImage::from_raw(width, height, working)
            .ok_or("invalid working image buffer during resize")?;
        let resized = image::imageops::resize(
            &source,
            next_width,
            next_height,
            image::imageops::FilterType::Lanczos3,
        );
        width = next_width;
        height = next_height;
        working = resized.into_raw();
        encoded = encode_png_bytes(width, height, &working)?;
        shrink_passes += 1;

        if shrink_passes > 48 {
            break;
        }
    }

    if encoded.len() > MAX_OUTPUT_BYTES {
        while encoded.len() > MAX_OUTPUT_BYTES
            && width > MIN_IMAGE_DIMENSION
            && height > MIN_IMAGE_DIMENSION
        {
            let width_scale = (MAX_OUTPUT_BYTES as f32 / encoded.len() as f32).sqrt() * 0.95;
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

            let source = image::GrayImage::from_raw(width, height, working)
                .ok_or("invalid working image buffer during final resize")?;
            let resized = image::imageops::resize(
                &source,
                target_width,
                target_height,
                image::imageops::FilterType::Lanczos3,
            );
            width = target_width;
            height = target_height;
            working = resized.into_raw();
            encoded = encode_png_bytes(width, height, &working)?;
        }
    }

    let final_size = encoded.len();
    std::fs::write(output, encoded)?;
    Ok((width, height, final_size))
}

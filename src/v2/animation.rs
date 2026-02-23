//! Animation helpers for V2 graph execution.
//!
//! This module provides deterministic slow parameter modulation and MP4
//! assembly for vertical social-video outputs.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use super::cli::AnimationConfig;
use super::graph::GenerateLayerNode;

/// Returns the number of frames to render for one animation clip.
pub fn total_frames(config: &AnimationConfig) -> u32 {
    config.seconds.saturating_mul(config.fps).max(1)
}

/// Build a unique temporary directory for rendered animation frames.
pub fn create_frame_dir(base_output: &str, clip_index: u32) -> Result<PathBuf, Box<dyn Error>> {
    let stem = Path::new(base_output)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("covergen_v2")
        .replace(
            |ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-',
            "_",
        );
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let dir_name = format!(
        "{}_frames_clip{}_{}_{}",
        stem,
        clip_index + 1,
        std::process::id(),
        now.as_millis()
    );
    let path = std::env::temp_dir().join(dir_name);
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Compute one frame filename in ffmpeg-compatible sequence format.
pub fn frame_filename(frame_index: u32) -> String {
    format!("frame_{:06}.png", frame_index + 1)
}

/// Compute clip output path. Multiple clips receive numeric suffixes.
pub fn clip_output_path(base: &str, clip_index: u32, total_clips: u32) -> PathBuf {
    let base_path = Path::new(base);
    if total_clips <= 1 {
        return base_path.to_path_buf();
    }

    let parent = base_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = base_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("covergen_v2_animation");
    let ext = base_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mp4");
    let name = format!("{}_{}.{}", stem, clip_index + 1, ext);
    if parent.as_os_str().is_empty() {
        PathBuf::from(name)
    } else {
        parent.join(name)
    }
}

/// Apply gentle, deterministic modulation to a layer for one animation frame.
pub fn modulate_layer_for_frame(
    layer: GenerateLayerNode,
    frame_index: u32,
    total_frames: u32,
    layer_index: u32,
) -> GenerateLayerNode {
    let phase = if total_frames <= 1 {
        0.0
    } else {
        (frame_index as f32 / (total_frames - 1) as f32) * std::f32::consts::TAU
    };
    let layer_phase = phase + (layer_index as f32 * 0.47);

    let center_dx = layer_phase.sin() * 0.06;
    let center_dy = (layer_phase * 0.83).cos() * 0.06;
    let zoom_mod = 1.0 + (layer_phase * 0.21).sin() * 0.06;
    let fill_mod = 1.0 + (layer_phase * 0.33).cos() * 0.05;
    let iter_mod = 1.0 + (layer_phase * 0.27).sin() * 0.10;
    let warp_mod = 1.0 + (layer_phase * 0.41).sin() * 0.12;
    let mix_mod = (layer_phase * 0.19).sin() * 0.14;
    let contrast_mod = 1.0 + (layer_phase * 0.23).cos() * 0.08;
    let opacity_mod = 1.0 + (layer_phase * 0.31).sin() * 0.10;

    GenerateLayerNode {
        symmetry: layer.symmetry,
        symmetry_style: layer.symmetry_style,
        iterations: ((layer.iterations as f32 * iter_mod)
            .round()
            .clamp(48.0, 1_200.0)) as u32,
        seed: layer.seed,
        fill_scale: (layer.fill_scale * fill_mod).clamp(0.6, 2.6),
        fractal_zoom: (layer.fractal_zoom * zoom_mod).clamp(0.3, 2.2),
        art_style: layer.art_style,
        art_style_secondary: layer.art_style_secondary,
        art_style_mix: (layer.art_style_mix + mix_mod).clamp(0.0, 1.0),
        bend_strength: layer.bend_strength,
        warp_strength: (layer.warp_strength * warp_mod).clamp(0.0, 1.9),
        warp_frequency: (layer.warp_frequency + (layer_phase * 0.37).sin() * 0.45).clamp(0.2, 6.2),
        tile_scale: layer.tile_scale,
        tile_phase: (layer.tile_phase + (layer_phase * 0.29).sin() * 0.11).rem_euclid(1.0),
        center_x: (layer.center_x + center_dx).clamp(-0.5, 0.5),
        center_y: (layer.center_y + center_dy).clamp(-0.5, 0.5),
        shader_layer_count: layer.shader_layer_count,
        blend_mode: layer.blend_mode,
        opacity: (layer.opacity * opacity_mod).clamp(0.0, 1.0),
        contrast: (layer.contrast * contrast_mod).clamp(1.0, 2.8),
    }
}

/// Encode a rendered frame directory into an H.264 MP4 using ffmpeg.
pub fn encode_frames_to_mp4(
    frame_dir: &Path,
    fps: u32,
    output_path: &Path,
) -> Result<(), Box<dyn Error>> {
    let check = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map_err(|err| {
            format!("ffmpeg not found in PATH ({err}); install ffmpeg to encode V2 animations")
        })?;
    if !check.status.success() {
        return Err("ffmpeg is unavailable; cannot encode animation".into());
    }

    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-framerate")
        .arg(fps.to_string())
        .arg("-i")
        .arg("frame_%06d.png")
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-movflags")
        .arg("+faststart")
        .arg(output_path)
        .current_dir(frame_dir)
        .status()?;

    if !status.success() {
        return Err(format!(
            "ffmpeg failed to encode MP4 from frames in {}",
            frame_dir.display()
        )
        .into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LayerBlendMode;

    fn sample_layer() -> GenerateLayerNode {
        GenerateLayerNode {
            symmetry: 3,
            symmetry_style: 1,
            iterations: 220,
            seed: 11,
            fill_scale: 1.4,
            fractal_zoom: 0.85,
            art_style: 2,
            art_style_secondary: 3,
            art_style_mix: 0.4,
            bend_strength: 0.5,
            warp_strength: 0.8,
            warp_frequency: 2.2,
            tile_scale: 0.9,
            tile_phase: 0.3,
            center_x: 0.1,
            center_y: -0.1,
            shader_layer_count: 4,
            blend_mode: LayerBlendMode::Normal,
            opacity: 0.7,
            contrast: 1.3,
        }
    }

    #[test]
    fn modulation_stays_within_bounds() {
        let layer = sample_layer();
        let modulated = modulate_layer_for_frame(layer, 45, 900, 2);
        assert!((48..=1200).contains(&modulated.iterations));
        assert!((0.6..=2.6).contains(&modulated.fill_scale));
        assert!((0.3..=2.2).contains(&modulated.fractal_zoom));
        assert!((0.0..=1.0).contains(&modulated.art_style_mix));
        assert!((0.0..=1.0).contains(&modulated.opacity));
        assert!((1.0..=2.8).contains(&modulated.contrast));
    }
}

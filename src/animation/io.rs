//! Frame-directory image loading and in-process MP4 encoding.

use std::error::Error;
use std::path::{Path, PathBuf};

use image::DynamicImage;

use super::RawVideoEncoder;

/// Encode a rendered frame directory into an H.264 MP4 without shelling out.
pub fn encode_frames_to_mp4(
    frame_dir: &Path,
    fps: u32,
    output_path: &Path,
) -> Result<(), Box<dyn Error>> {
    let frame_paths = sorted_frame_paths(frame_dir)?;
    if frame_paths.is_empty() {
        return Err(format!("no PNG frames found in {}", frame_dir.display()).into());
    }

    let first_frame = image::open(&frame_paths[0])?;
    let first_rgba = first_frame.to_rgba8();
    let (width, height) = first_rgba.dimensions();
    let mut encoder = RawVideoEncoder::spawn(width, height, fps, output_path)?;

    let mut bgra = Vec::new();
    rgba_to_bgra(first_rgba.as_raw(), &mut bgra);
    encoder.write_bgra_frame(&bgra)?;

    for path in frame_paths.iter().skip(1) {
        let frame = image::open(path)?;
        verify_frame_dimensions(path, &frame, width, height)?;
        let rgba = frame.to_rgba8();
        rgba_to_bgra(rgba.as_raw(), &mut bgra);
        encoder.write_bgra_frame(&bgra)?;
    }

    encoder.finish()
}

fn sorted_frame_paths(frame_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut frame_paths = Vec::new();
    for entry in std::fs::read_dir(frame_dir)? {
        let path = entry?.path();
        let is_png = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("png"))
            .unwrap_or(false);
        if is_png {
            frame_paths.push(path);
        }
    }
    frame_paths.sort();
    Ok(frame_paths)
}

fn verify_frame_dimensions(
    path: &Path,
    frame: &DynamicImage,
    expected_width: u32,
    expected_height: u32,
) -> Result<(), Box<dyn Error>> {
    let width = frame.width();
    let height = frame.height();
    if width == expected_width && height == expected_height {
        return Ok(());
    }
    Err(format!(
        "frame {} has dimensions {}x{}, expected {}x{}",
        path.display(),
        width,
        height,
        expected_width,
        expected_height
    )
    .into())
}

fn rgba_to_bgra(rgba: &[u8], out_bgra: &mut Vec<u8>) {
    out_bgra.clear();
    out_bgra.reserve_exact(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        out_bgra.push(pixel[2]);
        out_bgra.push(pixel[1]);
        out_bgra.push(pixel[0]);
        out_bgra.push(pixel[3]);
    }
}

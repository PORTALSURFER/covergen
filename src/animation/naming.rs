//! Animation clip/frame naming and output path helpers.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime_config::AnimationConfig;

/// Returns the number of frames to render for one animation clip.
pub fn total_frames(config: &AnimationConfig) -> u32 {
    config.seconds.saturating_mul(config.fps).max(1)
}

/// Build a unique temporary directory for rendered animation frames.
pub fn create_frame_dir(base_output: &str, clip_index: u32) -> Result<PathBuf, Box<dyn Error>> {
    let stem = Path::new(base_output)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("covergen")
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

/// Compute one frame filename in zero-padded sequence format.
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
        .unwrap_or("covergen_animation");
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

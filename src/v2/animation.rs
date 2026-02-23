//! Animation helpers for V2 graph execution.
//!
//! This module handles clip timing, output naming, and ffmpeg integration for
//! both frame-directory and direct-stream encoding paths.

use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use super::cli::AnimationConfig;

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

/// Encode a rendered frame directory into an H.264 MP4 using ffmpeg.
pub fn encode_frames_to_mp4(
    frame_dir: &Path,
    fps: u32,
    output_path: &Path,
) -> Result<(), Box<dyn Error>> {
    ensure_ffmpeg_available()?;

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

/// Streaming raw grayscale frame encoder backed by an ffmpeg subprocess.
pub struct RawVideoEncoder {
    child: Child,
    stdin: Option<ChildStdin>,
    expected_frame_bytes: usize,
}

impl RawVideoEncoder {
    /// Spawn ffmpeg and configure stdin for raw grayscale frame streaming.
    pub fn spawn(
        width: u32,
        height: u32,
        fps: u32,
        output_path: &Path,
    ) -> Result<Self, Box<dyn Error>> {
        ensure_ffmpeg_available()?;
        let expected_frame_bytes = (width as usize)
            .checked_mul(height as usize)
            .ok_or("invalid frame dimensions for streaming encoder")?;

        let mut child = Command::new("ffmpeg")
            .arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-f")
            .arg("rawvideo")
            .arg("-pix_fmt")
            .arg("gray")
            .arg("-s:v")
            .arg(format!("{}x{}", width, height))
            .arg("-r")
            .arg(fps.to_string())
            .arg("-i")
            .arg("pipe:0")
            .arg("-an")
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-movflags")
            .arg("+faststart")
            .arg(output_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("failed to start ffmpeg rawvideo encoder: {err}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or("failed to open ffmpeg stdin for rawvideo stream")?;

        Ok(Self {
            child,
            stdin: Some(stdin),
            expected_frame_bytes,
        })
    }

    /// Push one grayscale frame into ffmpeg stdin.
    pub fn write_gray_frame(&mut self, frame_gray: &[u8]) -> Result<(), Box<dyn Error>> {
        if frame_gray.len() != self.expected_frame_bytes {
            return Err(format!(
                "invalid frame byte count: expected {}, got {}",
                self.expected_frame_bytes,
                frame_gray.len()
            )
            .into());
        }

        let stdin = self
            .stdin
            .as_mut()
            .ok_or("ffmpeg stdin is not available for frame streaming")?;
        stdin.write_all(frame_gray)?;
        Ok(())
    }

    /// Finalize stream and wait for ffmpeg to finish encoding.
    pub fn finish(mut self) -> Result<(), Box<dyn Error>> {
        if let Some(mut stdin) = self.stdin.take() {
            stdin.flush()?;
        }
        let output = self.child.wait_with_output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ffmpeg failed while streaming rawvideo: {stderr}").into());
        }
        Ok(())
    }
}

fn ensure_ffmpeg_available() -> Result<(), Box<dyn Error>> {
    let check = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map_err(|err| {
            format!("ffmpeg not found in PATH ({err}); install ffmpeg to encode V2 animations")
        })?;
    if !check.status.success() {
        return Err("ffmpeg is unavailable; cannot encode animation".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_frames_is_never_zero() {
        let cfg = AnimationConfig {
            enabled: true,
            seconds: 0,
            fps: 0,
            keep_frames: false,
            reels: false,
        };
        assert_eq!(total_frames(&cfg), 1);
    }
}

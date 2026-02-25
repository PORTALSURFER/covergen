//! Animation helpers for graph execution.
//!
//! This module handles clip timing, output naming, and ffmpeg integration for
//! both frame-directory and direct-stream encoding paths.

use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use super::runtime_config::AnimationConfig;

/// Optional environment override for H.264 encoder selection.
///
/// Supported values:
/// - `auto` (default)
/// - `nvenc` / `h264_nvenc`
/// - `libx264` / `x264`
const H264_ENCODER_ENV: &str = "COVERGEN_H264_ENCODER";

/// Concrete ffmpeg encoder backend for H.264 export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum H264Encoder {
    Nvenc,
    Libx264,
}

impl H264Encoder {
    fn codec_name(self) -> &'static str {
        match self {
            Self::Nvenc => "h264_nvenc",
            Self::Libx264 => "libx264",
        }
    }

    fn output_pixel_format(self) -> &'static str {
        match self {
            // NVENC performs best with NV12 input surfaces.
            Self::Nvenc => "nv12",
            Self::Libx264 => "yuv420p",
        }
    }

    fn extra_args(self) -> &'static [&'static str] {
        match self {
            // Favor high-quality VBR CQ defaults for export while keeping
            // encoding fully on the GPU when NVENC is available.
            Self::Nvenc => &[
                "-preset", "p5", "-tune", "hq", "-rc", "vbr", "-cq", "19", "-b:v", "0",
            ],
            Self::Libx264 => &[],
        }
    }
}

/// Raw frame layout accepted by the streaming encoder stdin path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StreamFrameFormat {
    Gray8,
    Bgra8,
}

impl StreamFrameFormat {
    fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Gray8 => 1,
            Self::Bgra8 => 4,
        }
    }

    fn input_pixel_format(self) -> &'static str {
        match self {
            Self::Gray8 => "gray",
            Self::Bgra8 => "bgra",
        }
    }
}

/// Frame-transfer architecture used by one export request.
///
/// A future zero-copy GPU handoff mode is planned but not active yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportDataPath {
    CpuReadback,
    CpuReadbackGpuUpload,
}

/// Encoder selection outcome for one export request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EncoderSelection {
    preferred: H264Encoder,
    allow_nvenc_fallback: bool,
}

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

/// Encode a rendered frame directory into an H.264 MP4 using ffmpeg.
pub fn encode_frames_to_mp4(
    frame_dir: &Path,
    fps: u32,
    output_path: &Path,
) -> Result<(), Box<dyn Error>> {
    ensure_ffmpeg_available()?;
    let selection = select_encoder()?;
    let first_try =
        encode_frames_to_mp4_with_encoder(frame_dir, fps, output_path, selection.preferred);
    if let Err(err) = first_try {
        if selection.allow_nvenc_fallback {
            eprintln!(
                "[v2] {} export unavailable ({}); falling back to libx264",
                selection.preferred.codec_name(),
                err
            );
            encode_frames_to_mp4_with_encoder(frame_dir, fps, output_path, H264Encoder::Libx264)?;
        } else {
            return Err(err);
        }
    }
    Ok(())
}

/// Streaming raw grayscale frame encoder backed by an ffmpeg subprocess.
pub struct RawVideoEncoder {
    child: Child,
    stdin: Option<ChildStdin>,
    expected_frame_bytes: usize,
    encoder: H264Encoder,
    frame_format: StreamFrameFormat,
    data_path: ExportDataPath,
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
        let selection = select_encoder()?;
        let preferred_format = preferred_stream_frame_format(selection.preferred);
        let first_try = spawn_rawvideo_encoder(
            width,
            height,
            fps,
            output_path,
            selection.preferred,
            preferred_format,
        );
        match first_try {
            Ok(encoder) => Ok(encoder),
            Err(err) if selection.allow_nvenc_fallback => {
                eprintln!(
                    "[v2] {} stream export unavailable ({}); falling back to libx264",
                    selection.preferred.codec_name(),
                    err
                );
                spawn_rawvideo_encoder(
                    width,
                    height,
                    fps,
                    output_path,
                    H264Encoder::Libx264,
                    preferred_stream_frame_format(H264Encoder::Libx264),
                )
            }
            Err(err) => Err(err),
        }
    }

    /// Frame layout required by this encoder.
    pub fn frame_format(&self) -> StreamFrameFormat {
        self.frame_format
    }

    /// Export data-transfer mode selected for this stream.
    pub fn data_path(&self) -> ExportDataPath {
        self.data_path
    }

    /// Push one grayscale frame into ffmpeg stdin.
    pub fn write_gray_frame(&mut self, frame_gray: &[u8]) -> Result<(), Box<dyn Error>> {
        if self.frame_format != StreamFrameFormat::Gray8 {
            return Err("stream encoder expects BGRA frames, not grayscale".into());
        }
        self.write_frame(frame_gray)
    }

    /// Push one BGRA frame into ffmpeg stdin.
    pub fn write_bgra_frame(&mut self, frame_bgra: &[u8]) -> Result<(), Box<dyn Error>> {
        if self.frame_format != StreamFrameFormat::Bgra8 {
            return Err("stream encoder expects grayscale frames, not BGRA".into());
        }
        self.write_frame(frame_bgra)
    }

    fn write_frame(&mut self, frame_bytes: &[u8]) -> Result<(), Box<dyn Error>> {
        if frame_bytes.len() != self.expected_frame_bytes {
            return Err(format!(
                "invalid frame byte count: expected {}, got {}",
                self.expected_frame_bytes,
                frame_bytes.len()
            )
            .into());
        }

        let stdin = self
            .stdin
            .as_mut()
            .ok_or("ffmpeg stdin is not available for frame streaming")?;
        stdin.write_all(frame_bytes)?;
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
            return Err(format!(
                "ffmpeg {} failed while streaming rawvideo: {}",
                self.encoder.codec_name(),
                stderr.trim()
            )
            .into());
        }
        Ok(())
    }
}

fn spawn_rawvideo_encoder(
    width: u32,
    height: u32,
    fps: u32,
    output_path: &Path,
    encoder: H264Encoder,
    frame_format: StreamFrameFormat,
) -> Result<RawVideoEncoder, Box<dyn Error>> {
    let expected_frame_bytes = checked_frame_bytes(width, height, frame_format)?;
    let mut command = Command::new("ffmpeg");
    command
        .arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg(frame_format.input_pixel_format())
        .arg("-s:v")
        .arg(format!("{}x{}", width, height))
        .arg("-r")
        .arg(fps.to_string())
        .arg("-i")
        .arg("pipe:0")
        .arg("-an");
    if encoder == H264Encoder::Nvenc && frame_format == StreamFrameFormat::Bgra8 {
        command.arg("-vf").arg(nvenc_upload_filter_graph());
    }
    append_h264_encoder_args(&mut command, encoder);
    command
        .arg(output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|err| {
        format!(
            "failed to start ffmpeg {} encoder: {err}",
            encoder.codec_name()
        )
    })?;

    let stdin = child
        .stdin
        .take()
        .ok_or("failed to open ffmpeg stdin for rawvideo stream")?;
    Ok(RawVideoEncoder {
        child,
        stdin: Some(stdin),
        expected_frame_bytes,
        encoder,
        frame_format,
        data_path: data_path_for_encoder(encoder),
    })
}

fn checked_frame_bytes(
    width: u32,
    height: u32,
    frame_format: StreamFrameFormat,
) -> Result<usize, Box<dyn Error>> {
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or("invalid frame dimensions for streaming encoder")?;
    pixels
        .checked_mul(frame_format.bytes_per_pixel())
        .ok_or_else(|| "invalid frame byte count for streaming encoder".into())
}

fn preferred_stream_frame_format(encoder: H264Encoder) -> StreamFrameFormat {
    match encoder {
        H264Encoder::Nvenc => StreamFrameFormat::Bgra8,
        H264Encoder::Libx264 => StreamFrameFormat::Gray8,
    }
}

fn data_path_for_encoder(encoder: H264Encoder) -> ExportDataPath {
    match encoder {
        H264Encoder::Nvenc => ExportDataPath::CpuReadbackGpuUpload,
        H264Encoder::Libx264 => ExportDataPath::CpuReadback,
    }
}

fn nvenc_upload_filter_graph() -> &'static str {
    "hwupload_cuda,scale_cuda=format=nv12"
}

fn append_h264_encoder_args(command: &mut Command, encoder: H264Encoder) {
    command.arg("-c:v").arg(encoder.codec_name());
    for arg in encoder.extra_args() {
        command.arg(arg);
    }
    command
        .arg("-pix_fmt")
        .arg(encoder.output_pixel_format())
        .arg("-movflags")
        .arg("+faststart");
}

fn encode_frames_to_mp4_with_encoder(
    frame_dir: &Path,
    fps: u32,
    output_path: &Path,
    encoder: H264Encoder,
) -> Result<(), Box<dyn Error>> {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-framerate")
        .arg(fps.to_string())
        .arg("-i")
        .arg("frame_%06d.png");
    append_h264_encoder_args(&mut command, encoder);
    let output = command.arg(output_path).current_dir(frame_dir).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ffmpeg {} failed to encode MP4 from frames in {}: {}",
            encoder.codec_name(),
            frame_dir.display(),
            stderr.trim()
        )
        .into());
    }
    Ok(())
}

fn select_encoder() -> Result<EncoderSelection, Box<dyn Error>> {
    if let Some(forced) = forced_encoder_from_env()? {
        return Ok(EncoderSelection {
            preferred: forced,
            allow_nvenc_fallback: false,
        });
    }
    if cfg!(windows) && probe_nvenc_encoder() {
        return Ok(EncoderSelection {
            preferred: H264Encoder::Nvenc,
            allow_nvenc_fallback: true,
        });
    }
    Ok(EncoderSelection {
        preferred: H264Encoder::Libx264,
        allow_nvenc_fallback: false,
    })
}

fn forced_encoder_from_env() -> Result<Option<H264Encoder>, Box<dyn Error>> {
    let raw = match std::env::var(H264_ENCODER_ENV) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(err) => {
            return Err(format!(
                "failed to read {H264_ENCODER_ENV} override for H.264 encoder selection: {err}"
            )
            .into())
        }
    };
    parse_encoder_override(&raw).map_err(|err| err.into())
}

fn parse_encoder_override(raw: &str) -> Result<Option<H264Encoder>, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "auto" => Ok(None),
        "nvenc" | "h264_nvenc" => Ok(Some(H264Encoder::Nvenc)),
        "x264" | "libx264" => Ok(Some(H264Encoder::Libx264)),
        _ => Err(format!(
            "invalid {H264_ENCODER_ENV} value '{}'; expected auto|nvenc|h264_nvenc|libx264|x264",
            raw
        )),
    }
}

fn probe_nvenc_encoder() -> bool {
    let output = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("color=c=black:s=16x16:d=0.04")
        .arg("-frames:v")
        .arg("1")
        .arg("-an")
        .arg("-vf")
        .arg("format=bgra,hwupload_cuda,scale_cuda=format=nv12")
        .arg("-c:v")
        .arg(H264Encoder::Nvenc.codec_name())
        .arg("-f")
        .arg("null")
        .arg("-")
        .output();
    output
        .map(|result| result.status.success())
        .unwrap_or(false)
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
            motion: crate::runtime_config::AnimationMotion::Normal,
        };
        assert_eq!(total_frames(&cfg), 1);
    }

    #[test]
    fn parse_encoder_override_accepts_auto_and_known_aliases() {
        assert_eq!(parse_encoder_override("auto"), Ok(None));
        assert_eq!(parse_encoder_override(""), Ok(None));
        assert_eq!(
            parse_encoder_override("h264_nvenc"),
            Ok(Some(H264Encoder::Nvenc))
        );
        assert_eq!(
            parse_encoder_override("nvenc"),
            Ok(Some(H264Encoder::Nvenc))
        );
        assert_eq!(
            parse_encoder_override("libx264"),
            Ok(Some(H264Encoder::Libx264))
        );
        assert_eq!(
            parse_encoder_override("x264"),
            Ok(Some(H264Encoder::Libx264))
        );
    }

    #[test]
    fn parse_encoder_override_rejects_unknown_value() {
        let err = parse_encoder_override("vp9").expect_err("unknown encoder should fail");
        assert!(err.contains(H264_ENCODER_ENV));
    }
}

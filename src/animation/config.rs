//! Stream encoder configuration, validation, and sizing helpers.

use std::error::Error;

use super::MP4_MOVIE_TIMESCALE;

pub(super) const STREAM_FRAME_FORMAT_ENV: &str = "COVERGEN_STREAM_FRAME_FORMAT";
pub(super) const STREAM_ENCODER_ENV: &str = "COVERGEN_STREAM_ENCODER";

/// Raw frame layout accepted by the streaming encoder stdin path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StreamFrameFormat {
    Gray8,
    Bgra8,
}

impl StreamFrameFormat {
    pub(super) fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Gray8 => 1,
            Self::Bgra8 => 4,
        }
    }
}

/// Frame-transfer architecture used by one export request.
///
/// A future zero-copy GPU handoff mode is planned but not active yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum ExportDataPath {
    CpuReadback,
    CpuReadbackGpuUpload,
    #[cfg(windows)]
    CpuReadbackGpuEncode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StreamEncoderPreference {
    Auto,
    OpenH264,
    Nvenc,
}

pub(super) fn validate_encoder_input(
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(), Box<dyn Error>> {
    if fps == 0 {
        return Err("invalid fps: expected value >= 1".into());
    }
    if width == 0 || height == 0 {
        return Err("invalid frame dimensions: width and height must be >= 1".into());
    }
    if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(format!(
            "H.264 export requires even dimensions; got {}x{}",
            width, height
        )
        .into());
    }
    if MP4_MOVIE_TIMESCALE / fps == 0 {
        return Err(format!(
            "invalid fps {fps}: exceeds MP4 timescale {}",
            MP4_MOVIE_TIMESCALE
        )
        .into());
    }
    Ok(())
}

pub(super) fn preferred_stream_encoder() -> Result<StreamEncoderPreference, Box<dyn Error>> {
    let raw = match std::env::var(STREAM_ENCODER_ENV) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(StreamEncoderPreference::Auto),
        Err(err) => {
            return Err(format!(
                "failed to read {STREAM_ENCODER_ENV} override for stream encoder: {err}"
            )
            .into())
        }
    };
    parse_stream_encoder_preference(raw.as_str()).map_err(|err| err.into())
}

pub(super) fn parse_stream_encoder_preference(
    raw: &str,
) -> Result<StreamEncoderPreference, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "auto" => Ok(StreamEncoderPreference::Auto),
        "openh264" | "open_h264" | "software" | "cpu" => Ok(StreamEncoderPreference::OpenH264),
        "nvenc" | "gpu" | "hardware" => Ok(StreamEncoderPreference::Nvenc),
        _ => Err(format!(
            "invalid {STREAM_ENCODER_ENV} value '{}'; expected auto|openh264|nvenc",
            raw
        )),
    }
}

pub(super) fn preferred_stream_frame_format() -> Result<StreamFrameFormat, Box<dyn Error>> {
    let raw = match std::env::var(STREAM_FRAME_FORMAT_ENV) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(StreamFrameFormat::Bgra8),
        Err(err) => {
            return Err(format!(
                "failed to read {STREAM_FRAME_FORMAT_ENV} override for stream frame format: {err}"
            )
            .into())
        }
    };
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "gray" | "gray8" => Ok(StreamFrameFormat::Gray8),
        "bgra" | "bgra8" => Ok(StreamFrameFormat::Bgra8),
        _ => Err(format!(
            "invalid {STREAM_FRAME_FORMAT_ENV} value '{}'; expected gray|gray8|bgra|bgra8",
            raw
        )
        .into()),
    }
}

pub(super) fn data_path_for_frame_format(frame_format: StreamFrameFormat) -> ExportDataPath {
    match frame_format {
        StreamFrameFormat::Gray8 => ExportDataPath::CpuReadback,
        StreamFrameFormat::Bgra8 => ExportDataPath::CpuReadbackGpuUpload,
    }
}

pub(super) fn recommended_bitrate(width: u32, height: u32, fps: u32) -> u32 {
    let pixels_per_second = (width as u64)
        .saturating_mul(height as u64)
        .saturating_mul(fps as u64);
    let bits_per_pixel = 8u64;
    let estimated = pixels_per_second.saturating_mul(bits_per_pixel);
    estimated.clamp(2_000_000, 24_000_000) as u32
}

pub(super) fn checked_frame_bytes(
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

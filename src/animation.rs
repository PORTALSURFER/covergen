//! Animation helpers for graph execution.
//!
//! This module handles clip timing, output naming, and in-process H.264/MP4
//! encoding for both frame-directory and direct-stream export paths.

use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::DynamicImage;
use mp4::{AvcConfig, MediaConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig, TrackType};
use openh264::encoder::{BitRate, Encoder, EncoderConfig, FrameRate, RateControlMode, UsageType};
use openh264::formats::{BgraSliceU8, YUVBuffer};

use super::runtime_config::AnimationConfig;

const MP4_MOVIE_TIMESCALE: u32 = 90_000;
const MP4_TRACK_ID_VIDEO: u32 = 1;
const H264_NAL_TYPE_IDR: u8 = 5;
const H264_NAL_TYPE_SPS: u8 = 7;
const H264_NAL_TYPE_PPS: u8 = 8;
const STREAM_FRAME_FORMAT_ENV: &str = "COVERGEN_STREAM_FRAME_FORMAT";

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
}

/// Frame-transfer architecture used by one export request.
///
/// A future zero-copy GPU handoff mode is planned but not active yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportDataPath {
    CpuReadback,
    CpuReadbackGpuUpload,
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

/// Streaming raw frame encoder backed by in-process OpenH264 + MP4 muxing.
pub struct RawVideoEncoder {
    encoder: Encoder,
    muxer: Mp4Writer<std::io::BufWriter<File>>,
    yuv: YUVBuffer,
    gray_to_bgra_scratch: Vec<u8>,
    expected_frame_bytes: usize,
    frame_format: StreamFrameFormat,
    data_path: ExportDataPath,
    width: u32,
    height: u32,
    frame_duration_ticks: u32,
    frame_ticks_accumulator: u64,
    frame_index: u64,
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
    track_ready: bool,
}

impl RawVideoEncoder {
    /// Create one streaming encoder session.
    pub fn spawn(
        width: u32,
        height: u32,
        fps: u32,
        output_path: &Path,
    ) -> Result<Self, Box<dyn Error>> {
        validate_encoder_input(width, height, fps)?;
        let frame_format = preferred_stream_frame_format()?;

        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(recommended_bitrate(width, height, fps)))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .rate_control_mode(RateControlMode::Quality)
            .usage_type(UsageType::ScreenContentNonRealTime)
            .skip_frames(false);
        let encoder = Encoder::with_api_config(openh264::OpenH264API::from_source(), config)?;

        let file = File::create(output_path)?;
        let writer = std::io::BufWriter::new(file);
        let mp4_config = Mp4Config {
            major_brand: "isom".parse()?,
            minor_version: 0,
            compatible_brands: vec![
                "isom".parse()?,
                "iso2".parse()?,
                "avc1".parse()?,
                "mp41".parse()?,
            ],
            timescale: MP4_MOVIE_TIMESCALE,
        };
        let muxer = Mp4Writer::write_start(writer, &mp4_config)?;

        Ok(Self {
            encoder,
            muxer,
            yuv: YUVBuffer::new(width as usize, height as usize),
            gray_to_bgra_scratch: Vec::new(),
            expected_frame_bytes: checked_frame_bytes(width, height, frame_format)?,
            frame_format,
            data_path: data_path_for_frame_format(frame_format),
            width,
            height,
            frame_duration_ticks: MP4_MOVIE_TIMESCALE / fps,
            frame_ticks_accumulator: 0,
            frame_index: 0,
            sps: None,
            pps: None,
            track_ready: false,
        })
    }

    /// Frame layout required by this encoder.
    pub fn frame_format(&self) -> StreamFrameFormat {
        self.frame_format
    }

    /// Export data-transfer mode selected for this stream.
    pub fn data_path(&self) -> ExportDataPath {
        self.data_path
    }

    /// Push one grayscale frame into the stream.
    pub fn write_gray_frame(&mut self, frame_gray: &[u8]) -> Result<(), Box<dyn Error>> {
        if self.frame_format != StreamFrameFormat::Gray8 {
            return Err("stream encoder expects BGRA frames, not grayscale".into());
        }
        if frame_gray.len() != self.expected_frame_bytes {
            return Err(format!(
                "invalid frame byte count: expected {}, got {}",
                self.expected_frame_bytes,
                frame_gray.len()
            )
            .into());
        }
        let mut scratch = std::mem::take(&mut self.gray_to_bgra_scratch);
        scratch.clear();
        scratch.reserve_exact(frame_gray.len().saturating_mul(4));
        for &value in frame_gray {
            scratch.push(value);
            scratch.push(value);
            scratch.push(value);
            scratch.push(255);
        }
        let result = self.encode_one_bgra_frame(&scratch);
        self.gray_to_bgra_scratch = scratch;
        result
    }

    /// Push one BGRA frame into the stream.
    pub fn write_bgra_frame(&mut self, frame_bgra: &[u8]) -> Result<(), Box<dyn Error>> {
        if self.frame_format != StreamFrameFormat::Bgra8 {
            return Err("stream encoder expects grayscale frames, not BGRA".into());
        }
        if frame_bgra.len() != self.expected_frame_bytes {
            return Err(format!(
                "invalid frame byte count: expected {}, got {}",
                self.expected_frame_bytes,
                frame_bgra.len()
            )
            .into());
        }
        self.encode_one_bgra_frame(frame_bgra)
    }

    fn encode_one_bgra_frame(&mut self, frame_bgra: &[u8]) -> Result<(), Box<dyn Error>> {
        let bgra = BgraSliceU8::new(frame_bgra, (self.width as usize, self.height as usize));
        self.yuv.read_rgb(bgra);
        if self.frame_index == 0 {
            self.encoder.force_intra_frame();
        }
        let encoded = self.encoder.encode(&self.yuv)?;
        let mut sample_payload = Vec::new();
        let mut is_sync = false;

        for layer_index in 0..encoded.num_layers() {
            let layer = encoded
                .layer(layer_index)
                .ok_or("encoded layer index out of bounds")?;
            for nal_index in 0..layer.nal_count() {
                let nal = layer
                    .nal_unit(nal_index)
                    .ok_or("encoded NAL index out of bounds")?;
                let payload = strip_annex_b_start_code(nal);
                if payload.is_empty() {
                    continue;
                }
                let nal_type = payload[0] & 0x1F;
                if nal_type == H264_NAL_TYPE_SPS {
                    self.sps = Some(payload.to_vec());
                    continue;
                }
                if nal_type == H264_NAL_TYPE_PPS {
                    self.pps = Some(payload.to_vec());
                    continue;
                }
                if nal_type == H264_NAL_TYPE_IDR {
                    is_sync = true;
                }
                append_length_prefixed_nal(&mut sample_payload, payload)?;
            }
        }

        if sample_payload.is_empty() {
            return Err("encoded frame contained no MP4 sample payload".into());
        }
        self.ensure_track()?;

        let sample = Mp4Sample {
            start_time: self.frame_ticks_accumulator,
            duration: self.frame_duration_ticks,
            rendering_offset: 0,
            is_sync,
            bytes: sample_payload.into(),
        };
        self.muxer.write_sample(MP4_TRACK_ID_VIDEO, &sample)?;

        self.frame_index = self.frame_index.saturating_add(1);
        self.frame_ticks_accumulator = self
            .frame_ticks_accumulator
            .saturating_add(self.frame_duration_ticks as u64);
        Ok(())
    }

    /// Finalize stream and write the MP4 trailer.
    pub fn finish(mut self) -> Result<(), Box<dyn Error>> {
        if self.frame_index == 0 {
            return Err("cannot finish empty video stream; no frames were written".into());
        }
        self.muxer.write_end()?;
        Ok(())
    }

    fn ensure_track(&mut self) -> Result<(), Box<dyn Error>> {
        if self.track_ready {
            return Ok(());
        }
        let sps = self
            .sps
            .as_ref()
            .ok_or("missing SPS NAL from encoder output; cannot initialize MP4 track")?;
        let pps = self
            .pps
            .as_ref()
            .ok_or("missing PPS NAL from encoder output; cannot initialize MP4 track")?;
        let track = TrackConfig {
            track_type: TrackType::Video,
            timescale: MP4_MOVIE_TIMESCALE,
            language: String::from("und"),
            media_conf: MediaConfig::AvcConfig(AvcConfig {
                width: u16::try_from(self.width)
                    .map_err(|_| format!("video width {} exceeds MP4/H.264 limits", self.width))?,
                height: u16::try_from(self.height).map_err(|_| {
                    format!("video height {} exceeds MP4/H.264 limits", self.height)
                })?,
                seq_param_set: sps.clone(),
                pic_param_set: pps.clone(),
            }),
        };
        self.muxer.add_track(&track)?;
        self.track_ready = true;
        Ok(())
    }
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

fn validate_encoder_input(width: u32, height: u32, fps: u32) -> Result<(), Box<dyn Error>> {
    if fps == 0 {
        return Err("invalid fps: expected value >= 1".into());
    }
    if width == 0 || height == 0 {
        return Err("invalid frame dimensions: width and height must be >= 1".into());
    }
    if width % 2 != 0 || height % 2 != 0 {
        return Err(format!(
            "OpenH264 requires even dimensions; got {}x{}",
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

fn preferred_stream_frame_format() -> Result<StreamFrameFormat, Box<dyn Error>> {
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

fn data_path_for_frame_format(frame_format: StreamFrameFormat) -> ExportDataPath {
    match frame_format {
        StreamFrameFormat::Gray8 => ExportDataPath::CpuReadback,
        StreamFrameFormat::Bgra8 => ExportDataPath::CpuReadbackGpuUpload,
    }
}

fn recommended_bitrate(width: u32, height: u32, fps: u32) -> u32 {
    let pixels_per_second = (width as u64)
        .saturating_mul(height as u64)
        .saturating_mul(fps as u64);
    let bits_per_pixel = 8u64;
    let estimated = pixels_per_second.saturating_mul(bits_per_pixel);
    estimated.clamp(2_000_000, 24_000_000) as u32
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

fn append_length_prefixed_nal(dst: &mut Vec<u8>, payload: &[u8]) -> Result<(), Box<dyn Error>> {
    let len = u32::try_from(payload.len()).map_err(|_| "NAL payload is too large")?;
    dst.extend_from_slice(&len.to_be_bytes());
    dst.extend_from_slice(payload);
    Ok(())
}

fn strip_annex_b_start_code(nal: &[u8]) -> &[u8] {
    if nal.starts_with(&[0, 0, 0, 1]) {
        return &nal[4..];
    }
    if nal.starts_with(&[0, 0, 1]) {
        return &nal[3..];
    }
    nal
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
    fn frame_filename_is_zero_padded() {
        assert_eq!(frame_filename(0), "frame_000001.png");
        assert_eq!(frame_filename(41), "frame_000042.png");
    }

    #[test]
    fn strip_annex_b_removes_common_prefixes() {
        assert_eq!(strip_annex_b_start_code(&[0, 0, 1, 0x67]), &[0x67]);
        assert_eq!(strip_annex_b_start_code(&[0, 0, 0, 1, 0x68]), &[0x68]);
        assert_eq!(strip_annex_b_start_code(&[0x65, 0xAA]), &[0x65, 0xAA]);
    }

    #[test]
    fn validate_encoder_input_rejects_odd_dimensions() {
        let err = validate_encoder_input(1279, 720, 30).expect_err("odd width must fail");
        assert!(err.to_string().contains("even dimensions"));
    }

    #[test]
    fn recommended_bitrate_is_clamped() {
        assert_eq!(recommended_bitrate(64, 64, 1), 2_000_000);
        assert_eq!(recommended_bitrate(3840, 2160, 60), 24_000_000);
    }
}

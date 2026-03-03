//! Animation helpers for graph execution.
//!
//! This module is organized by responsibility:
//! - `naming`: clip/frame naming and output path helpers
//! - `io`: frame-directory ingestion and MP4 stream encode orchestration
//! - `mux`: audio/video muxing helpers
//! - `config`: encoder input validation and env-driven stream options
//! - `backend`: OpenH264/NVENC backend setup and platform glue
//! - `h264`: NAL parsing and MP4 sample packing helpers

mod backend;
mod config;
mod h264;
mod io;
mod mux;
mod naming;

use std::error::Error;
use std::fs::File;
#[cfg(windows)]
use std::panic::AssertUnwindSafe;
use std::path::Path;

use mp4::{AvcConfig, MediaConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig, TrackType};
#[cfg(windows)]
use nvenc::sys::enums::{NVencBufferFormat, NVencPicStruct, NVencPicType};
use openh264::encoder::Encoder;
use openh264::formats::{BgraSliceU8, YUVBuffer};

use backend::create_stream_backend;
use config::{checked_frame_bytes, preferred_stream_frame_format, validate_encoder_input};
#[cfg(test)]
use config::{parse_stream_encoder_preference, recommended_bitrate, StreamEncoderPreference};
#[cfg(windows)]
use h264::parse_annex_b_packet;
use h264::{append_h264_unit_to_payload, strip_annex_b_start_code, EncodedFramePayload};

pub(crate) use config::{ExportDataPath, StreamFrameFormat};
pub use io::encode_frames_to_mp4;
pub use mux::mux_wav_audio_into_mp4;
pub use naming::{clip_output_path, create_frame_dir, frame_filename, total_frames};

const MP4_MOVIE_TIMESCALE: u32 = 90_000;
const MP4_TRACK_ID_VIDEO: u32 = 1;

#[allow(clippy::large_enum_variant)]
enum RawEncoderBackend {
    OpenH264 {
        encoder: Encoder,
        yuv: YUVBuffer,
    },
    #[cfg(windows)]
    Nvenc(backend::NvencEncoderState),
}

/// Streaming raw frame encoder backed by hardware NVENC when available with
/// OpenH264 fallback for broad compatibility.
pub struct RawVideoEncoder {
    backend: RawEncoderBackend,
    muxer: Mp4Writer<std::io::BufWriter<File>>,
    gray_to_bgra_scratch: Vec<u8>,
    expected_frame_bytes: usize,
    frame_format: StreamFrameFormat,
    data_path: ExportDataPath,
    width: u32,
    height: u32,
    #[cfg(windows)]
    fps: u32,
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
        let (backend, data_path) = create_stream_backend(width, height, fps, frame_format)?;

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
            backend,
            muxer,
            gray_to_bgra_scratch: Vec::new(),
            expected_frame_bytes: checked_frame_bytes(width, height, frame_format)?,
            frame_format,
            data_path,
            width,
            height,
            #[cfg(windows)]
            fps,
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
        let encoded = match &mut self.backend {
            RawEncoderBackend::OpenH264 { encoder, yuv } => Self::encode_openh264_frame(
                encoder,
                yuv,
                self.width,
                self.height,
                self.frame_index,
                frame_bgra,
            )?,
            #[cfg(windows)]
            RawEncoderBackend::Nvenc(state) => {
                match Self::encode_nvenc_frame_guarded(
                    state,
                    self.width,
                    self.height,
                    self.frame_index,
                    self.frame_ticks_accumulator,
                    frame_bgra,
                ) {
                    Ok(encoded) => encoded,
                    Err(err) => {
                        backend::disable_nvenc_runtime();
                        // We can safely switch codecs only before writing the first sample.
                        if self.frame_index == 0 {
                            eprintln!(
                                "[export] NVENC frame encode failed, falling back to OpenH264: {err}"
                            );
                            let mut fallback = backend::create_compatible_encoder(
                                self.width,
                                self.height,
                                self.fps,
                            )?;
                            let mut yuv = YUVBuffer::new(self.width as usize, self.height as usize);
                            let encoded = Self::encode_openh264_frame(
                                &mut fallback,
                                &mut yuv,
                                self.width,
                                self.height,
                                self.frame_index,
                                frame_bgra,
                            )?;
                            self.backend = RawEncoderBackend::OpenH264 {
                                encoder: fallback,
                                yuv,
                            };
                            encoded
                        } else {
                            return Err(err.into());
                        }
                    }
                }
            }
        };
        if let Some(sps) = encoded.sps {
            self.sps = Some(sps);
        }
        if let Some(pps) = encoded.pps {
            self.pps = Some(pps);
        }
        if encoded.sample_payload.is_empty() {
            return Err("encoded frame contained no MP4 sample payload".into());
        }
        self.ensure_track()?;
        let sample = Mp4Sample {
            start_time: self.frame_ticks_accumulator,
            duration: self.frame_duration_ticks,
            rendering_offset: 0,
            is_sync: encoded.is_sync,
            bytes: encoded.sample_payload.into(),
        };
        self.muxer.write_sample(MP4_TRACK_ID_VIDEO, &sample)?;
        self.frame_index = self.frame_index.saturating_add(1);
        self.frame_ticks_accumulator = self
            .frame_ticks_accumulator
            .saturating_add(self.frame_duration_ticks as u64);
        Ok(())
    }

    fn encode_openh264_frame(
        encoder: &mut Encoder,
        yuv: &mut YUVBuffer,
        width: u32,
        height: u32,
        frame_index: u64,
        frame_bgra: &[u8],
    ) -> Result<EncodedFramePayload, Box<dyn Error>> {
        let bgra = BgraSliceU8::new(frame_bgra, (width as usize, height as usize));
        yuv.read_rgb(bgra);
        if frame_index == 0 {
            encoder.force_intra_frame();
        }
        let encoded = encoder.encode(yuv)?;
        let mut payload = EncodedFramePayload {
            sample_payload: Vec::new(),
            is_sync: false,
            sps: None,
            pps: None,
        };
        for layer_index in 0..encoded.num_layers() {
            let layer = encoded
                .layer(layer_index)
                .ok_or("encoded layer index out of bounds")?;
            for nal_index in 0..layer.nal_count() {
                let nal = layer
                    .nal_unit(nal_index)
                    .ok_or("encoded NAL index out of bounds")?;
                let unit = strip_annex_b_start_code(nal);
                if unit.is_empty() {
                    continue;
                }
                append_h264_unit_to_payload(unit, &mut payload)?;
            }
        }
        Ok(payload)
    }

    #[cfg(windows)]
    fn encode_nvenc_frame_guarded(
        state: &mut backend::NvencEncoderState,
        width: u32,
        height: u32,
        frame_index: u64,
        frame_timestamp: u64,
        frame_bgra: &[u8],
    ) -> Result<EncodedFramePayload, String> {
        match backend::catch_unwind_silent(AssertUnwindSafe(|| {
            Self::encode_nvenc_frame(
                state,
                width,
                height,
                frame_index,
                frame_timestamp,
                frame_bgra,
            )
        })) {
            Ok(result) => result.map_err(|err| err.to_string()),
            Err(payload) => Err(format!(
                "NVENC panicked while encoding frame {frame_index}: {}",
                backend::panic_payload_message(payload)
            )),
        }
    }

    #[cfg(windows)]
    fn encode_nvenc_frame(
        state: &mut backend::NvencEncoderState,
        width: u32,
        height: u32,
        frame_index: u64,
        frame_timestamp: u64,
        frame_bgra: &[u8],
    ) -> Result<EncodedFramePayload, Box<dyn Error>> {
        backend::copy_bgra_into_nvenc_input(&state.input, width, height, frame_bgra)?;
        let pic_type = if frame_index == 0 {
            NVencPicType::IDR
        } else {
            NVencPicType::P
        };
        state
            .encoder
            .encode_picture(
                &state.input,
                &state.bitstream,
                frame_index as usize,
                frame_timestamp,
                NVencBufferFormat::ARGB,
                NVencPicStruct::Frame,
                pic_type,
                None,
            )
            .map_err(|err| format!("NVENC failed to encode frame {frame_index}: {err:?}"))?;
        let bitstream = state.bitstream.try_lock(true).map_err(|err| {
            format!("NVENC failed to lock bitstream for frame {frame_index}: {err:?}")
        })?;
        parse_annex_b_packet(bitstream.as_slice())
    }

    /// Finalize stream and write the MP4 trailer.
    pub fn finish(mut self) -> Result<(), Box<dyn Error>> {
        if self.frame_index == 0 {
            return Err("cannot finish empty video stream; no frames were written".into());
        }
        #[cfg(windows)]
        if let RawEncoderBackend::Nvenc(state) = &self.backend {
            state
                .encoder
                .end_encode()
                .map_err(|err| format!("NVENC failed to flush encoder: {err:?}"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_frames_is_never_zero() {
        let cfg = crate::runtime_config::AnimationConfig {
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
    fn stream_encoder_preference_parses_auto_value() {
        assert_eq!(
            parse_stream_encoder_preference("auto").expect("auto preference"),
            StreamEncoderPreference::Auto
        );
    }

    #[test]
    fn stream_encoder_preference_parses_supported_values() {
        assert_eq!(
            parse_stream_encoder_preference("nvenc").expect("nvenc preference"),
            StreamEncoderPreference::Nvenc
        );
        assert_eq!(
            parse_stream_encoder_preference("openh264").expect("openh264 preference"),
            StreamEncoderPreference::OpenH264
        );
        assert!(parse_stream_encoder_preference("unsupported").is_err());
    }

    #[test]
    fn recommended_bitrate_is_clamped() {
        assert_eq!(recommended_bitrate(64, 64, 1), 2_000_000);
        assert_eq!(recommended_bitrate(3840, 2160, 60), 24_000_000);
    }
}

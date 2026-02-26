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
#[cfg(windows)]
use nvenc::bitstream::BitStream as NvencBitStream;
#[cfg(windows)]
use nvenc::encoder::Encoder as NvencEncoder;
#[cfg(windows)]
use nvenc::input_buffer::InputBuffer as NvencInputBuffer;
#[cfg(windows)]
use nvenc::session::{InitParams as NvencInitParams, NeedsConfig as NvencNeedsConfig, Session};
#[cfg(windows)]
use nvenc::sys::enums::{
    NVencBufferFormat, NVencMemoryHeap, NVencParamsRcMode, NVencPicStruct, NVencPicType,
    NVencTuningInfo,
};
#[cfg(windows)]
use nvenc::sys::guids::{NV_ENC_CODEC_H264_GUID, NV_ENC_PRESET_P4_GUID};
#[cfg(windows)]
use nvenc::sys::result::NVencError;
#[cfg(windows)]
use nvenc::sys::version::{NVENC_MAJOR_VERSION, NVENC_MINOR_VERSION};
#[cfg(windows)]
use windows::core::Interface;
#[cfg(windows)]
use windows::Win32::Foundation::HMODULE;
#[cfg(windows)]
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0,
};
#[cfg(windows)]
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION,
};
#[cfg(windows)]
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter, IDXGIAdapter1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    DXGI_ERROR_NOT_FOUND,
};

const MP4_MOVIE_TIMESCALE: u32 = 90_000;
const MP4_TRACK_ID_VIDEO: u32 = 1;
const H264_NAL_TYPE_IDR: u8 = 5;
const H264_NAL_TYPE_SPS: u8 = 7;
const H264_NAL_TYPE_PPS: u8 = 8;
const STREAM_FRAME_FORMAT_ENV: &str = "COVERGEN_STREAM_FRAME_FORMAT";
const STREAM_ENCODER_ENV: &str = "COVERGEN_STREAM_ENCODER";
#[cfg(windows)]
const NVIDIA_VENDOR_ID: u32 = 0x10DE;

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
    #[cfg(windows)]
    CpuReadbackGpuEncode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamEncoderPreference {
    Auto,
    OpenH264,
    Nvenc,
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

enum RawEncoderBackend {
    OpenH264 {
        encoder: Encoder,
        yuv: YUVBuffer,
    },
    #[cfg(windows)]
    Nvenc(NvencEncoderState),
}

#[cfg(windows)]
struct NvencEncoderState {
    _device: ID3D11Device,
    encoder: NvencEncoder,
    input: NvencInputBuffer,
    bitstream: NvencBitStream,
}

struct EncodedFramePayload {
    sample_payload: Vec<u8>,
    is_sync: bool,
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
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
            RawEncoderBackend::Nvenc(state) => Self::encode_nvenc_frame(
                state,
                self.width,
                self.height,
                self.frame_index,
                self.frame_ticks_accumulator,
                frame_bgra,
            )?,
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
    fn encode_nvenc_frame(
        state: &mut NvencEncoderState,
        width: u32,
        height: u32,
        frame_index: u64,
        frame_timestamp: u64,
        frame_bgra: &[u8],
    ) -> Result<EncodedFramePayload, Box<dyn Error>> {
        copy_bgra_into_nvenc_input(&state.input, width, height, frame_bgra)?;
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

fn create_stream_backend(
    width: u32,
    height: u32,
    fps: u32,
    frame_format: StreamFrameFormat,
) -> Result<(RawEncoderBackend, ExportDataPath), Box<dyn Error>> {
    let preference = preferred_stream_encoder()?;
    #[cfg(windows)]
    if matches!(
        preference,
        StreamEncoderPreference::Auto | StreamEncoderPreference::Nvenc
    ) {
        match create_nvenc_backend(width, height, fps) {
            Ok(state) => {
                return Ok((
                    RawEncoderBackend::Nvenc(state),
                    ExportDataPath::CpuReadbackGpuEncode,
                ));
            }
            Err(err) if matches!(preference, StreamEncoderPreference::Nvenc) => {
                return Err(err);
            }
            Err(err) => {
                eprintln!("[export] NVENC unavailable, falling back to OpenH264: {err}");
            }
        }
    }
    #[cfg(not(windows))]
    if matches!(preference, StreamEncoderPreference::Nvenc) {
        return Err(
            format!("{STREAM_ENCODER_ENV}=nvenc is only supported on Windows builds").into(),
        );
    }
    let encoder = create_compatible_encoder(width, height, fps)?;
    Ok((
        RawEncoderBackend::OpenH264 {
            encoder,
            yuv: YUVBuffer::new(width as usize, height as usize),
        },
        data_path_for_frame_format(frame_format),
    ))
}

#[cfg(windows)]
fn create_nvenc_backend(
    width: u32,
    height: u32,
    fps: u32,
) -> Result<NvencEncoderState, Box<dyn Error>> {
    let version_probe = probe_nvenc_api_versions();
    let device = create_nvenc_device()?;
    let session: Session<NvencNeedsConfig> = Session::open_dx(&device).map_err(|err| {
        if err == NVencError::InvalidVersion {
            format!(
                "failed to create NVENC DX session: {err:?}; {}",
                version_probe.describe_mismatch_hint()
            )
        } else {
            format!(
                "failed to create NVENC DX session: {err:?}; {}",
                version_probe.describe_probe_result()
            )
        }
    })?;
    let (session, mut config) = session
        .get_encode_preset_config_ex(
            NV_ENC_CODEC_H264_GUID,
            NV_ENC_PRESET_P4_GUID,
            NVencTuningInfo::HighQuality,
        )
        .map_err(|err| format!("failed to fetch NVENC H.264 preset config: {err:?}"))?;
    config.preset_cfg.rc_params.rate_control_mode = NVencParamsRcMode::VBR;
    config.preset_cfg.rc_params.average_bit_rate = recommended_bitrate(width, height, fps);
    config.preset_cfg.gop_len = 0xffff_ffff;
    config.preset_cfg.frame_interval_p = 1;
    let init = NvencInitParams {
        encode_guid: NV_ENC_CODEC_H264_GUID,
        preset_guid: NV_ENC_PRESET_P4_GUID,
        resolution: [width, height],
        aspect_ratio: [width, height],
        frame_rate: [fps, 1],
        tuning_info: NVencTuningInfo::HighQuality,
        buffer_format: NVencBufferFormat::ARGB,
        encode_config: &mut config.preset_cfg,
        enable_ptd: true,
        max_encoder_resolution: [width, height],
    };
    let encoder = session
        .init_encoder(init)
        .map_err(|err| format!("failed to initialize NVENC H.264 encoder: {err:?}"))?;
    let input = encoder
        .create_input_buffer(
            width,
            height,
            NVencMemoryHeap::AutoSelect,
            NVencBufferFormat::ARGB,
        )
        .map_err(|err| format!("failed to create NVENC input buffer: {err:?}"))?;
    let bitstream = encoder
        .create_bitstream_buffer()
        .map_err(|err| format!("failed to create NVENC bitstream buffer: {err:?}"))?;
    Ok(NvencEncoderState {
        _device: device,
        encoder,
        input,
        bitstream,
    })
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
struct NvencApiVersionProbe {
    required_major: u16,
    required_minor: u8,
    driver_max: Option<(u16, u8)>,
}

#[cfg(windows)]
impl NvencApiVersionProbe {
    fn describe_probe_result(self) -> String {
        match self.driver_max {
            Some((major, minor)) => format!(
                "nvenc api required {}.{}, driver reports {}.{}",
                self.required_major, self.required_minor, major, minor
            ),
            None => format!(
                "nvenc api required {}.{}, driver max API could not be queried",
                self.required_major, self.required_minor
            ),
        }
    }

    fn describe_mismatch_hint(self) -> String {
        match self.driver_max {
            Some((major, minor)) => format!(
                "nvenc api mismatch (required {}.{}, driver reports {}.{}); update the NVIDIA driver or use {STREAM_ENCODER_ENV}=openh264",
                self.required_major, self.required_minor, major, minor
            ),
            None => format!(
                "nvenc api mismatch (required {}.{}); update the NVIDIA driver or use {STREAM_ENCODER_ENV}=openh264",
                self.required_major, self.required_minor
            ),
        }
    }
}

#[cfg(windows)]
fn probe_nvenc_api_versions() -> NvencApiVersionProbe {
    NvencApiVersionProbe {
        required_major: NVENC_MAJOR_VERSION,
        required_minor: NVENC_MINOR_VERSION,
        driver_max: read_nvenc_driver_max_api_version(),
    }
}

#[cfg(windows)]
fn read_nvenc_driver_max_api_version() -> Option<(u16, u8)> {
    let library = nvenc::nvenc_init().ok()?;
    let raw_version = library.get_max_version().ok()?;
    Some(decode_nvenc_api_version(raw_version))
}

#[cfg(windows)]
fn decode_nvenc_api_version(version: u32) -> (u16, u8) {
    // `nvenc` currently normalizes this into an API-style packed integer.
    let major = (version & 0xFFFF) as u16;
    let minor = ((version >> 24) & 0xFF) as u8;
    (major, minor)
}

#[cfg(windows)]
fn create_nvenc_device() -> Result<ID3D11Device, Box<dyn Error>> {
    let preferred_adapter = find_nvenc_adapter()?;
    if let Some(adapter) = preferred_adapter.as_ref() {
        if let Ok(base_adapter) = adapter.cast::<IDXGIAdapter>() {
            if let Ok(device) = create_d3d11_device(Some(&base_adapter), D3D_DRIVER_TYPE_UNKNOWN) {
                return Ok(device);
            }
        }
    }

    let device = create_d3d11_device(None, D3D_DRIVER_TYPE_HARDWARE).map_err(|err| {
        if preferred_adapter.is_none() {
            format!(
                "failed to create D3D11 device for NVENC: {err}; no NVIDIA DXGI adapter was found"
            )
        } else {
            format!(
                "failed to create D3D11 device for NVENC: {err}; NVIDIA adapter selection failed"
            )
        }
    })?;
    Ok(device)
}

#[cfg(windows)]
fn create_d3d11_device(
    adapter: Option<&IDXGIAdapter>,
    driver_type: windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE,
) -> Result<ID3D11Device, Box<dyn Error>> {
    let mut device = None;
    unsafe {
        D3D11CreateDevice(
            adapter,
            driver_type,
            HMODULE(std::ptr::null_mut()),
            D3D11_CREATE_DEVICE_FLAG(0),
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
    }
    .map_err(|err| format!("D3D11CreateDevice failed: {err}"))?;
    device.ok_or("D3D11CreateDevice returned no device".into())
}

#[cfg(windows)]
fn find_nvenc_adapter() -> Result<Option<IDXGIAdapter1>, Box<dyn Error>> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }?;
    let mut index = 0u32;
    loop {
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(err) if err.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(err) => return Err(format!("failed to enumerate DXGI adapters: {err}").into()),
        };
        index = index.saturating_add(1);
        let desc = unsafe { adapter.GetDesc1() }
            .map_err(|err| format!("failed to query DXGI adapter info: {err}"))?;
        let is_software = (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0;
        if desc.VendorId == NVIDIA_VENDOR_ID && !is_software {
            return Ok(Some(adapter));
        }
    }
    Ok(None)
}

#[cfg(windows)]
fn copy_bgra_into_nvenc_input(
    input: &NvencInputBuffer,
    width: u32,
    height: u32,
    frame_bgra: &[u8],
) -> Result<(), Box<dyn Error>> {
    let row_bytes = width as usize * StreamFrameFormat::Bgra8.bytes_per_pixel();
    let lock = input
        .lock()
        .map_err(|err| format!("failed to lock NVENC input buffer: {err:?}"))?;
    let pitch = lock.pitch() as usize;
    if pitch < row_bytes {
        return Err(format!(
            "NVENC input pitch {} is smaller than row byte width {}",
            pitch, row_bytes
        )
        .into());
    }
    unsafe {
        let dst = lock.data_ptr();
        for row in 0..height as usize {
            let src_start = row * row_bytes;
            let src_end = src_start + row_bytes;
            std::ptr::copy_nonoverlapping(
                frame_bgra[src_start..src_end].as_ptr(),
                dst.add(row * pitch),
                row_bytes,
            );
        }
    }
    Ok(())
}

fn append_h264_unit_to_payload(
    unit: &[u8],
    payload: &mut EncodedFramePayload,
) -> Result<(), Box<dyn Error>> {
    if unit.is_empty() {
        return Ok(());
    }
    let nal_type = unit[0] & 0x1F;
    if nal_type == H264_NAL_TYPE_SPS {
        payload.sps = Some(unit.to_vec());
        return Ok(());
    }
    if nal_type == H264_NAL_TYPE_PPS {
        payload.pps = Some(unit.to_vec());
        return Ok(());
    }
    if nal_type == H264_NAL_TYPE_IDR {
        payload.is_sync = true;
    }
    append_length_prefixed_nal(&mut payload.sample_payload, unit)
}

#[cfg(windows)]
fn parse_annex_b_packet(packet: &[u8]) -> Result<EncodedFramePayload, Box<dyn Error>> {
    let mut payload = EncodedFramePayload {
        sample_payload: Vec::new(),
        is_sync: false,
        sps: None,
        pps: None,
    };
    for unit in annex_b_units(packet) {
        append_h264_unit_to_payload(unit, &mut payload)?;
    }
    Ok(payload)
}

#[cfg(windows)]
fn annex_b_units(packet: &[u8]) -> Vec<&[u8]> {
    let mut units = Vec::new();
    let mut cursor = 0usize;
    while let Some((start, prefix_len)) = find_annex_b_start_code(packet, cursor) {
        let unit_start = start + prefix_len;
        let next = find_annex_b_start_code(packet, unit_start)
            .map(|(index, _)| index)
            .unwrap_or(packet.len());
        if unit_start < next {
            units.push(&packet[unit_start..next]);
        }
        cursor = next;
    }
    units
}

#[cfg(windows)]
fn find_annex_b_start_code(packet: &[u8], from: usize) -> Option<(usize, usize)> {
    if packet.len() < 3 || from >= packet.len() {
        return None;
    }
    let mut idx = from;
    while idx + 3 <= packet.len() {
        if idx + 4 <= packet.len()
            && packet[idx] == 0
            && packet[idx + 1] == 0
            && packet[idx + 2] == 0
            && packet[idx + 3] == 1
        {
            return Some((idx, 4));
        }
        if packet[idx] == 0 && packet[idx + 1] == 0 && packet[idx + 2] == 1 {
            return Some((idx, 3));
        }
        idx += 1;
    }
    None
}

fn create_compatible_encoder(width: u32, height: u32, fps: u32) -> Result<Encoder, Box<dyn Error>> {
    let usage_fallback = [
        UsageType::ScreenContentRealTime,
        UsageType::CameraVideoRealTime,
        UsageType::CameraVideoNonRealTime,
        UsageType::ScreenContentNonRealTime,
    ];
    let mut last_error = String::new();
    for usage in usage_fallback {
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(recommended_bitrate(width, height, fps)))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .rate_control_mode(RateControlMode::Quality)
            .usage_type(usage)
            .skip_frames(false);
        match Encoder::with_api_config(openh264::OpenH264API::from_source(), config) {
            Ok(encoder) => {
                return Ok(encoder);
            }
            Err(err) => {
                last_error = err.to_string();
            }
        }
    }
    Err(format!("OpenH264 initialization failed for all usage profiles: {last_error}").into())
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

fn preferred_stream_encoder() -> Result<StreamEncoderPreference, Box<dyn Error>> {
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

fn parse_stream_encoder_preference(raw: &str) -> Result<StreamEncoderPreference, String> {
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

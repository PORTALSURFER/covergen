//! Encoder backend creation and platform-specific hardware setup.

use std::error::Error;
#[cfg(windows)]
use std::panic::{self, AssertUnwindSafe};
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::Mutex;

use openh264::encoder::{BitRate, Encoder, EncoderConfig, FrameRate, RateControlMode, UsageType};
use openh264::formats::YUVBuffer;

use super::config::{
    data_path_for_frame_format, preferred_stream_encoder, recommended_bitrate, ExportDataPath,
    StreamEncoderPreference, StreamFrameFormat, STREAM_ENCODER_ENV,
};
use super::RawEncoderBackend;
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

#[cfg(windows)]
const NVIDIA_VENDOR_ID: u32 = 0x10DE;
#[cfg(windows)]
static NVENC_RUNTIME_DISABLED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static NVENC_PANIC_HOOK_LOCK: Mutex<()> = Mutex::new(());

#[cfg(windows)]
pub(super) fn disable_nvenc_runtime() {
    NVENC_RUNTIME_DISABLED.store(true, Ordering::Relaxed);
}

pub(super) fn create_stream_backend(
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
        if matches!(preference, StreamEncoderPreference::Auto)
            && NVENC_RUNTIME_DISABLED.load(Ordering::Relaxed)
        {
            let encoder = create_compatible_encoder(width, height, fps)?;
            return Ok((
                RawEncoderBackend::OpenH264 {
                    encoder,
                    yuv: YUVBuffer::new(width as usize, height as usize),
                },
                data_path_for_frame_format(frame_format),
            ));
        }
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
                disable_nvenc_runtime();
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

pub(super) fn create_compatible_encoder(
    width: u32,
    height: u32,
    fps: u32,
) -> Result<Encoder, Box<dyn Error>> {
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

#[cfg(windows)]
pub(super) struct NvencEncoderState {
    pub(super) _device: ID3D11Device,
    pub(super) encoder: NvencEncoder,
    pub(super) input: NvencInputBuffer,
    pub(super) bitstream: NvencBitStream,
}

#[cfg(windows)]
pub(super) fn create_nvenc_backend(
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
pub(super) fn copy_bgra_into_nvenc_input(
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

#[cfg(windows)]
pub(super) fn catch_unwind_silent<F, R>(func: F) -> std::thread::Result<R>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    let _hook_guard = match NVENC_PANIC_HOOK_LOCK.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(func);
    panic::set_hook(previous_hook);
    result
}

#[cfg(windows)]
pub(super) fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(msg) = payload.downcast_ref::<&str>() {
        return (*msg).to_string();
    }
    if let Some(msg) = payload.downcast_ref::<String>() {
        return msg.clone();
    }
    "unknown panic payload".to_string()
}

//! Live timeline-audio preview built from user-supplied WAV files.
//!
//! On Windows, this controller decodes WAV data and keeps audio playback
//! synchronized to the timeline clock for play/pause/scrub workflows.
//! On other platforms it degrades to a no-op preview controller.

use super::state::ExportMenuState;

#[cfg(windows)]
use std::num::{NonZeroU16, NonZeroU32};
#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use hound::{SampleFormat, WavReader};
#[cfg(windows)]
use rodio::{buffer::SamplesBuffer, DeviceSinkBuilder, MixerDeviceSink, Player, Source};

#[cfg(windows)]
const RESYNC_THRESHOLD_MIN_MS: u64 = 12;
#[cfg(windows)]
const RESYNC_THRESHOLD_MAX_MS: u64 = 40;

#[cfg(windows)]
#[derive(Clone, Debug)]
struct LoadedWavClip {
    channels: NonZeroU16,
    sample_rate: NonZeroU32,
    samples: Vec<f32>,
    duration: Duration,
}

/// Timeline audio preview controller.
#[derive(Default)]
pub(crate) struct TimelineAudioPreview {
    #[cfg(windows)]
    sink: Option<MixerDeviceSink>,
    #[cfg(windows)]
    player: Option<Player>,
    #[cfg(windows)]
    clip: Option<LoadedWavClip>,
    #[cfg(windows)]
    clip_path: Option<PathBuf>,
    #[cfg(windows)]
    last_frame_index: Option<u32>,
}

impl TimelineAudioPreview {
    /// Synchronize WAV playback to timeline state for one GUI frame.
    pub(crate) fn sync(
        &mut self,
        export_menu: &ExportMenuState,
        paused: bool,
        frame_index: u32,
        timeline_total_frames: u32,
        timeline_fps: u32,
    ) {
        #[cfg(not(windows))]
        {
            let _ = (
                export_menu,
                paused,
                frame_index,
                timeline_total_frames,
                timeline_fps,
            );
        }
        #[cfg(windows)]
        {
            let requested_path = export_menu.audio_wav_path();
            if requested_path != self.clip_path {
                self.reload_clip(requested_path.as_deref());
            }
            let Some(clip_duration) = self.clip.as_ref().map(|clip| clip.duration) else {
                self.stop();
                self.last_frame_index = Some(frame_index);
                return;
            };
            let volume = export_menu.parsed_audio_volume();
            let target = timeline_position(
                frame_index,
                timeline_total_frames,
                timeline_fps,
                clip_duration,
            );

            if paused {
                if let Some(player) = self.player.as_ref() {
                    player.pause();
                    player.set_volume(volume);
                    if self.last_frame_index != Some(frame_index) {
                        let _ = player.try_seek(target);
                    }
                }
                self.last_frame_index = Some(frame_index);
                return;
            }

            if self.player.is_none() {
                if !self.start_player(target, volume) {
                    self.last_frame_index = Some(frame_index);
                    return;
                }
            }
            let Some(player) = self.player.as_ref() else {
                self.last_frame_index = Some(frame_index);
                return;
            };

            player.play();
            player.set_volume(volume);
            let current = wrapped_duration(player.get_pos(), clip_duration);
            let drift = duration_diff(current, target, clip_duration);
            let frame_secs = 1.0 / timeline_fps.max(1) as f64;
            let resync_threshold = Duration::from_secs_f64(
                frame_secs
                    .clamp(
                        RESYNC_THRESHOLD_MIN_MS as f64 / 1000.0,
                        RESYNC_THRESHOLD_MAX_MS as f64 / 1000.0,
                    )
                    .max(f64::EPSILON),
            );
            let loop_wrapped = self
                .last_frame_index
                .map(|prev| frame_index < prev)
                .unwrap_or(false);
            if drift > resync_threshold || loop_wrapped {
                let _ = player.try_seek(target);
            }
            self.last_frame_index = Some(frame_index);
        }
    }

    /// Drop active playback objects.
    pub(crate) fn stop(&mut self) {
        #[cfg(windows)]
        {
            if let Some(player) = self.player.take() {
                player.stop();
            }
            self.sink = None;
        }
    }

    #[cfg(windows)]
    fn reload_clip(&mut self, path: Option<&Path>) {
        self.stop();
        self.clip = None;
        self.clip_path = path.map(Path::to_path_buf);
        self.last_frame_index = None;
        let Some(path) = path else {
            return;
        };
        match load_wav_clip(path) {
            Ok(clip) => self.clip = Some(clip),
            Err(err) => {
                eprintln!("[audio] failed to load WAV {}: {err}", path.display());
                self.clip_path = None;
            }
        }
    }

    #[cfg(windows)]
    fn start_player(&mut self, target: Duration, volume: f32) -> bool {
        let Some(clip) = self.clip.as_ref() else {
            return false;
        };
        if self.sink.is_none() {
            match DeviceSinkBuilder::open_default_sink() {
                Ok(sink) => self.sink = Some(sink),
                Err(err) => {
                    eprintln!("[audio] failed to open default output sink: {err}");
                    return false;
                }
            }
        }
        let Some(sink) = self.sink.as_ref() else {
            return false;
        };
        let player = Player::connect_new(sink.mixer());
        let source = SamplesBuffer::new(clip.channels, clip.sample_rate, clip.samples.clone())
            .repeat_infinite();
        player.append(source);
        player.set_volume(volume);
        let _ = player.try_seek(target);
        self.player = Some(player);
        true
    }
}

#[cfg(windows)]
fn load_wav_clip(path: &Path) -> Result<LoadedWavClip, String> {
    let mut reader = WavReader::open(path).map_err(|err| err.to_string())?;
    let spec = reader.spec();
    let channels =
        NonZeroU16::new(spec.channels).ok_or_else(|| "wav channels must be >= 1".to_string())?;
    let sample_rate = NonZeroU32::new(spec.sample_rate)
        .ok_or_else(|| "wav sample rate must be >= 1".to_string())?;
    let samples = match spec.sample_format {
        SampleFormat::Float => decode_float_samples(&mut reader)?,
        SampleFormat::Int => decode_int_samples(&mut reader, spec.bits_per_sample)?,
    };
    if samples.is_empty() {
        return Err("wav contains no samples".to_string());
    }
    let duration_secs = samples.len() as f64 / channels.get() as f64 / sample_rate.get() as f64;
    Ok(LoadedWavClip {
        channels,
        sample_rate,
        samples,
        duration: Duration::from_secs_f64(duration_secs.max(0.0)),
    })
}

#[cfg(windows)]
fn decode_float_samples(
    reader: &mut WavReader<std::io::BufReader<std::fs::File>>,
) -> Result<Vec<f32>, String> {
    reader
        .samples::<f32>()
        .map(|sample| sample.map_err(|err| err.to_string()))
        .collect()
}

#[cfg(windows)]
fn decode_int_samples(
    reader: &mut WavReader<std::io::BufReader<std::fs::File>>,
    bits_per_sample: u16,
) -> Result<Vec<f32>, String> {
    match bits_per_sample {
        0 => Err("invalid wav bits-per-sample: 0".to_string()),
        1..=8 => reader
            .samples::<i8>()
            .map(|sample| {
                sample
                    .map(|value| (value as f32 / i8::MAX as f32).clamp(-1.0, 1.0))
                    .map_err(|err| err.to_string())
            })
            .collect(),
        9..=16 => reader
            .samples::<i16>()
            .map(|sample| {
                sample
                    .map(|value| (value as f32 / i16::MAX as f32).clamp(-1.0, 1.0))
                    .map_err(|err| err.to_string())
            })
            .collect(),
        _ => {
            let shift = bits_per_sample.saturating_sub(1) as usize;
            let denom = ((1_i64 << shift) - 1) as f32;
            reader
                .samples::<i32>()
                .map(|sample| {
                    sample
                        .map(|value| (value as f32 / denom).clamp(-1.0, 1.0))
                        .map_err(|err| err.to_string())
                })
                .collect()
        }
    }
}

#[cfg(windows)]
fn timeline_position(frame_index: u32, frame_total: u32, fps: u32, duration: Duration) -> Duration {
    if duration.is_zero() {
        return Duration::ZERO;
    }
    if frame_total > 1 {
        let normalized = (frame_index % frame_total) as f64 / frame_total as f64;
        return Duration::from_secs_f64(duration.as_secs_f64() * normalized);
    }
    if fps == 0 {
        return Duration::ZERO;
    }
    let seconds = frame_index as f64 / fps.max(1) as f64;
    let wrapped = seconds % duration.as_secs_f64().max(f64::EPSILON);
    Duration::from_secs_f64(wrapped)
}

#[cfg(windows)]
fn wrapped_duration(duration: Duration, period: Duration) -> Duration {
    if period.is_zero() {
        return Duration::ZERO;
    }
    Duration::from_secs_f64(duration.as_secs_f64() % period.as_secs_f64().max(f64::EPSILON))
}

#[cfg(windows)]
fn duration_diff(a: Duration, b: Duration, period: Duration) -> Duration {
    if period.is_zero() {
        return Duration::ZERO;
    }
    let a_secs = wrapped_duration(a, period).as_secs_f64();
    let b_secs = wrapped_duration(b, period).as_secs_f64();
    let direct = (a_secs - b_secs).abs();
    let wrapped = (period.as_secs_f64() - direct).abs();
    Duration::from_secs_f64(direct.min(wrapped))
}

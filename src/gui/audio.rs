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
#[cfg(any(windows, test))]
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

/// Minimal player controls required by timeline sync logic.
#[cfg(windows)]
trait TimelinePlayer {
    /// Resume playback.
    fn play(&self);
    /// Pause playback.
    fn pause(&self);
    /// Set output gain.
    fn set_volume(&self, volume: f32);
    /// Return current playback position.
    fn get_pos(&self) -> Duration;
    /// Seek to one playback position.
    fn try_seek(&self, target: Duration) -> bool;
    /// Stop playback and release player-side resources.
    fn stop(&self);
}

/// Backend that creates and owns platform audio output resources.
#[cfg(windows)]
trait TimelineAudioBackend {
    /// Create one playback session for `clip` at `target` with `volume`.
    fn start_player(
        &mut self,
        clip: &LoadedWavClip,
        target: Duration,
        volume: f32,
    ) -> Result<Box<dyn TimelinePlayer>, String>;

    /// Drop backend output resources.
    fn reset_output(&mut self);
}

/// Default rodio-backed output backend used on Windows.
#[cfg(windows)]
#[derive(Default)]
struct RodioTimelineAudioBackend {
    sink: Option<MixerDeviceSink>,
}

/// `TimelinePlayer` adapter around `rodio::Player`.
#[cfg(windows)]
struct RodioTimelinePlayer {
    player: Player,
}

#[cfg(windows)]
impl TimelinePlayer for RodioTimelinePlayer {
    fn play(&self) {
        self.player.play();
    }

    fn pause(&self) {
        self.player.pause();
    }

    fn set_volume(&self, volume: f32) {
        self.player.set_volume(volume);
    }

    fn get_pos(&self) -> Duration {
        self.player.get_pos()
    }

    fn try_seek(&self, target: Duration) -> bool {
        self.player.try_seek(target).is_ok()
    }

    fn stop(&self) {
        self.player.stop();
    }
}

#[cfg(windows)]
impl TimelineAudioBackend for RodioTimelineAudioBackend {
    fn start_player(
        &mut self,
        clip: &LoadedWavClip,
        target: Duration,
        volume: f32,
    ) -> Result<Box<dyn TimelinePlayer>, String> {
        if self.sink.is_none() {
            let sink = DeviceSinkBuilder::open_default_sink()
                .map_err(|err| format!("failed to open default output sink: {err}"))?;
            self.sink = Some(sink);
        }
        let Some(sink) = self.sink.as_ref() else {
            return Err("audio output sink was not initialized".to_string());
        };
        let player = Player::connect_new(sink.mixer());
        let source = SamplesBuffer::new(clip.channels, clip.sample_rate, clip.samples.clone())
            .repeat_infinite();
        player.append(source);
        player.set_volume(volume);
        let _ = player.try_seek(target);
        Ok(Box::new(RodioTimelinePlayer { player }))
    }

    fn reset_output(&mut self) {
        self.sink = None;
    }
}

/// Timeline audio preview controller.
#[cfg_attr(not(windows), derive(Default))]
pub(crate) struct TimelineAudioPreview {
    #[cfg(windows)]
    backend: Box<dyn TimelineAudioBackend>,
    #[cfg(windows)]
    player: Option<Box<dyn TimelinePlayer>>,
    #[cfg(windows)]
    clip: Option<LoadedWavClip>,
    #[cfg(windows)]
    clip_path: Option<PathBuf>,
    #[cfg(windows)]
    last_frame_index: Option<u32>,
}

#[cfg(windows)]
impl Default for TimelineAudioPreview {
    fn default() -> Self {
        #[cfg(windows)]
        {
            return Self {
                backend: Box::new(RodioTimelineAudioBackend::default()),
                player: None,
                clip: None,
                clip_path: None,
                last_frame_index: None,
            };
        }
        #[cfg(not(windows))]
        {
            Self {}
        }
    }
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
            self.sync_windows(
                export_menu,
                paused,
                frame_index,
                timeline_total_frames,
                timeline_fps,
            );
        }
    }

    /// Drop active playback objects.
    pub(crate) fn stop(&mut self) {
        #[cfg(windows)]
        {
            if let Some(player) = self.player.take() {
                player.stop();
            }
            self.backend.reset_output();
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
        match self.backend.start_player(clip, target, volume) {
            Ok(player) => {
                self.player = Some(player);
                true
            }
            Err(err) => {
                eprintln!("[audio] {err}");
                false
            }
        }
    }

    #[cfg(windows)]
    fn sync_windows(
        &mut self,
        export_menu: &ExportMenuState,
        paused: bool,
        frame_index: u32,
        timeline_total_frames: u32,
        timeline_fps: u32,
    ) {
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
            self.sync_paused(frame_index, target, volume);
            return;
        }
        self.sync_playing(frame_index, target, volume, clip_duration, timeline_fps);
    }

    #[cfg(windows)]
    fn sync_paused(&mut self, frame_index: u32, target: Duration, volume: f32) {
        if let Some(player) = self.player.as_ref() {
            player.pause();
            player.set_volume(volume);
            if self.last_frame_index != Some(frame_index) {
                let _ = player.try_seek(target);
            }
        }
        self.last_frame_index = Some(frame_index);
    }

    #[cfg(windows)]
    fn sync_playing(
        &mut self,
        frame_index: u32,
        target: Duration,
        volume: f32,
        clip_duration: Duration,
        timeline_fps: u32,
    ) {
        if self.player.is_none() && !self.start_player(target, volume) {
            self.last_frame_index = Some(frame_index);
            return;
        }
        let Some(player) = self.player.as_ref() else {
            self.last_frame_index = Some(frame_index);
            return;
        };

        player.play();
        player.set_volume(volume);
        let current = wrapped_duration(player.get_pos(), clip_duration);
        let drift = duration_diff(current, target, clip_duration);
        let loop_wrapped = self
            .last_frame_index
            .map(|prev| frame_index < prev)
            .unwrap_or(false);
        if drift > resync_threshold(timeline_fps) || loop_wrapped {
            let _ = player.try_seek(target);
        }
        self.last_frame_index = Some(frame_index);
    }
}

#[cfg(all(test, windows))]
impl TimelineAudioPreview {
    /// Build one preview controller with an injected fake backend.
    fn with_backend_for_tests(backend: Box<dyn TimelineAudioBackend>) -> Self {
        Self {
            backend,
            player: None,
            clip: None,
            clip_path: None,
            last_frame_index: None,
        }
    }

    /// Install one synthetic clip directly for deterministic sync tests.
    fn set_clip_for_tests(&mut self, duration: Duration) {
        self.clip = Some(LoadedWavClip {
            channels: NonZeroU16::new(1).expect("non-zero channels"),
            sample_rate: NonZeroU32::new(48_000).expect("non-zero sample rate"),
            samples: vec![0.0, 0.0],
            duration,
        });
        // Keep `None` so default export-menu path does not trigger reloads.
        self.clip_path = None;
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

#[cfg(any(windows, test))]
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

#[cfg(any(windows, test))]
fn wrapped_duration(duration: Duration, period: Duration) -> Duration {
    if period.is_zero() {
        return Duration::ZERO;
    }
    Duration::from_secs_f64(duration.as_secs_f64() % period.as_secs_f64().max(f64::EPSILON))
}

#[cfg(any(windows, test))]
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

#[cfg(windows)]
fn resync_threshold(timeline_fps: u32) -> Duration {
    let frame_secs = 1.0 / timeline_fps.max(1) as f64;
    Duration::from_secs_f64(
        frame_secs
            .clamp(
                RESYNC_THRESHOLD_MIN_MS as f64 / 1000.0,
                RESYNC_THRESHOLD_MAX_MS as f64 / 1000.0,
            )
            .max(f64::EPSILON),
    )
}

#[cfg(test)]
mod tests {
    use super::{duration_diff, timeline_position, wrapped_duration};
    use std::time::Duration;

    #[test]
    fn timeline_position_uses_frame_total_loop_domain() {
        let period = Duration::from_secs_f64(8.0);
        let pos = timeline_position(75, 100, 60, period);
        assert!((pos.as_secs_f64() - 6.0).abs() < 1e-6);
    }

    #[test]
    fn timeline_position_falls_back_to_fps_domain_without_frame_total() {
        let period = Duration::from_secs_f64(2.0);
        let pos = timeline_position(150, 0, 60, period);
        assert!((pos.as_secs_f64() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn wrapped_duration_wraps_by_period() {
        let wrapped = wrapped_duration(Duration::from_secs_f64(5.75), Duration::from_secs_f64(2.0));
        assert!((wrapped.as_secs_f64() - 1.75).abs() < 1e-6);
    }

    #[test]
    fn duration_diff_uses_shortest_wrapped_distance() {
        let period = Duration::from_secs_f64(4.0);
        let diff = duration_diff(
            Duration::from_secs_f64(3.9),
            Duration::from_secs_f64(0.1),
            period,
        );
        assert!((diff.as_secs_f64() - 0.2).abs() < 1e-6);
    }

    #[cfg(windows)]
    mod windows_sync {
        use super::super::{TimelineAudioBackend, TimelineAudioPreview, TimelinePlayer};
        use crate::gui::state::ExportMenuState;
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        #[derive(Debug, Default)]
        struct FakeAudioState {
            start_calls: usize,
            play_calls: usize,
            pause_calls: usize,
            stop_calls: usize,
            volumes: Vec<f32>,
            seeks: Vec<Duration>,
            current_pos: Duration,
        }

        #[derive(Clone, Debug)]
        struct FakeTimelineAudioBackend {
            state: Arc<Mutex<FakeAudioState>>,
        }

        #[derive(Debug)]
        struct FakeTimelinePlayer {
            state: Arc<Mutex<FakeAudioState>>,
        }

        impl TimelinePlayer for FakeTimelinePlayer {
            fn play(&self) {
                let mut state = self.state.lock().expect("lock fake state");
                state.play_calls = state.play_calls.saturating_add(1);
            }

            fn pause(&self) {
                let mut state = self.state.lock().expect("lock fake state");
                state.pause_calls = state.pause_calls.saturating_add(1);
            }

            fn set_volume(&self, volume: f32) {
                let mut state = self.state.lock().expect("lock fake state");
                state.volumes.push(volume);
            }

            fn get_pos(&self) -> Duration {
                self.state.lock().expect("lock fake state").current_pos
            }

            fn try_seek(&self, target: Duration) -> bool {
                let mut state = self.state.lock().expect("lock fake state");
                state.seeks.push(target);
                state.current_pos = target;
                true
            }

            fn stop(&self) {
                let mut state = self.state.lock().expect("lock fake state");
                state.stop_calls = state.stop_calls.saturating_add(1);
            }
        }

        impl TimelineAudioBackend for FakeTimelineAudioBackend {
            fn start_player(
                &mut self,
                _clip: &super::super::LoadedWavClip,
                target: Duration,
                volume: f32,
            ) -> Result<Box<dyn TimelinePlayer>, String> {
                {
                    let mut state = self.state.lock().expect("lock fake state");
                    state.start_calls = state.start_calls.saturating_add(1);
                    state.volumes.push(volume);
                    state.seeks.push(target);
                    state.current_pos = target;
                }
                Ok(Box::new(FakeTimelinePlayer {
                    state: Arc::clone(&self.state),
                }))
            }

            fn reset_output(&mut self) {}
        }

        fn build_preview_with_fake_backend() -> (
            TimelineAudioPreview,
            Arc<Mutex<FakeAudioState>>,
            ExportMenuState,
        ) {
            let state = Arc::new(Mutex::new(FakeAudioState::default()));
            let backend = FakeTimelineAudioBackend {
                state: Arc::clone(&state),
            };
            let mut preview = TimelineAudioPreview::with_backend_for_tests(Box::new(backend));
            preview.set_clip_for_tests(Duration::from_secs_f64(10.0));
            let mut menu = ExportMenuState::closed();
            menu.audio_volume = "0.75".to_string();
            (preview, state, menu)
        }

        #[test]
        fn paused_sync_seeks_only_when_frame_changes() {
            let (mut preview, state, menu) = build_preview_with_fake_backend();
            preview.sync(&menu, false, 0, 100, 60);
            {
                let snapshot = state.lock().expect("lock fake state");
                assert_eq!(snapshot.start_calls, 1);
                assert_eq!(snapshot.seeks.len(), 1);
            }
            {
                let mut edit = state.lock().expect("lock fake state");
                edit.current_pos = Duration::from_secs_f64(2.0);
            }
            preview.sync(&menu, true, 10, 100, 60);
            {
                let snapshot = state.lock().expect("lock fake state");
                assert!(snapshot.pause_calls >= 1);
                assert_eq!(snapshot.seeks.len(), 2);
                let last_seek = snapshot.seeks.last().copied().expect("seek exists");
                assert!((last_seek.as_secs_f64() - 1.0).abs() < 1e-6);
            }
            preview.sync(&menu, true, 10, 100, 60);
            {
                let snapshot = state.lock().expect("lock fake state");
                assert_eq!(snapshot.seeks.len(), 2);
            }
        }

        #[test]
        fn playing_sync_resyncs_when_drift_exceeds_threshold() {
            let (mut preview, state, menu) = build_preview_with_fake_backend();
            preview.sync(&menu, false, 0, 100, 60);
            {
                let mut edit = state.lock().expect("lock fake state");
                edit.current_pos = Duration::from_secs_f64(0.25);
            }
            preview.sync(&menu, false, 50, 100, 60);
            let snapshot = state.lock().expect("lock fake state");
            assert_eq!(snapshot.seeks.len(), 2);
            let last_seek = snapshot.seeks.last().copied().expect("seek exists");
            assert!((last_seek.as_secs_f64() - 5.0).abs() < 1e-6);
        }

        #[test]
        fn playing_sync_resyncs_on_loop_wrap_even_with_low_drift() {
            let (mut preview, state, menu) = build_preview_with_fake_backend();
            preview.sync(&menu, false, 90, 100, 60);
            {
                let mut edit = state.lock().expect("lock fake state");
                edit.current_pos = Duration::from_secs_f64(0.5005);
            }
            preview.sync(&menu, false, 5, 100, 60);
            let snapshot = state.lock().expect("lock fake state");
            assert_eq!(snapshot.seeks.len(), 2);
            let last_seek = snapshot.seeks.last().copied().expect("seek exists");
            assert!((last_seek.as_secs_f64() - 0.5).abs() < 1e-6);
        }
    }
}

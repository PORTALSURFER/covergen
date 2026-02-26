//! Main menu and export submenu popup state models.

use std::path::{Path, PathBuf};

use hound::WavReader;

use crate::gui::geometry::Rect;
use crate::gui::timeline::{total_frames_from_music, TIMELINE_DEFAULT_TOTAL_FRAMES};

/// Main-menu popup geometry constants.
pub(crate) const MAIN_MENU_WIDTH: i32 = 180;
const MAIN_MENU_ITEM_HEIGHT: i32 = 24;
const MAIN_MENU_INNER_PADDING: i32 = 6;
const MAIN_MENU_TITLE_HEIGHT: i32 = 24;
const MAIN_MENU_BOTTOM_PADDING: i32 = 8;

/// Export-submenu popup geometry constants.
pub(crate) const EXPORT_MENU_WIDTH: i32 = 420;
const EXPORT_MENU_ITEM_HEIGHT: i32 = 24;
const EXPORT_MENU_INNER_PADDING: i32 = 6;
const EXPORT_MENU_TITLE_HEIGHT: i32 = 24;
const EXPORT_MENU_BOTTOM_PADDING: i32 = 8;
const EXPORT_MENU_CLOSE_SIZE: i32 = 14;
const EXPORT_MENU_STATUS_HEIGHT: i32 = 20;
const EXPORT_MENU_PREVIEW_WIDTH: i32 = 180;
const EXPORT_MENU_PREVIEW_HEIGHT: i32 = 101;
const EXPORT_MENU_PREVIEW_GAP: i32 = 8;
const EXPORT_DEFAULT_BPM: &str = "120";
const EXPORT_DEFAULT_BARS: &str = "15";
const EXPORT_DEFAULT_BEATS_PER_BAR: &str = "4";
const EXPORT_DEFAULT_AUDIO_VOLUME: &str = "1.0";

/// Selectable main-menu rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainMenuItem {
    New,
    Save,
    Load,
    Export,
    Exit,
}

impl MainMenuItem {
    /// Return display label for one main-menu row.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::New => "New",
            Self::Save => "Save",
            Self::Load => "Load",
            Self::Export => "Export",
            Self::Exit => "Exit",
        }
    }
}

const MAIN_MENU_ITEMS: [MainMenuItem; 5] = [
    MainMenuItem::New,
    MainMenuItem::Save,
    MainMenuItem::Load,
    MainMenuItem::Export,
    MainMenuItem::Exit,
];

/// Selectable export-submenu rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportMenuItem {
    Directory,
    FileName,
    AudioWav,
    AudioVolume,
    Bpm,
    Bars,
    BeatsPerBar,
    Codec,
    StartStop,
    Preview,
}

const EXPORT_MENU_ITEMS: [ExportMenuItem; 10] = [
    ExportMenuItem::Directory,
    ExportMenuItem::FileName,
    ExportMenuItem::AudioWav,
    ExportMenuItem::AudioVolume,
    ExportMenuItem::Bpm,
    ExportMenuItem::Bars,
    ExportMenuItem::BeatsPerBar,
    ExportMenuItem::Codec,
    ExportMenuItem::StartStop,
    ExportMenuItem::Preview,
];

/// Backtick-toggleable hover main-menu state.
#[derive(Clone, Debug)]
pub(crate) struct MainMenuState {
    pub(crate) open: bool,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) selected: usize,
}

impl MainMenuState {
    /// Return closed main-menu state.
    pub(crate) const fn closed() -> Self {
        Self {
            open: false,
            x: 0,
            y: 0,
            selected: 0,
        }
    }

    /// Return opened menu state clamped to editor bounds.
    pub(crate) fn open_at(x: i32, y: i32, panel_width: usize, panel_height: usize) -> Self {
        let max_x = (panel_width as i32 - MAIN_MENU_WIDTH - 8).max(8);
        let max_y = (panel_height as i32 - main_menu_height() - 8).max(8);
        Self {
            open: true,
            x: x.clamp(8, max_x),
            y: y.clamp(8, max_y),
            selected: 0,
        }
    }

    /// Return menu rectangle in panel coordinates.
    pub(crate) fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, MAIN_MENU_WIDTH, main_menu_height())
    }

    /// Return selected row item.
    pub(crate) fn selected_item(&self) -> MainMenuItem {
        MAIN_MENU_ITEMS[self.selected.min(MAIN_MENU_ITEMS.len() - 1)]
    }

    /// Return row item at cursor coordinates.
    pub(crate) fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        for index in 0..MAIN_MENU_ITEMS.len() {
            if let Some(row) = self.entry_rect(index) {
                if row.contains(x, y) {
                    return Some(index);
                }
            }
        }
        None
    }

    /// Return row rectangle in panel coordinates.
    pub(crate) fn entry_rect(&self, index: usize) -> Option<Rect> {
        if index >= MAIN_MENU_ITEMS.len() {
            return None;
        }
        let y = self.y + MAIN_MENU_TITLE_HEIGHT + index as i32 * MAIN_MENU_ITEM_HEIGHT;
        Some(Rect::new(
            self.x + MAIN_MENU_INNER_PADDING,
            y,
            MAIN_MENU_WIDTH - MAIN_MENU_INNER_PADDING * 2,
            MAIN_MENU_ITEM_HEIGHT - 2,
        ))
    }

    /// Select one row index.
    pub(crate) fn select_index(&mut self, index: usize) -> bool {
        let next = index.min(MAIN_MENU_ITEMS.len() - 1);
        if next == self.selected {
            return false;
        }
        self.selected = next;
        true
    }

    /// Select previous row.
    pub(crate) fn select_prev(&mut self) -> bool {
        let old = self.selected;
        self.selected = self.selected.saturating_sub(1);
        old != self.selected
    }

    /// Select next row.
    pub(crate) fn select_next(&mut self) -> bool {
        let old = self.selected;
        self.selected = (self.selected + 1).min(MAIN_MENU_ITEMS.len() - 1);
        old != self.selected
    }

    /// Return immutable row list.
    pub(crate) const fn items(&self) -> &'static [MainMenuItem] {
        &MAIN_MENU_ITEMS
    }
}

/// Export submenu state used for H.264 GUI export controls.
#[derive(Clone, Debug)]
pub(crate) struct ExportMenuState {
    pub(crate) open: bool,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) selected: usize,
    pub(crate) directory: String,
    pub(crate) file_name: String,
    pub(crate) audio_wav: String,
    pub(crate) audio_volume: String,
    audio_wav_duration_secs: Option<f32>,
    audio_wav_probe_path: String,
    pub(crate) bpm: String,
    pub(crate) bars: String,
    pub(crate) beats_per_bar: String,
    pub(crate) exporting: bool,
    pub(crate) preview_frame: u32,
    pub(crate) preview_total: u32,
    pub(crate) status: String,
}

impl ExportMenuState {
    /// Return closed export-submenu state with default output target.
    pub(crate) fn closed() -> Self {
        Self {
            open: false,
            x: 0,
            y: 0,
            selected: 0,
            directory: if cfg!(windows) {
                "C:\\temp".to_string()
            } else {
                ".".to_string()
            },
            file_name: "export.mp4".to_string(),
            audio_wav: String::new(),
            audio_volume: EXPORT_DEFAULT_AUDIO_VOLUME.to_string(),
            audio_wav_duration_secs: None,
            audio_wav_probe_path: String::new(),
            bpm: EXPORT_DEFAULT_BPM.to_string(),
            bars: EXPORT_DEFAULT_BARS.to_string(),
            beats_per_bar: EXPORT_DEFAULT_BEATS_PER_BAR.to_string(),
            exporting: false,
            preview_frame: 0,
            preview_total: TIMELINE_DEFAULT_TOTAL_FRAMES,
            status: String::new(),
        }
    }

    /// Return opened submenu state clamped to editor bounds.
    pub(crate) fn open_at(x: i32, y: i32, panel_width: usize, panel_height: usize) -> Self {
        let mut menu = Self::closed();
        let max_x = (panel_width as i32 - EXPORT_MENU_WIDTH - 8).max(8);
        let max_y = (panel_height as i32 - export_menu_height() - 8).max(8);
        menu.open = true;
        menu.x = x.clamp(8, max_x);
        menu.y = y.clamp(8, max_y);
        menu
    }

    /// Return menu rectangle in panel coordinates.
    pub(crate) fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, EXPORT_MENU_WIDTH, export_menu_height())
    }

    /// Return title-bar close button rectangle in panel coordinates.
    pub(crate) fn close_button_rect(&self) -> Rect {
        Rect::new(
            self.x + EXPORT_MENU_WIDTH - EXPORT_MENU_CLOSE_SIZE - 6,
            self.y + 5,
            EXPORT_MENU_CLOSE_SIZE,
            EXPORT_MENU_CLOSE_SIZE,
        )
    }

    /// Return draggable title-bar rectangle in panel coordinates.
    pub(crate) fn title_bar_rect(&self) -> Rect {
        Rect::new(self.x, self.y, EXPORT_MENU_WIDTH, EXPORT_MENU_TITLE_HEIGHT)
    }

    /// Return selected row item.
    pub(crate) fn selected_item(&self) -> ExportMenuItem {
        EXPORT_MENU_ITEMS[self.selected.min(EXPORT_MENU_ITEMS.len() - 1)]
    }

    /// Return export-preview viewport rectangle in panel coordinates.
    pub(crate) fn preview_viewport_rect(&self) -> Rect {
        let rect = self.rect();
        let x = rect.x + rect.w - EXPORT_MENU_INNER_PADDING - EXPORT_MENU_PREVIEW_WIDTH;
        let y = rect.y + rect.h
            - EXPORT_MENU_BOTTOM_PADDING
            - EXPORT_MENU_STATUS_HEIGHT
            - EXPORT_MENU_PREVIEW_HEIGHT;
        Rect::new(x, y, EXPORT_MENU_PREVIEW_WIDTH, EXPORT_MENU_PREVIEW_HEIGHT)
    }

    /// Return row item at cursor coordinates.
    pub(crate) fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        for index in 0..EXPORT_MENU_ITEMS.len() {
            if let Some(row) = self.entry_rect(index) {
                if row.contains(x, y) {
                    return Some(index);
                }
            }
        }
        None
    }

    /// Return row rectangle in panel coordinates.
    pub(crate) fn entry_rect(&self, index: usize) -> Option<Rect> {
        if index >= EXPORT_MENU_ITEMS.len() {
            return None;
        }
        let y = self.y + EXPORT_MENU_TITLE_HEIGHT + index as i32 * EXPORT_MENU_ITEM_HEIGHT;
        Some(Rect::new(
            self.x + EXPORT_MENU_INNER_PADDING,
            y,
            EXPORT_MENU_WIDTH - EXPORT_MENU_INNER_PADDING * 2,
            EXPORT_MENU_ITEM_HEIGHT - 2,
        ))
    }

    /// Select one row index.
    pub(crate) fn select_index(&mut self, index: usize) -> bool {
        let next = index.min(EXPORT_MENU_ITEMS.len() - 1);
        if next == self.selected {
            return false;
        }
        self.selected = next;
        true
    }

    /// Select previous row.
    pub(crate) fn select_prev(&mut self) -> bool {
        let old = self.selected;
        self.selected = self.selected.saturating_sub(1);
        old != self.selected
    }

    /// Select next row.
    pub(crate) fn select_next(&mut self) -> bool {
        let old = self.selected;
        self.selected = (self.selected + 1).min(EXPORT_MENU_ITEMS.len() - 1);
        old != self.selected
    }

    /// Return immutable row list.
    pub(crate) const fn items(&self) -> &'static [ExportMenuItem] {
        &EXPORT_MENU_ITEMS
    }

    /// Return configured output path combining directory and file name.
    pub(crate) fn output_path(&self) -> PathBuf {
        let mut path = PathBuf::from(self.directory.trim());
        if self.directory.trim().is_empty() {
            path = PathBuf::from(".");
        }
        let raw_name = self.file_name.trim();
        let name = if raw_name.is_empty() {
            "export.mp4".to_string()
        } else if Path::new(raw_name).extension().is_none() {
            format!("{raw_name}.mp4")
        } else {
            raw_name.to_string()
        };
        path.join(name)
    }

    /// Update status line shown at the bottom of the export submenu.
    pub(crate) fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    /// Return parsed BPM value, falling back to defaults when invalid.
    pub(crate) fn parsed_bpm(&self) -> f32 {
        parse_positive_f32_or_default(self.bpm.as_str(), EXPORT_DEFAULT_BPM)
    }

    /// Return parsed bars value, falling back to defaults when invalid.
    pub(crate) fn parsed_bars(&self) -> u32 {
        parse_positive_u32_or_default(self.bars.as_str(), EXPORT_DEFAULT_BARS)
    }

    /// Return parsed beats-per-bar value, falling back to defaults when invalid.
    pub(crate) fn parsed_beats_per_bar(&self) -> u32 {
        parse_positive_u32_or_default(self.beats_per_bar.as_str(), EXPORT_DEFAULT_BEATS_PER_BAR)
    }

    /// Return parsed audio output volume in `[0.0, 2.0]`.
    pub(crate) fn parsed_audio_volume(&self) -> f32 {
        parse_positive_f32_or_default(self.audio_volume.as_str(), EXPORT_DEFAULT_AUDIO_VOLUME)
            .clamp(0.0, 2.0)
    }

    /// Refresh cached WAV duration metadata if the configured path changed.
    pub(crate) fn refresh_audio_duration_cache(&mut self) {
        let trimmed = self.audio_wav.trim();
        if trimmed == self.audio_wav_probe_path {
            return;
        }
        self.audio_wav_probe_path.clear();
        self.audio_wav_probe_path.push_str(trimmed);
        self.audio_wav_duration_secs = probe_wav_duration_secs(trimmed);
    }

    /// Return cached WAV duration in seconds when a valid file is configured.
    pub(crate) fn audio_duration_secs(&self) -> Option<f32> {
        self.audio_wav_duration_secs
    }

    /// Return bar count derived from audio length and current tempo settings.
    pub(crate) fn derived_bars_from_audio(&self) -> Option<f32> {
        let duration = self.audio_wav_duration_secs?;
        let bpm = self.parsed_bpm();
        let beats_per_bar = self.parsed_beats_per_bar().max(1) as f32;
        if !duration.is_finite() || duration <= 0.0 || !bpm.is_finite() || bpm <= 0.0 {
            return None;
        }
        Some((duration * bpm / 60.0) / beats_per_bar)
    }

    /// Return derived timeline frame count from the current music settings.
    pub(crate) fn timeline_total_frames(&self, fps: u32) -> u32 {
        if let Some(duration_secs) = self.audio_wav_duration_secs {
            return total_frames_from_audio_length(duration_secs, fps, self.parsed_bpm());
        }
        total_frames_from_music(
            fps,
            self.parsed_bpm(),
            self.parsed_bars(),
            self.parsed_beats_per_bar(),
        )
        .max(1)
    }

    /// Return configured WAV path when the field is non-empty.
    pub(crate) fn audio_wav_path(&self) -> Option<PathBuf> {
        let trimmed = self.audio_wav.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    }

    /// Move the popup to `x/y`, clamped to editor panel bounds.
    pub(crate) fn move_to(
        &mut self,
        x: i32,
        y: i32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let max_x = (panel_width as i32 - EXPORT_MENU_WIDTH - 8).max(8);
        let max_y = (panel_height as i32 - export_menu_height() - 8).max(8);
        let next_x = x.clamp(8, max_x);
        let next_y = y.clamp(8, max_y);
        if self.x == next_x && self.y == next_y {
            return false;
        }
        self.x = next_x;
        self.y = next_y;
        true
    }
}

/// Return full main-menu popup height.
pub(crate) fn main_menu_height() -> i32 {
    MAIN_MENU_TITLE_HEIGHT
        + MAIN_MENU_ITEM_HEIGHT * MAIN_MENU_ITEMS.len() as i32
        + MAIN_MENU_BOTTOM_PADDING
}

/// Return full export-submenu popup height.
pub(crate) fn export_menu_height() -> i32 {
    EXPORT_MENU_TITLE_HEIGHT
        + EXPORT_MENU_ITEM_HEIGHT * EXPORT_MENU_ITEMS.len() as i32
        + EXPORT_MENU_PREVIEW_GAP
        + EXPORT_MENU_PREVIEW_HEIGHT
        + EXPORT_MENU_STATUS_HEIGHT
        + EXPORT_MENU_BOTTOM_PADDING
}

fn parse_positive_f32_or_default(raw: &str, fallback: &str) -> f32 {
    let parsed = raw.trim().parse::<f32>().ok().filter(|value| *value > 0.0);
    if let Some(value) = parsed {
        return value;
    }
    fallback.parse::<f32>().unwrap_or(120.0)
}

fn parse_positive_u32_or_default(raw: &str, fallback: &str) -> u32 {
    let parsed = raw.trim().parse::<u32>().ok().filter(|value| *value > 0);
    if let Some(value) = parsed {
        return value;
    }
    fallback.parse::<u32>().unwrap_or(1)
}

fn probe_wav_duration_secs(path_raw: &str) -> Option<f32> {
    if path_raw.trim().is_empty() {
        return None;
    }
    let path = Path::new(path_raw.trim());
    let reader = WavReader::open(path).ok()?;
    let spec = reader.spec();
    if spec.sample_rate == 0 {
        return None;
    }
    let duration_samples = reader.duration() as f64;
    let sample_rate = spec.sample_rate as f64;
    Some((duration_samples / sample_rate) as f32)
}

fn total_frames_from_audio_length(duration_secs: f32, fps: u32, bpm: f32) -> u32 {
    if fps == 0
        || !duration_secs.is_finite()
        || duration_secs <= 0.0
        || !bpm.is_finite()
        || bpm <= 0.0
    {
        return 1;
    }
    // Convert audio length -> beat domain via BPM and back to seconds so timeline
    // length stays audio-accurate while preserving tempo-derived semantics.
    let beats = duration_secs as f64 * bpm as f64 / 60.0;
    let normalized_secs = beats * 60.0 / bpm as f64;
    (normalized_secs * fps as f64).round().max(1.0) as u32
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use hound::{SampleFormat, WavSpec, WavWriter};

    use super::{ExportMenuState, MainMenuItem, MainMenuState};

    fn temp_wav_path(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("covergen_{test_name}_{nanos}.wav"))
    }

    fn write_silence_wav(path: &PathBuf, sample_rate: u32, seconds: f32) {
        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(path, spec).expect("create temp wav");
        let total_samples = (sample_rate as f32 * seconds).round().max(0.0) as usize;
        for _ in 0..total_samples {
            writer.write_sample(0i16).expect("write sample");
        }
        writer.finalize().expect("finalize temp wav");
    }

    #[test]
    fn main_menu_selection_clamps_to_last_item() {
        let mut menu = MainMenuState::open_at(20, 20, 420, 480);
        assert!(menu.select_index(100));
        assert_eq!(menu.selected_item(), MainMenuItem::Exit);
        assert!(!menu.select_index(100));
    }

    #[test]
    fn export_menu_output_path_adds_mp4_extension_when_missing() {
        let mut menu = ExportMenuState::closed();
        menu.directory = "./out".to_string();
        menu.file_name = "clip".to_string();
        assert_eq!(
            menu.output_path().to_string_lossy(),
            "./out/clip.mp4".to_string()
        );
    }

    #[test]
    fn export_menu_uses_wav_duration_for_timeline_frame_count() {
        let wav_path = temp_wav_path("timeline_frames");
        write_silence_wav(&wav_path, 48_000, 2.0);

        let mut menu = ExportMenuState::closed();
        menu.audio_wav = wav_path.to_string_lossy().to_string();
        menu.bpm = "100".to_string();
        menu.bars = "1".to_string();
        menu.beats_per_bar = "4".to_string();
        menu.refresh_audio_duration_cache();

        assert_eq!(menu.timeline_total_frames(60), 120);
        let derived = menu
            .derived_bars_from_audio()
            .expect("derived bars should be available");
        assert!((derived - (2.0 * 100.0 / 60.0 / 4.0)).abs() < 0.001);

        let _ = fs::remove_file(wav_path);
    }
}

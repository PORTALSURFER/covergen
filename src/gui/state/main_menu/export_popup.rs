//! Export submenu popup state and validation helpers.

use std::path::{Path, PathBuf};

use hound::WavReader;

use super::super::popup_list;
use crate::gui::geometry::Rect;
use crate::gui::timeline::{total_frames_from_music, TIMELINE_DEFAULT_TOTAL_FRAMES};

/// Export-submenu popup width in panel-space pixels.
const EXPORT_MENU_WIDTH: i32 = 420;
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
const EXPORT_DEFAULT_BPM_VALUE: f32 = 120.0;
const EXPORT_DEFAULT_BARS_VALUE: u32 = 15;
const EXPORT_DEFAULT_BEATS_PER_BAR_VALUE: u32 = 4;
const EXPORT_DEFAULT_AUDIO_VOLUME_VALUE: f32 = 1.0;

/// Selectable rows in the export submenu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportMenuItem {
    Directory,
    FileName,
    Bars,
    BeatsPerBar,
    Codec,
    StartStop,
    Preview,
}

const EXPORT_MENU_ITEMS: [ExportMenuItem; 7] = [
    ExportMenuItem::Directory,
    ExportMenuItem::FileName,
    ExportMenuItem::Bars,
    ExportMenuItem::BeatsPerBar,
    ExportMenuItem::Codec,
    ExportMenuItem::StartStop,
    ExportMenuItem::Preview,
];

/// Runtime state for the export submenu popup.
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
    /// Return a closed export popup with default values.
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

    /// Return an opened popup state clamped to editor bounds.
    pub(crate) fn open_at(x: i32, y: i32, panel_width: usize, panel_height: usize) -> Self {
        let mut menu = Self::closed();
        let max_x = (panel_width as i32 - EXPORT_MENU_WIDTH - 8).max(8);
        let max_y = (panel_height as i32 - export_menu_height() - 8).max(8);
        menu.open = true;
        menu.x = x.clamp(8, max_x);
        menu.y = y.clamp(8, max_y);
        menu
    }

    /// Return popup bounds in panel-space coordinates.
    pub(crate) fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, EXPORT_MENU_WIDTH, export_menu_height())
    }

    /// Return title-bar close button bounds.
    pub(crate) fn close_button_rect(&self) -> Rect {
        Rect::new(
            self.x + EXPORT_MENU_WIDTH - EXPORT_MENU_CLOSE_SIZE - 6,
            self.y + 5,
            EXPORT_MENU_CLOSE_SIZE,
            EXPORT_MENU_CLOSE_SIZE,
        )
    }

    /// Return title-bar draggable area bounds.
    pub(crate) fn title_bar_rect(&self) -> Rect {
        Rect::new(self.x, self.y, EXPORT_MENU_WIDTH, EXPORT_MENU_TITLE_HEIGHT)
    }

    /// Return the currently selected row.
    pub(crate) fn selected_item(&self) -> ExportMenuItem {
        EXPORT_MENU_ITEMS[self.selected.min(EXPORT_MENU_ITEMS.len() - 1)]
    }

    /// Return export-preview viewport bounds.
    pub(crate) fn preview_viewport_rect(&self) -> Rect {
        let rect = self.rect();
        let x = rect.x + rect.w - EXPORT_MENU_INNER_PADDING - EXPORT_MENU_PREVIEW_WIDTH;
        let y = rect.y + rect.h
            - EXPORT_MENU_BOTTOM_PADDING
            - EXPORT_MENU_STATUS_HEIGHT
            - EXPORT_MENU_PREVIEW_HEIGHT;
        Rect::new(x, y, EXPORT_MENU_PREVIEW_WIDTH, EXPORT_MENU_PREVIEW_HEIGHT)
    }

    /// Return hovered row index at one cursor point.
    pub(crate) fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        popup_list::item_at(EXPORT_MENU_ITEMS.len(), x, y, |index| {
            self.entry_rect(index)
        })
    }

    /// Return one row bounds in panel-space coordinates.
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

    /// Select a specific row index.
    pub(crate) fn select_index(&mut self, index: usize) -> bool {
        popup_list::select_index(&mut self.selected, index, EXPORT_MENU_ITEMS.len())
    }

    /// Select the previous row.
    pub(crate) fn select_prev(&mut self) -> bool {
        popup_list::select_prev(&mut self.selected)
    }

    /// Select the next row.
    pub(crate) fn select_next(&mut self) -> bool {
        popup_list::select_next(&mut self.selected, EXPORT_MENU_ITEMS.len())
    }

    /// Return immutable row metadata for rendering.
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

    /// Update bottom status text.
    pub(crate) fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    /// Return parsed BPM, with fallback when invalid.
    pub(crate) fn parsed_bpm(&self) -> f32 {
        parse_positive_f32_or_default(self.bpm.as_str(), EXPORT_DEFAULT_BPM_VALUE)
    }

    /// Return parsed bars, with fallback when invalid.
    pub(crate) fn parsed_bars(&self) -> u32 {
        parse_positive_u32_or_default(self.bars.as_str(), EXPORT_DEFAULT_BARS_VALUE)
    }

    /// Return parsed beats-per-bar, with fallback when invalid.
    pub(crate) fn parsed_beats_per_bar(&self) -> u32 {
        parse_positive_u32_or_default(
            self.beats_per_bar.as_str(),
            EXPORT_DEFAULT_BEATS_PER_BAR_VALUE,
        )
    }

    /// Return parsed audio volume in `[0.0, 2.0]`.
    pub(crate) fn parsed_audio_volume(&self) -> f32 {
        parse_positive_f32_or_default(
            self.audio_volume.as_str(),
            EXPORT_DEFAULT_AUDIO_VOLUME_VALUE,
        )
        .clamp(0.0, 2.0)
    }

    /// Refresh cached WAV duration when configured path changes.
    pub(crate) fn refresh_audio_duration_cache(&mut self) {
        let trimmed = self.audio_wav.trim();
        if trimmed == self.audio_wav_probe_path {
            return;
        }
        self.audio_wav_probe_path.clear();
        self.audio_wav_probe_path.push_str(trimmed);
        self.audio_wav_duration_secs = probe_wav_duration_secs(trimmed);
    }

    /// Return cached WAV duration when available.
    pub(crate) fn audio_duration_secs(&self) -> Option<f32> {
        self.audio_wav_duration_secs
    }

    /// Return bars derived from audio length and tempo settings.
    pub(crate) fn derived_bars_from_audio(&self) -> Option<f32> {
        let duration = self.audio_wav_duration_secs?;
        let bpm = self.parsed_bpm();
        let beats_per_bar = self.parsed_beats_per_bar().max(1) as f32;
        if !duration.is_finite() || duration <= 0.0 || !bpm.is_finite() || bpm <= 0.0 {
            return None;
        }
        Some((duration * bpm / 60.0) / beats_per_bar)
    }

    /// Return derived timeline length in frames.
    pub(crate) fn timeline_total_frames(&self, fps: u32) -> u32 {
        if let Some(duration_secs) = self.audio_wav_duration_secs {
            return total_frames_from_audio_length(duration_secs, fps);
        }
        total_frames_from_music(
            fps,
            self.parsed_bpm(),
            self.parsed_bars(),
            self.parsed_beats_per_bar(),
        )
        .max(1)
    }

    /// Return configured WAV path when non-empty.
    pub(crate) fn audio_wav_path(&self) -> Option<PathBuf> {
        let trimmed = self.audio_wav.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    }

    /// Move the popup to one panel-space position, clamped to bounds.
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

fn export_menu_height() -> i32 {
    EXPORT_MENU_TITLE_HEIGHT
        + EXPORT_MENU_ITEM_HEIGHT * EXPORT_MENU_ITEMS.len() as i32
        + EXPORT_MENU_PREVIEW_GAP
        + EXPORT_MENU_PREVIEW_HEIGHT
        + EXPORT_MENU_STATUS_HEIGHT
        + EXPORT_MENU_BOTTOM_PADDING
}

fn parse_positive_f32_or_default(raw: &str, fallback: f32) -> f32 {
    let parsed = raw.trim().parse::<f32>().ok().filter(|value| *value > 0.0);
    parsed.unwrap_or(fallback)
}

fn parse_positive_u32_or_default(raw: &str, fallback: u32) -> u32 {
    let parsed = raw.trim().parse::<u32>().ok().filter(|value| *value > 0);
    parsed.unwrap_or(fallback)
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

fn total_frames_from_audio_length(duration_secs: f32, fps: u32) -> u32 {
    if fps == 0 || !duration_secs.is_finite() || duration_secs <= 0.0 {
        return 1;
    }
    (duration_secs as f64 * fps as f64).round().max(1.0) as u32
}

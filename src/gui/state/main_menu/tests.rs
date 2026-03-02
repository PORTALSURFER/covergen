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
    assert_eq!(menu.output_path(), PathBuf::from("./out").join("clip.mp4"));
}

#[test]
fn export_menu_uses_wav_duration_for_timeline_frame_count() {
    let wav_path = temp_wav_path("timeline_frames");
    write_silence_wav(&wav_path, 48_000, 2.0);

    let mut menu = ExportMenuState::closed();
    menu.audio_wav = wav_path.to_string_lossy().to_string();
    menu.bpm = "100".to_string();
    menu.beats_per_bar = "4".to_string();
    menu.refresh_audio_duration_cache();

    assert_eq!(menu.timeline_total_frames(60), 120);
    let derived = menu
        .derived_bars_from_audio()
        .expect("derived bars should be available");
    assert!((derived - (2.0 * 100.0 / 60.0 / 4.0)).abs() < 0.001);

    let _ = fs::remove_file(wav_path);
}

#[test]
fn export_menu_audio_length_drives_timeline_even_when_bpm_changes() {
    let wav_path = temp_wav_path("timeline_audio_source");
    write_silence_wav(&wav_path, 48_000, 1.5);

    let mut menu = ExportMenuState::closed();
    menu.audio_wav = wav_path.to_string_lossy().to_string();
    menu.bpm = "80".to_string();
    menu.refresh_audio_duration_cache();
    let first = menu.timeline_total_frames(60);

    menu.bpm = "220".to_string();
    let second = menu.timeline_total_frames(60);

    assert_eq!(first, 90);
    assert_eq!(second, 90);
    let _ = fs::remove_file(wav_path);
}

#[test]
fn export_menu_manual_bar_length_drives_timeline_without_audio() {
    let mut menu = ExportMenuState::closed();
    menu.bar_length = "8".to_string();
    menu.bpm = "120".to_string();
    menu.beats_per_bar = "4".to_string();

    assert_eq!(menu.timeline_total_frames(60), 960);

    menu.bpm = "60".to_string();
    assert_eq!(menu.timeline_total_frames(60), 1_920);
}

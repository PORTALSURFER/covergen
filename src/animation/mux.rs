//! MP4 + WAV mux helpers for completed animation exports.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use ffmpeg_sidecar::command::FfmpegCommand;
use ffmpeg_sidecar::download::auto_download as auto_download_ffmpeg;

const AUDIO_MUX_BITRATE: &str = "320k";
const AUDIO_MUX_SAMPLE_RATE: &str = "48000";

/// Mux one WAV file into an existing MP4, reusing the encoded H.264 bitstream.
///
/// The video stream is copied without re-encode while audio is encoded as AAC
/// at high quality for broad playback compatibility.
pub fn mux_wav_audio_into_mp4(video_path: &Path, wav_path: &Path) -> Result<(), Box<dyn Error>> {
    if !video_path.exists() {
        return Err(format!("video file not found: {}", video_path.display()).into());
    }
    if !wav_path.exists() {
        return Err(format!("audio file not found: {}", wav_path.display()).into());
    }
    let is_wav = wav_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("wav"))
        .unwrap_or(false);
    if !is_wav {
        return Err(format!(
            "audio file must be .wav for muxing, got {}",
            wav_path.display()
        )
        .into());
    }

    auto_download_ffmpeg()
        .map_err(|err| format!("failed to prepare ffmpeg sidecar for audio mux: {err}"))?;
    let temp_output = mux_temp_output_path(video_path);
    let _ = fs::remove_file(temp_output.as_path());

    let input_video = video_path.to_string_lossy().to_string();
    let input_audio = wav_path.to_string_lossy().to_string();
    let output_path = temp_output.to_string_lossy().to_string();
    let mut command = FfmpegCommand::new();
    command
        .overwrite()
        .input(input_video.as_str())
        .input(input_audio.as_str())
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("1:a:0")
        .codec_video("copy")
        .codec_audio("aac")
        .arg("-b:a")
        .arg(AUDIO_MUX_BITRATE)
        .arg("-ar")
        .arg(AUDIO_MUX_SAMPLE_RATE)
        .arg("-movflags")
        .arg("+faststart")
        .arg("-shortest")
        .output(output_path.as_str());
    let mut child = command
        .spawn()
        .map_err(|err| format!("failed to spawn ffmpeg for audio mux: {err}"))?;
    let status = child
        .wait()
        .map_err(|err| format!("failed while waiting on ffmpeg audio mux: {err}"))?;
    if !status.success() {
        let _ = fs::remove_file(temp_output.as_path());
        return Err(format!(
            "ffmpeg audio mux failed with status code {:?}",
            status.code()
        )
        .into());
    }
    if let Err(first_rename_err) = fs::rename(temp_output.as_path(), video_path) {
        if video_path.exists() {
            fs::remove_file(video_path)
                .map_err(|err| format!("failed to replace mp4 during audio mux: {err}"))?;
        }
        fs::rename(temp_output.as_path(), video_path).map_err(|err| {
            format!(
                "failed to finalize muxed mp4 after retry: {err} (initial rename error: {first_rename_err})"
            )
        })?;
    }
    Ok(())
}

fn mux_temp_output_path(video_path: &Path) -> PathBuf {
    let parent = video_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = video_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("covergen_export");
    parent.join(format!("{stem}.audio_mux_tmp.mp4"))
}

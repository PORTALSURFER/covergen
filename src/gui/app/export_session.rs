//! GUI export session lifecycle and frame encoding helpers.

use super::*;
use crate::gui::renderer::TexPreviewCaptureState;

const EXPORT_PREVIEW_BG_B: u8 = 8;
const EXPORT_PREVIEW_BG_G: u8 = 8;
const EXPORT_PREVIEW_BG_R: u8 = 8;

/// Active export session metadata for GUI H.264 streaming.
pub(super) struct GuiExportSession {
    pub(super) encoder: RawVideoEncoder,
    pub(super) next_frame: u32,
    pub(super) total_frames: u32,
    pub(super) restore_paused: bool,
    pub(super) output_path: PathBuf,
    pub(super) audio_wav_path: Option<PathBuf>,
}

impl GuiApp {
    pub(super) fn try_start_export_from_request(&mut self) -> Result<(), Box<dyn Error>> {
        if !self.start_export_requested || self.export_session.is_some() {
            return Ok(());
        }
        let Some(frame) = self.tex_view.frame() else {
            self.state
                .export_menu
                .set_status("Export failed: preview output unavailable");
            self.start_export_requested = false;
            return Ok(());
        };
        let output_path = self.state.export_menu.output_path();
        let total_frames = self
            .state
            .export_menu
            .timeline_total_frames(self.config.animation.fps);
        let audio_wav_path = self.state.export_menu.audio_wav_path();
        if let Some(audio_path) = audio_wav_path.as_ref() {
            if !audio_path.exists() {
                self.state.export_menu.set_status(format!(
                    "Export failed: audio file not found: {}",
                    audio_path.display()
                ));
                self.start_export_requested = false;
                return Ok(());
            }
            if !is_wav_path(audio_path.as_path()) {
                self.state
                    .export_menu
                    .set_status("Export failed: audio file must be a .wav path for timeline sync");
                self.start_export_requested = false;
                return Ok(());
            }
        }
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(err) = fs::create_dir_all(parent) {
                    self.state
                        .export_menu
                        .set_status(format!("Export failed: {err}"));
                    self.start_export_requested = false;
                    return Ok(());
                }
            }
        }
        let encoder = match RawVideoEncoder::spawn(
            frame.texture_width,
            frame.texture_height,
            self.config.animation.fps,
            output_path.as_path(),
        ) {
            Ok(encoder) => encoder,
            Err(err) => {
                self.state
                    .export_menu
                    .set_status(format!("Export failed: {err}"));
                self.start_export_requested = false;
                return Ok(());
            }
        };
        self.export_session = Some(GuiExportSession {
            encoder,
            next_frame: 0,
            total_frames,
            restore_paused: self.state.paused,
            output_path: output_path.clone(),
            audio_wav_path,
        });
        self.state.export_menu.exporting = true;
        self.state.export_menu.preview_frame = 0;
        self.state.export_menu.preview_total = total_frames;
        self.state
            .export_menu
            .set_status(format!("Exporting: {}", output_path.display()));
        self.state.invalidation.invalidate_overlays();
        self.start_export_requested = false;
        Ok(())
    }

    pub(super) fn capture_export_frame(&mut self) -> Result<(), Box<dyn Error>> {
        let (width, height) = match self
            .renderer
            .capture_tex_preview_bgra(&mut self.export_bgra_scratch)
        {
            Ok(TexPreviewCaptureState::Ready(size)) => size,
            Ok(TexPreviewCaptureState::Pending) => return Ok(()),
            Ok(TexPreviewCaptureState::Unavailable) => {
                self.stop_export_session("failed");
                self.state
                    .export_menu
                    .set_status("Export failed: preview texture unavailable");
                self.state.invalidation.invalidate_overlays();
                return Ok(());
            }
            Err(err) => {
                self.stop_export_session("failed");
                self.state
                    .export_menu
                    .set_status(format!("Export failed: {err}"));
                self.state.invalidation.invalidate_overlays();
                return Ok(());
            }
        };

        let Some(session) = self.export_session.as_mut() else {
            return Ok(());
        };
        composite_export_bgra_over_preview_bg(&mut self.export_bgra_scratch);
        let write_result = match session.encoder.frame_format() {
            StreamFrameFormat::Gray8 => {
                fill_gray_from_bgra(
                    &self.export_bgra_scratch,
                    width,
                    height,
                    &mut self.export_gray_scratch,
                );
                session.encoder.write_gray_frame(&self.export_gray_scratch)
            }
            StreamFrameFormat::Bgra8 => session.encoder.write_bgra_frame(&self.export_bgra_scratch),
        };
        if let Err(err) = write_result {
            self.stop_export_session("failed");
            self.state
                .export_menu
                .set_status(format!("Export failed: {err}"));
            self.state.invalidation.invalidate_overlays();
            return Ok(());
        }
        session.next_frame = session.next_frame.saturating_add(1);
        self.state.export_menu.preview_frame = session.next_frame.min(session.total_frames);
        self.state.invalidation.invalidate_overlays();
        if session.next_frame >= session.total_frames {
            let _ = self.stop_export_session("completed");
        }
        Ok(())
    }

    pub(super) fn stop_export_session(&mut self, reason: &str) -> bool {
        self.start_export_requested = false;
        let Some(session) = self.export_session.take() else {
            self.state.export_menu.exporting = false;
            return false;
        };
        self.state.paused = session.restore_paused;
        self.state.export_menu.exporting = false;
        self.state.export_menu.preview_total = session.total_frames;
        self.state.export_menu.preview_frame = self
            .state
            .export_menu
            .preview_frame
            .min(session.total_frames);
        let should_mux_audio = reason != "failed";
        match session.encoder.finish() {
            Ok(()) => {
                let audio_mux_status = if should_mux_audio {
                    if let Some(audio_path) = session.audio_wav_path.as_ref() {
                        mux_wav_audio_into_mp4(session.output_path.as_path(), audio_path.as_path())
                            .map(|_| {
                                format!(
                                    "Export {reason}: {} (audio: {})",
                                    session.output_path.display(),
                                    audio_path.display()
                                )
                            })
                    } else {
                        Ok(format!(
                            "Export {reason}: {}",
                            session.output_path.display()
                        ))
                    }
                } else {
                    Ok(format!(
                        "Export {reason}: {}",
                        session.output_path.display()
                    ))
                };
                match audio_mux_status {
                    Ok(status) => self.state.export_menu.set_status(status),
                    Err(err) => self.state.export_menu.set_status(format!(
                        "Export {reason}: {} (audio mux failed: {err})",
                        session.output_path.display()
                    )),
                }
            }
            Err(err) => {
                self.state
                    .export_menu
                    .set_status(format!("Export failed: {err}"));
            }
        }
        self.state.invalidation.invalidate_overlays();
        true
    }
}

fn fill_gray_from_bgra(src_bgra: &[u8], width: u32, height: u32, dst_gray: &mut Vec<u8>) {
    let pixel_count = width as usize * height as usize;
    dst_gray.resize(pixel_count, 0);
    for (index, pixel) in src_bgra.chunks_exact(4).enumerate().take(pixel_count) {
        let b = pixel[0] as u16;
        let g = pixel[1] as u16;
        let r = pixel[2] as u16;
        let luma = (r * 77 + g * 150 + b * 29) / 256;
        dst_gray[index] = luma as u8;
    }
}

fn composite_export_bgra_over_preview_bg(frame_bgra: &mut [u8]) {
    for px in frame_bgra.chunks_exact_mut(4) {
        let alpha = px[3] as u16;
        if alpha >= 255 {
            continue;
        }
        let inv_alpha = 255u16.saturating_sub(alpha);
        let b = ((px[0] as u16).saturating_mul(alpha)
            + (EXPORT_PREVIEW_BG_B as u16).saturating_mul(inv_alpha)
            + 127)
            / 255;
        let g = ((px[1] as u16).saturating_mul(alpha)
            + (EXPORT_PREVIEW_BG_G as u16).saturating_mul(inv_alpha)
            + 127)
            / 255;
        let r = ((px[2] as u16).saturating_mul(alpha)
            + (EXPORT_PREVIEW_BG_R as u16).saturating_mul(inv_alpha)
            + 127)
            / 255;
        px[0] = b as u8;
        px[1] = g as u8;
        px[2] = r as u8;
        px[3] = 255;
    }
}

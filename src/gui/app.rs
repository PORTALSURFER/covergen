//! GUI application state and frame orchestration.

use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rfd::FileDialog;
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorIcon, Fullscreen, Window};

use crate::runtime_config::V2Config;
use crate::telemetry;
use crate::{
    animation::mux_wav_audio_into_mp4, animation::RawVideoEncoder, animation::StreamFrameFormat,
};

use super::audio::TimelineAudioPreview;
use super::input::InputCollector;
use super::interaction::{apply_preview_actions, step_timeline_if_running};
use super::perf::GuiPerfRecorder;
use super::project::{
    GuiProject, GuiProjectInvalidation, PersistedGuiProject, PersistedProjectLoadError,
    PersistedProjectLoadOutcome, PersistedProjectLoadWarning, ProjectNodeKind, NODE_WIDTH,
};
use super::renderer::GuiRenderer;
use super::scene::SceneBuilder;
use super::state::{InputSnapshot, PendingAppAction, PreviewState};
use super::tex_view::TexViewerGenerator;
use super::tex_view::TexViewerUpdate;
use super::timeline::{clamp_frame, editor_panel_height};

const MIN_PANEL_WIDTH: usize = 260;
const MIN_PREVIEW_WIDTH: usize = 320;
const DIVIDER_HIT_SLOP_PX: i32 = 6;
const GUI_LOCKED_FPS: u32 = 60;
const GUI_PROJECT_AUTOSAVE_FILE: &str = ".covergen_gui_graph.json";
const GUI_PROJECT_SAVE_FILE: &str = "covergen_gui_project.json";
#[cfg(test)]
const GUI_PROJECT_SAVE_FILE_LEGACY: &str = ".covergen_gui_project.json";
const EXPORT_PREVIEW_BG_B: u8 = 8;
const EXPORT_PREVIEW_BG_G: u8 = 8;
const EXPORT_PREVIEW_BG_R: u8 = 8;

/// Active export session metadata for GUI H.264 streaming.
struct GuiExportSession {
    encoder: RawVideoEncoder,
    next_frame: u32,
    total_frames: u32,
    restore_paused: bool,
    output_path: PathBuf,
    audio_wav_path: Option<PathBuf>,
}

/// Active divider drag metadata for panel resizing.
#[derive(Clone, Copy, Debug)]
struct PanelResizeDrag {
    grab_offset_px: i32,
}

/// Snapshot of project-scoped invalidation epochs captured pre-update.
#[derive(Clone, Copy, Debug)]
struct SceneInvalidationSnapshot {
    project: GuiProjectInvalidation,
}

impl SceneInvalidationSnapshot {
    /// Capture the minimal invalidation state needed for post-update diffing.
    fn capture(project: &GuiProject) -> Self {
        Self {
            project: project.invalidation(),
        }
    }
}

/// Frame scheduler and state owner for the realtime GUI loop.
pub(crate) struct GuiApp {
    config: V2Config,
    panel_width: usize,
    panel_resize_drag: Option<PanelResizeDrag>,
    resize_cursor_active: bool,
    window: Arc<Window>,
    renderer: GuiRenderer,
    project: GuiProject,
    state: PreviewState,
    input: InputCollector,
    scene: SceneBuilder,
    tex_view: TexViewerGenerator,
    perf: GuiPerfRecorder,
    frame_budget: Duration,
    frame_deadline: Instant,
    last_frame_start: Instant,
    frame_counter: u64,
    benchmark_frame_limit: Option<u64>,
    benchmark_node: Option<u32>,
    export_session: Option<GuiExportSession>,
    start_export_requested: bool,
    timeline_audio: TimelineAudioPreview,
    export_bgra_scratch: Vec<u8>,
    export_gray_scratch: Vec<u8>,
    close_requested: bool,
    needs_redraw: bool,
    continuous_redraw: bool,
    title_deadline: Instant,
    last_title: String,
}

impl GuiApp {
    /// Create one GPU-backed GUI app bound to the provided window.
    pub(crate) async fn new(config: V2Config, window: Arc<Window>) -> Result<Self, Box<dyn Error>> {
        let renderer = GuiRenderer::new(window.clone(), config.gui.vsync).await?;
        let panel_width = clamp_panel_width(launch_panel_width(renderer.width()), renderer.width());
        let project_load_begin = Instant::now();
        let benchmark_mode = is_benchmark_mode(&config);
        let mut project = if benchmark_mode {
            GuiProject::new_empty(config.width, config.height)
        } else {
            match load_autosaved_project(panel_width, renderer.height()) {
                Ok(Some(loaded)) => {
                    println!(
                        "[gui] loaded autosave from {}",
                        autosave_project_path().display()
                    );
                    log_project_load_warnings(autosave_project_path().as_path(), &loaded.warnings);
                    loaded.project
                }
                Ok(None) => GuiProject::new_empty(config.width, config.height),
                Err(err) => {
                    eprintln!("[gui] failed to load autosave: {err}");
                    GuiProject::new_empty(config.width, config.height)
                }
            }
        };
        let benchmark_node =
            maybe_seed_benchmark_nodes(&config, &mut project, panel_width, renderer.height());
        telemetry::record_timing("gui.startup.project_load", project_load_begin.elapsed());
        let state = PreviewState::new(&config);
        let frame_budget = frame_budget(GUI_LOCKED_FPS);
        let benchmark_frame_limit = benchmark_frame_limit(&config);
        let now = Instant::now();
        println!(
            "[gui] {}x{} @ {}hz locked ({:?})",
            renderer.width(),
            renderer.height(),
            GUI_LOCKED_FPS,
            config.gui.vsync
        );
        println!(
            "[gui] controls: Esc=quit, F11=fullscreen, Space=play/pause, Shift+A=add node menu, `=main menu, Tab=open node, F1=context help, RMB=select, RMB drag=marquee, RMB on bound param value=unbind, Delete=remove selected, Toggle box=expand/collapse, Arrows=param select/adjust, Alt+LMB drag=cut links, timeline(play/pause + scrub)"
        );
        Ok(Self {
            config,
            panel_width,
            panel_resize_drag: None,
            resize_cursor_active: false,
            window,
            renderer,
            project,
            state,
            input: InputCollector::default(),
            scene: SceneBuilder::default(),
            tex_view: TexViewerGenerator::default(),
            perf: GuiPerfRecorder::new(None),
            frame_budget,
            frame_deadline: now,
            last_frame_start: now,
            frame_counter: 0,
            benchmark_frame_limit,
            benchmark_node,
            export_session: None,
            start_export_requested: false,
            timeline_audio: TimelineAudioPreview::default(),
            export_bgra_scratch: Vec::new(),
            export_gray_scratch: Vec::new(),
            close_requested: false,
            needs_redraw: true,
            continuous_redraw: true,
            title_deadline: now,
            last_title: String::new(),
        }
        .with_perf_trace())
    }

    /// Return current redraw deadline for the event loop.
    pub(crate) fn frame_deadline(&self) -> Instant {
        self.frame_deadline
    }

    /// Return true when GUI requested a clean application exit.
    pub(crate) fn should_exit(&self) -> bool {
        self.close_requested
    }

    /// Return true when this event should terminate the GUI loop.
    pub(crate) fn handle_window_event(&mut self, event: &WindowEvent) -> bool {
        if self.toggle_fullscreen_if_requested(event) {
            self.needs_redraw = true;
            return false;
        }
        match event {
            WindowEvent::CloseRequested => true,
            WindowEvent::DroppedFile(path) => {
                self.handle_dropped_file(path.as_path());
                self.needs_redraw = true;
                false
            }
            WindowEvent::Resized(size) => {
                self.renderer.resize(size.width, size.height);
                self.panel_width = clamp_panel_width(self.panel_width, self.renderer.width());
                self.needs_redraw = true;
                false
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.update_resize_cursor(Some((position.x as i32, position.y as i32)));
                self.input.handle_event(event);
                self.needs_redraw = true;
                false
            }
            WindowEvent::CursorLeft { .. } => {
                self.update_resize_cursor(None);
                self.input.handle_event(event);
                self.needs_redraw = true;
                false
            }
            WindowEvent::ScaleFactorChanged { .. } => false,
            _ => {
                self.input.handle_event(event);
                self.needs_redraw = true;
                false
            }
        }
    }

    /// Assign a dropped `.wav` file path into the timeline audio slot.
    fn handle_dropped_file(&mut self, path: &Path) {
        if !is_wav_path(path) {
            self.state.export_menu.set_status(format!(
                "Ignored dropped file (expected .wav): {}",
                path.display()
            ));
            self.state.invalidation.invalidate_overlays();
            return;
        }
        self.state.export_menu.audio_wav = path.to_string_lossy().to_string();
        self.state.export_menu.refresh_audio_duration_cache();
        let timeline_frames = self
            .state
            .export_menu
            .timeline_total_frames(self.config.animation.fps);
        self.state.export_menu.preview_total = timeline_frames;
        if let Some(duration_secs) = self.state.export_menu.audio_duration_secs() {
            self.state.export_menu.set_status(format!(
                "Timeline WAV assigned: {} ({duration_secs:.2}s)",
                path.display()
            ));
        } else {
            self.state.export_menu.set_status(format!(
                "Timeline WAV assigned: {} (duration unavailable)",
                path.display()
            ));
        }
        self.state.invalidation.invalidate_timeline();
        self.state.invalidation.invalidate_overlays();
    }

    /// Toggle window fullscreen mode on `F11` key press.
    fn toggle_fullscreen_if_requested(&mut self, event: &WindowEvent) -> bool {
        let WindowEvent::KeyboardInput { event, .. } = event else {
            return false;
        };
        if event.state != ElementState::Pressed || event.repeat {
            return false;
        }
        if !matches!(event.physical_key, PhysicalKey::Code(KeyCode::F11)) {
            return false;
        }
        if self.window.fullscreen().is_some() {
            self.window.set_fullscreen(None);
        } else {
            self.window
                .set_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        true
    }

    /// Request redraw when the frame deadline has elapsed.
    pub(crate) fn request_redraw_if_due(&mut self) {
        if !self.continuous_redraw && !self.needs_redraw {
            return;
        }
        let now = Instant::now();
        if now < self.frame_deadline {
            return;
        }
        self.window.request_redraw();
        while self.frame_deadline <= now {
            self.frame_deadline += self.frame_budget;
        }
    }

    /// Advance input/state and render one frame.
    pub(crate) fn redraw(&mut self) -> Result<(), Box<dyn Error>> {
        let frame_start = Instant::now();
        let frame_delta = frame_start.saturating_duration_since(self.last_frame_start);
        self.last_frame_start = frame_start;

        let input_start = Instant::now();
        let snapshot = self
            .input
            .snapshot(self.renderer.width(), self.renderer.height());
        let input_elapsed = input_start.elapsed();

        let update_start = Instant::now();
        let capture_scene_invalidation = self.should_capture_scene_invalidation_snapshot(&snapshot);
        let scene_invalidation_before =
            capture_scene_invalidation.then(|| SceneInvalidationSnapshot::capture(&self.project));
        let (resize_changed, consume_editor_input) = self.apply_panel_resize_input(&snapshot);
        let mut scene_dirty = resize_changed;
        scene_dirty |= self
            .project
            .set_lfo_sync_bpm(self.state.export_menu.parsed_bpm());
        if consume_editor_input {
            self.state.drag = None;
            self.state.wire_drag = None;
            self.state.hover_param_target = None;
            self.state.hover_param = None;
            self.state.hover_insert_link = None;
            if !self.state.auto_expanded_binding_nodes.is_empty() {
                for node_id in self.state.auto_expanded_binding_nodes.drain(..) {
                    scene_dirty |= self.project.collapse_node(
                        node_id,
                        self.panel_width,
                        self.renderer.height(),
                    );
                }
            }
            self.state.invalidation.invalidate_nodes();
            self.state.invalidation.invalidate_wires();
            self.state.invalidation.invalidate_overlays();
            self.state.prev_left_down = snapshot.left_down;
        } else {
            scene_dirty |= apply_preview_actions(
                &self.config,
                snapshot,
                &mut self.project,
                self.renderer.width(),
                self.panel_width,
                self.renderer.height(),
                &mut self.state,
            );
        }
        scene_dirty |= self.handle_pending_app_actions()?;
        self.state.export_menu.refresh_audio_duration_cache();
        if self.start_export_requested && self.export_session.is_none() {
            if self.state.frame_index != 0 {
                self.state.frame_index = 0;
                scene_dirty = true;
                self.state.invalidation.invalidate_timeline();
                if self.project.has_signal_preview_nodes() {
                    self.state.invalidation.invalidate_nodes();
                }
            }
            if !self.state.paused {
                self.state.paused = true;
                self.state.invalidation.invalidate_timeline();
            }
        }
        if let Some(session) = self.export_session.as_ref() {
            if !self.state.paused {
                self.state.paused = true;
                self.state.invalidation.invalidate_timeline();
            }
            if self.state.frame_index != session.next_frame {
                self.state.frame_index = session.next_frame;
                scene_dirty = true;
                self.state.invalidation.invalidate_timeline();
                if self.project.has_signal_preview_nodes() {
                    self.state.invalidation.invalidate_nodes();
                }
            }
        }
        if self.config.gui.benchmark_drag {
            scene_dirty |= self.apply_synthetic_drag();
        }
        let timeline_total_frames = self
            .state
            .export_menu
            .timeline_total_frames(self.config.animation.fps);
        if self.export_session.is_none() && !self.start_export_requested {
            let timeline_advanced = step_timeline_if_running(
                &mut self.state,
                frame_delta,
                self.config.animation.fps,
                timeline_total_frames,
            );
            scene_dirty |= timeline_advanced;
            if timeline_advanced {
                self.state.invalidation.invalidate_timeline();
                if self.project.has_signal_preview_nodes() {
                    self.state.invalidation.invalidate_nodes();
                }
            }
        }
        let clamped_frame = clamp_frame(self.state.frame_index, timeline_total_frames);
        if clamped_frame != self.state.frame_index {
            self.state.frame_index = clamped_frame;
            scene_dirty = true;
            self.state.invalidation.invalidate_timeline();
            if self.project.has_signal_preview_nodes() {
                self.state.invalidation.invalidate_nodes();
            }
        }
        if self.export_session.is_none()
            && self.state.export_menu.preview_total != timeline_total_frames
        {
            self.state.export_menu.preview_total = timeline_total_frames;
            scene_dirty = true;
            self.state.invalidation.invalidate_timeline();
            self.state.invalidation.invalidate_overlays();
        }
        self.sync_timeline_audio_preview(timeline_total_frames);
        self.state.avg_fps = smoothed_fps(self.state.avg_fps, frame_delta);
        self.apply_project_scoped_invalidation(scene_invalidation_before, resize_changed);
        let update_elapsed = update_start.elapsed();
        let hit_test_scans = self.project.take_hit_test_scan_count();

        let mut scene_elapsed = Duration::ZERO;
        let mut render_elapsed = Duration::ZERO;
        let mut submit_count = 0u32;
        let mut upload_bytes = 0u64;
        let mut ui_alloc_bytes = 0u64;
        let mut bridge_intersection_tests = 0u64;
        let mut signal_scope_samples = 0u64;
        let mut signal_scope_eval_ms = 0.0f64;
        let mut scene_nodes_ms = 0.0f64;
        let mut scene_edges_ms = 0.0f64;
        let mut scene_overlays_ms = 0.0f64;
        let export_active = self.export_session.is_some() || self.start_export_requested;
        if scene_dirty || self.needs_redraw || export_active {
            self.tex_view.update(
                &self.project,
                TexViewerUpdate {
                    viewport_width: self.renderer.width(),
                    viewport_height: self.renderer.height(),
                    panel_width: self.panel_width,
                    frame_index: self.state.frame_index,
                    timeline_total_frames,
                    timeline_fps: self.config.animation.fps,
                    tex_eval_epoch: self.state.invalidation.tex_eval,
                },
            );
            self.try_start_export_from_request()?;
            let scene_start = Instant::now();
            let frame = self.scene.build(
                &self.project,
                &self.state,
                self.renderer.width(),
                self.renderer.height(),
                self.panel_width,
                self.config.animation.fps,
            );
            scene_elapsed = scene_start.elapsed();
            ui_alloc_bytes = frame.ui_alloc_bytes;
            bridge_intersection_tests = frame.bridge_intersection_tests;
            signal_scope_samples = frame.signal_scope_samples;
            signal_scope_eval_ms = frame.signal_scope_eval_ms;
            scene_nodes_ms = frame.nodes_ms;
            scene_edges_ms = frame.edges_ms;
            scene_overlays_ms = frame.overlays_ms;

            let render_start = Instant::now();
            self.renderer.render(
                frame,
                self.tex_view.frame(),
                self.panel_width,
                self.state.avg_fps,
            )?;
            render_elapsed = render_start.elapsed();
            let render_perf = self.renderer.take_perf_counters();
            submit_count = render_perf.submit_count;
            upload_bytes = render_perf.upload_bytes;
            ui_alloc_bytes = ui_alloc_bytes.saturating_add(render_perf.alloc_bytes);
            if self.export_session.is_some() {
                self.capture_export_frame()?;
            }
        }

        let total_elapsed = frame_start.elapsed();
        telemetry::record_counter_u64("gui.gpu.submit_count_per_frame", submit_count as u64);
        telemetry::record_counter_u64("gui.gpu.upload_bytes_per_frame", upload_bytes);
        telemetry::record_counter_u64("gui.hit_test.scan_count_per_frame", hit_test_scans);
        telemetry::record_counter_u64(
            "gui.wire.bridge_intersection_tests_per_frame",
            bridge_intersection_tests,
        );
        telemetry::record_counter_u64("signal_scope_samples_per_frame", signal_scope_samples);
        telemetry::record_timing_ms("signal_scope_eval_ms", signal_scope_eval_ms);
        telemetry::record_timing_ms("scene.nodes_ms", scene_nodes_ms);
        telemetry::record_timing_ms("scene.edges_ms", scene_edges_ms);
        telemetry::record_timing_ms("scene.overlays_ms", scene_overlays_ms);
        telemetry::record_counter_u64("gui.ui.alloc_bytes_per_frame", ui_alloc_bytes);
        let total_secs = total_elapsed.as_secs_f64();
        if total_secs > 0.0 {
            telemetry::record_counter(
                "gui.ui.alloc_bytes_per_second",
                ui_alloc_bytes as f64 / total_secs,
            );
        }
        self.perf.record(
            self.frame_counter,
            input_elapsed,
            update_elapsed,
            scene_elapsed,
            render_elapsed,
            total_elapsed,
            submit_count,
            upload_bytes,
            hit_test_scans,
            bridge_intersection_tests,
            ui_alloc_bytes,
        );
        if self.frame_counter == 0 {
            telemetry::record_timing("gui.startup.first_frame.total", total_elapsed);
            telemetry::record_timing("gui.startup.first_frame.scene", scene_elapsed);
            telemetry::record_timing("gui.startup.first_frame.render", render_elapsed);
        }
        self.update_loop_policy();
        self.update_title(frame_start);
        self.needs_redraw = false;
        self.frame_counter = self.frame_counter.wrapping_add(1);
        if self
            .benchmark_frame_limit
            .is_some_and(|limit| self.frame_counter >= limit)
        {
            self.close_requested = true;
        }
        Ok(())
    }

    /// Propagate project mutation deltas into scoped scene/tex epochs.
    fn apply_project_scoped_invalidation(
        &mut self,
        snapshot_before: Option<SceneInvalidationSnapshot>,
        resize_changed: bool,
    ) {
        if let Some(snapshot_before) = snapshot_before {
            let project_after = self.project.invalidation();
            if snapshot_before.project.nodes != project_after.nodes {
                self.state.invalidation.invalidate_nodes();
            }
            if snapshot_before.project.wires != project_after.wires {
                self.state.invalidation.invalidate_wires();
                self.state.invalidation.invalidate_overlays();
            }
            if snapshot_before.project.tex_eval != project_after.tex_eval {
                self.state.invalidation.invalidate_tex_eval();
            }
        }

        if resize_changed {
            self.state.invalidation.invalidate_nodes();
            self.state.invalidation.invalidate_wires();
            self.state.invalidation.invalidate_overlays();
            self.state.invalidation.invalidate_timeline();
        }
    }

    /// Return true when this frame can plausibly mutate project invalidation epochs.
    fn should_capture_scene_invalidation_snapshot(&self, input: &InputSnapshot) -> bool {
        if self.frame_counter == 0
            || self.state.pending_app_action.is_some()
            || self.config.gui.benchmark_drag
            || self.state.request_new_project
        {
            return true;
        }
        if self.state.drag.is_some()
            || self.state.wire_drag.is_some()
            || self.state.link_cut.is_some()
            || self.state.param_edit.is_some()
            || self.state.param_scrub.is_some()
            || self.state.param_dropdown.is_some()
            || self.state.timeline_bpm_edit.is_some()
            || self.state.timeline_bar_edit.is_some()
            || self.state.right_marquee.is_some()
            || self.state.export_menu_drag.is_some()
        {
            return true;
        }
        if (self.project.lfo_sync_bpm() - self.state.export_menu.parsed_bpm().clamp(1.0, 400.0))
            .abs()
            > f32::EPSILON
        {
            return true;
        }
        input_has_project_mutation_intent(input)
    }

    /// Flush trace output before event-loop shutdown.
    pub(crate) fn shutdown(&mut self) -> Result<(), Box<dyn Error>> {
        self.timeline_audio.stop();
        let _ = self.stop_export_session("stopped");
        if !is_benchmark_mode(&self.config) {
            save_autosaved_project(&self.project)?;
        }
        self.perf.flush()
    }

    fn handle_pending_app_actions(&mut self) -> Result<bool, Box<dyn Error>> {
        let Some(action) = self.state.pending_app_action.take() else {
            return Ok(false);
        };
        match action {
            PendingAppAction::SaveProject => {
                let Some(path) = pick_save_project_path() else {
                    self.state.export_menu.set_status("Save canceled");
                    return Ok(true);
                };
                match save_project_file(&self.project, path.as_path()) {
                    Ok(()) => {
                        self.state
                            .export_menu
                            .set_status(format!("Saved project: {}", path.display()));
                        println!("[gui] saved project: {}", path.display());
                    }
                    Err(err) => {
                        self.state
                            .export_menu
                            .set_status(format!("Save failed: {err}"));
                    }
                }
                Ok(true)
            }
            PendingAppAction::LoadProject => {
                let Some(path) = pick_load_project_path() else {
                    self.state.export_menu.set_status("Load canceled");
                    return Ok(true);
                };
                match load_project_file(path.as_path(), self.panel_width, self.renderer.height()) {
                    Ok(loaded) => {
                        let warning_count = loaded.warnings.len();
                        self.project = loaded.project;
                        self.state = PreviewState::new(&self.config);
                        self.state.invalidation.invalidate_all();
                        self.start_export_requested = false;
                        let _ = self.stop_export_session("stopped");
                        self.state
                            .export_menu
                            .set_status(load_status_message(path.as_path(), warning_count));
                        println!("[gui] loaded project: {}", path.display());
                        log_project_load_warnings(path.as_path(), &loaded.warnings);
                    }
                    Err(err) => {
                        self.state
                            .export_menu
                            .set_status(format!("Load failed: {err}"));
                    }
                }
                Ok(true)
            }
            PendingAppAction::StartExport => {
                self.start_export_requested = true;
                self.state.export_menu.set_status("Preparing export...");
                self.state.paused = true;
                Ok(true)
            }
            PendingAppAction::StopExport => Ok(self.stop_export_session("stopped by user")),
            PendingAppAction::ResetFeedback {
                feedback_node_id,
                accumulation_texture_node_id,
            } => {
                let cleared = self
                    .renderer
                    .reset_feedback_history(feedback_node_id, accumulation_texture_node_id);
                if cleared {
                    self.state.export_menu.set_status(format!(
                        "Reset feedback history for node #{feedback_node_id}"
                    ));
                } else {
                    self.state.export_menu.set_status(format!(
                        "Feedback history already clear for node #{feedback_node_id}"
                    ));
                }
                self.state.invalidation.invalidate_overlays();
                Ok(true)
            }
            PendingAppAction::Exit => {
                self.close_requested = true;
                Ok(true)
            }
        }
    }

    fn try_start_export_from_request(&mut self) -> Result<(), Box<dyn Error>> {
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

    fn capture_export_frame(&mut self) -> Result<(), Box<dyn Error>> {
        let (width, height) = match self
            .renderer
            .capture_tex_preview_bgra(&mut self.export_bgra_scratch)
        {
            Ok(Some(size)) => size,
            Ok(None) => {
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

    fn stop_export_session(&mut self, reason: &str) -> bool {
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

    fn update_title(&mut self, now: Instant) {
        if now < self.title_deadline {
            return;
        }
        self.title_deadline = now + Duration::from_millis(250);
        let paused = if self.state.paused {
            "paused"
        } else {
            "running"
        };
        let title = format!(
            "covergen graph | {} | viewport={}x{} | target={}x{} | nodes={} | frame={} | {:.1} fps | {}",
            self.project.name,
            self.renderer.width(),
            self.renderer.height(),
            self.project.preview_width,
            self.project.preview_height,
            self.project.node_count(),
            self.state.frame_index,
            self.state.avg_fps,
            paused
        );
        if title != self.last_title {
            self.window.set_title(&title);
            self.last_title = title;
        }
    }

    fn apply_synthetic_drag(&mut self) -> bool {
        let Some(node_id) = self.benchmark_node else {
            return false;
        };
        let phase = self.frame_counter as f32 / GUI_LOCKED_FPS as f32;
        let cx = (self.panel_width as f32 * 0.5) as i32;
        let cy = (self.renderer.height() as f32 * 0.5) as i32;
        let x = cx + (phase * 2.7).sin().mul_add(120.0, 0.0) as i32;
        let y = cy + (phase * 1.9).cos().mul_add(90.0, 0.0) as i32;
        let changed =
            self.project
                .move_node(node_id, x, y, self.panel_width, self.renderer.height());
        self.run_synthetic_interaction_queries(x, y);
        changed
    }

    /// Emit deterministic hit-test style workload so CI can gate scan regressions.
    fn run_synthetic_interaction_queries(&self, x: i32, y: i32) {
        let max_x = self.panel_width.saturating_sub(1) as i32;
        let max_y = self.renderer.height().saturating_sub(1) as i32;
        let sample_x = x.clamp(0, max_x);
        let sample_y = y.clamp(0, max_y);
        let _ = self.project.node_at(sample_x, sample_y);
        let _ = self
            .project
            .node_at((sample_x + NODE_WIDTH / 2).clamp(0, max_x), sample_y);
        let _ = self.project.output_pin_at(
            (sample_x + NODE_WIDTH + 8).clamp(0, max_x),
            sample_y + 12,
            12,
        );
        let _ = self
            .project
            .input_pin_at((sample_x - 8).clamp(0, max_x), sample_y + 12, 12, None);
        let _ = self.project.node_ids_overlapping_graph_rect(
            sample_x - NODE_WIDTH,
            sample_y - 90,
            sample_x + NODE_WIDTH,
            sample_y + 90,
        );
    }

    fn with_perf_trace(mut self) -> Self {
        self.perf = GuiPerfRecorder::new(self.config.gui.perf_trace.clone());
        self
    }

    fn apply_panel_resize_input(&mut self, input: &InputSnapshot) -> (bool, bool) {
        let mut changed = false;
        let mut consumed = false;
        if input.left_clicked && self.try_begin_panel_resize(input.mouse_pos) {
            consumed = true;
        }
        let Some(drag) = self.panel_resize_drag else {
            return (changed, consumed);
        };
        consumed = true;
        if !input.left_down {
            self.panel_resize_drag = None;
            self.update_resize_cursor(input.mouse_pos);
            return (changed, consumed);
        }
        let Some((mx, _)) = input.mouse_pos else {
            return (changed, consumed);
        };
        let requested = (mx - drag.grab_offset_px + 1).max(1) as usize;
        let next_width = clamp_panel_width(requested, self.renderer.width());
        if next_width != self.panel_width {
            self.panel_width = next_width;
            changed = true;
        }
        (changed, consumed)
    }

    fn try_begin_panel_resize(&mut self, mouse_pos: Option<(i32, i32)>) -> bool {
        let Some((mx, my)) = mouse_pos else {
            return false;
        };
        if !on_panel_divider(mx, my, self.panel_width, self.renderer.height()) {
            return false;
        }
        let divider_x = self.panel_width as i32 - 1;
        self.panel_resize_drag = Some(PanelResizeDrag {
            grab_offset_px: mx - divider_x,
        });
        self.state.drag = None;
        self.state.wire_drag = None;
        self.state.hover_param_target = None;
        self.state.hover_param = None;
        self.state.hover_insert_link = None;
        true
    }

    fn update_resize_cursor(&mut self, mouse_pos: Option<(i32, i32)>) {
        let resize_active = self.panel_resize_drag.is_some()
            || mouse_pos
                .map(|(mx, my)| on_panel_divider(mx, my, self.panel_width, self.renderer.height()))
                .unwrap_or(false);
        if resize_active == self.resize_cursor_active {
            return;
        }
        self.resize_cursor_active = resize_active;
        let icon = if resize_active {
            CursorIcon::EwResize
        } else {
            CursorIcon::Default
        };
        self.window.set_cursor_icon(icon);
    }

    fn sync_timeline_audio_preview(&mut self, timeline_total_frames: u32) {
        self.timeline_audio.sync(
            &self.state.export_menu,
            self.state.paused,
            self.state.frame_index,
            timeline_total_frames,
            self.config.animation.fps,
        );
    }

    fn update_loop_policy(&mut self) {
        self.continuous_redraw = !self.state.paused
            || state_has_transient_ui(&self.state)
            || self.panel_resize_drag.is_some()
            || self.export_session.is_some()
            || self.start_export_requested;
        if self.config.gui.benchmark_drag || self.benchmark_frame_limit.is_some() {
            self.continuous_redraw = true;
        }
    }
}

fn state_has_transient_ui(state: &PreviewState) -> bool {
    state.drag.is_some()
        || state.wire_drag.is_some()
        || state.link_cut.is_some()
        || state.pan_drag.is_some()
        || state.right_marquee.is_some()
        || state.timeline_scrub_active
        || state.timeline_volume_drag_active
        || state.param_edit.is_some()
        || state.param_dropdown.is_some()
        || state.menu.open
        || state.main_menu.open
        || state.export_menu.open
        || state.export_menu.exporting
}

fn maybe_seed_benchmark_nodes(
    config: &V2Config,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
) -> Option<u32> {
    if !config.gui.benchmark_drag {
        return None;
    }
    if project.node_count() > 0 {
        return project.nodes().first().map(|node| node.id());
    }
    let source = project.add_node(ProjectNodeKind::TexSolid, 24, 32, panel_width, panel_height);
    let mut previous = source;
    let mut drag_target = source;
    let mut chain = Vec::new();
    for index in 0..10 {
        let kind = if index == 4 {
            ProjectNodeKind::TexFeedback
        } else {
            ProjectNodeKind::TexTransform2D
        };
        let x = if index % 2 == 0 { 188 } else { 24 };
        let y = 96 + index * 64;
        let node_id = project.add_node(kind, x, y, panel_width, panel_height);
        let _ = project.connect_image_link(previous, node_id);
        if index == 6 {
            drag_target = node_id;
        }
        chain.push(node_id);
        previous = node_id;
    }
    let lfo = project.add_node(
        ProjectNodeKind::CtlLfo,
        24,
        96 + 10 * 64,
        panel_width,
        panel_height,
    );
    for (index, node_id) in chain.iter().copied().enumerate() {
        if index % 3 == 0 {
            let _ = project.connect_signal_link_to_param(lfo, node_id, 0);
        }
    }
    let _out = project.add_node(
        ProjectNodeKind::IoWindowOut,
        188,
        96 + 11 * 64,
        panel_width,
        panel_height,
    );
    let _ = project.connect_image_link(previous, _out);
    Some(drag_target)
}

fn benchmark_frame_limit(config: &V2Config) -> Option<u64> {
    match config.gui.benchmark_frames {
        0 => None,
        frames => Some(frames as u64),
    }
}

fn is_benchmark_mode(config: &V2Config) -> bool {
    config.gui.benchmark_drag || config.gui.benchmark_frames > 0
}

fn is_wav_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("wav"))
        .unwrap_or(false)
}

/// Return the process working directory, or `.` when unavailable.
fn working_directory() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Return autosave file path in one base directory.
fn autosave_project_path_in(base_dir: &Path) -> PathBuf {
    base_dir.join(GUI_PROJECT_AUTOSAVE_FILE)
}

/// Return autosave file path in the process working directory.
fn autosave_project_path() -> PathBuf {
    autosave_project_path_in(working_directory().as_path())
}

/// Return explicit save/load project path in one base directory.
#[cfg(test)]
fn manual_project_path_in(base_dir: &Path) -> PathBuf {
    base_dir.join(GUI_PROJECT_SAVE_FILE)
}

/// Return native picker initial directory for manual project save/load.
fn project_picker_directory() -> PathBuf {
    working_directory()
}

/// Open one native save-file picker for GUI projects.
fn pick_save_project_path() -> Option<PathBuf> {
    FileDialog::new()
        .set_title("Save Project")
        .set_directory(project_picker_directory())
        .set_file_name(GUI_PROJECT_SAVE_FILE)
        .add_filter("Covergen Project", &["json"])
        .save_file()
}

/// Open one native open-file picker for GUI projects.
fn pick_load_project_path() -> Option<PathBuf> {
    FileDialog::new()
        .set_title("Load Project")
        .set_directory(project_picker_directory())
        .add_filter("Covergen Project", &["json"])
        .pick_file()
}

/// Return legacy hidden project path used by older GUI builds.
#[cfg(test)]
fn legacy_manual_project_path_in(base_dir: &Path) -> PathBuf {
    base_dir.join(GUI_PROJECT_SAVE_FILE_LEGACY)
}

/// Return ordered project-load candidates for one base directory.
#[cfg(test)]
fn manual_project_load_candidates_in(base_dir: &Path) -> [PathBuf; 3] {
    [
        manual_project_path_in(base_dir),
        legacy_manual_project_path_in(base_dir),
        autosave_project_path_in(base_dir),
    ]
}

/// Load autosaved GUI graph if present.
fn load_autosaved_project(
    panel_width: usize,
    panel_height: usize,
) -> Result<Option<PersistedProjectLoadOutcome>, Box<dyn Error>> {
    let path = autosave_project_path();
    load_autosaved_project_from_path(path.as_path(), panel_width, panel_height)
}

/// Load one autosave project path, quarantining malformed/corrupt files.
fn load_autosaved_project_from_path(
    path: &Path,
    panel_width: usize,
    panel_height: usize,
) -> Result<Option<PersistedProjectLoadOutcome>, Box<dyn Error>> {
    match load_project_file_if_exists(path, panel_width, panel_height) {
        Ok(project) => Ok(project),
        Err(load_err) => {
            if !path.exists() || !should_quarantine_autosave_load_error(load_err.as_ref()) {
                return Err(load_err);
            }
            let quarantined = quarantine_corrupt_autosave(path)?;
            telemetry::record_counter_u64("gui.project.autosave_quarantined", 1);
            eprintln!(
                "[gui] quarantined corrupt autosave {} -> {} ({load_err})",
                path.display(),
                quarantined.display()
            );
            Ok(None)
        }
    }
}

/// Load the first existing project candidate from one directory.
#[cfg(test)]
fn load_manual_project_from_dir(
    base_dir: &Path,
    panel_width: usize,
    panel_height: usize,
) -> Result<Option<(PersistedProjectLoadOutcome, PathBuf)>, Box<dyn Error>> {
    for path in manual_project_load_candidates_in(base_dir) {
        match load_project_file_if_exists(path.as_path(), panel_width, panel_height) {
            Ok(Some(project)) => return Ok(Some((project, path))),
            Ok(None) => continue,
            Err(err) => {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidData,
                    format!("failed to load {}: {err}", path.display()),
                )
                .into());
            }
        }
    }
    Ok(None)
}

fn load_project_file_if_exists(
    path: &Path,
    panel_width: usize,
    panel_height: usize,
) -> Result<Option<PersistedProjectLoadOutcome>, Box<dyn Error>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(Box::new(err)),
    };
    let persisted = serde_json::from_slice::<PersistedGuiProject>(bytes.as_slice())?;
    let project = GuiProject::from_persisted_with_warnings(persisted, panel_width, panel_height)?;
    Ok(Some(project))
}

/// Load one explicit GUI project file path.
fn load_project_file(
    path: &Path,
    panel_width: usize,
    panel_height: usize,
) -> Result<PersistedProjectLoadOutcome, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let persisted = serde_json::from_slice::<PersistedGuiProject>(bytes.as_slice())?;
    Ok(GuiProject::from_persisted_with_warnings(
        persisted,
        panel_width,
        panel_height,
    )?)
}

/// Move one malformed autosave to a timestamped quarantine path.
fn quarantine_corrupt_autosave(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("covergen_gui_autosave");
    let quarantined = path.with_file_name(format!("{file_name}.corrupt-{timestamp}"));
    fs::rename(path, quarantined.as_path())?;
    Ok(quarantined)
}

/// Return true when one autosave load error indicates corrupt file contents.
fn should_quarantine_autosave_load_error(err: &(dyn Error + 'static)) -> bool {
    if err.is::<serde_json::Error>() || err.is::<PersistedProjectLoadError>() {
        return true;
    }
    err.downcast_ref::<std::io::Error>()
        .map(|io_err| io_err.kind() == ErrorKind::InvalidData)
        .unwrap_or(false)
}

/// Format one status line after loading a project from disk.
fn load_status_message(path: &Path, warning_count: usize) -> String {
    if warning_count == 0 {
        return format!("Loaded project: {}", path.display());
    }
    format!(
        "Loaded project: {} ({} dropped unknown params; see log)",
        path.display(),
        warning_count
    )
}

/// Emit non-fatal persisted-load warnings with actionable context.
fn log_project_load_warnings(path: &Path, warnings: &[PersistedProjectLoadWarning]) {
    if warnings.is_empty() {
        return;
    }
    eprintln!(
        "[gui] load warnings for {}: {} dropped unknown persisted params",
        path.display(),
        warnings.len()
    );
    for warning in warnings {
        eprintln!("[gui]   - {warning}");
    }
}

/// Save current GUI graph to autosave file atomically.
fn save_autosaved_project(project: &GuiProject) -> Result<(), Box<dyn Error>> {
    let path = autosave_project_path();
    save_project_file(project, path.as_path())
}

fn save_project_file(project: &GuiProject, path: &Path) -> Result<(), Box<dyn Error>> {
    let tmp = path.with_extension("tmp");
    let data = serde_json::to_vec_pretty(&project.to_persisted())?;
    fs::write(tmp.as_path(), data)?;
    let result = commit_saved_project_file(tmp.as_path(), path);
    if result.is_err() {
        let _ = fs::remove_file(tmp.as_path());
    }
    result
}

/// Commit one tmp project save file into the destination path.
///
/// This prefers direct rename (atomic replace on Unix). When direct rename
/// fails on platforms that do not replace existing files, it moves the
/// previous destination to a backup path and restores it on commit failure.
fn commit_saved_project_file(tmp: &Path, dst: &Path) -> Result<(), Box<dyn Error>> {
    if let Ok(meta) = fs::metadata(dst) {
        if !meta.is_file() {
            return Err(format!(
                "project save destination must be a file path: {}",
                dst.display()
            )
            .into());
        }
    }

    match fs::rename(tmp, dst) {
        Ok(()) => return Ok(()),
        Err(err) if !dst.exists() => {
            return Err(format!(
                "failed to finalize project save to {}: {err}",
                dst.display()
            )
            .into())
        }
        Err(_) => {}
    }

    let backup = dst.with_extension("bak");
    if backup.exists() {
        fs::remove_file(backup.as_path())?;
    }
    fs::rename(dst, backup.as_path())?;
    match fs::rename(tmp, dst) {
        Ok(()) => {
            let _ = fs::remove_file(backup.as_path());
            Ok(())
        }
        Err(err) => {
            let restore_result = fs::rename(backup.as_path(), dst);
            if let Err(restore_err) = restore_result {
                return Err(format!(
                    "failed to finalize project save to {}: {err}; failed to restore previous file: {restore_err}",
                    dst.display()
                )
                .into());
            }
            Err(format!(
                "failed to finalize project save to {}: {err}; previous file restored",
                dst.display()
            )
            .into())
        }
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

fn frame_budget(target_fps: u32) -> Duration {
    Duration::from_secs_f64(1.0 / target_fps.max(1) as f64)
}

fn clamp_panel_width(requested: usize, viewport_width: usize) -> usize {
    if viewport_width <= 1 {
        return 1;
    }
    let hard_max = viewport_width - 1;
    let min_width = MIN_PANEL_WIDTH.min(hard_max);
    let max_width = hard_max.saturating_sub(MIN_PREVIEW_WIDTH).max(min_width);
    requested.clamp(min_width, max_width)
}

/// Return initial editor-panel width so the right preview starts near 1/3.
fn launch_panel_width(viewport_width: usize) -> usize {
    viewport_width.saturating_mul(2) / 3
}

fn on_panel_divider(mx: i32, my: i32, panel_width: usize, panel_height: usize) -> bool {
    let editor_h = editor_panel_height(panel_height) as i32;
    if my < 0 || my >= editor_h {
        return false;
    }
    let divider_x = panel_width as i32 - 1;
    (mx - divider_x).abs() <= DIVIDER_HIT_SLOP_PX
}

fn smoothed_fps(previous: f32, frame_elapsed: Duration) -> f32 {
    let inst = 1.0 / frame_elapsed.as_secs_f32().max(1e-4);
    if previous <= 0.0 {
        return inst;
    }
    previous * 0.9 + inst * 0.1
}

fn input_has_project_mutation_intent(input: &InputSnapshot) -> bool {
    input.left_down
        || input.left_clicked
        || input.right_down
        || input.right_clicked
        || input.middle_down
        || input.middle_clicked
        || input.alt_down
        || input.shift_down
        || input.wheel_lines_y.abs() > f32::EPSILON
        || input.toggle_pause
        || input.new_project
        || input.focus_all
        || input.open_help
        || input.toggle_node_open
        || input.toggle_add_menu
        || input.toggle_main_menu
        || input.menu_up
        || input.menu_down
        || input.param_dec
        || input.param_inc
        || input.menu_accept
        || !input.typed_text.is_empty()
        || input.param_backspace
        || input.param_delete
        || input.param_select_all
        || input.param_commit
        || input.param_cancel
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("covergen_gui_app_{test_name}_{nanos}"));
        fs::create_dir_all(dir.as_path()).expect("create temp dir");
        dir
    }

    #[test]
    fn manual_project_load_candidates_prioritize_explicit_then_legacy_then_autosave() {
        let base_dir = Path::new("workspace");
        let candidates = manual_project_load_candidates_in(base_dir);
        assert_eq!(candidates[0], base_dir.join(GUI_PROJECT_SAVE_FILE));
        assert_eq!(candidates[1], base_dir.join(GUI_PROJECT_SAVE_FILE_LEGACY));
        assert_eq!(candidates[2], base_dir.join(GUI_PROJECT_AUTOSAVE_FILE));
    }

    #[test]
    fn load_manual_project_uses_legacy_file_when_explicit_missing() {
        let dir = temp_dir("legacy_fallback");
        let path = legacy_manual_project_path_in(dir.as_path());
        let project = GuiProject::new_empty(512, 288);
        save_project_file(&project, path.as_path()).expect("save legacy project");

        let (loaded, loaded_path) = load_manual_project_from_dir(dir.as_path(), 640, 480)
            .expect("load project")
            .expect("legacy fallback should return project");

        assert_eq!(loaded_path, path);
        assert_eq!(loaded.project.to_persisted().preview_width, 512);
        assert!(loaded.warnings.is_empty());

        let _ = fs::remove_dir_all(dir.as_path());
    }

    #[test]
    fn load_manual_project_prefers_explicit_file_over_legacy() {
        let dir = temp_dir("explicit_priority");
        let explicit = manual_project_path_in(dir.as_path());
        let legacy = legacy_manual_project_path_in(dir.as_path());
        let explicit_project = GuiProject::new_empty(1024, 576);
        let legacy_project = GuiProject::new_empty(320, 180);
        save_project_file(&legacy_project, legacy.as_path()).expect("save legacy project");
        save_project_file(&explicit_project, explicit.as_path()).expect("save explicit project");

        let (loaded, loaded_path) = load_manual_project_from_dir(dir.as_path(), 640, 480)
            .expect("load project")
            .expect("explicit project should return project");

        assert_eq!(loaded_path, explicit);
        assert_eq!(loaded.project.to_persisted().preview_width, 1024);
        assert!(loaded.warnings.is_empty());

        let _ = fs::remove_dir_all(dir.as_path());
    }

    #[test]
    fn load_autosaved_project_quarantines_corrupt_payload() {
        let dir = temp_dir("autosave_corrupt_quarantine");
        let autosave = autosave_project_path_in(dir.as_path());
        fs::write(autosave.as_path(), b"{not-valid-json").expect("write corrupt autosave");

        let loaded =
            load_autosaved_project_from_path(autosave.as_path(), 640, 480).expect("load autosave");
        assert!(
            loaded.is_none(),
            "corrupt autosave should be quarantined and treated as missing"
        );
        assert!(
            !autosave.exists(),
            "autosave path should be moved away after quarantine"
        );
        let mut quarantined_count = 0usize;
        for entry in fs::read_dir(dir.as_path()).expect("read temp dir") {
            let Ok(entry) = entry else {
                continue;
            };
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.starts_with(&format!("{GUI_PROJECT_AUTOSAVE_FILE}.corrupt-")) {
                quarantined_count = quarantined_count.saturating_add(1);
            }
        }
        assert_eq!(
            quarantined_count, 1,
            "exactly one quarantined autosave copy should be created"
        );

        let _ = fs::remove_dir_all(dir.as_path());
    }

    #[test]
    fn load_autosaved_project_does_not_quarantine_non_corrupt_io_errors() {
        let dir = temp_dir("autosave_non_corrupt_io_error");
        let autosave_dir = autosave_project_path_in(dir.as_path());
        fs::create_dir_all(autosave_dir.as_path()).expect("create autosave directory path");

        let result = load_autosaved_project_from_path(autosave_dir.as_path(), 640, 480);
        assert!(
            result.is_err(),
            "directory read failure should propagate as IO error"
        );
        assert!(
            autosave_dir.exists() && autosave_dir.is_dir(),
            "non-corrupt IO errors should not quarantine autosave path"
        );
        let corrupt_files = fs::read_dir(dir.as_path())
            .expect("read temp dir")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .map(|name| name.contains(".corrupt-"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            corrupt_files, 0,
            "non-corrupt IO failures should not create quarantine artifacts"
        );

        let _ = fs::remove_dir_all(dir.as_path());
    }

    #[test]
    fn bundled_circle_noise_feedback_example_loads() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/graphs/circle_noise_feedback_trail.json");
        let loaded = load_project_file(path.as_path(), 1280, 720).expect("load example project");
        assert!(loaded.warnings.is_empty());
        assert!(
            loaded
                .project
                .nodes()
                .iter()
                .any(|node| node.kind().stable_id() == "tex.feedback"),
            "example graph should include tex.feedback"
        );
        let circle_id = loaded
            .project
            .nodes()
            .iter()
            .find(|node| node.kind().stable_id() == "tex.circle")
            .map(|node| node.id())
            .expect("example graph should include tex.circle");
        let blend_id = loaded
            .project
            .nodes()
            .iter()
            .find(|node| node.kind().stable_id() == "tex.blend")
            .map(|node| node.id())
            .expect("example graph should include tex.blend");
        let blend_tex_param = loaded
            .project
            .node_param_slot_index(blend_id, "blend_tex")
            .expect("tex.blend should expose blend_tex");
        assert_eq!(
            loaded
                .project
                .texture_source_for_param(blend_id, blend_tex_param),
            Some(circle_id),
            "trail example should composite raw circle as the live layer"
        );
    }

    #[test]
    fn save_project_file_cleans_tmp_when_destination_is_invalid() {
        let dir = temp_dir("save_invalid_destination");
        let invalid_destination = dir.join("project.json");
        fs::create_dir_all(invalid_destination.as_path()).expect("create invalid destination dir");

        let project = GuiProject::new_empty(320, 240);
        let result = save_project_file(&project, invalid_destination.as_path());
        assert!(
            result.is_err(),
            "save should fail when destination is a directory"
        );
        assert!(
            !invalid_destination.with_extension("tmp").exists(),
            "failed save should not leave tmp files behind"
        );

        let _ = fs::remove_dir_all(dir.as_path());
    }

    #[test]
    fn save_project_file_overwrite_does_not_leave_backup_artifacts() {
        let dir = temp_dir("save_overwrite_backup_cleanup");
        let path = dir.join("graph.json");
        let project = GuiProject::new_empty(320, 240);
        let updated_project = GuiProject::new_empty(640, 360);

        save_project_file(&project, path.as_path()).expect("initial save should succeed");
        save_project_file(&updated_project, path.as_path()).expect("overwrite save should succeed");

        assert!(
            !path.with_extension("bak").exists(),
            "successful overwrite should not leave backup artifacts"
        );

        let _ = fs::remove_dir_all(dir.as_path());
    }
}

//! GUI application state and frame orchestration.

use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorIcon, Fullscreen, Window};

use crate::runtime_config::V2Config;
use crate::telemetry;
use crate::{animation::RawVideoEncoder, animation::StreamFrameFormat};

use super::input::InputCollector;
use super::interaction::{apply_preview_actions, step_timeline_if_running};
use super::perf::GuiPerfRecorder;
use super::project::{GuiProject, PersistedGuiProject, ProjectNodeKind};
use super::renderer::GuiRenderer;
use super::scene::SceneBuilder;
use super::state::{InputSnapshot, PendingAppAction, PreviewState};
use super::timeline::{editor_panel_height, TIMELINE_TOTAL_FRAMES};
use super::top_view::TopViewerGenerator;

const MIN_PANEL_WIDTH: usize = 260;
const MIN_PREVIEW_WIDTH: usize = 320;
const DIVIDER_HIT_SLOP_PX: i32 = 6;
const GUI_LOCKED_FPS: u32 = 60;
const GUI_PROJECT_AUTOSAVE_FILE: &str = ".covergen_gui_graph.json";
const GUI_PROJECT_SAVE_FILE: &str = ".covergen_gui_project.json";

/// Active export session metadata for GUI H.264 streaming.
struct GuiExportSession {
    encoder: RawVideoEncoder,
    next_frame: u32,
    total_frames: u32,
    restore_paused: bool,
    output_path: PathBuf,
}

/// Active divider drag metadata for panel resizing.
#[derive(Clone, Copy, Debug)]
struct PanelResizeDrag {
    grab_offset_px: i32,
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
    top_view: TopViewerGenerator,
    perf: GuiPerfRecorder,
    frame_budget: Duration,
    frame_deadline: Instant,
    last_frame_start: Instant,
    frame_counter: u64,
    benchmark_node: Option<u32>,
    export_session: Option<GuiExportSession>,
    start_export_requested: bool,
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
    pub(crate) async fn new(
        config: V2Config,
        panel_width: usize,
        window: Arc<Window>,
    ) -> Result<Self, Box<dyn Error>> {
        let renderer = GuiRenderer::new(window.clone(), config.gui.vsync).await?;
        let panel_width = clamp_panel_width(panel_width, renderer.width());
        let mut project = match load_autosaved_project(panel_width, renderer.height()) {
            Ok(Some(project)) => {
                println!(
                    "[gui] loaded autosave from {}",
                    autosave_project_path().display()
                );
                project
            }
            Ok(None) => GuiProject::new_empty(config.width, config.height),
            Err(err) => {
                eprintln!("[gui] failed to load autosave: {err}");
                GuiProject::new_empty(config.width, config.height)
            }
        };
        let benchmark_node =
            maybe_seed_benchmark_nodes(&config, &mut project, panel_width, renderer.height());
        let state = PreviewState::new(&config);
        let frame_budget = frame_budget(GUI_LOCKED_FPS);
        let now = Instant::now();
        println!(
            "[gui] {}x{} @ {}hz locked ({:?})",
            renderer.width(),
            renderer.height(),
            GUI_LOCKED_FPS,
            config.gui.vsync
        );
        println!(
            "[gui] controls: Esc=quit, F11=fullscreen, Space=add node menu, `=main menu, Tab=open node, RMB=select, RMB drag=marquee, RMB on bound param value=unbind, Delete=remove selected, Toggle box=expand/collapse, Arrows=param select/adjust, Alt+LMB drag=cut links, P=pause, timeline(play/pause + scrub)"
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
            top_view: TopViewerGenerator::default(),
            perf: GuiPerfRecorder::new(None),
            frame_budget,
            frame_deadline: now,
            last_frame_start: now,
            frame_counter: 0,
            benchmark_node,
            export_session: None,
            start_export_requested: false,
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
        let (resize_changed, consume_editor_input) = self.apply_panel_resize_input(&snapshot);
        let mut scene_dirty = resize_changed;
        if consume_editor_input {
            self.state.drag = None;
            self.state.wire_drag = None;
            self.state.hover_param_target = None;
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
        if self.start_export_requested && self.export_session.is_none() {
            if self.state.frame_index != 0 {
                self.state.frame_index = 0;
                scene_dirty = true;
            }
            self.state.paused = true;
        }
        if let Some(session) = self.export_session.as_ref() {
            self.state.paused = true;
            if self.state.frame_index != session.next_frame {
                self.state.frame_index = session.next_frame;
                scene_dirty = true;
            }
        }
        if self.config.gui.benchmark_drag {
            scene_dirty |= self.apply_synthetic_drag();
        }
        if self.export_session.is_none() && !self.start_export_requested {
            scene_dirty |=
                step_timeline_if_running(&mut self.state, frame_delta, self.config.animation.fps);
        }
        self.state.avg_fps = smoothed_fps(self.state.avg_fps, frame_delta);
        let update_elapsed = update_start.elapsed();
        let hit_test_scans = self.project.take_hit_test_scan_count();

        let mut scene_elapsed = Duration::ZERO;
        let mut render_elapsed = Duration::ZERO;
        let mut submit_count = 0u32;
        let mut upload_bytes = 0u64;
        let mut ui_alloc_bytes = 0u64;
        if scene_dirty || self.needs_redraw {
            self.top_view.update(
                &self.project,
                self.renderer.width(),
                self.renderer.height(),
                self.panel_width,
                self.state.frame_index,
                self.config.animation.fps,
            );
            self.try_start_export_from_request()?;
            let scene_start = Instant::now();
            let frame = self.scene.build(
                &self.project,
                &self.state,
                self.renderer.width(),
                self.renderer.height(),
                self.panel_width,
            );
            scene_elapsed = scene_start.elapsed();
            ui_alloc_bytes = frame.ui_alloc_bytes;

            let render_start = Instant::now();
            self.renderer.render(
                frame,
                self.top_view.frame(),
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
            ui_alloc_bytes,
        );
        self.update_loop_policy();
        self.update_title(frame_start);
        self.needs_redraw = false;
        self.frame_counter = self.frame_counter.wrapping_add(1);
        Ok(())
    }

    /// Flush trace output before event-loop shutdown.
    pub(crate) fn shutdown(&mut self) -> Result<(), Box<dyn Error>> {
        let _ = self.stop_export_session("stopped");
        save_autosaved_project(&self.project)?;
        self.perf.flush()
    }

    fn handle_pending_app_actions(&mut self) -> Result<bool, Box<dyn Error>> {
        let Some(action) = self.state.pending_app_action.take() else {
            return Ok(false);
        };
        match action {
            PendingAppAction::SaveProject => {
                let path = manual_project_path();
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
                let path = manual_project_path();
                match load_project_file(path.as_path(), self.panel_width, self.renderer.height()) {
                    Ok(loaded) => {
                        self.project = loaded;
                        self.state = PreviewState::new(&self.config);
                        self.start_export_requested = false;
                        let _ = self.stop_export_session("stopped");
                        println!("[gui] loaded project: {}", path.display());
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
        let Some(frame) = self.top_view.frame() else {
            self.state
                .export_menu
                .set_status("Export failed: preview output unavailable");
            self.start_export_requested = false;
            return Ok(());
        };
        let output_path = self.state.export_menu.output_path();
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
            total_frames: TIMELINE_TOTAL_FRAMES,
            restore_paused: self.state.paused,
            output_path: output_path.clone(),
        });
        self.state.export_menu.exporting = true;
        self.state.export_menu.preview_frame = 0;
        self.state.export_menu.preview_total = TIMELINE_TOTAL_FRAMES;
        self.state
            .export_menu
            .set_status(format!("Exporting: {}", output_path.display()));
        self.start_export_requested = false;
        Ok(())
    }

    fn capture_export_frame(&mut self) -> Result<(), Box<dyn Error>> {
        let (width, height) = match self
            .renderer
            .capture_top_preview_bgra(&mut self.export_bgra_scratch)
        {
            Ok(Some(size)) => size,
            Ok(None) => {
                self.stop_export_session("failed");
                self.state
                    .export_menu
                    .set_status("Export failed: preview texture unavailable");
                return Ok(());
            }
            Err(err) => {
                self.stop_export_session("failed");
                self.state
                    .export_menu
                    .set_status(format!("Export failed: {err}"));
                return Ok(());
            }
        };

        let Some(session) = self.export_session.as_mut() else {
            return Ok(());
        };
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
            return Ok(());
        }
        session.next_frame = session.next_frame.saturating_add(1);
        self.state.export_menu.preview_frame = session.next_frame.min(session.total_frames);
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
        match session.encoder.finish() {
            Ok(()) => {
                self.state.export_menu.set_status(format!(
                    "Export {reason}: {}",
                    session.output_path.display()
                ));
            }
            Err(err) => {
                self.state
                    .export_menu
                    .set_status(format!("Export failed: {err}"));
            }
        }
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
        self.project
            .move_node(node_id, x, y, self.panel_width, self.renderer.height())
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

    fn update_loop_policy(&mut self) {
        self.continuous_redraw = !self.state.paused
            || state_has_transient_ui(&self.state)
            || self.panel_resize_drag.is_some()
            || self.export_session.is_some()
            || self.start_export_requested;
        if self.config.gui.benchmark_drag {
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
    let top = project.add_node(
        ProjectNodeKind::TexSolid,
        120,
        120,
        panel_width,
        panel_height,
    );
    let _out = project.add_node(
        ProjectNodeKind::IoWindowOut,
        280,
        220,
        panel_width,
        panel_height,
    );
    Some(top)
}

/// Return autosave file path in the process working directory.
fn autosave_project_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(GUI_PROJECT_AUTOSAVE_FILE)
}

/// Return default explicit save/load project path in working directory.
fn manual_project_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(GUI_PROJECT_SAVE_FILE)
}

/// Load autosaved GUI graph if present.
fn load_autosaved_project(
    panel_width: usize,
    panel_height: usize,
) -> Result<Option<GuiProject>, Box<dyn Error>> {
    let path = autosave_project_path();
    load_project_file_if_exists(path.as_path(), panel_width, panel_height)
}

fn load_project_file(
    path: &Path,
    panel_width: usize,
    panel_height: usize,
) -> Result<GuiProject, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let persisted = serde_json::from_slice::<PersistedGuiProject>(bytes.as_slice())?;
    Ok(GuiProject::from_persisted(
        persisted,
        panel_width,
        panel_height,
    )?)
}

fn load_project_file_if_exists(
    path: &Path,
    panel_width: usize,
    panel_height: usize,
) -> Result<Option<GuiProject>, Box<dyn Error>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(Box::new(err)),
    };
    let persisted = serde_json::from_slice::<PersistedGuiProject>(bytes.as_slice())?;
    let project = GuiProject::from_persisted(persisted, panel_width, panel_height)?;
    Ok(Some(project))
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
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(tmp.as_path(), path)?;
    Ok(())
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

//! GUI application state and frame orchestration.

mod export_session;
mod frame_loop;
mod lifecycle;
mod panel_resize;
mod project_io;

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
use super::tex_view::{TexViewerGenerator, TexViewerUpdate};
use super::timeline::{clamp_frame, editor_panel_height};
use export_session::GuiExportSession;
use panel_resize::{clamp_panel_width, launch_panel_width, PanelResizeDrag};
use project_io::{
    autosave_project_path, is_wav_path, load_autosaved_project, load_project_file,
    load_status_message, log_project_load_warnings, pick_load_project_path, pick_save_project_path,
    save_autosaved_project, save_project_file,
};

const MIN_PANEL_WIDTH: usize = 260;
const MIN_PREVIEW_WIDTH: usize = 320;
const DIVIDER_HIT_SLOP_PX: i32 = 6;
const GUI_LOCKED_FPS: u32 = 60;
const GUI_PROJECT_AUTOSAVE_FILE: &str = ".covergen_gui_graph.json";
const GUI_PROJECT_SAVE_FILE: &str = "covergen_gui_project.json";
#[cfg(test)]
const GUI_PROJECT_SAVE_FILE_LEGACY: &str = ".covergen_gui_project.json";

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
    let output = project.add_node(
        ProjectNodeKind::IoWindowOut,
        188,
        96 + 11 * 64,
        panel_width,
        panel_height,
    );
    let _ = project.connect_image_link(previous, output);
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

/// Return the process working directory, or `.` when unavailable.
fn working_directory() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn frame_budget(target_fps: u32) -> Duration {
    Duration::from_secs_f64(1.0 / target_fps.max(1) as f64)
}

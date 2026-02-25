//! GUI application state and frame orchestration.

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::event::WindowEvent;
use winit::window::Window;

use crate::runtime_config::V2Config;

use super::input::InputCollector;
use super::interaction::{apply_preview_actions, step_timeline_if_running};
use super::perf::GuiPerfRecorder;
use super::project::{GuiProject, ProjectNodeKind};
use super::renderer::GuiRenderer;
use super::scene::SceneBuilder;
use super::state::PreviewState;

/// Frame scheduler and state owner for the realtime GUI loop.
pub(crate) struct GuiApp {
    config: V2Config,
    panel_width: usize,
    window: Arc<Window>,
    renderer: GuiRenderer,
    project: GuiProject,
    state: PreviewState,
    input: InputCollector,
    scene: SceneBuilder,
    perf: GuiPerfRecorder,
    frame_budget: Duration,
    frame_deadline: Instant,
    last_frame_start: Instant,
    frame_counter: u64,
    benchmark_node: Option<u32>,
}

impl GuiApp {
    /// Create one GPU-backed GUI app bound to the provided window.
    pub(crate) async fn new(
        config: V2Config,
        panel_width: usize,
        window: Arc<Window>,
    ) -> Result<Self, Box<dyn Error>> {
        let renderer = GuiRenderer::new(window.clone(), config.gui.vsync).await?;
        let mut project = GuiProject::new_empty(config.width, config.height);
        let benchmark_node =
            maybe_seed_benchmark_nodes(&config, &mut project, panel_width, renderer.height());
        let state = PreviewState::new(&config);
        let frame_budget = frame_budget(config.gui.target_fps);
        let now = Instant::now();
        println!(
            "[gui] {}x{} @ {}hz ({:?})",
            renderer.width(),
            renderer.height(),
            config.gui.target_fps,
            config.gui.vsync
        );
        println!("[gui] controls: Esc=quit, Space=pause, Tab=add node menu, R=new project");
        Ok(Self {
            config,
            panel_width,
            window,
            renderer,
            project,
            state,
            input: InputCollector::default(),
            scene: SceneBuilder::default(),
            perf: GuiPerfRecorder::new(None),
            frame_budget,
            frame_deadline: now,
            last_frame_start: now,
            frame_counter: 0,
            benchmark_node,
        }
        .with_perf_trace())
    }

    /// Return current redraw deadline for the event loop.
    pub(crate) fn frame_deadline(&self) -> Instant {
        self.frame_deadline
    }

    /// Return true when this event should terminate the GUI loop.
    pub(crate) fn handle_window_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CloseRequested => true,
            WindowEvent::Resized(size) => {
                self.renderer.resize(size.width, size.height);
                false
            }
            WindowEvent::ScaleFactorChanged { .. } => false,
            _ => {
                self.input.handle_event(event);
                false
            }
        }
    }

    /// Request redraw when the frame deadline has elapsed.
    pub(crate) fn request_redraw_if_due(&mut self) {
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
        apply_preview_actions(
            &self.config,
            snapshot,
            &mut self.project,
            self.panel_width,
            self.renderer.height(),
            &mut self.state,
        );
        if self.config.gui.benchmark_drag {
            self.apply_synthetic_drag();
        }
        step_timeline_if_running(&mut self.state, frame_delta, self.config.animation.fps);
        self.state.avg_fps = smoothed_fps(self.state.avg_fps, frame_delta);
        let update_elapsed = update_start.elapsed();

        let scene_start = Instant::now();
        let frame = self.scene.build(
            &self.project,
            &self.state,
            self.renderer.width(),
            self.renderer.height(),
            self.panel_width,
        );
        let scene_elapsed = scene_start.elapsed();

        let render_start = Instant::now();
        self.renderer.render(frame)?;
        let render_elapsed = render_start.elapsed();

        let total_elapsed = frame_start.elapsed();
        self.perf.record(
            self.frame_counter,
            input_elapsed,
            update_elapsed,
            scene_elapsed,
            render_elapsed,
            total_elapsed,
        );
        self.update_title();
        self.frame_counter = self.frame_counter.wrapping_add(1);
        Ok(())
    }

    /// Flush trace output before event-loop shutdown.
    pub(crate) fn shutdown(&self) -> Result<(), Box<dyn Error>> {
        self.perf.flush()
    }

    fn update_title(&self) {
        let paused = if self.state.paused {
            "paused"
        } else {
            "running"
        };
        let title = format!(
            "covergen TD | {} | viewport={}x{} | target={}x{} | nodes={} | frame={} | {:.1} fps | {}",
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
        self.window.set_title(&title);
    }

    fn apply_synthetic_drag(&mut self) {
        let Some(node_id) = self.benchmark_node else {
            return;
        };
        let phase = self.frame_counter as f32 / self.config.gui.target_fps.max(1) as f32;
        let cx = (self.panel_width as f32 * 0.5) as i32;
        let cy = (self.renderer.height() as f32 * 0.5) as i32;
        let x = cx + (phase * 2.7).sin().mul_add(120.0, 0.0) as i32;
        let y = cy + (phase * 1.9).cos().mul_add(90.0, 0.0) as i32;
        self.project
            .move_node(node_id, x, y, self.panel_width, self.renderer.height());
    }

    fn with_perf_trace(mut self) -> Self {
        self.perf = GuiPerfRecorder::new(self.config.gui.perf_trace.clone());
        self
    }
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
    let top = project.add_node(
        ProjectNodeKind::TopBasic,
        120,
        120,
        panel_width,
        panel_height,
    );
    let _out = project.add_node(ProjectNodeKind::Output, 280, 220, panel_width, panel_height);
    Some(top)
}

fn frame_budget(target_fps: u32) -> Duration {
    Duration::from_secs_f64(1.0 / target_fps.max(1) as f64)
}

fn smoothed_fps(previous: f32, frame_elapsed: Duration) -> f32 {
    let inst = 1.0 / frame_elapsed.as_secs_f32().max(1e-4);
    if previous <= 0.0 {
        return inst;
    }
    previous * 0.9 + inst * 0.1
}

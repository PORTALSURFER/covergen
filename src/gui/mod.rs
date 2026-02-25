//! Realtime TouchDesigner-style GUI preview for project-based authoring.
//!
//! Startup creates one empty project. Users can open an Add Node menu with
//! `Tab`, place nodes on the graph editor canvas, and drag nodes with mouse.

mod draw;
mod interaction;
mod node_editor;
mod project;
mod state;
mod window;

use std::error::Error;
use std::thread;
use std::time::{Duration, Instant};

use crate::runtime_config::{V2Args, V2Config};

use interaction::{apply_preview_actions, step_timeline_if_running};
use project::GuiProject;
use state::PreviewState;
use window::TopPreviewWindow;

const GUI_TARGET_FPS: u32 = 120;
const GUI_MAX_PREVIEW_DIM: u32 = 900;
const PANEL_WIDTH: usize = 420;

/// Launch the realtime split-panel GUI using standard runtime arguments.
pub(crate) async fn run_gui_preview(args: V2Args) -> Result<(), Box<dyn Error>> {
    let config = V2Config::from_args(args)?;
    let project = GuiProject::new_empty(config.width, config.height);
    let (preview_width, preview_height) =
        gui_preview_size(config.width, config.height, GUI_MAX_PREVIEW_DIM);
    let mut window =
        TopPreviewWindow::new(preview_width, preview_height, PANEL_WIDTH, GUI_TARGET_FPS)?;
    run_preview_loop(&config, project, &mut window)
}

fn run_preview_loop(
    config: &V2Config,
    mut project: GuiProject,
    window: &mut TopPreviewWindow,
) -> Result<(), Box<dyn Error>> {
    let frame_budget = target_frame_budget(GUI_TARGET_FPS);
    let mut state = PreviewState::new(config);
    let mut last_frame_start = Instant::now();
    print_controls_once(config, &project);

    while window.is_open() {
        let frame_start = Instant::now();
        let frame_delta = frame_start.saturating_duration_since(last_frame_start);
        last_frame_start = frame_start;
        let input = window.capture_input(state.prev_left_down);
        apply_preview_actions(
            config,
            input,
            &mut project,
            window.panel_width(),
            window.height(),
            &mut state,
        );
        step_timeline_if_running(&mut state, frame_delta, config.animation.fps);
        state.avg_fps = smoothed_fps(state.avg_fps, frame_delta);
        window.present(&project, &state)?;
        window.set_title(&window_title(config, &project, &state));
        sleep_to_frame_budget(frame_start, frame_budget);
    }
    Ok(())
}

fn target_frame_budget(target_fps: u32) -> Duration {
    Duration::from_secs_f64(1.0 / target_fps.max(1) as f64)
}

fn gui_preview_size(width: u32, height: u32, max_dim: u32) -> (u32, u32) {
    let width = width.max(64);
    let height = height.max(64);
    let longest = width.max(height);
    if longest <= max_dim {
        return (width, height);
    }
    let scale = max_dim as f32 / longest as f32;
    let scaled_w = ((width as f32 * scale).round() as u32).max(64);
    let scaled_h = ((height as f32 * scale).round() as u32).max(64);
    (scaled_w, scaled_h)
}

fn smoothed_fps(previous: f32, frame_elapsed: Duration) -> f32 {
    let inst = 1.0 / frame_elapsed.as_secs_f32().max(1e-4);
    if previous <= 0.0 {
        return inst;
    }
    previous * 0.9 + inst * 0.1
}

fn sleep_to_frame_budget(start: Instant, frame_budget: Duration) {
    if let Some(remaining) = frame_budget.checked_sub(start.elapsed()) {
        thread::sleep(remaining);
    }
}

fn window_title(config: &V2Config, project: &GuiProject, state: &PreviewState) -> String {
    let run_state = if state.paused { "paused" } else { "running" };
    format!(
        "covergen TD | {} | {}x{} | nodes={} | frame={} | {:.1} fps | {}",
        project.name,
        config.width,
        config.height,
        project.node_count(),
        state.frame_index,
        state.avg_fps,
        run_state
    )
}

fn print_controls_once(config: &V2Config, project: &GuiProject) {
    println!(
        "[gui] new empty project loaded | {} | {}x{}",
        project.name, config.width, config.height
    );
    let (display_w, display_h) = gui_preview_size(config.width, config.height, GUI_MAX_PREVIEW_DIM);
    println!(
        "[gui] editor preview buffer: {}x{} @ {}hz",
        display_w, display_h, GUI_TARGET_FPS
    );
    println!("[gui] controls: Esc=quit, Space=pause, Tab=add node menu, R=new project");
}

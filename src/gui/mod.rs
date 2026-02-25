//! Realtime TouchDesigner-style GUI preview for project-based authoring.
//!
//! Startup behavior currently creates a new empty project every launch.
//! The window is split into two panels:
//! - left: node editor surface bound to the current project
//! - right: TOP viewport canvas (blank until graph execution is connected)

mod draw;
mod node_editor;
mod project;

use std::error::Error;
use std::thread;
use std::time::{Duration, Instant};

use minifb::{Key, KeyRepeat, Window, WindowOptions};

use crate::runtime_config::{V2Args, V2Config};

use draw::{draw_line, draw_text};
use node_editor::NodeEditorLayout;
use project::GuiProject;

const PANEL_WIDTH: usize = 420;
const PREVIEW_BG: u32 = 0xFF0A0D12;
const DIVIDER_COLOR: u32 = 0xFF2A313A;
const HUD_TEXT_COLOR: u32 = 0xFFE5E7EB;

/// Launch the realtime split-panel GUI using standard runtime arguments.
pub(crate) async fn run_gui_preview(args: V2Args) -> Result<(), Box<dyn Error>> {
    let config = V2Config::from_args(args)?;
    let project = GuiProject::new_empty(config.width, config.height);
    let mut window = TopPreviewWindow::new(config.width, config.height, &project)?;
    run_preview_loop(&config, project, &mut window)
}

fn run_preview_loop(
    config: &V2Config,
    mut project: GuiProject,
    window: &mut TopPreviewWindow,
) -> Result<(), Box<dyn Error>> {
    let frame_budget = target_frame_budget(config.animation.fps);
    let mut state = PreviewState::new(config);
    print_controls_once(config, &project);

    while window.is_open() {
        let frame_start = Instant::now();
        apply_preview_actions(
            config,
            window.poll_actions(),
            &mut project,
            window,
            &mut state,
        );
        step_timeline_if_running(&mut state);
        state.avg_fps = smoothed_fps(state.avg_fps, frame_start.elapsed());
        window.present(&project, &state)?;
        window.set_title(&window_title(config, &project, &state));
        sleep_to_frame_budget(frame_start, frame_budget);
    }
    Ok(())
}

fn target_frame_budget(target_fps: u32) -> Duration {
    Duration::from_secs_f64(1.0 / target_fps.max(1) as f64)
}

fn apply_preview_actions(
    config: &V2Config,
    actions: PreviewActions,
    project: &mut GuiProject,
    window: &mut TopPreviewWindow,
    state: &mut PreviewState,
) {
    if actions.toggle_pause {
        state.paused = !state.paused;
    }
    if actions.new_project {
        *project = GuiProject::new_empty(config.width, config.height);
        window.set_project(project);
        state.frame_index = 0;
    }
}

fn step_timeline_if_running(state: &mut PreviewState) {
    if !state.paused {
        state.frame_index = state.frame_index.wrapping_add(1);
    }
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
        "covergen TD | {} | {}x{} | canvas={}x{} | nodes={} | frame={} | {:.1} fps | {}",
        project.name,
        config.width,
        config.height,
        project.preview_width,
        project.preview_height,
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
    println!("[gui] controls: Esc=quit, Space=pause/resume, R=new empty project");
}

#[derive(Clone, Copy, Debug, Default)]
struct PreviewActions {
    toggle_pause: bool,
    new_project: bool,
}

#[derive(Clone, Copy, Debug)]
struct PreviewState {
    frame_index: u32,
    total_frames: u32,
    paused: bool,
    avg_fps: f32,
}

impl PreviewState {
    fn new(config: &V2Config) -> Self {
        let total_frames = config
            .animation
            .seconds
            .saturating_mul(config.animation.fps)
            .max(1);
        Self {
            frame_index: 0,
            total_frames,
            paused: false,
            avg_fps: 0.0,
        }
    }
}

/// Host window for split-panel GUI rendering.
struct TopPreviewWindow {
    width: usize,
    height: usize,
    panel_width: usize,
    preview_width: usize,
    preview_height: usize,
    rgb: Vec<u32>,
    window: Window,
    editor: NodeEditorLayout,
}

impl TopPreviewWindow {
    fn new(
        preview_width: u32,
        preview_height: u32,
        project: &GuiProject,
    ) -> Result<Self, Box<dyn Error>> {
        let preview_width = usize::try_from(preview_width).map_err(|_| "invalid preview width")?;
        let preview_height =
            usize::try_from(preview_height).map_err(|_| "invalid preview height")?;
        let panel_width = PANEL_WIDTH;
        let width = panel_width
            .checked_add(preview_width)
            .ok_or("invalid split-panel width")?;
        let height = preview_height;

        let mut window = Window::new(
            "covergen TD",
            width,
            height,
            WindowOptions {
                resize: true,
                ..WindowOptions::default()
            },
        )?;
        window.set_target_fps(60);

        let editor = NodeEditorLayout::from_project(project, panel_width, height);
        let rgb = vec![
            PREVIEW_BG;
            width
                .checked_mul(height)
                .ok_or("invalid panel dimensions")?
        ];

        Ok(Self {
            width,
            height,
            panel_width,
            preview_width,
            preview_height,
            rgb,
            window,
            editor,
        })
    }

    fn set_project(&mut self, project: &GuiProject) {
        self.editor = NodeEditorLayout::from_project(project, self.panel_width, self.height);
    }

    fn is_open(&self) -> bool {
        self.window.is_open() && !self.window.is_key_down(Key::Escape)
    }

    fn poll_actions(&self) -> PreviewActions {
        PreviewActions {
            toggle_pause: self.window.is_key_pressed(Key::Space, KeyRepeat::No),
            new_project: self.window.is_key_pressed(Key::R, KeyRepeat::No),
        }
    }

    fn present(
        &mut self,
        project: &GuiProject,
        state: &PreviewState,
    ) -> Result<(), Box<dyn Error>> {
        self.rgb.fill(PREVIEW_BG);
        self.editor.draw(&mut self.rgb, self.width, self.height);
        self.draw_preview_canvas();
        self.draw_preview_hud(project, state);
        self.draw_divider();

        self.window
            .update_with_buffer(&self.rgb, self.width, self.height)?;
        Ok(())
    }

    fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    fn draw_preview_canvas(&mut self) {
        for y in 0..self.preview_height {
            let row = y * self.width + self.panel_width;
            for x in 0..self.preview_width {
                self.rgb[row + x] = PREVIEW_BG;
            }
        }
    }

    fn draw_preview_hud(&mut self, project: &GuiProject, state: &PreviewState) {
        let status = if state.paused { "PAUSED" } else { "RUNNING" };
        let text = format!(
            "TOP VIEWPORT  {}  {:.1} FPS  F{}/{}  {}  {}x{}",
            status,
            state.avg_fps,
            state.frame_index,
            state.total_frames,
            project.name,
            project.preview_width,
            project.preview_height
        );
        draw_text(
            &mut self.rgb,
            self.width,
            self.height,
            (self.panel_width + 12) as i32,
            12,
            &text,
            HUD_TEXT_COLOR,
        );
        draw_text(
            &mut self.rgb,
            self.width,
            self.height,
            (self.panel_width + 12) as i32,
            28,
            "No nodes in project. Add nodes to begin rendering.",
            0xFF9CA3AF,
        );
    }

    fn draw_divider(&mut self) {
        let x = self.panel_width as i32 - 1;
        draw_line(
            &mut self.rgb,
            self.width,
            self.height,
            x,
            0,
            x,
            self.height as i32 - 1,
            DIVIDER_COLOR,
        );
    }
}

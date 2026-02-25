//! Realtime TouchDesigner-style GUI preview for graph-native rendering.
//!
//! The window is split into two panels:
//! - left: lightweight node-editor view of the compiled graph
//! - right: live TOP texture preview from the retained GPU runtime

mod draw;
mod node_editor;

use std::error::Error;
use std::thread;
use std::time::{Duration, Instant};

use minifb::{Key, KeyRepeat, Window, WindowOptions};

use crate::animation::total_frames;
use crate::compiler::{compile_graph, CompiledGraph};
use crate::presets::build_preset_graph;
use crate::runtime::{
    apply_motion_temporal_constraints, create_renderer, create_runtime_buffers,
    finalize_luma_for_output, render_graph_frame, RuntimeBuffers,
};
use crate::runtime_config::{runtime_seed, V2Args, V2Config};

use draw::{draw_line, draw_text};
use node_editor::NodeEditorLayout;

const PANEL_WIDTH: usize = 420;
const PREVIEW_BG: u32 = 0xFF0A0D12;
const DIVIDER_COLOR: u32 = 0xFF2A313A;
const HUD_TEXT_COLOR: u32 = 0xFFE5E7EB;

/// Launch the realtime split-panel GUI using standard runtime arguments.
pub(crate) async fn run_gui_preview(args: V2Args) -> Result<(), Box<dyn Error>> {
    let config = V2Config::from_args(args)?;
    let graph = build_preset_graph(&config)?;
    let compiled = compile_graph(&graph)?;
    let mut renderer = create_renderer(&config, &compiled).await?;
    renderer.ensure_node_alias_buffers(
        compiled.resource_plan.gpu_peak_luma_slots,
        compiled.resource_plan.gpu_peak_mask_slots,
    )?;
    renderer.ensure_node_feedback_buffers(compiled.feedback_slots.len())?;
    renderer.reset_feedback_state()?;

    let mut buffers =
        create_runtime_buffers(compiled.width, compiled.height, config.width, config.height)?;
    let mut window = TopPreviewWindow::new(config.width, config.height, &compiled)?;
    run_preview_loop(&config, &compiled, &mut renderer, &mut buffers, &mut window)
}

fn run_preview_loop(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: &mut crate::gpu_render::GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
    window: &mut TopPreviewWindow,
) -> Result<(), Box<dyn Error>> {
    let frame_budget = target_frame_budget(config.animation.fps);
    let mut state = PreviewState::new(config, compiled);
    print_controls_once(config, compiled);

    while window.is_open() {
        let frame_start = Instant::now();
        apply_preview_actions(window.poll_actions(), renderer, &mut state)?;
        render_if_unpaused(config, compiled, renderer, buffers, &mut state)?;
        state.avg_fps = smoothed_fps(state.avg_fps, frame_start.elapsed());
        window.present(&buffers.output_gray, &state)?;
        window.set_title(&window_title(config, &state));
        sleep_to_frame_budget(frame_start, frame_budget);
    }
    Ok(())
}

fn target_frame_budget(target_fps: u32) -> Duration {
    Duration::from_secs_f64(1.0 / target_fps.max(1) as f64)
}

fn apply_preview_actions(
    actions: PreviewActions,
    renderer: &mut crate::gpu_render::GpuLayerRenderer,
    state: &mut PreviewState,
) -> Result<(), Box<dyn Error>> {
    if actions.toggle_pause {
        state.paused = !state.paused;
    }
    if actions.reseed {
        state.seed_offset = runtime_seed().wrapping_add(state.compiled_seed);
        state.frame_index = 0;
        renderer.reset_feedback_state()?;
    }
    Ok(())
}

fn render_if_unpaused(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: &mut crate::gpu_render::GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
    state: &mut PreviewState,
) -> Result<(), Box<dyn Error>> {
    if state.paused {
        return Ok(());
    }
    render_preview_frame(config, compiled, renderer, buffers, state)?;
    state.frame_index = state.frame_index.wrapping_add(1);
    Ok(())
}

fn render_preview_frame(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: &mut crate::gpu_render::GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
    state: &PreviewState,
) -> Result<(), Box<dyn Error>> {
    let graph_time = apply_motion_temporal_constraints(
        crate::node::GraphTimeInput::from_frame(
            state.frame_index % state.total_frames,
            state.total_frames,
        )
        .with_intensity(config.animation.motion.modulation_intensity()),
        config.animation.motion,
    );
    render_graph_frame(compiled, renderer, state.seed_offset, Some(graph_time))?;
    finalize_luma_for_output(config, renderer, buffers)?;
    Ok(())
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

fn window_title(config: &V2Config, state: &PreviewState) -> String {
    let run_state = if state.paused { "paused" } else { "running" };
    format!(
        "covergen TD | preset={} | {}x{} | frame={} | {:.1} fps | {}",
        config.preset, config.width, config.height, state.frame_index, state.avg_fps, run_state
    )
}

fn print_controls_once(config: &V2Config, compiled: &CompiledGraph) {
    println!(
        "[gui] split view started | left=node-editor right=top-preview | preset {} | nodes {}",
        config.preset,
        compiled.steps.len()
    );
    println!("[gui] controls: Esc=quit, Space=pause/resume, R=reseed");
}

#[derive(Clone, Copy, Debug, Default)]
struct PreviewActions {
    toggle_pause: bool,
    reseed: bool,
}

#[derive(Clone, Copy, Debug)]
struct PreviewState {
    frame_index: u32,
    total_frames: u32,
    paused: bool,
    seed_offset: u32,
    compiled_seed: u32,
    avg_fps: f32,
}

impl PreviewState {
    fn new(config: &V2Config, compiled: &CompiledGraph) -> Self {
        Self {
            frame_index: 0,
            total_frames: total_frames(&config.animation).max(2),
            paused: false,
            seed_offset: config.seed.wrapping_add(compiled.seed),
            compiled_seed: compiled.seed,
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
        compiled: &CompiledGraph,
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

        let editor = NodeEditorLayout::build(compiled, panel_width, height);
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

    fn is_open(&self) -> bool {
        self.window.is_open() && !self.window.is_key_down(Key::Escape)
    }

    fn poll_actions(&self) -> PreviewActions {
        PreviewActions {
            toggle_pause: self.window.is_key_pressed(Key::Space, KeyRepeat::No),
            reseed: self.window.is_key_pressed(Key::R, KeyRepeat::No),
        }
    }

    fn present(&mut self, gray: &[u8], state: &PreviewState) -> Result<(), Box<dyn Error>> {
        let expected = self
            .preview_width
            .checked_mul(self.preview_height)
            .ok_or("invalid preview plane")?;
        if gray.len() != expected {
            return Err(format!(
                "preview buffer mismatch: expected {}, got {}",
                expected,
                gray.len()
            )
            .into());
        }

        self.rgb.fill(PREVIEW_BG);
        self.editor.draw(&mut self.rgb, self.width, self.height);
        self.blit_preview(gray);
        self.draw_preview_hud(state);
        self.draw_divider();

        self.window
            .update_with_buffer(&self.rgb, self.width, self.height)?;
        Ok(())
    }

    fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    fn blit_preview(&mut self, gray: &[u8]) {
        for y in 0..self.preview_height {
            let src_row = y * self.preview_width;
            let dst_row = y * self.width + self.panel_width;
            for x in 0..self.preview_width {
                let value = gray[src_row + x] as u32;
                self.rgb[dst_row + x] = 0xFF00_0000 | (value << 16) | (value << 8) | value;
            }
        }
    }

    fn draw_preview_hud(&mut self, state: &PreviewState) {
        let status = if state.paused { "PAUSED" } else { "RUNNING" };
        let text = format!(
            "TOP PREVIEW {}  {:.1} FPS  F{}",
            status, state.avg_fps, state.frame_index
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

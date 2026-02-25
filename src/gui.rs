//! Realtime TOP preview window for graph-native rendering.
//!
//! This module provides a lightweight, TouchDesigner-style TOP viewer:
//! it continuously renders the selected graph preset on GPU and displays
//! the finalized output texture in a desktop window.

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

/// Launch the realtime TOP preview window using standard runtime args.
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
    let mut window = TopPreviewWindow::new(config.width, config.height)?;
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
        let frame_elapsed = frame_start.elapsed();
        state.avg_fps = smoothed_fps(state.avg_fps, frame_elapsed);
        window.present(&buffers.output_gray)?;
        window.set_title(&window_title(
            config,
            state.frame_index,
            state.paused,
            state.avg_fps,
        ));
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
    render_preview_frame(
        config,
        compiled,
        renderer,
        buffers,
        state.seed_offset,
        state.frame_index,
        state.total_frames,
    )?;
    state.frame_index = state.frame_index.wrapping_add(1);
    Ok(())
}

fn render_preview_frame(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: &mut crate::gpu_render::GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
    seed_offset: u32,
    frame_index: u32,
    total_frames: u32,
) -> Result<(), Box<dyn Error>> {
    let graph_time = apply_motion_temporal_constraints(
        crate::node::GraphTimeInput::from_frame(frame_index % total_frames, total_frames)
            .with_intensity(config.animation.motion.modulation_intensity()),
        config.animation.motion,
    );
    render_graph_frame(compiled, renderer, seed_offset, Some(graph_time))?;
    finalize_luma_for_output(config, renderer, buffers)?;
    Ok(())
}

fn smoothed_fps(previous: f32, frame_elapsed: Duration) -> f32 {
    let inst = 1.0 / frame_elapsed.as_secs_f32().max(1e-4);
    if previous <= 0.0 {
        inst
    } else {
        previous * 0.9 + inst * 0.1
    }
}

fn sleep_to_frame_budget(start: Instant, frame_budget: Duration) {
    if let Some(remaining) = frame_budget.checked_sub(start.elapsed()) {
        thread::sleep(remaining);
    }
}

fn window_title(config: &V2Config, frame_index: u32, paused: bool, fps: f32) -> String {
    let state = if paused { "paused" } else { "running" };
    format!(
        "covergen TOP | preset={} | {}x{} | frame={} | {:.1} fps | {}",
        config.preset, config.width, config.height, frame_index, fps, state
    )
}

fn print_controls_once(config: &V2Config, compiled: &CompiledGraph) {
    println!(
        "[gui] TOP preview started | preset {} | graph {}x{} -> output {}x{} | nodes {}",
        config.preset,
        compiled.width,
        compiled.height,
        config.width,
        config.height,
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

/// Host window that displays one grayscale TOP texture as RGB pixels.
struct TopPreviewWindow {
    width: usize,
    height: usize,
    rgb: Vec<u32>,
    window: Window,
}

impl TopPreviewWindow {
    fn new(width: u32, height: u32) -> Result<Self, Box<dyn Error>> {
        let width = usize::try_from(width).map_err(|_| "invalid preview width")?;
        let height = usize::try_from(height).map_err(|_| "invalid preview height")?;
        let mut window = Window::new(
            "covergen TOP",
            width,
            height,
            WindowOptions {
                resize: true,
                ..WindowOptions::default()
            },
        )?;
        window.set_target_fps(60);
        let rgb = vec![
            0u32;
            width
                .checked_mul(height)
                .ok_or("invalid preview dimensions")?
        ];
        Ok(Self {
            width,
            height,
            rgb,
            window,
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

    fn present(&mut self, gray: &[u8]) -> Result<(), Box<dyn Error>> {
        if gray.len() != self.rgb.len() {
            return Err(format!(
                "preview buffer mismatch: expected {} bytes, got {}",
                self.rgb.len(),
                gray.len()
            )
            .into());
        }
        for (rgb, gray) in self.rgb.iter_mut().zip(gray.iter().copied()) {
            let value = gray as u32;
            *rgb = 0xFF00_0000 | (value << 16) | (value << 8) | value;
        }
        self.window
            .update_with_buffer(&self.rgb, self.width, self.height)?;
        Ok(())
    }

    fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }
}

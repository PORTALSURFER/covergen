//! Realtime GPU-driven GUI preview for project-based authoring.
//!
//! The editor runs on `winit + wgpu` with a fixed interaction frame budget so
//! node dragging remains responsive under load.

mod app;
mod geometry;
mod input;
mod interaction;
mod perf;
mod project;
mod renderer;
mod scene;
mod state;

use std::error::Error;
use std::sync::Arc;

use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowBuilder;

use crate::runtime_config::{V2Args, V2Config};

use app::GuiApp;

const PANEL_WIDTH: usize = 420;
const MAX_PREVIEW_DIM: u32 = 900;

/// Launch the realtime split-panel GUI using standard runtime arguments.
pub(crate) async fn run_gui_preview(args: V2Args) -> Result<(), Box<dyn Error>> {
    let config = V2Config::from_args(args)?;
    let event_loop = EventLoop::new()?;
    let (preview_width, preview_height) =
        preview_size(config.width, config.height, MAX_PREVIEW_DIM);
    let window_size = LogicalSize::new(
        (preview_width as usize + PANEL_WIDTH) as f64,
        preview_height as f64,
    );
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("covergen TD")
            .with_inner_size(window_size)
            .with_resizable(true)
            .build(&event_loop)?,
    );
    let mut app = GuiApp::new(config, PANEL_WIDTH, window.clone()).await?;

    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::WaitUntil(app.frame_deadline()));
        match event {
            Event::WindowEvent { window_id, event } if window_id == window.id() => {
                if should_exit(&event) || app.handle_window_event(&event) {
                    if let Err(err) = app.shutdown() {
                        eprintln!("Error: failed to flush GUI perf trace: {err}");
                    }
                    target.exit();
                    return;
                }
                if matches!(event, WindowEvent::RedrawRequested) {
                    if let Err(err) = app.redraw() {
                        eprintln!("Error: {err}");
                        if let Err(shutdown_err) = app.shutdown() {
                            eprintln!("Error: failed to flush GUI perf trace: {shutdown_err}");
                        }
                        target.exit();
                    }
                }
            }
            Event::AboutToWait => {
                app.request_redraw_if_due();
            }
            _ => {}
        }
    })?;
    Ok(())
}

fn preview_size(width: u32, height: u32, max_dim: u32) -> (u32, u32) {
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

fn should_exit(event: &WindowEvent) -> bool {
    if !matches!(event, WindowEvent::KeyboardInput { .. }) {
        return false;
    }
    let WindowEvent::KeyboardInput { event, .. } = event else {
        return false;
    };
    event.state == ElementState::Pressed
        && !event.repeat
        && matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape))
}

//! Progress reporting utilities for long-running batch generation.

use std::io::{self, Write};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread;
use std::time::Duration;

/// Shared progress state consumed by the spinner thread and the renderer.
#[derive(Default)]
pub(crate) struct SpinnerState {
    /// Total images requested for the current batch.
    pub(crate) total_images: usize,
    current_image: AtomicUsize,
    current_layer: AtomicUsize,
    total_layers: AtomicUsize,
}

impl SpinnerState {
    /// Creates a new state initialized for a full batch.
    pub(crate) fn new(total_images: usize) -> Self {
        Self {
            total_images,
            ..Self::default()
        }
    }

    /// Updates counters for the next image.
    pub(crate) fn set_image(&self, image_index: usize, layer_total: usize) {
        self.current_image.store(image_index, Ordering::Relaxed);
        self.total_layers.store(layer_total, Ordering::Relaxed);
        self.current_layer.store(0, Ordering::Relaxed);
    }

    /// Updates the active layer index while rendering.
    pub(crate) fn set_layer(&self, layer_index: usize) {
        self.current_layer.store(layer_index, Ordering::Relaxed);
    }
}

/// Starts an animated spinner on stderr while generation is running.
pub(crate) fn start_spinner(state: Arc<SpinnerState>) -> (Arc<AtomicBool>, thread::JoinHandle<()>) {
    let running = Arc::new(AtomicBool::new(true));
    let thread_state = state;
    let running_thread = running.clone();
    let frames = ["|", "/", "-", "\\"];

    let handle = thread::spawn(move || {
        let mut tick = 0usize;
        while running_thread.load(Ordering::Acquire) {
            let image = thread_state.current_image.load(Ordering::Relaxed);
            let layer = thread_state.current_layer.load(Ordering::Relaxed);
            let total_layers = thread_state.total_layers.load(Ordering::Relaxed);
            let layer_text = if total_layers == 0 {
                "starting".to_string()
            } else {
                format!("layer {}/{}", layer, total_layers)
            };

            let _ = write!(
                io::stderr(),
                "\r{} image {}/{} {}",
                frames[tick % frames.len()],
                image,
                thread_state.total_images,
                layer_text,
            );
            let _ = io::stderr().flush();

            tick = tick.wrapping_add(1);
            thread::sleep(Duration::from_millis(90));
        }
    });

    (running, handle)
}

/// Print a progress-aligned status line to stderr without leaving spinner artifacts.
pub(crate) fn log_progress_message(message: &str) {
    let _ = write!(io::stderr(), "\r{:<120}\r", "");
    let _ = writeln!(io::stderr(), "{message}");
    let _ = io::stderr().flush();
}

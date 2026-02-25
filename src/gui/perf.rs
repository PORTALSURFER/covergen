//! Lightweight per-frame GUI performance sampling.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

/// Per-frame timing sample written to GUI trace output.
#[derive(Clone, Debug)]
pub(crate) struct GuiFrameSample {
    pub(crate) frame_index: u64,
    pub(crate) input_ms: f64,
    pub(crate) update_ms: f64,
    pub(crate) scene_ms: f64,
    pub(crate) render_ms: f64,
    pub(crate) total_ms: f64,
}

/// Optional trace recorder for GUI stage timings.
#[derive(Debug, Default)]
pub(crate) struct GuiPerfRecorder {
    output_path: Option<PathBuf>,
    samples: Vec<GuiFrameSample>,
}

impl GuiPerfRecorder {
    /// Create recorder that writes a CSV trace when `output_path` is provided.
    pub(crate) fn new(output_path: Option<String>) -> Self {
        Self {
            output_path: output_path.map(PathBuf::from),
            samples: Vec::new(),
        }
    }

    /// Store one timing sample for this GUI frame.
    pub(crate) fn record(
        &mut self,
        frame_index: u64,
        input: Duration,
        update: Duration,
        scene: Duration,
        render: Duration,
        total: Duration,
    ) {
        if self.output_path.is_none() {
            return;
        }
        self.samples.push(GuiFrameSample {
            frame_index,
            input_ms: millis(input),
            update_ms: millis(update),
            scene_ms: millis(scene),
            render_ms: millis(render),
            total_ms: millis(total),
        });
    }

    /// Flush captured samples to disk when tracing is enabled.
    pub(crate) fn flush(&self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(path) = self.output_path.as_ref() else {
            return Ok(());
        };
        let mut file = File::create(path)?;
        file.write_all(b"frame,input_ms,update_ms,scene_ms,render_ms,total_ms\n")?;
        for row in &self.samples {
            writeln!(
                file,
                "{},{:.4},{:.4},{:.4},{:.4},{:.4}",
                row.frame_index,
                row.input_ms,
                row.update_ms,
                row.scene_ms,
                row.render_ms,
                row.total_ms
            )?;
        }
        Ok(())
    }
}

fn millis(value: Duration) -> f64 {
    value.as_secs_f64() * 1000.0
}

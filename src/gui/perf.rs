//! Lightweight per-frame GUI performance sampling.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Duration;

const TRACE_HEADER: &[u8] = b"frame,input_ms,update_ms,scene_ms,render_ms,total_ms,submit_count,upload_bytes,hit_test_scans,bridge_intersection_tests,ui_alloc_bytes\n";
const TRACE_RING_CAPACITY: usize = 8_192;

/// Per-frame timing sample written to GUI trace output.
#[derive(Clone, Debug)]
pub(crate) struct GuiFrameSample {
    pub(crate) frame_index: u64,
    pub(crate) input_ms: f64,
    pub(crate) update_ms: f64,
    pub(crate) scene_ms: f64,
    pub(crate) render_ms: f64,
    pub(crate) total_ms: f64,
    pub(crate) submit_count: u32,
    pub(crate) upload_bytes: u64,
    pub(crate) hit_test_scans: u64,
    pub(crate) bridge_intersection_tests: u64,
    pub(crate) ui_alloc_bytes: u64,
}

/// Optional trace recorder for GUI stage timings.
#[derive(Debug, Default)]
pub(crate) struct GuiPerfRecorder {
    output_path: Option<PathBuf>,
    writer: Option<BufWriter<File>>,
    fallback_samples: VecDeque<GuiFrameSample>,
    writer_open_failed: bool,
    last_error: Option<String>,
}

impl GuiPerfRecorder {
    /// Create recorder that writes a CSV trace when `output_path` is provided.
    pub(crate) fn new(output_path: Option<String>) -> Self {
        let mut recorder = Self {
            output_path: output_path.map(PathBuf::from),
            writer: None,
            fallback_samples: VecDeque::with_capacity(TRACE_RING_CAPACITY),
            writer_open_failed: false,
            last_error: None,
        };
        recorder.ensure_writer_open();
        recorder
    }

    /// Store one timing sample for this GUI frame.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record(
        &mut self,
        frame_index: u64,
        input: Duration,
        update: Duration,
        scene: Duration,
        render: Duration,
        total: Duration,
        submit_count: u32,
        upload_bytes: u64,
        hit_test_scans: u64,
        bridge_intersection_tests: u64,
        ui_alloc_bytes: u64,
    ) {
        if self.output_path.is_none() {
            return;
        }
        let sample = GuiFrameSample {
            frame_index,
            input_ms: millis(input),
            update_ms: millis(update),
            scene_ms: millis(scene),
            render_ms: millis(render),
            total_ms: millis(total),
            submit_count,
            upload_bytes,
            hit_test_scans,
            bridge_intersection_tests,
            ui_alloc_bytes,
        };
        if self.writer.is_none() {
            self.ensure_writer_open();
        }
        if let Some(writer) = self.writer.as_mut() {
            if let Err(err) = write_sample_line(writer, &sample) {
                self.last_error = Some(err.to_string());
                self.writer = None;
                self.writer_open_failed = true;
            } else {
                return;
            }
        }
        self.push_fallback_sample(sample);
    }

    /// Flush captured samples to disk when tracing is enabled.
    pub(crate) fn flush(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.output_path.is_none() {
            return Ok(());
        }
        if self.writer.is_none() {
            self.writer_open_failed = false;
            self.ensure_writer_open();
        }
        if self.writer.is_none() {
            let reason = self
                .last_error
                .clone()
                .unwrap_or_else(|| "failed to open GUI perf trace writer".to_string());
            return Err(std::io::Error::other(reason).into());
        }
        self.flush_fallback_samples()?;
        if let Some(writer) = self.writer.as_mut() {
            writer.flush()?;
        }
        Ok(())
    }

    /// Attempt to open/initialize the trace writer when tracing is enabled.
    fn ensure_writer_open(&mut self) {
        if self.output_path.is_none() || self.writer.is_some() || self.writer_open_failed {
            return;
        }
        match self.try_open_writer() {
            Ok(writer) => {
                self.writer = Some(writer);
                self.writer_open_failed = false;
                self.last_error = None;
            }
            Err(err) => {
                self.writer_open_failed = true;
                self.last_error = Some(err.to_string());
            }
        }
    }

    /// Create a CSV writer and emit one header row.
    fn try_open_writer(&self) -> Result<BufWriter<File>, std::io::Error> {
        let Some(path) = self.output_path.as_ref() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "trace path missing",
            ));
        };
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(TRACE_HEADER)?;
        Ok(writer)
    }

    /// Keep only a bounded tail of captured samples when streaming is unavailable.
    fn push_fallback_sample(&mut self, sample: GuiFrameSample) {
        if self.fallback_samples.len() >= TRACE_RING_CAPACITY {
            let _ = self.fallback_samples.pop_front();
        }
        self.fallback_samples.push_back(sample);
    }

    /// Flush fallback ring-buffer samples into the active writer.
    fn flush_fallback_samples(&mut self) -> Result<(), std::io::Error> {
        while let Some(sample) = self.fallback_samples.pop_front() {
            if let Some(writer) = self.writer.as_mut() {
                write_sample_line(writer, &sample)?;
            }
        }
        Ok(())
    }
}

fn millis(value: Duration) -> f64 {
    value.as_secs_f64() * 1000.0
}

fn write_sample_line<W: Write>(
    writer: &mut W,
    sample: &GuiFrameSample,
) -> Result<(), std::io::Error> {
    writeln!(
        writer,
        "{},{:.4},{:.4},{:.4},{:.4},{:.4},{},{},{},{},{}",
        sample.frame_index,
        sample.input_ms,
        sample.update_ms,
        sample.scene_ms,
        sample.render_ms,
        sample.total_ms,
        sample.submit_count,
        sample.upload_bytes,
        sample.hit_test_scans,
        sample.bridge_intersection_tests,
        sample.ui_alloc_bytes
    )
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::{GuiPerfRecorder, TRACE_RING_CAPACITY};

    #[test]
    fn fallback_ring_buffer_is_bounded() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let trace_path =
            std::env::temp_dir().join(format!("covergen-missing-{unique}/gui_perf_trace.csv"));
        let mut recorder = GuiPerfRecorder::new(Some(trace_path.to_string_lossy().into_owned()));
        let sample_count = TRACE_RING_CAPACITY as u64 + 32;
        for frame in 0..sample_count {
            recorder.record(
                frame,
                Duration::from_millis(1),
                Duration::from_millis(1),
                Duration::from_millis(1),
                Duration::from_millis(1),
                Duration::from_millis(1),
                1,
                128,
                32,
                0,
                0,
            );
        }
        assert_eq!(recorder.fallback_samples.len(), TRACE_RING_CAPACITY);
        let first = recorder
            .fallback_samples
            .front()
            .expect("ring buffer should keep newest samples");
        assert_eq!(first.frame_index, 32);
    }

    #[test]
    fn recorder_streams_to_disk_without_fallback_growth() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let trace_path = std::env::temp_dir().join(format!("covergen_gui_trace_{unique}.csv"));
        let mut recorder = GuiPerfRecorder::new(Some(trace_path.to_string_lossy().into_owned()));
        recorder.record(
            0,
            Duration::from_millis(2),
            Duration::from_millis(3),
            Duration::from_millis(4),
            Duration::from_millis(5),
            Duration::from_millis(6),
            1,
            256,
            64,
            8,
            1024,
        );
        recorder.record(
            1,
            Duration::from_millis(1),
            Duration::from_millis(1),
            Duration::from_millis(1),
            Duration::from_millis(1),
            Duration::from_millis(1),
            1,
            128,
            16,
            4,
            0,
        );
        assert!(recorder.fallback_samples.is_empty());
        recorder.flush().expect("trace flush should succeed");

        let content = std::fs::read_to_string(&trace_path)
            .expect("trace file should be readable for assertions");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("0,"));
        assert!(lines[2].starts_with("1,"));

        let _ = std::fs::remove_file(trace_path);
    }
}

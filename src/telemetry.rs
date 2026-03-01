//! Lightweight runtime telemetry capture for benchmarking and performance analysis.
//!
//! Telemetry capture is opt-in and disabled by default. Call `begin_capture` to
//! start collecting timings, frame samples, and memory snapshots, then
//! `end_capture` to retrieve one immutable report.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

/// One named timing sample collected during a render run.
#[derive(Clone, Debug)]
pub(crate) struct TimingSample {
    /// Fully qualified timing scope, for example `v2.node.blend`.
    pub(crate) scope: String,
    /// Duration in milliseconds.
    pub(crate) ms: f64,
}

/// One frame render sample used for throughput and frame pacing analysis.
#[derive(Clone, Debug)]
pub(crate) struct FrameSample {
    /// Frame scope, typically `v2.animation.frame.total`.
    pub(crate) scope: String,
    /// Duration in milliseconds.
    pub(crate) ms: f64,
}

/// One process-memory snapshot sampled from `/proc/self/status`.
#[derive(Clone, Debug)]
pub(crate) struct MemorySample {
    /// Snapshot label identifying capture point.
    pub(crate) label: String,
    /// Resident set size in bytes at snapshot time.
    pub(crate) rss_bytes: u64,
    /// High-water resident set size in bytes at snapshot time.
    pub(crate) hwm_bytes: u64,
}

/// One named scalar counter sample.
#[derive(Clone, Debug)]
pub(crate) struct CounterSample {
    /// Fully qualified counter scope, for example `v2.gpu.upload_bytes.frame`.
    pub(crate) scope: String,
    /// Recorded counter value.
    pub(crate) value: f64,
}

/// Completed telemetry capture for one benchmark sample.
#[derive(Clone, Debug, Default)]
pub(crate) struct CaptureReport {
    /// Run label assigned at capture start.
    pub(crate) run_label: String,
    /// Recorded timing events.
    pub(crate) timings: Vec<TimingSample>,
    /// Recorded frame events.
    pub(crate) frames: Vec<FrameSample>,
    /// Recorded memory snapshots.
    pub(crate) memory: Vec<MemorySample>,
    /// Recorded scalar counters.
    pub(crate) counters: Vec<CounterSample>,
}

#[derive(Default)]
struct TelemetryState {
    active: HashMap<thread::ThreadId, CaptureReport>,
}

const LOCAL_CAPTURE_FLUSH_THRESHOLD: usize = 256;

#[derive(Default)]
struct ThreadLocalCaptureBuffer {
    timings: Vec<TimingSample>,
    frames: Vec<FrameSample>,
    memory: Vec<MemorySample>,
    counters: Vec<CounterSample>,
}

impl ThreadLocalCaptureBuffer {
    fn sample_count(&self) -> usize {
        self.timings
            .len()
            .saturating_add(self.frames.len())
            .saturating_add(self.memory.len())
            .saturating_add(self.counters.len())
    }

    fn is_empty(&self) -> bool {
        self.timings.is_empty()
            && self.frames.is_empty()
            && self.memory.is_empty()
            && self.counters.is_empty()
    }

    fn clear(&mut self) {
        self.timings.clear();
        self.frames.clear();
        self.memory.clear();
        self.counters.clear();
    }
}

thread_local! {
    static LOCAL_CAPTURE_BUFFER: RefCell<ThreadLocalCaptureBuffer> =
        RefCell::new(ThreadLocalCaptureBuffer::default());
}

fn telemetry_state() -> &'static Mutex<TelemetryState> {
    static STATE: OnceLock<Mutex<TelemetryState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(TelemetryState::default()))
}

fn capture_active_flag() -> &'static AtomicBool {
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    &ACTIVE
}

#[inline]
fn is_capture_active() -> bool {
    capture_active_flag().load(Ordering::Acquire)
}

fn with_state_mut<R>(f: impl FnOnce(&mut TelemetryState) -> R) -> R {
    let mut guard = telemetry_state()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut guard)
}

fn with_local_capture_buffer_mut<R>(f: impl FnOnce(&mut ThreadLocalCaptureBuffer) -> R) -> R {
    LOCAL_CAPTURE_BUFFER.with(|buffer| {
        let mut borrow = buffer.borrow_mut();
        f(&mut borrow)
    })
}

fn push_local_capture_sample(f: impl FnOnce(&mut ThreadLocalCaptureBuffer)) {
    let should_flush = with_local_capture_buffer_mut(|buffer| {
        f(buffer);
        buffer.sample_count() >= LOCAL_CAPTURE_FLUSH_THRESHOLD
    });
    if should_flush {
        flush_current_thread_capture_buffer();
    }
}

fn flush_current_thread_capture_buffer() {
    let pending = LOCAL_CAPTURE_BUFFER.with(|buffer| {
        let mut borrow = buffer.borrow_mut();
        std::mem::take(&mut *borrow)
    });
    if pending.is_empty() {
        return;
    }
    let thread_id = thread::current().id();
    with_state_mut(|state| {
        let Some(active) = state.active.get_mut(&thread_id) else {
            return;
        };
        active.timings.extend(pending.timings);
        active.frames.extend(pending.frames);
        active.memory.extend(pending.memory);
        active.counters.extend(pending.counters);
    });
}

/// Begin a fresh telemetry capture session and replace any prior active session.
pub(crate) fn begin_capture(run_label: impl Into<String>) {
    with_local_capture_buffer_mut(ThreadLocalCaptureBuffer::clear);
    let thread_id = thread::current().id();
    let report = CaptureReport {
        run_label: run_label.into(),
        ..CaptureReport::default()
    };
    with_state_mut(|state| {
        state.active.insert(thread_id, report);
    });
    capture_active_flag().store(true, Ordering::Release);
}

/// End the current telemetry capture session and return the captured report.
pub(crate) fn end_capture() -> Option<CaptureReport> {
    flush_current_thread_capture_buffer();
    let thread_id = thread::current().id();
    let (report, has_remaining_active) = with_state_mut(|state| {
        let report = state.active.remove(&thread_id);
        (report, !state.active.is_empty())
    });
    capture_active_flag().store(has_remaining_active, Ordering::Release);
    report
}

/// Record a timing sample for a named scope.
pub(crate) fn record_timing(scope: impl Into<String>, elapsed: Duration) {
    if !is_capture_active() {
        return;
    }
    record_timing_ms(scope, elapsed.as_secs_f64() * 1000.0);
}

/// Record a timing sample from a millisecond value.
pub(crate) fn record_timing_ms(scope: impl Into<String>, ms: f64) {
    if !is_capture_active() {
        return;
    }
    if !ms.is_finite() {
        return;
    }
    let scope = scope.into();
    push_local_capture_sample(|buffer| {
        buffer.timings.push(TimingSample { scope, ms });
    });
}

/// Record one animation frame timing sample.
pub(crate) fn record_frame(scope: impl Into<String>, elapsed: Duration) {
    if !is_capture_active() {
        return;
    }
    let ms = elapsed.as_secs_f64() * 1000.0;
    if !ms.is_finite() {
        return;
    }
    let scope = scope.into();
    push_local_capture_sample(|buffer| {
        buffer.frames.push(FrameSample { scope, ms });
    });
}

/// Snapshot process memory from `/proc/self/status` when capture is active.
pub(crate) fn snapshot_memory(label: impl Into<String>) {
    if !is_capture_active() {
        return;
    }
    let Some((rss_bytes, hwm_bytes)) = read_memory_bytes() else {
        return;
    };
    let label = label.into();
    push_local_capture_sample(|buffer| {
        buffer.memory.push(MemorySample {
            label,
            rss_bytes,
            hwm_bytes,
        });
    });
}

/// Record one scalar counter sample.
pub(crate) fn record_counter(scope: impl Into<String>, value: f64) {
    if !is_capture_active() {
        return;
    }
    if !value.is_finite() {
        return;
    }
    let scope = scope.into();
    push_local_capture_sample(|buffer| {
        buffer.counters.push(CounterSample { scope, value });
    });
}

/// Record one scalar counter sample from an integer value.
pub(crate) fn record_counter_u64(scope: impl Into<String>, value: u64) {
    if !is_capture_active() {
        return;
    }
    record_counter(scope, value as f64);
}

fn read_memory_bytes() -> Option<(u64, u64)> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let mut rss_bytes = None;
    let mut hwm_bytes = None;

    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            rss_bytes = parse_kib_line(line).map(|value| value.saturating_mul(1024));
        } else if line.starts_with("VmHWM:") {
            hwm_bytes = parse_kib_line(line).map(|value| value.saturating_mul(1024));
        }
    }

    Some((rss_bytes.unwrap_or(0), hwm_bytes.unwrap_or(0)))
}

fn parse_kib_line(line: &str) -> Option<u64> {
    line.split_whitespace().nth(1)?.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_lifecycle_collects_samples() {
        begin_capture("sample");
        record_timing_ms("scope", 4.0);
        record_frame("frame", Duration::from_millis(8));
        record_counter_u64("counter", 7);
        let report = end_capture().expect("capture should exist");
        assert_eq!(report.run_label, "sample");
        assert_eq!(report.timings.len(), 1);
        assert_eq!(report.frames.len(), 1);
        assert_eq!(report.counters.len(), 1);
    }

    #[test]
    fn capture_lifecycle_tracks_current_thread_state() {
        let _ = end_capture();
        begin_capture("sample");
        assert!(is_capture_active());
        assert!(end_capture().is_some());
        assert!(
            end_capture().is_none(),
            "capture should be cleared for the current thread after end_capture"
        );
    }

    #[test]
    fn record_calls_are_noop_when_capture_is_inactive() {
        let _ = end_capture();
        record_timing_ms("inactive.timing", 1.0);
        record_frame("inactive.frame", Duration::from_millis(1));
        snapshot_memory("inactive.memory");
        record_counter_u64("inactive.counter", 1);
        begin_capture("post-inactive");
        let report = end_capture().expect("capture should exist");
        assert!(report.timings.is_empty());
        assert!(report.frames.is_empty());
        assert!(report.memory.is_empty());
        assert!(report.counters.is_empty());
    }

    #[test]
    fn parse_kib_works_for_proc_lines() {
        assert_eq!(parse_kib_line("VmRSS:\t12345 kB"), Some(12345));
        assert_eq!(parse_kib_line("VmHWM:\t42 kB"), Some(42));
        assert_eq!(parse_kib_line("VmRSS:"), None);
    }
}

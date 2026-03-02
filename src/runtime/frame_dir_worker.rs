//! Frame-directory encoding worker and backpressure policies.

use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SendTimeoutError, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use image::codecs::png::CompressionType;

use crate::animation::frame_filename;
use crate::image_ops::encode_png_bytes;
use crate::telemetry;

/// Buffered frame queue depth for PNG frame-dir export workers.
///
/// Larger values absorb short encode spikes at the cost of memory; smaller
/// values propagate pressure to the producer sooner.
pub(crate) const FRAME_DIR_QUEUE_CAPACITY: usize = 8;

/// Sleep interval between bounded backpressure retries.
///
/// This keeps producer CPU usage low while waiting for worker progress.
const FRAME_DIR_SEND_RETRY_SLEEP_MS: u64 = 2;

/// Maximum producer wait budget before reporting frame submit backpressure.
///
/// Exceeding this budget fails fast so export does not hang indefinitely.
const FRAME_DIR_SEND_MAX_WAIT_MS: u64 = 1_500;

/// Maximum wait budget while finalizing worker completion.
///
/// Exceeding this budget reports deterministic shutdown failure telemetry.
const FRAME_DIR_FINISH_MAX_WAIT_MS: u64 = 1_500;

enum FrameDirWorkerJob {
    Frame { frame_index: u32, gray: Vec<u8> },
    Shutdown,
}

/// Dedicated PNG frame-dir worker used by animation export fallback mode.
pub(crate) struct FrameDirEncodeWorker {
    sender: Option<SyncSender<FrameDirWorkerJob>>,
    completion_rx: Receiver<Result<(), String>>,
    join_handle: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameDirSendPolicy {
    pub(crate) retry_sleep: Duration,
    pub(crate) max_wait: Duration,
}

impl FrameDirSendPolicy {
    pub(crate) fn default_policy() -> Self {
        Self {
            retry_sleep: Duration::from_millis(FRAME_DIR_SEND_RETRY_SLEEP_MS),
            max_wait: Duration::from_millis(FRAME_DIR_SEND_MAX_WAIT_MS),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameDirFinishPolicy {
    pub(crate) max_wait: Duration,
}

impl FrameDirFinishPolicy {
    fn default_policy() -> Self {
        Self {
            max_wait: Duration::from_millis(FRAME_DIR_FINISH_MAX_WAIT_MS),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameDirFinishWaitError {
    Timeout,
    Disconnected,
}

impl FrameDirEncodeWorker {
    pub(crate) fn spawn(dir: PathBuf, width: u32, height: u32) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<FrameDirWorkerJob>(FRAME_DIR_QUEUE_CAPACITY);
        let (completion_tx, completion_rx) = mpsc::channel::<Result<(), String>>();
        let join_handle = thread::spawn(move || {
            let result = run_frame_dir_worker_loop(receiver, dir, width, height);
            let _ = completion_tx.send(result);
        });
        Self {
            sender: Some(sender),
            completion_rx,
            join_handle: Some(join_handle),
        }
    }

    pub(crate) fn submit_gray(
        &self,
        frame_index: u32,
        frame: Vec<u8>,
    ) -> Result<(), Box<dyn Error>> {
        let Some(sender) = self.sender.as_ref() else {
            return Err("frame-dir worker is no longer accepting frames".into());
        };
        submit_frame_dir_with_backpressure(
            sender,
            FrameDirWorkerJob::Frame {
                frame_index,
                gray: frame,
            },
            FrameDirSendPolicy::default_policy(),
        )
    }

    pub(crate) fn finish(mut self) -> Result<(), Box<dyn Error>> {
        let finish_policy = FrameDirFinishPolicy::default_policy();
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(FrameDirWorkerJob::Shutdown);
            drop(sender);
        }
        let wait_start = Instant::now();
        let completion = wait_for_frame_dir_worker_completion(&self.completion_rx, finish_policy);
        telemetry::record_timing("v2.export.frame_dir.finish_wait", wait_start.elapsed());
        match completion {
            Ok(result) => {
                if let Some(join_handle) = self.join_handle.take() {
                    if join_handle.join().is_err() {
                        telemetry::record_counter_u64("v2.export.frame_dir.finish_panics", 1);
                        return Err("frame-dir encode worker panicked".into());
                    }
                }
                result.map_err(|err| err.into())
            }
            Err(FrameDirFinishWaitError::Timeout) => {
                telemetry::record_counter_u64("v2.export.frame_dir.finish_timeouts", 1);
                let _ = self.join_handle.take();
                Err(format!(
                    "frame-dir worker finalization timeout after {} ms",
                    finish_policy.max_wait.as_millis()
                )
                .into())
            }
            Err(FrameDirFinishWaitError::Disconnected) => {
                telemetry::record_counter_u64("v2.export.frame_dir.finish_disconnected", 1);
                if let Some(join_handle) = self.join_handle.take() {
                    if join_handle.join().is_err() {
                        telemetry::record_counter_u64("v2.export.frame_dir.finish_panics", 1);
                        return Err("frame-dir encode worker panicked".into());
                    }
                }
                Err("frame-dir worker completion channel disconnected".into())
            }
        }
    }
}

fn run_frame_dir_worker_loop(
    receiver: Receiver<FrameDirWorkerJob>,
    dir: PathBuf,
    width: u32,
    height: u32,
) -> Result<(), String> {
    while let Ok(job) = receiver.recv() {
        match job {
            FrameDirWorkerJob::Frame { frame_index, gray } => {
                let encoded = encode_png_bytes(width, height, &gray, CompressionType::Fast)
                    .map_err(|err| err.to_string())?;
                let frame_path = dir.join(frame_filename(frame_index));
                std::fs::write(frame_path, encoded).map_err(|err| err.to_string())?;
            }
            FrameDirWorkerJob::Shutdown => break,
        }
    }
    Ok(())
}

/// Wait for frame-dir worker completion with bounded timeout.
pub(crate) fn wait_for_frame_dir_worker_completion(
    completion_rx: &Receiver<Result<(), String>>,
    policy: FrameDirFinishPolicy,
) -> Result<Result<(), String>, FrameDirFinishWaitError> {
    match completion_rx.recv_timeout(policy.max_wait) {
        Ok(result) => Ok(result),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(FrameDirFinishWaitError::Timeout),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(FrameDirFinishWaitError::Disconnected),
    }
}

/// Submit one frame-dir worker payload with bounded backpressure retries.
///
/// This keeps the producer thread responsive under encoder stalls while
/// preserving deterministic failure behavior after a configured timeout.
pub(crate) fn submit_frame_dir_with_backpressure<T>(
    sender: &SyncSender<T>,
    payload: T,
    policy: FrameDirSendPolicy,
) -> Result<(), Box<dyn Error>> {
    let submit_start = Instant::now();
    let mut retries = 0u32;
    let mut payload = payload;
    loop {
        match sender.send_timeout(payload, policy.retry_sleep) {
            Ok(()) => {
                if retries > 0 {
                    let waited = submit_start.elapsed();
                    telemetry::record_counter_u64("v2.export.frame_dir.submit_stall_events", 1);
                    telemetry::record_counter_u64(
                        "v2.export.frame_dir.submit_stall_retries",
                        retries as u64,
                    );
                    telemetry::record_timing("v2.export.frame_dir.submit_stall_wait", waited);
                }
                return Ok(());
            }
            Err(SendTimeoutError::Disconnected(_)) => {
                telemetry::record_counter_u64("v2.export.frame_dir.submit_disconnected", 1);
                return Err("frame-dir worker channel closed unexpectedly".into());
            }
            Err(SendTimeoutError::Timeout(returned)) => {
                payload = returned;
                retries = retries.saturating_add(1);
                let waited = submit_start.elapsed();
                if waited >= policy.max_wait {
                    telemetry::record_counter_u64("v2.export.frame_dir.submit_timeouts", 1);
                    telemetry::record_counter_u64(
                        "v2.export.frame_dir.submit_timeout_retries",
                        retries as u64,
                    );
                    telemetry::record_timing("v2.export.frame_dir.submit_timeout_wait", waited);
                    return Err(format!(
                        "frame-dir worker backpressure timeout after {} ms (retries={}, queue_capacity={})",
                        waited.as_millis(),
                        retries,
                        FRAME_DIR_QUEUE_CAPACITY
                    )
                    .into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        submit_frame_dir_with_backpressure, wait_for_frame_dir_worker_completion,
        FrameDirEncodeWorker, FrameDirFinishPolicy, FrameDirFinishWaitError, FrameDirSendPolicy,
        FRAME_DIR_QUEUE_CAPACITY,
    };
    use crate::animation::frame_filename;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_temp_dir(prefix: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{nonce}"));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("stale temp path should be removable");
        }
        std::fs::create_dir_all(&dir).expect("test temp dir should be created");
        dir
    }

    #[test]
    fn frame_dir_worker_writes_gray_frames() {
        let dir = create_temp_dir("covergen-frame-dir-worker");
        let worker = FrameDirEncodeWorker::spawn(dir.clone(), 2, 2);
        worker
            .submit_gray(0, vec![0, 85, 170, 255])
            .expect("gray frame should enqueue");
        worker.finish().expect("frame-dir worker should finish");

        let frame = dir.join(frame_filename(0));
        assert!(frame.exists(), "encoded frame should exist on disk");
        std::fs::remove_dir_all(&dir).expect("test temp dir should be removable");
    }

    #[test]
    fn frame_dir_backpressure_timeout_is_bounded_with_deterministic_error() {
        let (sender, _receiver) = mpsc::sync_channel::<(u32, Vec<u8>)>(FRAME_DIR_QUEUE_CAPACITY);
        for index in 0..FRAME_DIR_QUEUE_CAPACITY {
            sender
                .try_send((index as u32, vec![0u8]))
                .expect("queue should fill without blocking");
        }
        let policy = FrameDirSendPolicy {
            retry_sleep: Duration::from_millis(1),
            max_wait: Duration::from_millis(8),
        };
        let begin = Instant::now();
        let err = submit_frame_dir_with_backpressure(&sender, (99, vec![0u8]), policy)
            .expect_err("full queue should hit bounded timeout");
        let elapsed = begin.elapsed();
        let message = err.to_string();
        assert!(
            message.contains("backpressure timeout"),
            "timeout message should be deterministic"
        );
        assert!(
            message.contains("queue_capacity=8"),
            "timeout message should include capacity context"
        );
        assert!(
            elapsed < Duration::from_millis(250),
            "backpressure timeout should stay bounded; elapsed={elapsed:?}"
        );
    }

    #[test]
    fn frame_dir_backpressure_retries_then_succeeds_when_consumer_unblocks() {
        let (sender, receiver) = mpsc::sync_channel::<(u32, Vec<u8>)>(FRAME_DIR_QUEUE_CAPACITY);
        for index in 0..FRAME_DIR_QUEUE_CAPACITY {
            sender
                .try_send((index as u32, vec![0u8]))
                .expect("queue should fill without blocking");
        }
        let drain_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            let _ = receiver.recv();
            thread::sleep(Duration::from_millis(50));
        });
        let policy = FrameDirSendPolicy {
            retry_sleep: Duration::from_millis(1),
            max_wait: Duration::from_millis(200),
        };
        submit_frame_dir_with_backpressure(&sender, (123, vec![0u8]), policy)
            .expect("submit should succeed after consumer drains one slot");
        let _ = drain_thread.join();
    }

    #[test]
    fn frame_dir_finish_timeout_is_bounded_with_deterministic_error() {
        let (_tx, rx) = mpsc::channel::<Result<(), String>>();
        let begin = Instant::now();
        let wait = wait_for_frame_dir_worker_completion(
            &rx,
            FrameDirFinishPolicy {
                max_wait: Duration::from_millis(8),
            },
        );
        let elapsed = begin.elapsed();
        assert!(matches!(wait, Err(FrameDirFinishWaitError::Timeout)));
        assert!(
            elapsed < Duration::from_millis(250),
            "frame-dir finish timeout should stay bounded; elapsed={elapsed:?}"
        );
    }

    #[test]
    fn frame_dir_finish_reports_disconnected_completion_channel() {
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        drop(tx);
        let wait = wait_for_frame_dir_worker_completion(
            &rx,
            FrameDirFinishPolicy {
                max_wait: Duration::from_millis(10),
            },
        );
        assert!(matches!(wait, Err(FrameDirFinishWaitError::Disconnected)));
    }
}

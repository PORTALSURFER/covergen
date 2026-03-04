use super::{
    submit_frame_dir_with_backpressure, wait_for_frame_dir_worker_completion, FrameDirEncodeWorker,
    FrameDirFinishPolicy, FrameDirFinishWaitError, FrameDirSendPolicy, FRAME_DIR_QUEUE_CAPACITY,
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
fn frame_dir_worker_recycles_gray_buffers_after_write() {
    let dir = create_temp_dir("covergen-frame-dir-worker-recycle");
    let worker = FrameDirEncodeWorker::spawn(dir.clone(), 2, 2);
    worker
        .submit_gray(0, vec![0, 85, 170, 255])
        .expect("gray frame should enqueue");
    let mut recycled = Vec::new();
    let wait_start = Instant::now();
    while recycled.is_empty() && wait_start.elapsed() < Duration::from_millis(500) {
        worker.drain_recycled_gray_buffers(&mut recycled);
        if recycled.is_empty() {
            thread::sleep(Duration::from_millis(5));
        }
    }
    assert_eq!(
        recycled.len(),
        1,
        "worker should recycle submitted frame buffers"
    );
    assert_eq!(
        recycled[0].len(),
        4,
        "recycled buffer length should match frame"
    );
    worker.finish().expect("frame-dir worker should finish");
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

//! Animation export orchestration for compiled graph execution.

use std::collections::VecDeque;
use std::error::Error;
use std::time::Instant;

use crate::animation::{
    clip_output_path, create_frame_dir, encode_frames_to_mp4, total_frames, RawVideoEncoder,
    StreamFrameFormat,
};
use crate::compiler::CompiledGraph;
use crate::gpu_render::GpuLayerRenderer;
use crate::node::GraphTimeInput;
use crate::runtime_config::V2Config;
use crate::runtime_progress::{finish_animation_progress_line, print_animation_progress};
use crate::telemetry;

use super::frame_dir_worker::FrameDirEncodeWorker;
use super::{
    apply_motion_temporal_constraints, finalize_output_settings, render_graph_frame, RuntimeBuffers,
};

struct StreamEncodeWorker {
    frame_format: StreamFrameFormat,
    encoder: RawVideoEncoder,
}

impl StreamEncodeWorker {
    fn spawn(encoder: RawVideoEncoder) -> Self {
        let frame_format = encoder.frame_format();
        Self {
            frame_format,
            encoder,
        }
    }

    fn submit_gray(&mut self, frame: &[u8]) -> Result<(), Box<dyn Error>> {
        if self.frame_format != StreamFrameFormat::Gray8 {
            return Err("stream worker expects BGRA frames, not grayscale".into());
        }
        self.encoder.write_gray_frame(frame)
    }

    fn submit_bgra(&mut self, frame: &[u8]) -> Result<(), Box<dyn Error>> {
        if self.frame_format != StreamFrameFormat::Bgra8 {
            return Err("stream worker expects grayscale frames, not BGRA".into());
        }
        self.encoder.write_bgra_frame(frame)
    }

    fn finish(self) -> Result<(), Box<dyn Error>> {
        self.encoder.finish()
    }
}

enum ClipEncodeWorker {
    Stream(Box<StreamEncodeWorker>),
    FrameDir(FrameDirEncodeWorker),
}

impl ClipEncodeWorker {
    fn frame_format(&self) -> StreamFrameFormat {
        match self {
            Self::Stream(worker) => worker.frame_format,
            Self::FrameDir(_) => StreamFrameFormat::Gray8,
        }
    }

    fn submit_gray(&mut self, frame_index: u32, frame: &[u8]) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Stream(worker) => worker.submit_gray(frame),
            Self::FrameDir(worker) => worker.submit_gray(frame_index, frame.to_vec()),
        }
    }

    fn submit_gray_owned(
        &mut self,
        frame_index: u32,
        frame: Vec<u8>,
    ) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Stream(worker) => worker.submit_gray(frame.as_slice()),
            Self::FrameDir(worker) => worker.submit_gray(frame_index, frame),
        }
    }

    fn submit_bgra(&mut self, frame: &[u8]) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Stream(worker) => worker.submit_bgra(frame),
            Self::FrameDir(_) => Err("frame-dir worker accepts grayscale frames only".into()),
        }
    }

    fn drain_recycled_gray_buffers(&self, pool: &mut Vec<Vec<u8>>) {
        if let Self::FrameDir(worker) = self {
            worker.drain_recycled_gray_buffers(pool);
        }
    }

    fn finish(self) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Stream(worker) => worker.finish(),
            Self::FrameDir(worker) => worker.finish(),
        }
    }
}

pub(super) fn execute_animation(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: &mut GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
) -> Result<(), Box<dyn Error>> {
    let frames = total_frames(&config.animation);
    let finalize_settings = finalize_output_settings(config);
    let readback_capacity = renderer.retained_output_readback_capacity().max(1);
    print_animation_progress(0, frames, 0.0, config.count, 0);
    for clip_index in 0..config.count {
        let clip_start = Instant::now();
        telemetry::snapshot_memory(format!("v2.animation.clip.{clip_index}.start"));
        let frame_dir = if config.animation.keep_frames {
            Some(create_frame_dir(&config.output, clip_index)?)
        } else {
            None
        };
        let clip_path = clip_output_path(&config.output, clip_index, config.count);
        let mut encode_worker = if let Some(dir) = frame_dir.as_ref() {
            ClipEncodeWorker::FrameDir(FrameDirEncodeWorker::spawn(
                dir.clone(),
                config.width,
                config.height,
            ))
        } else {
            let encoder = RawVideoEncoder::spawn(
                config.width,
                config.height,
                config.animation.fps,
                &clip_path,
            )?;
            println!(
                "[v2] stream export frame format {:?} | data path {:?} | zero-readback active: false (planned target architecture)",
                encoder.frame_format(),
                encoder.data_path()
            );
            ClipEncodeWorker::Stream(Box::new(StreamEncodeWorker::spawn(encoder)))
        };
        let mut pending_frame_indices = VecDeque::with_capacity(readback_capacity + 1);
        let mut gray_frame_scratch = vec![0u8; buffers.output_gray.len()];
        let mut bgra_frame_scratch = vec![0u8; buffers.output_bgra.len()];
        let mut frame_dir_gray_pool = Vec::with_capacity(readback_capacity + 1);
        let clip_seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(clip_index.wrapping_mul(0x6A09_E667));
        let motion = config.animation.motion;
        let modulation_intensity = motion.modulation_intensity();
        let use_seed_jitter = motion.use_seed_jitter();
        renderer.reset_feedback_state()?;
        let clip_encode_result = (|| -> Result<(), Box<dyn Error>> {
            for frame_index in 0..frames {
                let frame_start = Instant::now();
                let frame_seed_offset = if use_seed_jitter {
                    clip_seed_offset.wrapping_add(frame_index.wrapping_mul(0x9E37_79B9))
                } else {
                    clip_seed_offset
                };
                let graph_time = apply_motion_temporal_constraints(
                    GraphTimeInput::from_frame(frame_index, frames)
                        .with_intensity(modulation_intensity),
                    motion,
                );
                render_graph_frame(compiled, renderer, frame_seed_offset, Some(graph_time))?;
                renderer.submit_retained_output_readback(
                    finalize_settings.contrast,
                    finalize_settings.low_pct,
                    finalize_settings.high_pct,
                    finalize_settings.fast_mode,
                )?;
                pending_frame_indices.push_back(frame_index);
                while renderer.pending_retained_output_readbacks() >= readback_capacity {
                    drain_one_queued_export_frame(
                        renderer,
                        &mut encode_worker,
                        &mut pending_frame_indices,
                        &mut gray_frame_scratch,
                        &mut bgra_frame_scratch,
                        &mut frame_dir_gray_pool,
                    )?;
                }
                let frame_elapsed = frame_start.elapsed();
                telemetry::record_timing("v2.animation.frame.total", frame_elapsed);
                telemetry::record_frame("v2.animation.frame.total", frame_elapsed);
                print_animation_progress(
                    frame_index + 1,
                    frames,
                    clip_start.elapsed().as_secs_f64(),
                    config.count,
                    clip_index,
                );
            }
            while renderer.pending_retained_output_readbacks() > 0 {
                drain_one_queued_export_frame(
                    renderer,
                    &mut encode_worker,
                    &mut pending_frame_indices,
                    &mut gray_frame_scratch,
                    &mut bgra_frame_scratch,
                    &mut frame_dir_gray_pool,
                )?;
            }
            if !pending_frame_indices.is_empty() {
                return Err(
                    "internal export pipeline mismatch: frame index queue not drained".into(),
                );
            }
            Ok(())
        })();
        finish_animation_progress_line();
        complete_encode_worker(clip_encode_result, encode_worker)?;

        if let Some(dir) = frame_dir.as_ref() {
            encode_frames_to_mp4(dir, config.animation.fps, &clip_path)?;
            if !config.animation.keep_frames {
                std::fs::remove_dir_all(dir)?;
            }
        }

        println!(
            "[v2] animation {} | {}s @ {}fps | {} frames | {}",
            clip_index + 1,
            config.animation.seconds,
            config.animation.fps,
            frames,
            clip_path.display()
        );
        telemetry::record_timing("v2.animation.clip.total", clip_start.elapsed());
        telemetry::snapshot_memory(format!("v2.animation.clip.{clip_index}.end"));
    }
    Ok(())
}

/// Finalize one clip encode worker and preserve the most actionable failure.
///
/// This keeps encoder cleanup deterministic even when clip rendering fails
/// mid-stream, and reports both errors when work and finalization fail.
fn complete_encode_worker(
    clip_work_result: Result<(), Box<dyn Error>>,
    worker: ClipEncodeWorker,
) -> Result<(), Box<dyn Error>> {
    let finish_result = worker.finish();
    match (clip_work_result, finish_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(finish_err)) => Err(finish_err),
        (Err(work_err), Ok(())) => Err(work_err),
        (Err(work_err), Err(finish_err)) => Err(format!(
            "{work_err}; additionally encoder finalization failed: {finish_err}"
        )
        .into()),
    }
}

fn drain_one_queued_export_frame(
    renderer: &mut GpuLayerRenderer,
    worker: &mut ClipEncodeWorker,
    pending_frame_indices: &mut VecDeque<u32>,
    gray_frame_scratch: &mut Vec<u8>,
    bgra_frame_scratch: &mut Vec<u8>,
    frame_dir_gray_pool: &mut Vec<Vec<u8>>,
) -> Result<(), Box<dyn Error>> {
    worker.drain_recycled_gray_buffers(frame_dir_gray_pool);
    let frame_index = pending_frame_indices
        .pop_front()
        .ok_or("queued readback had no matching frame index")?;
    let finalize_start = Instant::now();
    match worker.frame_format() {
        StreamFrameFormat::Gray8 => {
            if matches!(worker, ClipEncodeWorker::FrameDir(_)) {
                let mut owned_frame =
                    take_reusable_gray_frame(gray_frame_scratch.len(), frame_dir_gray_pool);
                renderer.collect_retained_output_gray_queued(owned_frame.as_mut_slice())?;
                telemetry::record_timing(
                    "v2.gpu.node.finalize_retained_output",
                    finalize_start.elapsed(),
                );
                worker.submit_gray_owned(frame_index, owned_frame)?;
                worker.drain_recycled_gray_buffers(frame_dir_gray_pool);
                return Ok(());
            }
            renderer.collect_retained_output_gray_queued(gray_frame_scratch.as_mut_slice())?;
            telemetry::record_timing(
                "v2.gpu.node.finalize_retained_output",
                finalize_start.elapsed(),
            );
            worker.submit_gray(frame_index, gray_frame_scratch.as_slice())?;
        }
        StreamFrameFormat::Bgra8 => {
            renderer.collect_retained_output_bgra_queued(bgra_frame_scratch.as_mut_slice())?;
            telemetry::record_timing(
                "v2.gpu.node.finalize_retained_output",
                finalize_start.elapsed(),
            );
            worker.submit_bgra(bgra_frame_scratch.as_slice())?;
        }
    }
    Ok(())
}

fn take_reusable_gray_frame(frame_len: usize, pool: &mut Vec<Vec<u8>>) -> Vec<u8> {
    if let Some(mut frame) = pool.pop() {
        if frame.len() != frame_len {
            frame.resize(frame_len, 0);
        }
        return frame;
    }
    vec![0u8; frame_len]
}

#[cfg(test)]
mod tests {
    use super::complete_encode_worker;
    use super::ClipEncodeWorker;
    use crate::runtime::frame_dir_worker::FrameDirEncodeWorker;
    use std::io::Error as IoError;
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
    fn complete_encode_worker_preserves_primary_work_error() {
        let dir = create_temp_dir("covergen-frame-dir-complete-ok");

        let worker = ClipEncodeWorker::FrameDir(FrameDirEncodeWorker::spawn(dir.clone(), 2, 2));
        let err = complete_encode_worker(Err(IoError::other("work failed").into()), worker)
            .expect_err("primary work failure should be returned");
        assert!(
            err.to_string().contains("work failed"),
            "primary work failure message should be preserved"
        );

        std::fs::remove_dir_all(&dir).expect("test temp dir should be removable");
    }

    #[test]
    fn complete_encode_worker_reports_finalize_error_when_both_fail() {
        let dir = create_temp_dir("covergen-frame-dir-complete-missing");

        let mut worker = ClipEncodeWorker::FrameDir(FrameDirEncodeWorker::spawn(dir.clone(), 2, 2));
        std::fs::remove_dir_all(&dir).expect("test should remove output dir before finalize");
        worker
            .submit_gray(0, &[0, 85, 170, 255])
            .expect("gray frame should enqueue");
        let err = complete_encode_worker(Err(IoError::other("work failed").into()), worker)
            .expect_err("combined work/finalize failure should be returned");
        let message = err.to_string();
        assert!(
            message.contains("work failed"),
            "primary work failure should be included in combined message"
        );
        assert!(
            message.contains("encoder finalization failed"),
            "finalization failure context should be included"
        );
    }
}

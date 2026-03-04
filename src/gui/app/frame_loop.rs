//! Per-frame update and redraw orchestration.

use super::*;
use crate::gui::interaction::InteractionFrameContext;
use crate::gui::scene::SceneBuildRequest;

/// Snapshot of project-scoped invalidation epochs captured pre-update.
#[derive(Clone, Copy, Debug)]
struct SceneInvalidationSnapshot {
    project: GuiProjectInvalidation,
}

impl SceneInvalidationSnapshot {
    /// Capture the minimal invalidation state needed for post-update diffing.
    fn capture(project: &GuiProject) -> Self {
        Self {
            project: project.invalidation(),
        }
    }
}

/// Inputs captured at the start of a GUI frame.
#[derive(Clone, Debug)]
struct FrameInputPhase {
    frame_delta: Duration,
    input_elapsed: Duration,
    snapshot: InputSnapshot,
}

/// Update-phase outputs needed by rendering and telemetry.
#[derive(Clone, Copy, Debug)]
struct FrameUpdatePhase {
    scene_dirty: bool,
    timeline_total_frames: u32,
    export_active: bool,
    update_elapsed: Duration,
    hit_test_scans: u64,
}

/// Render-phase telemetry captured while encoding/submitting work.
#[derive(Clone, Copy, Debug, Default)]
struct FrameRenderPhase {
    scene_elapsed: Duration,
    render_elapsed: Duration,
    submit_count: u32,
    upload_bytes: u64,
    ui_alloc_bytes: u64,
    bridge_intersection_tests: u64,
    signal_scope_samples: u64,
    signal_scope_eval_ms: f64,
    scene_nodes_ms: f64,
    scene_edges_ms: f64,
    scene_overlays_ms: f64,
}

impl GuiApp {
    /// Advance input/state and render one frame.
    pub(crate) fn redraw(&mut self) -> Result<(), Box<dyn Error>> {
        let frame_start = Instant::now();
        let input_phase = self.capture_input_phase(frame_start);
        let update_phase =
            self.apply_update_phase(&input_phase.snapshot, input_phase.frame_delta)?;
        let render_phase = self.render_phase(
            update_phase.scene_dirty,
            update_phase.export_active,
            update_phase.timeline_total_frames,
        )?;

        let total_elapsed = frame_start.elapsed();
        self.record_frame_metrics(
            total_elapsed,
            input_phase.input_elapsed,
            &update_phase,
            &render_phase,
        );
        self.finalize_frame(frame_start);
        Ok(())
    }

    /// Capture frame delta and immutable input snapshot at frame start.
    fn capture_input_phase(&mut self, frame_start: Instant) -> FrameInputPhase {
        let frame_delta = frame_start.saturating_duration_since(self.last_frame_start);
        self.last_frame_start = frame_start;
        let input_start = Instant::now();
        let snapshot = self
            .input
            .snapshot(self.renderer.width(), self.renderer.height());
        let input_elapsed = input_start.elapsed();
        FrameInputPhase {
            frame_delta,
            input_elapsed,
            snapshot,
        }
    }

    /// Apply interaction/timeline/export updates and return update telemetry.
    fn apply_update_phase(
        &mut self,
        snapshot: &InputSnapshot,
        frame_delta: Duration,
    ) -> Result<FrameUpdatePhase, Box<dyn Error>> {
        let update_start = Instant::now();
        let capture_scene_invalidation = self.should_capture_scene_invalidation_snapshot(snapshot);
        let scene_invalidation_before =
            capture_scene_invalidation.then(|| SceneInvalidationSnapshot::capture(&self.project));
        let (resize_changed, consume_editor_input) = self.apply_panel_resize_input(snapshot);
        let mut scene_dirty = resize_changed;
        scene_dirty |= self
            .project
            .set_lfo_sync_bpm(self.state.export_menu.parsed_bpm());
        if consume_editor_input {
            scene_dirty |= self.consume_editor_input(snapshot.left_down);
        } else {
            scene_dirty |= apply_preview_actions(
                InteractionFrameContext::new(
                    &self.config,
                    self.renderer.width(),
                    self.panel_width,
                    self.renderer.height(),
                ),
                snapshot.clone(),
                &mut self.project,
                &mut self.state,
            );
        }
        scene_dirty |= self.handle_pending_app_actions()?;
        self.state.export_menu.refresh_audio_duration_cache();
        scene_dirty |= self.update_export_pause_and_frame_state();
        if self.config.gui.benchmark_drag {
            scene_dirty |= self.apply_synthetic_drag();
        }
        let timeline_total_frames = self
            .state
            .export_menu
            .timeline_total_frames(self.config.animation.fps);
        scene_dirty |= self.update_timeline_state(frame_delta, timeline_total_frames);
        self.sync_timeline_audio_preview(timeline_total_frames);
        self.state.avg_fps = smoothed_fps(self.state.avg_fps, frame_delta);
        self.apply_project_scoped_invalidation(scene_invalidation_before, resize_changed);
        Ok(FrameUpdatePhase {
            scene_dirty,
            timeline_total_frames,
            export_active: self.export_session.is_some() || self.start_export_requested,
            update_elapsed: update_start.elapsed(),
            hit_test_scans: self.project.take_hit_test_scan_count(),
        })
    }

    /// Reset editor interactions when resize handles consume input for the frame.
    fn consume_editor_input(&mut self, left_down: bool) -> bool {
        self.state.drag = None;
        self.state.wire_drag = None;
        self.state.hover_param_target = None;
        self.state.hover_param = None;
        self.state.hover_insert_link = None;
        let mut scene_dirty = false;
        if !self.state.auto_expanded_binding_nodes.is_empty() {
            for node_id in self.state.auto_expanded_binding_nodes.drain(..) {
                scene_dirty |=
                    self.project
                        .collapse_node(node_id, self.panel_width, self.renderer.height());
            }
        }
        self.state.invalidation.invalidate_nodes();
        self.state.invalidation.invalidate_wires();
        self.state.invalidation.invalidate_overlays();
        self.state.prev_left_down = left_down;
        scene_dirty
    }

    /// Keep export-controlled timeline pause/frame state in sync.
    fn update_export_pause_and_frame_state(&mut self) -> bool {
        let mut scene_dirty = false;
        if self.start_export_requested && self.export_session.is_none() {
            if self.state.frame_index != 0 {
                self.state.frame_index = 0;
                scene_dirty = true;
                self.state.invalidation.invalidate_timeline();
                if self.project.has_signal_preview_nodes() {
                    self.state.invalidation.invalidate_nodes();
                }
            }
            if !self.state.paused {
                self.state.paused = true;
                self.state.invalidation.invalidate_timeline();
            }
        }
        if let Some(session) = self.export_session.as_ref() {
            if !self.state.paused {
                self.state.paused = true;
                self.state.invalidation.invalidate_timeline();
            }
            if self.state.frame_index != session.next_frame {
                self.state.frame_index = session.next_frame;
                scene_dirty = true;
                self.state.invalidation.invalidate_timeline();
                if self.project.has_signal_preview_nodes() {
                    self.state.invalidation.invalidate_nodes();
                }
            }
        }
        scene_dirty
    }

    /// Advance frame/timeline counters and keep preview totals in sync.
    fn update_timeline_state(&mut self, frame_delta: Duration, timeline_total_frames: u32) -> bool {
        let mut scene_dirty = false;
        if self.export_session.is_none() && !self.start_export_requested {
            let timeline_advanced = step_timeline_if_running(
                &mut self.state,
                frame_delta,
                self.config.animation.fps,
                timeline_total_frames,
            );
            scene_dirty |= timeline_advanced;
            if timeline_advanced {
                self.state.invalidation.invalidate_timeline();
                if self.project.has_signal_preview_nodes() {
                    self.state.invalidation.invalidate_nodes();
                }
            }
        }
        let clamped_frame = clamp_frame(self.state.frame_index, timeline_total_frames);
        if clamped_frame != self.state.frame_index {
            self.state.frame_index = clamped_frame;
            scene_dirty = true;
            self.state.invalidation.invalidate_timeline();
            if self.project.has_signal_preview_nodes() {
                self.state.invalidation.invalidate_nodes();
            }
        }
        if self.export_session.is_none()
            && !self.start_export_requested
            && self.state.export_menu.preview_total != timeline_total_frames
        {
            self.state.export_menu.preview_total = timeline_total_frames;
            scene_dirty = true;
            self.state.invalidation.invalidate_timeline();
            self.state.invalidation.invalidate_overlays();
        }
        scene_dirty
    }

    /// Build retained scene + issue renderer submission when redraw is required.
    fn render_phase(
        &mut self,
        scene_dirty: bool,
        export_active: bool,
        timeline_total_frames: u32,
    ) -> Result<FrameRenderPhase, Box<dyn Error>> {
        let mut phase = FrameRenderPhase::default();
        if !(scene_dirty || self.needs_redraw || export_active) {
            return Ok(phase);
        }
        self.tex_view.update(
            &self.project,
            TexViewerUpdate {
                viewport_width: self.renderer.width(),
                viewport_height: self.renderer.height(),
                panel_width: self.panel_width,
                frame_index: self.state.frame_index,
                timeline_total_frames,
                timeline_fps: self.config.animation.fps,
                tex_eval_epoch: self.state.invalidation.tex_eval,
            },
        );
        self.try_start_export_from_request()?;
        let scene_start = Instant::now();
        let frame = self.scene.build(
            &self.project,
            &self.state,
            SceneBuildRequest::new(
                self.renderer.width(),
                self.renderer.height(),
                self.panel_width,
                self.config.animation.fps,
            ),
        );
        phase.scene_elapsed = scene_start.elapsed();
        phase.ui_alloc_bytes = frame.ui_alloc_bytes;
        phase.bridge_intersection_tests = frame.bridge_intersection_tests;
        phase.signal_scope_samples = frame.signal_scope_samples;
        phase.signal_scope_eval_ms = frame.signal_scope_eval_ms;
        phase.scene_nodes_ms = frame.nodes_ms;
        phase.scene_edges_ms = frame.edges_ms;
        phase.scene_overlays_ms = frame.overlays_ms;

        let render_start = Instant::now();
        self.renderer.render(
            frame,
            self.tex_view.frame(),
            self.panel_width,
            self.state.avg_fps,
        )?;
        phase.render_elapsed = render_start.elapsed();
        let render_perf = self.renderer.take_perf_counters();
        phase.submit_count = render_perf.submit_count;
        phase.upload_bytes = render_perf.upload_bytes;
        phase.ui_alloc_bytes = phase.ui_alloc_bytes.saturating_add(render_perf.alloc_bytes);
        if self.export_session.is_some() {
            self.capture_export_frame()?;
        }
        Ok(phase)
    }

    /// Record per-frame telemetry and performance counters.
    fn record_frame_metrics(
        &mut self,
        total_elapsed: Duration,
        input_elapsed: Duration,
        update: &FrameUpdatePhase,
        render: &FrameRenderPhase,
    ) {
        telemetry::record_counter_u64("gui.gpu.submit_count_per_frame", render.submit_count as u64);
        telemetry::record_counter_u64("gui.gpu.upload_bytes_per_frame", render.upload_bytes);
        telemetry::record_counter_u64("gui.hit_test.scan_count_per_frame", update.hit_test_scans);
        telemetry::record_counter_u64(
            "gui.wire.bridge_intersection_tests_per_frame",
            render.bridge_intersection_tests,
        );
        telemetry::record_counter_u64(
            "signal_scope_samples_per_frame",
            render.signal_scope_samples,
        );
        telemetry::record_timing_ms("signal_scope_eval_ms", render.signal_scope_eval_ms);
        telemetry::record_timing_ms("scene.nodes_ms", render.scene_nodes_ms);
        telemetry::record_timing_ms("scene.edges_ms", render.scene_edges_ms);
        telemetry::record_timing_ms("scene.overlays_ms", render.scene_overlays_ms);
        telemetry::record_counter_u64("gui.ui.alloc_bytes_per_frame", render.ui_alloc_bytes);
        let total_secs = total_elapsed.as_secs_f64();
        if total_secs > 0.0 {
            telemetry::record_counter(
                "gui.ui.alloc_bytes_per_second",
                render.ui_alloc_bytes as f64 / total_secs,
            );
        }
        self.perf.record(
            self.frame_counter,
            input_elapsed,
            update.update_elapsed,
            render.scene_elapsed,
            render.render_elapsed,
            total_elapsed,
            render.submit_count,
            render.upload_bytes,
            update.hit_test_scans,
            render.bridge_intersection_tests,
            render.ui_alloc_bytes,
        );
        if self.frame_counter == 0 {
            telemetry::record_timing("gui.startup.first_frame.total", total_elapsed);
            telemetry::record_timing("gui.startup.first_frame.scene", render.scene_elapsed);
            telemetry::record_timing("gui.startup.first_frame.render", render.render_elapsed);
        }
    }

    /// Finalize loop policy/title and benchmark frame bookkeeping.
    fn finalize_frame(&mut self, frame_start: Instant) {
        self.update_loop_policy();
        self.update_title(frame_start);
        self.needs_redraw = false;
        let next_frame_counter = self.frame_counter.wrapping_add(1);
        if let Some(limit) = self.benchmark_frame_limit {
            let should_log_progress = next_frame_counter == 1
                || next_frame_counter.is_multiple_of(GUI_BENCH_PROGRESS_LOG_EVERY_FRAMES)
                || next_frame_counter >= limit;
            if should_log_progress {
                let completed = next_frame_counter.min(limit);
                let percent = ((completed as f64 * 100.0) / limit.max(1) as f64).min(100.0);
                println!(
                    "[gui-bench] progress frame {}/{} ({:.1}%) avg_fps={:.1}",
                    completed, limit, percent, self.state.avg_fps
                );
            }
        }
        self.frame_counter = next_frame_counter;
        if self
            .benchmark_frame_limit
            .is_some_and(|limit| self.frame_counter >= limit)
        {
            if !self.close_requested {
                println!(
                    "[gui-bench] reached frame limit ({}); closing",
                    self.frame_counter
                );
            }
            self.close_requested = true;
        }
    }

    /// Propagate project mutation deltas into scoped scene/tex epochs.
    fn apply_project_scoped_invalidation(
        &mut self,
        snapshot_before: Option<SceneInvalidationSnapshot>,
        resize_changed: bool,
    ) {
        if let Some(snapshot_before) = snapshot_before {
            let project_after = self.project.invalidation();
            if snapshot_before.project.nodes != project_after.nodes {
                self.state.invalidation.invalidate_nodes();
            }
            if snapshot_before.project.wires != project_after.wires {
                self.state.invalidation.invalidate_wires();
                self.state.invalidation.invalidate_overlays();
            }
            if snapshot_before.project.tex_eval != project_after.tex_eval {
                self.state.invalidation.invalidate_tex_eval();
            }
        }

        if resize_changed {
            self.state.invalidation.invalidate_nodes();
            self.state.invalidation.invalidate_wires();
            self.state.invalidation.invalidate_overlays();
            self.state.invalidation.invalidate_timeline();
        }
    }

    /// Return true when this frame can plausibly mutate project invalidation epochs.
    fn should_capture_scene_invalidation_snapshot(&self, input: &InputSnapshot) -> bool {
        if self.frame_counter == 0
            || self.state.pending_app_action.is_some()
            || self.config.gui.benchmark_drag
            || self.state.request_new_project
        {
            return true;
        }
        if self.state.drag.is_some()
            || self.state.wire_drag.is_some()
            || self.state.link_cut.is_some()
            || self.state.param_edit.is_some()
            || self.state.param_scrub.is_some()
            || self.state.param_dropdown.is_some()
            || self.state.timeline_bpm_edit.is_some()
            || self.state.timeline_bar_edit.is_some()
            || self.state.right_marquee.is_some()
            || self.state.export_menu_drag.is_some()
        {
            return true;
        }
        if (self.project.lfo_sync_bpm() - self.state.export_menu.parsed_bpm().clamp(1.0, 400.0))
            .abs()
            > f32::EPSILON
        {
            return true;
        }
        input_has_project_mutation_intent(input)
    }
}

fn smoothed_fps(previous: f32, frame_elapsed: Duration) -> f32 {
    let inst = 1.0 / frame_elapsed.as_secs_f32().max(1e-4);
    if previous <= 0.0 {
        return inst;
    }
    previous * 0.9 + inst * 0.1
}

fn input_has_project_mutation_intent(input: &InputSnapshot) -> bool {
    input.left_down
        || input.left_clicked
        || input.right_down
        || input.right_clicked
        || input.middle_down
        || input.middle_clicked
        || input.alt_down
        || input.shift_down
        || input.wheel_lines_y.abs() > f32::EPSILON
        || input.toggle_pause
        || input.new_project
        || input.focus_all
        || input.open_help
        || input.toggle_node_open
        || input.toggle_add_menu
        || input.toggle_main_menu
        || input.menu_up
        || input.menu_down
        || input.param_dec
        || input.param_inc
        || input.menu_accept
        || !input.typed_text.is_empty()
        || input.param_backspace
        || input.param_delete
        || input.param_select_all
        || input.param_commit
        || input.param_cancel
}

#[cfg(test)]
mod tests {
    use super::{input_has_project_mutation_intent, smoothed_fps};
    use crate::gui::state::InputSnapshot;
    use std::time::Duration;

    #[test]
    fn smoothed_fps_uses_instant_value_when_previous_is_uninitialized() {
        let fps = smoothed_fps(0.0, Duration::from_millis(16));
        assert!(fps > 62.0 && fps < 63.0);
    }

    #[test]
    fn smoothed_fps_blends_previous_and_instant_values() {
        let fps = smoothed_fps(60.0, Duration::from_millis(10));
        assert!((fps - 64.0).abs() < 1e-3);
    }

    #[test]
    fn mutation_intent_is_false_for_idle_input() {
        assert!(!input_has_project_mutation_intent(&InputSnapshot::default()));
    }

    #[test]
    fn mutation_intent_is_true_for_text_or_click_input() {
        let text_input = InputSnapshot {
            typed_text: "n".to_string(),
            ..InputSnapshot::default()
        };
        assert!(input_has_project_mutation_intent(&text_input));

        let click_input = InputSnapshot {
            left_clicked: true,
            ..InputSnapshot::default()
        };
        assert!(input_has_project_mutation_intent(&click_input));
    }
}

//! Per-frame update and redraw orchestration.

use super::*;

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

impl GuiApp {
    /// Advance input/state and render one frame.
    pub(crate) fn redraw(&mut self) -> Result<(), Box<dyn Error>> {
        let frame_start = Instant::now();
        let frame_delta = frame_start.saturating_duration_since(self.last_frame_start);
        self.last_frame_start = frame_start;

        let input_start = Instant::now();
        let snapshot = self
            .input
            .snapshot(self.renderer.width(), self.renderer.height());
        let input_elapsed = input_start.elapsed();

        let update_start = Instant::now();
        let capture_scene_invalidation = self.should_capture_scene_invalidation_snapshot(&snapshot);
        let scene_invalidation_before =
            capture_scene_invalidation.then(|| SceneInvalidationSnapshot::capture(&self.project));
        let (resize_changed, consume_editor_input) = self.apply_panel_resize_input(&snapshot);
        let mut scene_dirty = resize_changed;
        scene_dirty |= self
            .project
            .set_lfo_sync_bpm(self.state.export_menu.parsed_bpm());
        if consume_editor_input {
            self.state.drag = None;
            self.state.wire_drag = None;
            self.state.hover_param_target = None;
            self.state.hover_param = None;
            self.state.hover_insert_link = None;
            if !self.state.auto_expanded_binding_nodes.is_empty() {
                for node_id in self.state.auto_expanded_binding_nodes.drain(..) {
                    scene_dirty |= self.project.collapse_node(
                        node_id,
                        self.panel_width,
                        self.renderer.height(),
                    );
                }
            }
            self.state.invalidation.invalidate_nodes();
            self.state.invalidation.invalidate_wires();
            self.state.invalidation.invalidate_overlays();
            self.state.prev_left_down = snapshot.left_down;
        } else {
            scene_dirty |= apply_preview_actions(
                &self.config,
                snapshot,
                &mut self.project,
                self.renderer.width(),
                self.panel_width,
                self.renderer.height(),
                &mut self.state,
            );
        }
        scene_dirty |= self.handle_pending_app_actions()?;
        self.state.export_menu.refresh_audio_duration_cache();
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
        if self.config.gui.benchmark_drag {
            scene_dirty |= self.apply_synthetic_drag();
        }
        let timeline_total_frames = self
            .state
            .export_menu
            .timeline_total_frames(self.config.animation.fps);
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
        self.sync_timeline_audio_preview(timeline_total_frames);
        self.state.avg_fps = smoothed_fps(self.state.avg_fps, frame_delta);
        self.apply_project_scoped_invalidation(scene_invalidation_before, resize_changed);
        let update_elapsed = update_start.elapsed();
        let hit_test_scans = self.project.take_hit_test_scan_count();

        let mut scene_elapsed = Duration::ZERO;
        let mut render_elapsed = Duration::ZERO;
        let mut submit_count = 0u32;
        let mut upload_bytes = 0u64;
        let mut ui_alloc_bytes = 0u64;
        let mut bridge_intersection_tests = 0u64;
        let mut signal_scope_samples = 0u64;
        let mut signal_scope_eval_ms = 0.0f64;
        let mut scene_nodes_ms = 0.0f64;
        let mut scene_edges_ms = 0.0f64;
        let mut scene_overlays_ms = 0.0f64;
        let export_active = self.export_session.is_some() || self.start_export_requested;
        if scene_dirty || self.needs_redraw || export_active {
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
                self.renderer.width(),
                self.renderer.height(),
                self.panel_width,
                self.config.animation.fps,
            );
            scene_elapsed = scene_start.elapsed();
            ui_alloc_bytes = frame.ui_alloc_bytes;
            bridge_intersection_tests = frame.bridge_intersection_tests;
            signal_scope_samples = frame.signal_scope_samples;
            signal_scope_eval_ms = frame.signal_scope_eval_ms;
            scene_nodes_ms = frame.nodes_ms;
            scene_edges_ms = frame.edges_ms;
            scene_overlays_ms = frame.overlays_ms;

            let render_start = Instant::now();
            self.renderer.render(
                frame,
                self.tex_view.frame(),
                self.panel_width,
                self.state.avg_fps,
            )?;
            render_elapsed = render_start.elapsed();
            let render_perf = self.renderer.take_perf_counters();
            submit_count = render_perf.submit_count;
            upload_bytes = render_perf.upload_bytes;
            ui_alloc_bytes = ui_alloc_bytes.saturating_add(render_perf.alloc_bytes);
            if self.export_session.is_some() {
                self.capture_export_frame()?;
            }
        }

        let total_elapsed = frame_start.elapsed();
        telemetry::record_counter_u64("gui.gpu.submit_count_per_frame", submit_count as u64);
        telemetry::record_counter_u64("gui.gpu.upload_bytes_per_frame", upload_bytes);
        telemetry::record_counter_u64("gui.hit_test.scan_count_per_frame", hit_test_scans);
        telemetry::record_counter_u64(
            "gui.wire.bridge_intersection_tests_per_frame",
            bridge_intersection_tests,
        );
        telemetry::record_counter_u64("signal_scope_samples_per_frame", signal_scope_samples);
        telemetry::record_timing_ms("signal_scope_eval_ms", signal_scope_eval_ms);
        telemetry::record_timing_ms("scene.nodes_ms", scene_nodes_ms);
        telemetry::record_timing_ms("scene.edges_ms", scene_edges_ms);
        telemetry::record_timing_ms("scene.overlays_ms", scene_overlays_ms);
        telemetry::record_counter_u64("gui.ui.alloc_bytes_per_frame", ui_alloc_bytes);
        let total_secs = total_elapsed.as_secs_f64();
        if total_secs > 0.0 {
            telemetry::record_counter(
                "gui.ui.alloc_bytes_per_second",
                ui_alloc_bytes as f64 / total_secs,
            );
        }
        self.perf.record(
            self.frame_counter,
            input_elapsed,
            update_elapsed,
            scene_elapsed,
            render_elapsed,
            total_elapsed,
            submit_count,
            upload_bytes,
            hit_test_scans,
            bridge_intersection_tests,
            ui_alloc_bytes,
        );
        if self.frame_counter == 0 {
            telemetry::record_timing("gui.startup.first_frame.total", total_elapsed);
            telemetry::record_timing("gui.startup.first_frame.scene", scene_elapsed);
            telemetry::record_timing("gui.startup.first_frame.render", render_elapsed);
        }
        self.update_loop_policy();
        self.update_title(frame_start);
        self.needs_redraw = false;
        let next_frame_counter = self.frame_counter.wrapping_add(1);
        if let Some(limit) = self.benchmark_frame_limit {
            let should_log_progress = next_frame_counter == 1
                || next_frame_counter % GUI_LOCKED_FPS as u64 == 0
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
        Ok(())
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

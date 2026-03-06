//! GUI app startup, window-event handling, and action dispatch.

use super::*;

impl GuiApp {
    /// Create one GPU-backed GUI app bound to the provided window.
    pub(crate) async fn new(config: V2Config, window: Arc<Window>) -> Result<Self, Box<dyn Error>> {
        let renderer = GuiRenderer::new(window.clone(), config.gui.vsync).await?;
        let panel_width = clamp_panel_width(launch_panel_width(renderer.width()), renderer.width());
        let benchmark_mode = is_benchmark_mode(&config);
        let mut project = GuiProject::new_empty(config.width, config.height);
        let benchmark_node =
            maybe_seed_benchmark_nodes(&config, &mut project, panel_width, renderer.height());
        let state = PreviewState::new(&config);
        let frame_budget = frame_budget(GUI_LOCKED_FPS);
        let benchmark_frame_limit = benchmark_frame_limit(&config);
        let now = Instant::now();
        let mut app = Self {
            config,
            panel_width,
            panel_resize_drag: None,
            resize_cursor_active: false,
            window,
            renderer,
            project,
            pending_autosave_load: None,
            state,
            input: InputCollector::default(),
            scene: SceneBuilder::default(),
            tex_view: TexViewerGenerator::default(),
            perf: GuiPerfRecorder::new(None),
            frame_budget,
            frame_deadline: now,
            last_frame_start: now,
            frame_counter: 0,
            benchmark_frame_limit,
            benchmark_node,
            export_session: None,
            start_export_requested: false,
            timeline_audio: TimelineAudioPreview::default(),
            export_bgra_scratch: Vec::new(),
            export_gray_scratch: Vec::new(),
            close_requested: false,
            needs_redraw: true,
            continuous_redraw: true,
            title_deadline: now,
            last_title: String::new(),
        }
        .with_perf_trace();
        if benchmark_mode {
            telemetry::record_timing("gui.startup.project_load", Duration::ZERO);
        } else {
            app.pending_autosave_load = app.spawn_pending_autosave_load();
        }
        println!(
            "[gui] {}x{} @ {}hz locked ({:?})",
            app.renderer.width(),
            app.renderer.height(),
            GUI_LOCKED_FPS,
            app.config.gui.vsync
        );
        println!(
            "[gui] controls: Esc=quit, F11=fullscreen, Space=play/pause, Shift+A=add node menu, `=main menu, Tab=open node, F1=context help, RMB=select, RMB drag=marquee, RMB on bound param value=unbind, Delete=remove selected, Toggle box=expand/collapse, Arrows=param select/adjust, Alt+LMB drag on param=scrub value, Alt+LMB drag elsewhere=cut links, timeline(play/pause + scrub)"
        );
        if let Some(limit) = benchmark_frame_limit {
            println!(
                "[gui-bench] benchmark mode active; auto-exit after {} frames",
                limit
            );
        }
        Ok(app)
    }

    /// Return current redraw deadline for the event loop.
    pub(crate) fn frame_deadline(&self) -> Instant {
        self.frame_deadline
    }

    /// Return true when GUI requested a clean application exit.
    pub(crate) fn should_exit(&self) -> bool {
        self.close_requested
    }

    /// Return true when this event should terminate the GUI loop.
    pub(crate) fn handle_window_event(&mut self, event: &WindowEvent) -> bool {
        if self.toggle_fullscreen_if_requested(event) {
            self.needs_redraw = true;
            return false;
        }
        match event {
            WindowEvent::CloseRequested => true,
            WindowEvent::DroppedFile(path) => {
                self.handle_dropped_file(path.as_path());
                self.needs_redraw = true;
                false
            }
            WindowEvent::Resized(size) => {
                self.renderer.resize(size.width, size.height);
                self.panel_width = clamp_panel_width(self.panel_width, self.renderer.width());
                self.needs_redraw = true;
                false
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.update_resize_cursor(Some((position.x as i32, position.y as i32)));
                self.input.handle_event(event);
                self.needs_redraw = true;
                false
            }
            WindowEvent::CursorLeft { .. } => {
                self.update_resize_cursor(None);
                self.input.handle_event(event);
                self.needs_redraw = true;
                false
            }
            WindowEvent::ScaleFactorChanged { .. } => false,
            _ => {
                self.input.handle_event(event);
                self.needs_redraw = true;
                false
            }
        }
    }

    /// Assign a dropped `.wav` file path into the timeline audio slot.
    fn handle_dropped_file(&mut self, path: &Path) {
        if !is_wav_path(path) {
            self.state.export_menu.set_status(format!(
                "Ignored dropped file (expected .wav): {}",
                path.display()
            ));
            self.state.invalidation.invalidate_overlays();
            return;
        }
        self.state.export_menu.audio_wav = path.to_string_lossy().to_string();
        self.state.export_menu.refresh_audio_duration_cache();
        let timeline_frames = self
            .state
            .export_menu
            .timeline_total_frames(self.config.animation.fps);
        self.state.export_menu.preview_total = timeline_frames;
        if let Some(duration_secs) = self.state.export_menu.audio_duration_secs() {
            self.state.export_menu.set_status(format!(
                "Timeline WAV assigned: {} ({duration_secs:.2}s)",
                path.display()
            ));
        } else {
            self.state.export_menu.set_status(format!(
                "Timeline WAV assigned: {} (duration unavailable)",
                path.display()
            ));
        }
        self.state.invalidation.invalidate_timeline();
        self.state.invalidation.invalidate_overlays();
    }

    /// Toggle window fullscreen mode on `F11` key press.
    fn toggle_fullscreen_if_requested(&mut self, event: &WindowEvent) -> bool {
        let WindowEvent::KeyboardInput { event, .. } = event else {
            return false;
        };
        if event.state != ElementState::Pressed || event.repeat {
            return false;
        }
        if !matches!(event.physical_key, PhysicalKey::Code(KeyCode::F11)) {
            return false;
        }
        if self.window.fullscreen().is_some() {
            self.window.set_fullscreen(None);
        } else {
            self.window
                .set_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        true
    }

    /// Request redraw when the frame deadline has elapsed.
    pub(crate) fn request_redraw_if_due(&mut self) {
        if !self.continuous_redraw && !self.needs_redraw {
            return;
        }
        let now = Instant::now();
        if now < self.frame_deadline {
            return;
        }
        self.window.request_redraw();
        while self.frame_deadline <= now {
            self.frame_deadline += self.frame_budget;
        }
    }

    /// Flush trace output before event-loop shutdown.
    pub(crate) fn shutdown(&mut self) -> Result<(), Box<dyn Error>> {
        self.timeline_audio.stop();
        let _ = self.stop_export_session("stopped");
        if !is_benchmark_mode(&self.config) {
            save_autosaved_project(&self.project)?;
        }
        self.perf.flush()
    }

    pub(super) fn handle_pending_app_actions(&mut self) -> Result<bool, Box<dyn Error>> {
        let Some(action) = self.state.pending_app_action.take() else {
            return Ok(false);
        };
        match action {
            PendingAppAction::SaveProject => {
                let Some(path) = pick_save_project_path() else {
                    self.state.export_menu.set_status("Save canceled");
                    return Ok(true);
                };
                match save_project_file(&self.project, path.as_path()) {
                    Ok(()) => {
                        self.state
                            .export_menu
                            .set_status(format!("Saved project: {}", path.display()));
                        println!("[gui] saved project: {}", path.display());
                    }
                    Err(err) => {
                        self.state
                            .export_menu
                            .set_status(format!("Save failed: {err}"));
                    }
                }
                Ok(true)
            }
            PendingAppAction::LoadProject => {
                let Some(path) = pick_load_project_path() else {
                    self.state.export_menu.set_status("Load canceled");
                    return Ok(true);
                };
                match load_project_file(path.as_path(), self.panel_width, self.renderer.height()) {
                    Ok(loaded) => {
                        let warning_count = loaded.warnings.len();
                        self.pending_autosave_load = None;
                        self.replace_loaded_project(loaded.project);
                        self.state
                            .export_menu
                            .set_status(load_status_message(path.as_path(), warning_count));
                        println!("[gui] loaded project: {}", path.display());
                        log_project_load_warnings(path.as_path(), &loaded.warnings);
                    }
                    Err(err) => {
                        self.state
                            .export_menu
                            .set_status(format!("Load failed: {err}"));
                    }
                }
                Ok(true)
            }
            PendingAppAction::StartExport => {
                self.start_export_requested = true;
                self.state.export_menu.set_status("Preparing export...");
                self.state.paused = true;
                Ok(true)
            }
            PendingAppAction::StopExport => Ok(self.stop_export_session("stopped by user")),
            PendingAppAction::ResetFeedback {
                feedback_node_id,
                accumulation_texture_node_id,
            } => {
                let cleared = self
                    .renderer
                    .reset_feedback_history(feedback_node_id, accumulation_texture_node_id);
                if cleared {
                    self.state.export_menu.set_status(format!(
                        "Reset feedback history for node #{feedback_node_id}"
                    ));
                } else {
                    self.state.export_menu.set_status(format!(
                        "Feedback history already clear for node #{feedback_node_id}"
                    ));
                }
                self.state.invalidation.invalidate_overlays();
                Ok(true)
            }
            PendingAppAction::Exit => {
                self.close_requested = true;
                Ok(true)
            }
        }
    }
}

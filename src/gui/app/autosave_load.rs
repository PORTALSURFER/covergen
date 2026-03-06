//! Deferred autosave loading so first-frame startup stays responsive.

use super::*;
use std::sync::mpsc::TryRecvError;

impl GuiApp {
    /// Start one background autosave read/parse task for post-startup recovery.
    pub(super) fn spawn_pending_autosave_load(&self) -> Option<PendingAutosaveLoad> {
        let (tx, rx) = mpsc::channel();
        let started_at = Instant::now();
        let launch_project_invalidation = self.project.invalidation();
        let spawn_result = thread::Builder::new()
            .name("gui-autosave-load".to_string())
            .spawn(move || {
                let result = load_autosaved_project_payload().map_err(|err| format!("{err}"));
                let _ = tx.send(result);
            });
        match spawn_result {
            Ok(_) => Some(PendingAutosaveLoad {
                started_at,
                launch_project_invalidation,
                rx,
            }),
            Err(err) => {
                eprintln!("[gui] failed to spawn autosave loader: {err}");
                None
            }
        }
    }

    /// Poll the background autosave task and apply it once the payload is ready.
    pub(super) fn try_apply_pending_autosave_load(&mut self) -> Result<bool, Box<dyn Error>> {
        let Some(pending) = self.pending_autosave_load.as_ref() else {
            return Ok(false);
        };
        let load_result = match pending.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(false),
            Err(TryRecvError::Disconnected) => Err(String::from("autosave loader disconnected")),
        };
        let pending = self
            .pending_autosave_load
            .take()
            .expect("pending autosave state should exist while applying");
        telemetry::record_timing("gui.startup.project_load", pending.started_at.elapsed());
        let Some(persisted) =
            load_result.map_err(|err| format!("failed to load autosave: {err}"))?
        else {
            return Ok(false);
        };
        if !should_apply_pending_autosave(
            pending.launch_project_invalidation,
            self.project.invalidation(),
        ) {
            eprintln!("[gui] skipped autosave restore because the project changed during startup");
            return Ok(false);
        }
        let loaded = GuiProject::from_persisted_with_warnings(
            persisted,
            self.panel_width,
            self.renderer.height(),
        )
        .map_err(|err| format!("failed to restore autosave on main thread: {err}"))?;
        println!(
            "[gui] loaded autosave from {}",
            autosave_project_path().display()
        );
        log_project_load_warnings(autosave_project_path().as_path(), &loaded.warnings);
        self.replace_loaded_project(loaded.project);
        Ok(true)
    }

    /// Replace the current project and reset retained runtime state around it.
    pub(super) fn replace_loaded_project(&mut self, project: GuiProject) {
        let _ = self.stop_export_session("stopped");
        self.timeline_audio.stop();
        self.start_export_requested = false;
        self.project = project;
        self.state = PreviewState::new(&self.config);
        self.state.invalidation.invalidate_all();
        self.scene = SceneBuilder::default();
        self.tex_view = TexViewerGenerator::default();
        self.needs_redraw = true;
    }
}

/// Return true when startup autosave recovery can still safely replace the project.
fn should_apply_pending_autosave(
    launch_invalidation: GuiProjectInvalidation,
    current_invalidation: GuiProjectInvalidation,
) -> bool {
    launch_invalidation == current_invalidation
}

#[cfg(test)]
mod tests {
    use super::should_apply_pending_autosave;
    use crate::gui::project::GuiProjectInvalidation;

    #[test]
    fn pending_autosave_applies_when_project_is_unchanged() {
        let launch = GuiProjectInvalidation {
            nodes: 1,
            wires: 2,
            tex_eval: 3,
        };
        assert!(should_apply_pending_autosave(launch, launch));
    }

    #[test]
    fn pending_autosave_skips_when_project_mutated_since_launch() {
        let launch = GuiProjectInvalidation {
            nodes: 1,
            wires: 2,
            tex_eval: 3,
        };
        let current = GuiProjectInvalidation {
            nodes: 1,
            wires: 3,
            tex_eval: 3,
        };
        assert!(!should_apply_pending_autosave(launch, current));
    }
}

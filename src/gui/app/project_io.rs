//! GUI project persistence, file picking, and autosave recovery.

use super::*;
use rfd::FileDialog;
use std::io::ErrorKind;
use std::time::{SystemTime, UNIX_EPOCH};

/// Controls behavior for one persisted project load attempt.
#[derive(Clone, Copy, Debug)]
struct ProjectLoadOptions {
    allow_missing: bool,
    quarantine_corrupt_autosave: bool,
}

impl ProjectLoadOptions {
    const fn autosave() -> Self {
        Self {
            allow_missing: true,
            quarantine_corrupt_autosave: true,
        }
    }

    const fn explicit() -> Self {
        Self {
            allow_missing: false,
            quarantine_corrupt_autosave: false,
        }
    }

    #[cfg(test)]
    const fn optional_manual_candidate() -> Self {
        Self {
            allow_missing: true,
            quarantine_corrupt_autosave: false,
        }
    }
}

/// Return autosave file path in one base directory.
fn autosave_project_path_in(base_dir: &Path) -> PathBuf {
    base_dir.join(GUI_PROJECT_AUTOSAVE_FILE)
}

/// Return autosave file path in the process working directory.
pub(super) fn autosave_project_path() -> PathBuf {
    autosave_project_path_in(working_directory().as_path())
}

/// Return explicit save/load project path in one base directory.
#[cfg(test)]
fn manual_project_path_in(base_dir: &Path) -> PathBuf {
    base_dir.join(GUI_PROJECT_SAVE_FILE)
}

/// Return native picker initial directory for manual project save/load.
fn project_picker_directory() -> PathBuf {
    working_directory()
}

/// Open one native save-file picker for GUI projects.
pub(super) fn pick_save_project_path() -> Option<PathBuf> {
    FileDialog::new()
        .set_title("Save Project")
        .set_directory(project_picker_directory())
        .set_file_name(GUI_PROJECT_SAVE_FILE)
        .add_filter("Covergen Project", &["json"])
        .save_file()
}

/// Open one native open-file picker for GUI projects.
pub(super) fn pick_load_project_path() -> Option<PathBuf> {
    FileDialog::new()
        .set_title("Load Project")
        .set_directory(project_picker_directory())
        .add_filter("Covergen Project", &["json"])
        .pick_file()
}

/// Return legacy hidden project path used by older GUI builds.
#[cfg(test)]
fn legacy_manual_project_path_in(base_dir: &Path) -> PathBuf {
    base_dir.join(GUI_PROJECT_SAVE_FILE_LEGACY)
}

/// Return ordered project-load candidates for one base directory.
#[cfg(test)]
fn manual_project_load_candidates_in(base_dir: &Path) -> [PathBuf; 3] {
    [
        manual_project_path_in(base_dir),
        legacy_manual_project_path_in(base_dir),
        autosave_project_path_in(base_dir),
    ]
}

/// Load the autosave payload without rebuilding the full GUI project on this thread.
pub(super) fn load_autosaved_project_payload() -> Result<Option<PersistedGuiProject>, Box<dyn Error>>
{
    let path = autosave_project_path();
    load_project_payload_with_options(path.as_path(), ProjectLoadOptions::autosave())
}

/// Load the first existing project candidate from one directory.
#[cfg(test)]
fn load_manual_project_from_dir(
    base_dir: &Path,
    panel_width: usize,
    panel_height: usize,
) -> Result<Option<(PersistedProjectLoadOutcome, PathBuf)>, Box<dyn Error>> {
    for path in manual_project_load_candidates_in(base_dir) {
        match load_project_with_options(
            path.as_path(),
            panel_width,
            panel_height,
            ProjectLoadOptions::optional_manual_candidate(),
        ) {
            Ok(Some(project)) => return Ok(Some((project, path))),
            Ok(None) => continue,
            Err(err) => {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidData,
                    format!("failed to load {}: {err}", path.display()),
                )
                .into());
            }
        }
    }
    Ok(None)
}

/// Load one explicit GUI project file path.
pub(super) fn load_project_file(
    path: &Path,
    panel_width: usize,
    panel_height: usize,
) -> Result<PersistedProjectLoadOutcome, Box<dyn Error>> {
    load_project_with_options(
        path,
        panel_width,
        panel_height,
        ProjectLoadOptions::explicit(),
    )?
    .ok_or_else(|| {
        format!(
            "failed to load project file {}: file does not exist",
            path.display()
        )
        .into()
    })
}

/// Unified persisted-project load path for autosave/manual load variants.
fn load_project_with_options(
    path: &Path,
    panel_width: usize,
    panel_height: usize,
    options: ProjectLoadOptions,
) -> Result<Option<PersistedProjectLoadOutcome>, Box<dyn Error>> {
    let Some(persisted) = load_project_payload_with_options(path, options)? else {
        return Ok(None);
    };
    build_project_from_payload(path, persisted, panel_width, panel_height, options)
}

/// Read and parse one persisted project payload while preserving autosave quarantine behavior.
fn load_project_payload_with_options(
    path: &Path,
    options: ProjectLoadOptions,
) -> Result<Option<PersistedGuiProject>, Box<dyn Error>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound && options.allow_missing => return Ok(None),
        Err(err) => return Err(Box::new(err)),
    };
    let persisted = match serde_json::from_slice::<PersistedGuiProject>(bytes.as_slice()) {
        Ok(value) => value,
        Err(err) => return handle_corrupt_project_load(path, options, err),
    };
    Ok(Some(persisted))
}

/// Rebuild one in-memory GUI project from a persisted payload with autosave quarantine fallback.
fn build_project_from_payload(
    path: &Path,
    persisted: PersistedGuiProject,
    panel_width: usize,
    panel_height: usize,
    options: ProjectLoadOptions,
) -> Result<Option<PersistedProjectLoadOutcome>, Box<dyn Error>> {
    match GuiProject::from_persisted_with_warnings(persisted, panel_width, panel_height) {
        Ok(value) => Ok(Some(value)),
        Err(err) => handle_corrupt_project_load(path, options, err),
    }
}

/// Quarantine one malformed autosave when the error class indicates broken project contents.
fn handle_corrupt_project_load<T, E>(
    path: &Path,
    options: ProjectLoadOptions,
    err: E,
) -> Result<Option<T>, Box<dyn Error>>
where
    E: Error + 'static,
{
    if options.quarantine_corrupt_autosave
        && path.exists()
        && should_quarantine_autosave_load_error(&err)
    {
        let quarantined = quarantine_corrupt_autosave(path)?;
        telemetry::record_counter_u64("gui.project.autosave_quarantined", 1);
        eprintln!(
            "[gui] quarantined corrupt autosave {} -> {} ({err})",
            path.display(),
            quarantined.display()
        );
        return Ok(None);
    }
    Err(Box::new(err))
}

/// Move one malformed autosave to a timestamped quarantine path.
fn quarantine_corrupt_autosave(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    quarantine_corrupt_autosave_with(path, |source, destination| fs::rename(source, destination))
}

fn quarantine_corrupt_autosave_with<F>(path: &Path, rename: F) -> Result<PathBuf, Box<dyn Error>>
where
    F: FnOnce(&Path, &Path) -> Result<(), std::io::Error>,
{
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("covergen_gui_autosave");
    let quarantined = path.with_file_name(format!("{file_name}.corrupt-{timestamp}"));
    rename(path, quarantined.as_path())?;
    Ok(quarantined)
}

/// Return true when one autosave load error indicates corrupt file contents.
fn should_quarantine_autosave_load_error(err: &(dyn Error + 'static)) -> bool {
    if err.is::<serde_json::Error>() || err.is::<PersistedProjectLoadError>() {
        return true;
    }
    err.downcast_ref::<std::io::Error>()
        .map(|io_err| io_err.kind() == ErrorKind::InvalidData)
        .unwrap_or(false)
}

/// Format one status line after loading a project from disk.
pub(super) fn load_status_message(path: &Path, warning_count: usize) -> String {
    if warning_count == 0 {
        return format!("Loaded project: {}", path.display());
    }
    format!(
        "Loaded project: {} ({} dropped unknown params; see log)",
        path.display(),
        warning_count
    )
}

/// Emit non-fatal persisted-load warnings with actionable context.
pub(super) fn log_project_load_warnings(path: &Path, warnings: &[PersistedProjectLoadWarning]) {
    if warnings.is_empty() {
        return;
    }
    eprintln!(
        "[gui] load warnings for {}: {} dropped unknown persisted params",
        path.display(),
        warnings.len()
    );
    for warning in warnings {
        eprintln!("[gui]   - {warning}");
    }
}

/// Save current GUI graph to autosave file atomically.
pub(super) fn save_autosaved_project(project: &GuiProject) -> Result<(), Box<dyn Error>> {
    let path = autosave_project_path();
    save_project_file(project, path.as_path())
}

pub(super) fn save_project_file(project: &GuiProject, path: &Path) -> Result<(), Box<dyn Error>> {
    let tmp = path.with_extension("tmp");
    let data = serde_json::to_vec_pretty(&project.to_persisted())?;
    fs::write(tmp.as_path(), data)?;
    let result = commit_saved_project_file(tmp.as_path(), path);
    if result.is_err() {
        let _ = fs::remove_file(tmp.as_path());
    }
    result
}

/// Commit one tmp project save file into the destination path.
///
/// This prefers direct rename (atomic replace on Unix). When direct rename
/// fails on platforms that do not replace existing files, it moves the
/// previous destination to a backup path and restores it on commit failure.
fn commit_saved_project_file(tmp: &Path, dst: &Path) -> Result<(), Box<dyn Error>> {
    if let Ok(meta) = fs::metadata(dst) {
        if !meta.is_file() {
            return Err(format!(
                "project save destination must be a file path: {}",
                dst.display()
            )
            .into());
        }
    }

    match fs::rename(tmp, dst) {
        Ok(()) => return Ok(()),
        Err(err) if !dst.exists() => {
            return Err(format!(
                "failed to finalize project save to {}: {err}",
                dst.display()
            )
            .into())
        }
        Err(_) => {}
    }

    let backup = dst.with_extension("bak");
    if backup.exists() {
        fs::remove_file(backup.as_path())?;
    }
    fs::rename(dst, backup.as_path())?;
    match fs::rename(tmp, dst) {
        Ok(()) => {
            let _ = fs::remove_file(backup.as_path());
            Ok(())
        }
        Err(err) => {
            let restore_result = fs::rename(backup.as_path(), dst);
            if let Err(restore_err) = restore_result {
                return Err(format!(
                    "failed to finalize project save to {}: {err}; failed to restore previous file: {restore_err}",
                    dst.display()
                )
                .into());
            }
            Err(format!(
                "failed to finalize project save to {}: {err}; previous file restored",
                dst.display()
            )
            .into())
        }
    }
}

pub(super) fn is_wav_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("wav"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests;

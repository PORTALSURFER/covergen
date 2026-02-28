use super::*;
use std::io::ErrorKind;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("covergen_gui_app_{test_name}_{nanos}"));
    fs::create_dir_all(dir.as_path()).expect("create temp dir");
    dir
}

#[test]
fn manual_project_load_candidates_prioritize_explicit_then_legacy_then_autosave() {
    let base_dir = Path::new("workspace");
    let candidates = manual_project_load_candidates_in(base_dir);
    assert_eq!(candidates[0], base_dir.join(GUI_PROJECT_SAVE_FILE));
    assert_eq!(candidates[1], base_dir.join(GUI_PROJECT_SAVE_FILE_LEGACY));
    assert_eq!(candidates[2], base_dir.join(GUI_PROJECT_AUTOSAVE_FILE));
}

#[test]
fn load_manual_project_uses_legacy_file_when_explicit_missing() {
    let dir = temp_dir("legacy_fallback");
    let path = legacy_manual_project_path_in(dir.as_path());
    let project = GuiProject::new_empty(512, 288);
    save_project_file(&project, path.as_path()).expect("save legacy project");

    let (loaded, loaded_path) = load_manual_project_from_dir(dir.as_path(), 640, 480)
        .expect("load project")
        .expect("legacy fallback should return project");

    assert_eq!(loaded_path, path);
    assert_eq!(loaded.project.to_persisted().preview_width, 512);
    assert!(loaded.warnings.is_empty());

    let _ = fs::remove_dir_all(dir.as_path());
}

#[test]
fn load_manual_project_prefers_explicit_file_over_legacy() {
    let dir = temp_dir("explicit_priority");
    let explicit = manual_project_path_in(dir.as_path());
    let legacy = legacy_manual_project_path_in(dir.as_path());
    let explicit_project = GuiProject::new_empty(1024, 576);
    let legacy_project = GuiProject::new_empty(320, 180);
    save_project_file(&legacy_project, legacy.as_path()).expect("save legacy project");
    save_project_file(&explicit_project, explicit.as_path()).expect("save explicit project");

    let (loaded, loaded_path) = load_manual_project_from_dir(dir.as_path(), 640, 480)
        .expect("load project")
        .expect("explicit project should return project");

    assert_eq!(loaded_path, explicit);
    assert_eq!(loaded.project.to_persisted().preview_width, 1024);
    assert!(loaded.warnings.is_empty());

    let _ = fs::remove_dir_all(dir.as_path());
}

#[test]
fn load_autosaved_project_quarantines_corrupt_payload() {
    let dir = temp_dir("autosave_corrupt_quarantine");
    let autosave = autosave_project_path_in(dir.as_path());
    fs::write(autosave.as_path(), b"{not-valid-json").expect("write corrupt autosave");

    let loaded =
        load_project_with_options(autosave.as_path(), 640, 480, ProjectLoadOptions::autosave())
            .expect("load autosave");
    assert!(
        loaded.is_none(),
        "corrupt autosave should be quarantined and treated as missing"
    );
    assert!(
        !autosave.exists(),
        "autosave path should be moved away after quarantine"
    );
    let mut quarantined_count = 0usize;
    for entry in fs::read_dir(dir.as_path()).expect("read temp dir") {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(&format!("{GUI_PROJECT_AUTOSAVE_FILE}.corrupt-")) {
            quarantined_count = quarantined_count.saturating_add(1);
        }
    }
    assert_eq!(
        quarantined_count, 1,
        "exactly one quarantined autosave copy should be created"
    );

    let _ = fs::remove_dir_all(dir.as_path());
}

#[test]
fn load_autosaved_project_does_not_quarantine_non_corrupt_io_errors() {
    let dir = temp_dir("autosave_non_corrupt_io_error");
    let autosave_dir = autosave_project_path_in(dir.as_path());
    fs::create_dir_all(autosave_dir.as_path()).expect("create autosave directory path");

    let result = load_project_with_options(
        autosave_dir.as_path(),
        640,
        480,
        ProjectLoadOptions::autosave(),
    );
    assert!(
        result.is_err(),
        "directory read failure should propagate as IO error"
    );
    assert!(
        autosave_dir.exists() && autosave_dir.is_dir(),
        "non-corrupt IO errors should not quarantine autosave path"
    );
    let corrupt_files = fs::read_dir(dir.as_path())
        .expect("read temp dir")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|name| name.contains(".corrupt-"))
                .unwrap_or(false)
        })
        .count();
    assert_eq!(
        corrupt_files, 0,
        "non-corrupt IO failures should not create quarantine artifacts"
    );

    let _ = fs::remove_dir_all(dir.as_path());
}

#[test]
fn quarantine_corrupt_autosave_surfaces_rename_failure() {
    let dir = temp_dir("autosave_quarantine_rename_failure");
    let autosave = autosave_project_path_in(dir.as_path());
    fs::write(autosave.as_path(), b"{broken-json").expect("write corrupt autosave");

    let err = quarantine_corrupt_autosave_with(autosave.as_path(), |_src, _dst| {
        Err(std::io::Error::new(
            ErrorKind::PermissionDenied,
            "permission denied while renaming",
        ))
    })
    .expect_err("rename failure should be surfaced");
    assert!(
        err.to_string().contains("permission denied"),
        "rename failure message should include actionable context"
    );

    let _ = fs::remove_dir_all(dir.as_path());
}

#[test]
fn bundled_circle_noise_feedback_example_loads() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/graphs/circle_noise_feedback_trail.json");
    let loaded = load_project_file(path.as_path(), 1280, 720).expect("load example project");
    assert!(loaded.warnings.is_empty());
    assert!(
        loaded
            .project
            .nodes()
            .iter()
            .any(|node| node.kind().stable_id() == "tex.feedback"),
        "example graph should include tex.feedback"
    );
    let circle_id = loaded
        .project
        .nodes()
        .iter()
        .find(|node| node.kind().stable_id() == "tex.circle")
        .map(|node| node.id())
        .expect("example graph should include tex.circle");
    let blend_id = loaded
        .project
        .nodes()
        .iter()
        .find(|node| node.kind().stable_id() == "tex.blend")
        .map(|node| node.id())
        .expect("example graph should include tex.blend");
    let blend_tex_param = loaded
        .project
        .node_param_slot_index(blend_id, "blend_tex")
        .expect("tex.blend should expose blend_tex");
    assert_eq!(
        loaded
            .project
            .texture_source_for_param(blend_id, blend_tex_param),
        Some(circle_id),
        "trail example should composite raw circle as the live layer"
    );
}

#[test]
fn save_project_file_cleans_tmp_when_destination_is_invalid() {
    let dir = temp_dir("save_invalid_destination");
    let invalid_destination = dir.join("project.json");
    fs::create_dir_all(invalid_destination.as_path()).expect("create invalid destination dir");

    let project = GuiProject::new_empty(320, 240);
    let result = save_project_file(&project, invalid_destination.as_path());
    assert!(
        result.is_err(),
        "save should fail when destination is a directory"
    );
    assert!(
        !invalid_destination.with_extension("tmp").exists(),
        "failed save should not leave tmp files behind"
    );

    let _ = fs::remove_dir_all(dir.as_path());
}

#[test]
fn save_project_file_overwrite_does_not_leave_backup_artifacts() {
    let dir = temp_dir("save_overwrite_backup_cleanup");
    let path = dir.join("graph.json");
    let project = GuiProject::new_empty(320, 240);
    let updated_project = GuiProject::new_empty(640, 360);

    save_project_file(&project, path.as_path()).expect("initial save should succeed");
    save_project_file(&updated_project, path.as_path()).expect("overwrite save should succeed");

    assert!(
        !path.with_extension("bak").exists(),
        "successful overwrite should not leave backup artifacts"
    );

    let _ = fs::remove_dir_all(dir.as_path());
}

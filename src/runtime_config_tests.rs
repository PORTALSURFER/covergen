//! Tests for runtime CLI configuration parsing and validation.

use crate::art_direction::{
    ChaosDirection, EnergyDirection, MoodDirection, PaletteDirection, SymmetryDirection,
};
use crate::runtime_config::{AnimationMotion, GuiVsync, V2Config, V2Profile};

#[test]
fn reels_mode_enables_animation_and_dimensions() {
    let cfg = V2Config::parse(vec!["--reels".to_string()]).expect("reels configuration");
    assert_eq!(cfg.width, 1080);
    assert_eq!(cfg.height, 1920);
    assert!(cfg.animation.enabled);
}

#[test]
fn animate_output_defaults_to_mp4_extension() {
    let cfg = V2Config::parse(vec![
        "--animate".to_string(),
        "--output".to_string(),
        "clip".to_string(),
    ])
    .expect("animation configuration");
    assert!(cfg.output.ends_with(".mp4"));
}

#[test]
fn motion_profile_parses_with_alias() {
    let cfg =
        V2Config::parse(vec!["--motion".to_string(), "soft".to_string()]).expect("motion profile");
    assert_eq!(cfg.animation.motion, AnimationMotion::Gentle);
}

#[test]
fn profile_parses_with_alias() {
    let cfg = V2Config::parse(vec!["--profile".to_string(), "perf".to_string()])
        .expect("runtime profile");
    assert_eq!(cfg.profile, V2Profile::Performance);
}

#[test]
fn explicit_seed_is_preserved() {
    let cfg = V2Config::parse(vec!["--seed".to_string(), "12345".to_string()])
        .expect("seeded configuration");
    assert_eq!(cfg.seed, 12345);
}

#[test]
fn omitted_seed_generates_runtime_seed() {
    let cfg = V2Config::parse(Vec::new()).expect("default configuration");
    assert_ne!(cfg.seed, 0);
}

#[test]
fn gui_target_fps_defaults_to_sixty() {
    let cfg = V2Config::parse(Vec::new()).expect("default configuration");
    assert_eq!(cfg.gui.target_fps, 60);
}

#[test]
fn animation_defaults_to_thirty_seconds_at_sixty_fps() {
    let cfg = V2Config::parse(Vec::new()).expect("default configuration");
    assert_eq!(cfg.animation.seconds, 30);
    assert_eq!(cfg.animation.fps, 60);
}

#[test]
fn parse_exploration_flags() {
    let cfg = V2Config::parse(vec![
        "--explore-candidates".to_string(),
        "12".to_string(),
        "--explore-size".to_string(),
        "256".to_string(),
    ])
    .expect("exploration configuration");
    assert_eq!(cfg.selection.explore_candidates, 12);
    assert_eq!(cfg.selection.explore_size, 256);
}

#[test]
fn parse_manifest_flags() {
    let cfg = V2Config::parse(vec![
        "--manifest-in".to_string(),
        "fixtures/replay.json".to_string(),
        "--manifest-out".to_string(),
        "out/replay.json".to_string(),
    ])
    .expect("manifest flags should parse");
    assert_eq!(cfg.manifest_in.as_deref(), Some("fixtures/replay.json"));
    assert_eq!(cfg.manifest_out.as_deref(), Some("out/replay.json"));
}

#[test]
fn low_res_explore_config_scales_dimensions() {
    let cfg = V2Config::parse(vec![
        "--width".to_string(),
        "1920".to_string(),
        "--height".to_string(),
        "1080".to_string(),
        "--explore-candidates".to_string(),
        "10".to_string(),
        "--explore-size".to_string(),
        "320".to_string(),
    ])
    .expect("explore configuration");
    let low = cfg
        .low_res_explore_config()
        .expect("low-res explore config should be available");
    assert_eq!(low.width, 320);
    assert_eq!(low.height, 180);
    assert_eq!(low.antialias, 1);
    assert!(!low.selection.enabled());
}

#[test]
fn exploration_rejected_for_animation_mode() {
    let err = V2Config::parse(vec![
        "--animate".to_string(),
        "--explore-candidates".to_string(),
        "8".to_string(),
    ])
    .expect_err("animation+exploration should be rejected");
    assert!(err.to_string().contains("explore-candidates"));
}

#[test]
fn exploration_rejected_for_manifest_replay_mode() {
    let err = V2Config::parse(vec![
        "--manifest-in".to_string(),
        "fixtures/replay.json".to_string(),
        "--explore-candidates".to_string(),
        "8".to_string(),
    ])
    .expect_err("manifest+exploration should be rejected");
    assert!(err.to_string().contains("explore-candidates"));
}

#[test]
fn art_direction_flags_parse() {
    let cfg = V2Config::parse(vec![
        "--mood".to_string(),
        "dark".to_string(),
        "--energy".to_string(),
        "high".to_string(),
        "--symmetry".to_string(),
        "strong".to_string(),
        "--chaos".to_string(),
        "wild".to_string(),
        "--palette".to_string(),
        "mono".to_string(),
    ])
    .expect("art direction configuration");
    assert_eq!(cfg.art_direction.mood, MoodDirection::Moody);
    assert_eq!(cfg.art_direction.energy, EnergyDirection::High);
    assert_eq!(cfg.art_direction.symmetry, SymmetryDirection::High);
    assert_eq!(cfg.art_direction.chaos, ChaosDirection::Wild);
    assert_eq!(cfg.art_direction.palette, PaletteDirection::Monochrome);
}

#[test]
fn gui_flags_parse() {
    let cfg = V2Config::parse(vec![
        "--gui-target-fps".to_string(),
        "200".to_string(),
        "--gui-vsync".to_string(),
        "adaptive".to_string(),
        "--gui-perf-trace".to_string(),
        "target/gui_trace.csv".to_string(),
        "--gui-benchmark-drag".to_string(),
    ])
    .expect("gui flags should parse");
    assert_eq!(cfg.gui.target_fps, 200);
    assert_eq!(cfg.gui.vsync, GuiVsync::Adaptive);
    assert_eq!(cfg.gui.perf_trace.as_deref(), Some("target/gui_trace.csv"));
    assert!(cfg.gui.benchmark_drag);
}

#[test]
fn gui_target_fps_validation_rejects_extreme_values() {
    let err = V2Config::parse(vec!["--gui-target-fps".to_string(), "10".to_string()])
        .expect_err("gui-target-fps below minimum should be rejected");
    assert!(err.to_string().contains("gui-target-fps"));
}

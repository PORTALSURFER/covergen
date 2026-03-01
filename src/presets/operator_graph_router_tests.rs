//! Tests for the `op-router` preset.

use super::{build_preset_graph_with_catalogs, NodeCatalog, SubgraphCatalog};
use crate::graph::NodeKind;
use crate::runtime_config::{AnimationConfig, AnimationMotion, V2Config, V2Profile};

fn config(seed: u32) -> V2Config {
    V2Config {
        width: 512,
        height: 512,
        seed,
        count: 1,
        output: "test.png".to_string(),
        layers: 6,
        antialias: 1,
        preset: "op-router".to_string(),
        profile: V2Profile::Quality,
        manifest_out: None,
        manifest_in: None,
        art_direction: crate::art_direction::ArtDirectionConfig::default(),
        animation: AnimationConfig {
            enabled: false,
            seconds: 30,
            fps: 30,
            keep_frames: false,
            motion: AnimationMotion::Normal,
        },
        selection: crate::runtime_config::SelectionConfig {
            explore_candidates: 0,
            explore_size: 320,
            novelty_window: 0,
        },
        gui: crate::runtime_config::GuiConfig::default(),
    }
}

#[test]
fn operator_router_is_seed_deterministic() {
    let presets = super::preset_catalog::PresetCatalog::with_builtins().expect("preset catalog");
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let cfg = config(4242);

    let a = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules).expect("graph a");
    let b = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules).expect("graph b");
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn operator_router_has_lane_taps_and_mixed_ops() {
    let presets = super::preset_catalog::PresetCatalog::with_builtins().expect("preset catalog");
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let cfg = config(7878);

    let graph = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules)
        .expect("graph should build");

    let mut outputs = 0usize;
    let mut cameras = 0usize;
    let mut layers = 0usize;
    let mut masks = 0usize;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::Output(_) => outputs += 1,
            NodeKind::TopCameraRender(_) => cameras += 1,
            NodeKind::GenerateLayer(_) => layers += 1,
            NodeKind::Mask(_) => masks += 1,
            _ => {}
        }
    }

    assert!(outputs >= 4);
    assert!(cameras >= 4);
    assert!(layers >= 4);
    assert!(masks >= 2);
}

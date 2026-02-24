//! Tests for TouchDesigner-style presets and random graph constraints.

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
        layers: 5,
        antialias: 1,
        preset: "td-random-network".to_string(),
        profile: V2Profile::Quality,
        animation: AnimationConfig {
            enabled: false,
            seconds: 30,
            fps: 30,
            keep_frames: false,
            motion: AnimationMotion::Normal,
        },
    }
}

#[test]
fn td_random_network_is_seed_deterministic() {
    let presets = super::preset_catalog::PresetCatalog::with_builtins().expect("preset catalog");
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let cfg = config(17);

    let a = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules).expect("graph a");
    let b = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules).expect("graph b");
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn td_random_network_contains_chop_sop_camera_and_masking() {
    let presets = super::preset_catalog::PresetCatalog::with_builtins().expect("preset catalog");
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let cfg = config(33);
    let graph = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules).expect("graph");

    let mut has_chop = false;
    let mut has_sop = false;
    let mut has_camera = false;
    let mut has_source_noise = false;
    let mut has_mask = false;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::ChopLfo(_) | NodeKind::ChopMath(_) | NodeKind::ChopRemap(_) => {
                has_chop = true
            }
            NodeKind::SopCircle(_) | NodeKind::SopSphere(_) => has_sop = true,
            NodeKind::TopCameraRender(_) => has_camera = true,
            NodeKind::SourceNoise(_) => has_source_noise = true,
            NodeKind::Mask(_) => has_mask = true,
            _ => {}
        }
    }

    assert!(has_chop);
    assert!(has_sop);
    assert!(has_camera);
    assert!(has_source_noise);
    assert!(has_mask);
}

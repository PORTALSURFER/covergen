//! Tests for the `op-multi-stage` preset topology.

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
        preset: "op-multi-stage".to_string(),
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
fn operator_multi_stage_is_seed_deterministic() {
    let presets = super::preset_catalog::PresetCatalog::with_builtins().expect("preset catalog");
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let cfg = config(101);

    let a = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules).expect("graph a");
    let b = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules).expect("graph b");
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn operator_multi_stage_contains_structured_td_families() {
    let presets = super::preset_catalog::PresetCatalog::with_builtins().expect("preset catalog");
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let cfg = config(303);

    let graph = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules)
        .expect("graph should build");

    let mut chop = 0usize;
    let mut sop = 0usize;
    let mut camera = 0usize;
    let mut source_noise = 0usize;
    let mut mask = 0usize;
    let mut blend = 0usize;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::ChopLfo(_) | NodeKind::ChopMath(_) | NodeKind::ChopRemap(_) => chop += 1,
            NodeKind::SopCircle(_) | NodeKind::SopSphere(_) => sop += 1,
            NodeKind::TopCameraRender(_) => camera += 1,
            NodeKind::SourceNoise(_) => source_noise += 1,
            NodeKind::Mask(_) => mask += 1,
            NodeKind::Blend(_) => blend += 1,
            _ => {}
        }
    }

    assert!(chop >= 5);
    assert!(sop >= 4);
    assert!(camera >= 3);
    assert!(source_noise >= 3);
    assert!(mask >= 3);
    assert!(blend >= 3);
}

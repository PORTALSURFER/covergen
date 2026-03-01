//! Tests for the `op-modular-network` preset.

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
        preset: "op-modular-network".to_string(),
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
        },
        gui: crate::runtime_config::GuiConfig::default(),
    }
}

#[test]
fn operator_modular_network_is_seed_deterministic() {
    let presets = super::preset_catalog::PresetCatalog::with_builtins().expect("preset catalog");
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let cfg = config(515);

    let a = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules).expect("graph a");
    let b = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules).expect("graph b");
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn operator_modular_network_has_multiple_outputs_and_modules() {
    let presets = super::preset_catalog::PresetCatalog::with_builtins().expect("preset catalog");
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let cfg = config(717);

    let graph = build_preset_graph_with_catalogs(&cfg, &presets, &nodes, &modules)
        .expect("graph should build");

    let mut output_count = 0usize;
    let mut has_generate = false;
    let mut has_source_noise = false;
    let mut has_mask = false;
    let mut has_camera = false;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::Output(_) => output_count += 1,
            NodeKind::GenerateLayer(_) => has_generate = true,
            NodeKind::SourceNoise(_) => has_source_noise = true,
            NodeKind::Mask(_) => has_mask = true,
            NodeKind::TopCameraRender(_) => has_camera = true,
            _ => {}
        }
    }

    assert!(output_count >= 3);
    assert!(has_generate);
    assert!(has_source_noise);
    assert!(has_mask);
    assert!(has_camera);
}

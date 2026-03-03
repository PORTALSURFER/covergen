//! Shared test fixtures/helpers for operator-graph preset tests.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::{build_preset_graph_with_catalogs, NodeCatalog, SubgraphCatalog};
use crate::graph::GpuGraph;
use crate::runtime_config::{AnimationConfig, AnimationMotion, V2Config, V2Profile};

/// Build a deterministic V2 config for one operator preset test case.
pub(super) fn preset_test_config(
    seed: u32,
    preset: &'static str,
    layers: u32,
    width: u32,
    height: u32,
) -> V2Config {
    V2Config {
        width,
        height,
        seed,
        count: 1,
        output: "test.png".to_string(),
        layers,
        antialias: 1,
        preset: preset.to_string(),
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

/// Build one preset graph with builtin catalogs for a test configuration.
pub(super) fn build_graph(config: &V2Config) -> GpuGraph {
    let presets = super::preset_catalog::PresetCatalog::with_builtins().expect("preset catalog");
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    build_preset_graph_with_catalogs(config, &presets, &nodes, &modules).expect("graph")
}

/// Assert one preset config emits identical graphs when compiled twice.
pub(super) fn assert_seed_deterministic(config: &V2Config) {
    let first = build_graph(config);
    let second = build_graph(config);
    assert_eq!(graph_fingerprint(&first), graph_fingerprint(&second));
}

/// Return a stable hash fingerprint for one generated graph.
fn graph_fingerprint(graph: &GpuGraph) -> u64 {
    let payload = serde_json::to_vec(graph).expect("graph should serialize for fingerprint");
    let mut hasher = DefaultHasher::new();
    payload.hash(&mut hasher);
    hasher.finish()
}

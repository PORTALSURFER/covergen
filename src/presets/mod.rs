//! Programmatic graph preset generation for V2.
//!
//! This module exposes catalog-based preset composition so new presets, nodes,
//! and reusable subgraph modules can be registered without editing central
//! `match` statements.

use super::graph::{GpuGraph, GraphBuildError};
use super::runtime_config::V2Config;

mod families;
mod grammar;
mod graph_art_direction;
mod module_invocation;
pub mod node_catalog;
mod operator_graph;
mod operator_graph_cascade;
#[cfg(test)]
mod operator_graph_cascade_tests;
mod operator_graph_feedback_atlas;
#[cfg(test)]
mod operator_graph_feedback_atlas_tests;
mod operator_graph_hyperweave;
#[cfg(test)]
mod operator_graph_hyperweave_tests;
mod operator_graph_modular_network;
#[cfg(test)]
mod operator_graph_modular_network_tests;
mod operator_graph_multi_stage;
#[cfg(test)]
mod operator_graph_multi_stage_tests;
mod operator_graph_orbit_forge;
#[cfg(test)]
mod operator_graph_orbit_forge_tests;
mod operator_graph_patchwork;
#[cfg(test)]
mod operator_graph_patchwork_tests;
mod operator_graph_router;
#[cfg(test)]
mod operator_graph_router_tests;
mod operator_graph_signal_lab;
#[cfg(test)]
mod operator_graph_signal_lab_tests;
mod operator_graph_sphere_noise_geo;
#[cfg(test)]
mod operator_graph_sphere_noise_geo_tests;
mod operator_graph_stage_primitives;
#[cfg(test)]
mod operator_graph_test_support;
#[cfg(test)]
mod operator_graph_tests;
pub mod preset_catalog;
mod primitives;
pub mod subgraph_catalog;
#[cfg(test)]
mod subgraph_catalog_tests;
mod subgraph_motifs;

use node_catalog::NodeCatalog;
use preset_catalog::PresetCatalog;
use preset_catalog::PresetContext;
use subgraph_catalog::SubgraphCatalog;

/// Build a deterministic graph from the selected V2 preset and default catalogs.
pub fn build_preset_graph(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    let presets = PresetCatalog::with_builtins()?;
    let nodes = NodeCatalog::with_builtins()?;
    let modules = SubgraphCatalog::with_builtins()?;
    build_preset_graph_with_catalogs(config, &presets, &nodes, &modules)
}

/// Build a deterministic graph from explicit preset/node/module catalogs.
pub fn build_preset_graph_with_catalogs(
    config: &V2Config,
    presets: &PresetCatalog,
    nodes: &NodeCatalog,
    modules: &SubgraphCatalog,
) -> Result<GpuGraph, GraphBuildError> {
    let context = PresetContext {
        config,
        nodes,
        modules,
    };
    let mut graph = presets.build(&config.preset, context)?;
    graph_art_direction::apply_graph_art_direction(&mut graph, config.art_direction);
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art_direction::{ChaosDirection, PaletteDirection};
    use crate::graph::NodeKind;
    use crate::runtime_config::{AnimationConfig, AnimationMotion, V2Profile};

    fn config_for(preset: &str) -> V2Config {
        V2Config {
            width: 512,
            height: 512,
            seed: 7,
            count: 1,
            output: "test.png".to_string(),
            layers: 4,
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

    #[test]
    fn all_builtin_presets_build_graph_native_topologies() {
        let presets = PresetCatalog::with_builtins().expect("preset catalog should register");
        let nodes = NodeCatalog::with_builtins().expect("node catalog should register");
        let modules = SubgraphCatalog::with_builtins().expect("module catalog should register");

        for preset in presets.keys() {
            let mut config = config_for(preset);
            config.preset = preset.to_string();
            let graph = build_preset_graph_with_catalogs(&config, &presets, &nodes, &modules)
                .expect("preset should build");
            let has_graph_native_ops = graph
                .nodes
                .iter()
                .any(|node| !matches!(node.kind, NodeKind::GenerateLayer(_) | NodeKind::Output(_)));
            assert!(
                has_graph_native_ops,
                "preset {preset} should include graph-native nodes"
            );
        }
    }

    #[test]
    fn unknown_preset_reports_catalog_keys() {
        let presets = PresetCatalog::with_builtins().expect("preset catalog should register");
        let nodes = NodeCatalog::with_builtins().expect("node catalog should register");
        let modules = SubgraphCatalog::with_builtins().expect("module catalog should register");

        let err =
            build_preset_graph_with_catalogs(&config_for("unknown"), &presets, &nodes, &modules)
                .expect_err("unknown preset should fail");
        let text = err.to_string();
        for preset in presets.keys() {
            assert!(text.contains(preset));
        }
    }

    #[test]
    fn art_direction_controls_modify_generated_parameters() {
        let presets = PresetCatalog::with_builtins().expect("preset catalog should register");
        let nodes = NodeCatalog::with_builtins().expect("node catalog should register");
        let modules = SubgraphCatalog::with_builtins().expect("module catalog should register");

        let base = config_for("hybrid-stack");
        let mut stylized = base.clone();
        stylized.art_direction.chaos = ChaosDirection::Wild;
        stylized.art_direction.palette = PaletteDirection::Neon;

        let graph_base = build_preset_graph_with_catalogs(&base, &presets, &nodes, &modules)
            .expect("baseline graph");
        let graph_stylized =
            build_preset_graph_with_catalogs(&stylized, &presets, &nodes, &modules)
                .expect("stylized graph");

        let base_layer = graph_base.nodes.iter().find_map(|node| match node.kind {
            NodeKind::GenerateLayer(layer) => Some(layer),
            _ => None,
        });
        let stylized_layer = graph_stylized
            .nodes
            .iter()
            .find_map(|node| match node.kind {
                NodeKind::GenerateLayer(layer) => Some(layer),
                _ => None,
            });
        let (Some(base_layer), Some(stylized_layer)) = (base_layer, stylized_layer) else {
            panic!("expected at least one generate layer node");
        };

        assert_ne!(base_layer.art_style, stylized_layer.art_style);
        assert_ne!(base_layer.warp_strength, stylized_layer.warp_strength);
    }
}

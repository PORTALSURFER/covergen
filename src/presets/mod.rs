//! Programmatic graph preset generation for V2.
//!
//! This module exposes catalog-based preset composition so new presets, nodes,
//! and reusable subgraph modules can be registered without editing central
//! `match` statements.

use super::graph::{GpuGraph, GraphBuildError};
use super::runtime_config::V2Config;

mod families;
mod grammar;
pub mod node_catalog;
pub mod preset_catalog;
mod primitives;
pub mod subgraph_catalog;
mod touchdesigner;

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
    presets.build(&config.preset, context)
}

#[cfg(test)]
mod tests {
    use super::*;
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
}

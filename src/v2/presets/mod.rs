//! Programmatic graph preset generation for V2.

use super::cli::V2Config;
use super::graph::{GpuGraph, GraphBuildError};

mod families;
mod primitives;

const PRESET_NAMES: &[&str] = &[
    "hybrid-stack",
    "field-weave",
    "node-weave",
    "mask-atlas",
    "warp-grid",
];

/// Build a deterministic graph from the selected V2 preset and CLI config.
pub fn build_preset_graph(config: &V2Config) -> Result<GpuGraph, GraphBuildError> {
    let preset = PresetKind::parse(&config.preset).ok_or_else(|| {
        GraphBuildError::new(format!(
            "unknown v2 preset '{}', expected {}",
            config.preset,
            PRESET_NAMES.join("|")
        ))
    })?;

    match preset {
        PresetKind::HybridStack => families::build_hybrid_stack(config),
        PresetKind::FieldWeave => families::build_field_weave(config),
        PresetKind::NodeWeave => families::build_node_weave(config),
        PresetKind::MaskAtlas => families::build_mask_atlas(config),
        PresetKind::WarpGrid => families::build_warp_grid(config),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PresetKind {
    HybridStack,
    FieldWeave,
    NodeWeave,
    MaskAtlas,
    WarpGrid,
}

impl PresetKind {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "hybrid-stack" | "hybrid" => Some(Self::HybridStack),
            "field-weave" | "field" => Some(Self::FieldWeave),
            "node-weave" | "node" => Some(Self::NodeWeave),
            "mask-atlas" | "atlas" => Some(Self::MaskAtlas),
            "warp-grid" | "grid" => Some(Self::WarpGrid),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::cli::{AnimationConfig, AnimationMotion, V2Profile};
    use crate::v2::graph::NodeKind;

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
                reels: false,
                motion: AnimationMotion::Normal,
            },
        }
    }

    #[test]
    fn all_presets_build_graph_native_topologies() {
        for preset in PRESET_NAMES {
            let graph = build_preset_graph(&config_for(preset)).expect("preset should build");
            let has_graph_native_ops = graph
                .nodes
                .iter()
                .any(|node| !matches!(node.kind, NodeKind::GenerateLayer(_) | NodeKind::Output));
            assert!(
                has_graph_native_ops,
                "preset {preset} should include graph-native nodes"
            );
        }
    }

    #[test]
    fn unknown_preset_reports_full_known_set() {
        let err = build_preset_graph(&config_for("unknown")).expect_err("should fail");
        let text = err.to_string();
        for preset in PRESET_NAMES {
            assert!(text.contains(preset));
        }
    }
}

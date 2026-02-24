//! Graph serialization and replay-manifest I/O.
//!
//! A replay manifest captures both the generated graph and runtime config so a
//! run can be reproduced exactly without rebuilding from presets.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::graph::GpuGraph;
use crate::runtime_config::V2Config;

/// Replay manifest schema for exact graph reproducibility.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayManifest {
    /// Schema version used for backward-compatible parsing.
    pub schema_version: u32,
    /// UTC timestamp as unix milliseconds.
    pub created_unix_ms: u128,
    /// Runtime config used to execute the graph.
    pub config: V2Config,
    /// Fully materialized graph payload.
    pub graph: GpuGraph,
}

impl ReplayManifest {
    /// Build a new replay manifest from one config and generated graph.
    pub fn from_graph(config: &V2Config, graph: &GpuGraph) -> Self {
        Self {
            schema_version: 1,
            created_unix_ms: unix_millis_now(),
            config: config.clone(),
            graph: graph.clone(),
        }
    }
}

/// Load one replay manifest from a JSON file.
pub fn load_manifest(path: &Path) -> Result<ReplayManifest, Box<dyn Error>> {
    let bytes = std::fs::read(path)
        .map_err(|err| format!("failed to read replay manifest '{}': {err}", path.display()))?;
    let manifest: ReplayManifest = serde_json::from_slice(&bytes).map_err(|err| {
        format!(
            "failed to parse replay manifest '{}': {err}",
            path.display()
        )
    })?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported replay manifest schema {} in '{}'",
            manifest.schema_version,
            path.display()
        )
        .into());
    }
    Ok(manifest)
}

/// Save one replay manifest JSON file and return the resolved output path.
pub fn save_manifest(
    path: &Path,
    config: &V2Config,
    graph: &GpuGraph,
) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "failed to create replay manifest directory '{}': {err}",
                    parent.display()
                )
            })?;
        }
    }

    let manifest = ReplayManifest::from_graph(config, graph);
    let payload = serde_json::to_vec_pretty(&manifest).map_err(|err| {
        format!(
            "failed to serialize replay manifest '{}': {err}",
            path.display()
        )
    })?;
    std::fs::write(path, payload).map_err(|err| {
        format!(
            "failed to write replay manifest '{}': {err}",
            path.display()
        )
    })?;
    Ok(path.to_path_buf())
}

fn unix_millis_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{GraphBuilder, OutputNode};
    use crate::runtime_config::V2Config;

    #[test]
    fn replay_manifest_roundtrip_preserves_graph_shape() {
        let config = V2Config::parse(vec!["--size".to_string(), "192".to_string()])
            .expect("config should parse");
        let mut builder = GraphBuilder::new(64, 64, 11);
        let layer = builder.add_generate_layer(crate::graph::GenerateLayerNode {
            symmetry: 4,
            symmetry_style: 1,
            iterations: 120,
            seed: 7,
            fill_scale: 1.0,
            fractal_zoom: 1.0,
            art_style: 2,
            art_style_secondary: 3,
            art_style_mix: 0.5,
            bend_strength: 0.2,
            warp_strength: 0.3,
            warp_frequency: 1.2,
            tile_scale: 0.7,
            tile_phase: 0.1,
            center_x: 0.0,
            center_y: 0.0,
            shader_layer_count: 3,
            blend_mode: crate::model::LayerBlendMode::Normal,
            opacity: 1.0,
            contrast: 1.2,
            temporal: crate::node::GenerateLayerTemporal::default(),
        });
        let out = builder.add_output_with_contract(OutputNode::primary());
        builder.connect_luma(layer, out);
        let graph = builder.build().expect("graph should validate");

        let mut path = std::env::temp_dir();
        path.push(format!(
            "covergen_manifest_roundtrip_{}_{}.json",
            std::process::id(),
            unix_millis_now()
        ));

        save_manifest(&path, &config, &graph).expect("manifest write");
        let loaded = load_manifest(&path).expect("manifest read");
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.graph.nodes.len(), graph.nodes.len());
        assert_eq!(loaded.graph.edges.len(), graph.edges.len());
        assert_eq!(loaded.graph.width, graph.width);
        assert_eq!(loaded.graph.height, graph.height);
        assert_eq!(loaded.config.seed, config.seed);
    }
}

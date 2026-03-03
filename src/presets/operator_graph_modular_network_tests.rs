//! Tests for the `op-modular-network` preset.

use super::operator_graph_test_support::{assert_seed_deterministic, build_graph, preset_test_config};
use crate::graph::NodeKind;
use crate::runtime_config::V2Config;

fn config(seed: u32) -> V2Config {
    preset_test_config(seed, "op-modular-network", 6, 512, 512)
}

#[test]
fn operator_modular_network_is_seed_deterministic() {
    let cfg = config(515);
    assert_seed_deterministic(&cfg);
}

#[test]
fn operator_modular_network_has_multiple_outputs_and_modules() {
    let cfg = config(717);

    let graph = build_graph(&cfg);

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

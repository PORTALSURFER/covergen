//! Tests for operator-family presets and random graph constraints.

use super::operator_graph_test_support::{assert_seed_deterministic, build_graph, preset_test_config};
use crate::graph::NodeKind;
use crate::runtime_config::V2Config;

fn config(seed: u32) -> V2Config {
    preset_test_config(seed, "op-random-network", 5, 512, 512)
}

#[test]
fn operator_random_network_is_seed_deterministic() {
    let cfg = config(17);
    assert_seed_deterministic(&cfg);
}

#[test]
fn operator_random_network_contains_chop_sop_camera_and_masking() {
    let cfg = config(33);
    let graph = build_graph(&cfg);

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

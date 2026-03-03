//! Tests for the `op-patchwork` preset.

use super::operator_graph_test_support::{
    assert_seed_deterministic, build_graph, preset_test_config,
};
use crate::graph::NodeKind;
use crate::runtime_config::V2Config;

fn config(seed: u32) -> V2Config {
    preset_test_config(seed, "op-patchwork", 6, 512, 512)
}

#[test]
fn operator_patchwork_is_seed_deterministic() {
    let cfg = config(1234);
    assert_seed_deterministic(&cfg);
}

#[test]
fn operator_patchwork_has_mixed_topology_and_taps() {
    let cfg = config(5678);

    let graph = build_graph(&cfg);

    let mut outputs = 0usize;
    let mut cameras = 0usize;
    let mut generates = 0usize;
    let mut masks = 0usize;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::Output(_) => outputs += 1,
            NodeKind::TopCameraRender(_) => cameras += 1,
            NodeKind::GenerateLayer(_) => generates += 1,
            NodeKind::Mask(_) => masks += 1,
            _ => {}
        }
    }

    assert!(outputs >= 3);
    assert!(cameras >= 4);
    assert!(generates >= 3);
    assert!(masks >= 2);
}

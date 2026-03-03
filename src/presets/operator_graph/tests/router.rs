//! Tests for the `op-router` preset.

use super::operator_graph_test_support::{
    assert_seed_deterministic, build_graph, preset_test_config,
};
use crate::graph::NodeKind;
use crate::runtime_config::V2Config;

fn config(seed: u32) -> V2Config {
    preset_test_config(seed, "op-router", 6, 512, 512)
}

#[test]
fn operator_router_is_seed_deterministic() {
    let cfg = config(4242);
    assert_seed_deterministic(&cfg);
}

#[test]
fn operator_router_has_lane_taps_and_mixed_ops() {
    let cfg = config(7878);

    let graph = build_graph(&cfg);

    let mut outputs = 0usize;
    let mut cameras = 0usize;
    let mut layers = 0usize;
    let mut masks = 0usize;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::Output(_) => outputs += 1,
            NodeKind::TopCameraRender(_) => cameras += 1,
            NodeKind::GenerateLayer(_) => layers += 1,
            NodeKind::Mask(_) => masks += 1,
            _ => {}
        }
    }

    assert!(outputs >= 4);
    assert!(cameras >= 4);
    assert!(layers >= 4);
    assert!(masks >= 2);
}

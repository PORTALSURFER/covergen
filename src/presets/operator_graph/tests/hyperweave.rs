//! Tests for the `op-hyperweave` preset.

use super::operator_graph_test_support::{
    assert_seed_deterministic, build_graph, preset_test_config,
};
use crate::graph::NodeKind;
use crate::runtime_config::V2Config;

fn config(seed: u32) -> V2Config {
    preset_test_config(seed, "op-hyperweave", 6, 512, 512)
}

#[test]
fn operator_hyperweave_is_seed_deterministic() {
    let cfg = config(1122);
    assert_seed_deterministic(&cfg);
}

#[test]
fn operator_hyperweave_emits_rich_taps_and_ops() {
    let cfg = config(3344);

    let graph = build_graph(&cfg);

    let mut outputs = 0usize;
    let mut cameras = 0usize;
    let mut masks = 0usize;
    let mut blends = 0usize;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::Output(_) => outputs += 1,
            NodeKind::TopCameraRender(_) => cameras += 1,
            NodeKind::Mask(_) => masks += 1,
            NodeKind::Blend(_) => blends += 1,
            _ => {}
        }
    }

    assert!(outputs >= 4);
    assert!(cameras >= 5);
    assert!(masks >= 3);
    assert!(blends >= 2);
}

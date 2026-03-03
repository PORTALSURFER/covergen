//! Tests for the `op-cascade-lab` operator-family preset.

use super::operator_graph_test_support::{assert_seed_deterministic, build_graph, preset_test_config};
use crate::graph::NodeKind;
use crate::runtime_config::V2Config;

fn config(seed: u32) -> V2Config {
    preset_test_config(seed, "op-cascade-lab", 6, 512, 512)
}

#[test]
fn operator_cascade_lab_is_seed_deterministic() {
    let cfg = config(77);
    assert_seed_deterministic(&cfg);
}

#[test]
fn operator_cascade_lab_contains_td_and_graph_native_families() {
    let cfg = config(99);

    let graph = build_graph(&cfg);

    let mut chop = 0usize;
    let mut sop = 0usize;
    let mut camera = 0usize;
    let mut layer = 0usize;
    let mut source = 0usize;
    let mut mask = 0usize;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::ChopLfo(_) | NodeKind::ChopMath(_) | NodeKind::ChopRemap(_) => chop += 1,
            NodeKind::SopCircle(_) | NodeKind::SopSphere(_) => sop += 1,
            NodeKind::TopCameraRender(_) => camera += 1,
            NodeKind::GenerateLayer(_) => layer += 1,
            NodeKind::SourceNoise(_) => source += 1,
            NodeKind::Mask(_) => mask += 1,
            _ => {}
        }
    }

    assert!(chop >= 4);
    assert!(sop >= 4);
    assert!(camera >= 4);
    assert!(layer >= 3);
    assert!(source >= 3);
    assert!(mask >= 3);
}

//! Tests for the `op-multi-stage` preset topology.

use super::operator_graph_test_support::{
    assert_seed_deterministic, build_graph, preset_test_config,
};
use crate::graph::NodeKind;
use crate::runtime_config::V2Config;

fn config(seed: u32) -> V2Config {
    preset_test_config(seed, "op-multi-stage", 6, 512, 512)
}

#[test]
fn operator_multi_stage_is_seed_deterministic() {
    let cfg = config(101);
    assert_seed_deterministic(&cfg);
}

#[test]
fn operator_multi_stage_contains_structured_td_families() {
    let cfg = config(303);

    let graph = build_graph(&cfg);

    let mut chop = 0usize;
    let mut sop = 0usize;
    let mut camera = 0usize;
    let mut source_noise = 0usize;
    let mut mask = 0usize;
    let mut blend = 0usize;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::ChopLfo(_) | NodeKind::ChopMath(_) | NodeKind::ChopRemap(_) => chop += 1,
            NodeKind::SopCircle(_) | NodeKind::SopSphere(_) => sop += 1,
            NodeKind::TopCameraRender(_) => camera += 1,
            NodeKind::SourceNoise(_) => source_noise += 1,
            NodeKind::Mask(_) => mask += 1,
            NodeKind::Blend(_) => blend += 1,
            _ => {}
        }
    }

    assert!(chop >= 5);
    assert!(sop >= 4);
    assert!(camera >= 3);
    assert!(source_noise >= 3);
    assert!(mask >= 3);
    assert!(blend >= 3);
}

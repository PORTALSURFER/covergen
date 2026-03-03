//! Tests for the `op-orbit-forge` preset.

use super::operator_graph_test_support::{
    assert_seed_deterministic, build_graph, preset_test_config,
};
use crate::graph::NodeKind;
use crate::runtime_config::V2Config;

fn config(seed: u32) -> V2Config {
    preset_test_config(seed, "op-orbit-forge", 7, 512, 512)
}

#[test]
fn operator_orbit_forge_is_seed_deterministic() {
    let cfg = config(1701);
    assert_seed_deterministic(&cfg);
}

#[test]
fn operator_orbit_forge_has_rich_multilane_topology() {
    let cfg = config(9182);
    let graph = build_graph(&cfg);

    let mut outputs = 0usize;
    let mut cameras = 0usize;
    let mut lfos = 0usize;
    let mut masks = 0usize;
    let mut blends = 0usize;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::Output(_) => outputs += 1,
            NodeKind::TopCameraRender(_) => cameras += 1,
            NodeKind::ChopLfo(_) => lfos += 1,
            NodeKind::Mask(_) => masks += 1,
            NodeKind::Blend(_) => blends += 1,
            _ => {}
        }
    }

    assert!(outputs >= 5);
    assert!(cameras >= 5);
    assert!(lfos >= 4);
    assert!(masks >= 3);
    assert!(blends >= 3);
}

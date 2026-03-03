//! Tests for the `op-signal-lab` preset.

use super::operator_graph_test_support::{
    assert_seed_deterministic, build_graph, preset_test_config,
};
use crate::graph::NodeKind;
use crate::runtime_config::V2Config;

fn config(seed: u32) -> V2Config {
    preset_test_config(seed, "op-signal-lab", 6, 512, 512)
}

#[test]
fn operator_signal_lab_is_seed_deterministic() {
    let cfg = config(6451);
    assert_seed_deterministic(&cfg);
}

#[test]
fn operator_signal_lab_exposes_sop_top_chop_flow() {
    let cfg = config(9412);

    let graph = build_graph(&cfg);

    let mut outputs = 0usize;
    let mut cameras = 0usize;
    let mut lfos = 0usize;
    let mut circles = 0usize;
    let mut spheres = 0usize;
    let mut masks = 0usize;

    for node in &graph.nodes {
        match node.kind {
            NodeKind::Output(_) => outputs += 1,
            NodeKind::TopCameraRender(_) => cameras += 1,
            NodeKind::ChopLfo(_) => lfos += 1,
            NodeKind::SopCircle(_) => circles += 1,
            NodeKind::SopSphere(_) => spheres += 1,
            NodeKind::Mask(_) => masks += 1,
            _ => {}
        }
    }

    assert!(outputs >= 4);
    assert!(cameras >= 4);
    assert!(lfos >= 3);
    assert!(circles >= 3);
    assert!(spheres >= 3);
    assert!(masks >= 2);
}

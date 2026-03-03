//! Tests for the dedicated sphere-noise-geometry operator-graph preset.

use super::operator_graph_test_support::{build_graph, preset_test_config};
use crate::graph::NodeKind;
use crate::node::PortType;
use crate::runtime_config::V2Config;

fn config(seed: u32) -> V2Config {
    preset_test_config(seed, "op-sphere-noise-geo", 4, 640, 640)
}

#[test]
fn preset_contains_sop_geometry_chain() {
    let graph = build_graph(&config(123));

    let mut sphere = None;
    let mut noise = None;
    let mut geometry = None;
    let mut camera = None;
    for node in &graph.nodes {
        match node.kind {
            NodeKind::SopSphere(_) => sphere = Some(node.id),
            NodeKind::SourceNoise(spec) if matches!(spec.output_port, PortType::ChannelScalar) => {
                noise = Some(node.id)
            }
            NodeKind::SopGeometry(_) => geometry = Some(node.id),
            NodeKind::TopCameraRender(_) => camera = Some(node.id),
            _ => {}
        }
    }
    let (Some(sphere), Some(noise), Some(geometry), Some(camera)) =
        (sphere, noise, geometry, camera)
    else {
        panic!("expected sphere, scalar-noise, sop-geometry, and camera nodes");
    };

    assert!(graph.edges.iter().any(|edge| {
        edge.from == sphere && edge.to == geometry && edge.to_port == PortType::SopPrimitive
    }));
    assert!(graph.edges.iter().any(|edge| {
        edge.from == noise && edge.to == geometry && edge.to_port == PortType::ChannelScalar
    }));
    assert!(graph.edges.iter().any(|edge| {
        edge.from == geometry && edge.to == camera && edge.to_port == PortType::SopPrimitive
    }));
}

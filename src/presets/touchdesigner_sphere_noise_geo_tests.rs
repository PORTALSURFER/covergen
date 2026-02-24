//! Tests for the dedicated sphere-noise-geometry TouchDesigner preset.

use super::{build_preset_graph_with_catalogs, NodeCatalog, SubgraphCatalog};
use crate::graph::NodeKind;
use crate::node::PortType;
use crate::runtime_config::{AnimationConfig, AnimationMotion, V2Config, V2Profile};

fn config(seed: u32) -> V2Config {
    V2Config {
        width: 640,
        height: 640,
        seed,
        count: 1,
        output: "test.png".to_string(),
        layers: 4,
        antialias: 1,
        preset: "td-sphere-noise-geo".to_string(),
        profile: V2Profile::Quality,
        manifest_out: None,
        manifest_in: None,
        art_direction: crate::art_direction::ArtDirectionConfig::default(),
        animation: AnimationConfig {
            enabled: false,
            seconds: 30,
            fps: 30,
            keep_frames: false,
            motion: AnimationMotion::Normal,
        },
        selection: crate::runtime_config::SelectionConfig {
            explore_candidates: 0,
            explore_size: 320,
        },
    }
}

#[test]
fn preset_contains_sop_geometry_chain() {
    let presets = super::preset_catalog::PresetCatalog::with_builtins().expect("preset catalog");
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let graph =
        build_preset_graph_with_catalogs(&config(123), &presets, &nodes, &modules).expect("graph");

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

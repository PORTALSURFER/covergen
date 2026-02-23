use super::*;
use crate::model::LayerBlendMode;
use crate::v2::node::GenerateLayerTemporal;

fn sample_layer() -> GenerateLayerNode {
    GenerateLayerNode {
        symmetry: 3,
        symmetry_style: 1,
        iterations: 180,
        seed: 42,
        fill_scale: 1.2,
        fractal_zoom: 0.8,
        art_style: 1,
        art_style_secondary: 2,
        art_style_mix: 0.4,
        bend_strength: 0.5,
        warp_strength: 0.4,
        warp_frequency: 2.0,
        tile_scale: 0.9,
        tile_phase: 0.3,
        center_x: 0.0,
        center_y: 0.0,
        shader_layer_count: 3,
        blend_mode: LayerBlendMode::Normal,
        opacity: 1.0,
        contrast: 1.2,
        temporal: GenerateLayerTemporal::default(),
    }
}

#[test]
fn builds_valid_linear_graph() {
    let mut builder = GraphBuilder::new(1024, 1024, 7);
    let layer = builder.add_generate_layer(sample_layer());
    let out = builder.add_output();
    builder.connect_luma(layer, out);
    let graph = builder.build().expect("graph should validate");
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
}

#[test]
fn rejects_cycle() {
    let mut builder = GraphBuilder::new(1024, 1024, 7);
    let a = builder.add_generate_layer(sample_layer());
    let b = builder.add_generate_layer(sample_layer());
    let out = builder.add_output();
    builder.connect_luma(a, b);
    builder.connect_luma(b, a);
    builder.connect_luma(b, out);
    let err = builder.build().expect_err("cycle must be rejected");
    assert!(err.to_string().contains("cycle"));
}

#[test]
fn rejects_output_without_input() {
    let mut builder = GraphBuilder::new(1024, 1024, 7);
    let _layer = builder.add_generate_layer(sample_layer());
    let _out = builder.add_output();
    let err = builder
        .build()
        .expect_err("output without edge must be rejected");
    assert!(err.to_string().contains("requires 1..=1 inputs"));
}

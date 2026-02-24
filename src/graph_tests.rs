use super::*;
use crate::chop::{ChopLfoNode, ChopWave};
use crate::model::LayerBlendMode;
use crate::node::GenerateLayerTemporal;
use crate::sop::{SopCircleNode, TopCameraRenderNode};

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

#[test]
fn rejects_multiple_primary_outputs() {
    let mut builder = GraphBuilder::new(1024, 1024, 7);
    let layer = builder.add_generate_layer(sample_layer());
    let primary_a = builder.add_output();
    let primary_b = builder.add_output_with_contract(OutputNode {
        role: OutputRole::Primary,
        slot: 2,
    });
    builder.connect_luma(layer, primary_a);
    builder.connect_luma(layer, primary_b);
    let err = builder
        .build()
        .expect_err("multiple primary outputs must be rejected");
    assert!(err.to_string().contains("exactly one primary output"));
}

#[test]
fn rejects_duplicate_output_slots() {
    let mut builder = GraphBuilder::new(1024, 1024, 7);
    let layer = builder.add_generate_layer(sample_layer());
    let primary = builder.add_output();
    let tap_a = builder.add_output_tap(1);
    let tap_b = builder.add_output_tap(1);
    builder.connect_luma(layer, primary);
    builder.connect_luma(layer, tap_a);
    builder.connect_luma(layer, tap_b);
    let err = builder
        .build()
        .expect_err("duplicate output slots must be rejected");
    assert!(err.to_string().contains("duplicate output slot"));
}

#[test]
fn classifies_nodes_by_operator_family() {
    let layer = NodeKind::GenerateLayer(sample_layer());
    let out = NodeKind::Output(OutputNode::primary());
    assert_eq!(layer.operator_family(), OperatorFamily::Top);
    assert_eq!(out.operator_family(), OperatorFamily::Output);
}

#[test]
fn validates_chop_sop_to_top_camera_graph() {
    let mut builder = GraphBuilder::new(256, 256, 19);
    let lfo = builder.add_chop_lfo(ChopLfoNode {
        wave: ChopWave::Sine,
        frequency: 0.6,
        phase: 0.0,
        amplitude: 0.5,
        offset: 1.0,
    });
    let circle = builder.add_sop_circle(SopCircleNode {
        radius: 0.28,
        feather: 0.04,
        center_x: 0.0,
        center_y: 0.0,
    });
    let camera = builder.add_top_camera_render(TopCameraRenderNode {
        exposure: 1.2,
        gamma: 1.0,
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        rotate: 0.0,
        invert: false,
    });
    let out = builder.add_output();

    builder.connect_sop_input(circle, camera, 0);
    builder.connect_channel_input(lfo, camera, 1);
    builder.connect_luma(camera, out);
    builder.build().expect("graph should validate");
}

#[test]
fn validates_stateful_feedback_node_contract() {
    let mut builder = GraphBuilder::new(256, 256, 41);
    let source = builder.add_generate_layer(sample_layer());
    let feedback = builder.add_stateful_feedback(StatefulFeedbackNode { mix: 0.6 });
    let out = builder.add_output();
    builder.connect_luma(source, feedback);
    builder.connect_luma(feedback, out);
    builder
        .build()
        .expect("stateful feedback graph should validate");
}

#[test]
fn rejects_stateful_feedback_without_input() {
    let mut builder = GraphBuilder::new(256, 256, 42);
    let feedback = builder.add_stateful_feedback(StatefulFeedbackNode { mix: 0.4 });
    let out = builder.add_output();
    builder.connect_luma(feedback, out);
    let err = builder
        .build()
        .expect_err("stateful feedback node requires one luma input");
    assert!(err.to_string().contains("requires 1..=1 inputs"));
}

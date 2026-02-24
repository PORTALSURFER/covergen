use super::*;
use crate::graph::GraphBuilder;
use crate::model::LayerBlendMode;
use crate::node::{
    BlendNode, BlendTemporal, GenerateLayerNode, GenerateLayerTemporal, MaskNode, MaskTemporal,
    StatefulFeedbackNode,
};

fn sample_layer() -> GenerateLayerNode {
    GenerateLayerNode {
        symmetry: 4,
        symmetry_style: 1,
        iterations: 200,
        seed: 1,
        fill_scale: 1.0,
        fractal_zoom: 0.8,
        art_style: 2,
        art_style_secondary: 3,
        art_style_mix: 0.5,
        bend_strength: 0.4,
        warp_strength: 0.3,
        warp_frequency: 2.5,
        tile_scale: 1.0,
        tile_phase: 0.2,
        center_x: 0.0,
        center_y: 0.0,
        shader_layer_count: 3,
        blend_mode: LayerBlendMode::Normal,
        opacity: 1.0,
        contrast: 1.1,
        temporal: GenerateLayerTemporal::default(),
    }
}

#[test]
fn compiles_linear_layer_graph() {
    let mut builder = GraphBuilder::new(512, 512, 123);
    let a = builder.add_generate_layer(sample_layer());
    let b = builder.add_generate_layer(sample_layer());
    let out = builder.add_output();
    builder.connect_luma(a, b);
    builder.connect_luma(b, out);
    let graph = builder.build().expect("graph should build");
    let compiled = compile_graph(&graph).expect("graph should compile");
    assert_eq!(compiled.steps.len(), 3);
    assert!(!compiled.has_non_layer_nodes);
    assert!(compiled.can_use_retained_layer_path);
    assert_eq!(compiled.primary_output_node, out);
    assert_eq!(compiled.output_bindings.len(), 1);
    assert_eq!(compiled.output_bindings[0].source_node, b);
}

#[test]
fn branching_layer_graph_disables_retained_path() {
    let mut builder = GraphBuilder::new(512, 512, 123);
    let a = builder.add_generate_layer(sample_layer());
    let b = builder.add_generate_layer(sample_layer());
    let c = builder.add_generate_layer(sample_layer());
    let out = builder.add_output();
    builder.connect_luma(a, b);
    builder.connect_luma(a, c);
    builder.connect_luma(b, out);
    let graph = builder.build().expect("graph should build");
    let compiled = compile_graph(&graph).expect("graph should compile");
    assert!(!compiled.has_non_layer_nodes);
    assert!(!compiled.can_use_retained_layer_path);
}

#[test]
fn merged_graph_disables_retained_path() {
    let mut builder = GraphBuilder::new(512, 512, 123);
    let a = builder.add_generate_layer(sample_layer());
    let b = builder.add_generate_layer(sample_layer());
    let blend = builder.add_blend(BlendNode {
        mode: LayerBlendMode::Overlay,
        opacity: 0.8,
        temporal: BlendTemporal::default(),
    });
    let out = builder.add_output();
    builder.connect_luma_input(a, blend, 0);
    builder.connect_luma_input(b, blend, 1);
    builder.connect_luma(blend, out);
    let graph = builder.build().expect("graph should build");
    let compiled = compile_graph(&graph).expect("graph should compile");
    assert!(compiled.has_non_layer_nodes);
    assert!(!compiled.can_use_retained_layer_path);
}

#[test]
fn compiles_mask_node_graph() {
    let mut builder = GraphBuilder::new(256, 256, 9);
    let src = builder.add_generate_layer(sample_layer());
    let mask = builder.add_mask(MaskNode {
        threshold: 0.5,
        softness: 0.1,
        invert: false,
        temporal: MaskTemporal::default(),
    });
    let out = builder.add_output();
    builder.connect_luma(src, mask);
    builder.connect_mask_input(mask, out, 0);
    let err = builder
        .build()
        .expect_err("output cannot accept mask input");
    assert!(err.to_string().contains("to-port mismatch"));
}

#[test]
fn resource_plan_reuses_alias_slots_for_non_overlapping_luma_values() {
    let mut builder = GraphBuilder::new(256, 256, 5);
    let a = builder.add_generate_layer(sample_layer());
    let b = builder.add_generate_layer(sample_layer());
    let c = builder.add_generate_layer(sample_layer());
    let blend = builder.add_blend(BlendNode {
        mode: LayerBlendMode::Screen,
        opacity: 0.7,
        temporal: BlendTemporal::default(),
    });
    let out = builder.add_output();

    builder.connect_luma(a, b);
    builder.connect_luma_input(b, blend, 0);
    builder.connect_luma_input(c, blend, 1);
    builder.connect_luma(blend, out);

    let graph = builder.build().expect("graph should build");
    let compiled = compile_graph(&graph).expect("graph should compile");

    let luma_lifetimes: Vec<_> = compiled
        .resource_plan
        .lifetimes
        .iter()
        .filter_map(|(node_id, lifetime)| {
            (lifetime.kind == CompiledValueKind::Luma).then_some((*node_id, *lifetime))
        })
        .collect();
    assert!(luma_lifetimes.len() >= 4);
    assert!(compiled.resource_plan.peak_luma_slots < luma_lifetimes.len());

    let mut by_slot = std::collections::HashMap::new();
    for (node_id, lifetime) in &luma_lifetimes {
        by_slot
            .entry(lifetime.alias_slot)
            .or_insert_with(Vec::new)
            .push((*node_id, *lifetime));
    }

    let reused_slots = by_slot.values().filter(|values| values.len() > 1).count();
    assert!(reused_slots > 0);

    for values in by_slot.values().filter(|values| values.len() > 1) {
        for i in 0..values.len() {
            for j in (i + 1)..values.len() {
                let left = values[i].1;
                let right = values[j].1;
                let non_overlapping =
                    left.last_step < right.first_step || right.last_step < left.first_step;
                assert!(non_overlapping, "aliased lifetimes must not overlap");
            }
        }
    }

    assert_eq!(
        compiled.resource_plan.releases_by_step.len(),
        compiled.steps.len()
    );
    assert_eq!(
        compiled.resource_plan.gpu_releases_by_step.len(),
        compiled.steps.len()
    );
    assert!(compiled.resource_plan.gpu_peak_luma_slots > 0);
}

#[test]
fn gpu_resource_plan_tracks_lifetimes_for_all_renderable_nodes() {
    let mut builder = GraphBuilder::new(320, 240, 42);
    let a = builder.add_generate_layer(sample_layer());
    let b = builder.add_generate_layer(sample_layer());
    let blend = builder.add_blend(BlendNode {
        mode: LayerBlendMode::Overlay,
        opacity: 0.65,
        temporal: BlendTemporal::default(),
    });
    let mask = builder.add_mask(MaskNode {
        threshold: 0.52,
        softness: 0.18,
        invert: false,
        temporal: MaskTemporal::default(),
    });
    let out = builder.add_output();

    builder.connect_luma(a, mask);
    builder.connect_luma_input(a, blend, 0);
    builder.connect_luma_input(b, blend, 1);
    builder.connect_mask_input(mask, blend, 2);
    builder.connect_luma(blend, out);

    let graph = builder.build().expect("graph should build");
    let compiled = compile_graph(&graph).expect("graph should compile");

    assert!(compiled.resource_plan.gpu_lifetime_for(a).is_some());
    assert!(compiled.resource_plan.gpu_lifetime_for(b).is_some());
    assert!(compiled.resource_plan.gpu_lifetime_for(blend).is_some());
    assert!(compiled.resource_plan.gpu_lifetime_for(mask).is_some());
    assert!(compiled.resource_plan.gpu_lifetime_for(out).is_none());
    assert!(compiled.resource_plan.gpu_peak_luma_slots >= 1);
    assert!(compiled.resource_plan.gpu_peak_mask_slots >= 1);
}

#[test]
fn compiles_multiple_outputs_with_primary_and_tap() {
    let mut builder = GraphBuilder::new(256, 256, 17);
    let a = builder.add_generate_layer(sample_layer());
    let b = builder.add_generate_layer(sample_layer());
    let primary = builder.add_output();
    let tap = builder.add_output_tap(1);
    builder.connect_luma(a, b);
    builder.connect_luma(b, primary);
    builder.connect_luma(a, tap);

    let graph = builder.build().expect("graph should build");
    let compiled = compile_graph(&graph).expect("graph should compile");

    assert_eq!(compiled.primary_output_node, primary);
    assert_eq!(compiled.output_bindings.len(), 2);
    let tap_binding = compiled
        .output_bindings
        .iter()
        .find(|binding| binding.output_node == tap)
        .copied()
        .expect("tap binding should exist");
    assert_eq!(tap_binding.source_node, a);
    assert_eq!(tap_binding.slot, 1);
}

#[test]
fn compiles_stateful_feedback_with_persistent_slots() {
    let mut builder = GraphBuilder::new(256, 256, 23);
    let source = builder.add_generate_layer(sample_layer());
    let feedback = builder.add_stateful_feedback(StatefulFeedbackNode { mix: 0.55 });
    let out = builder.add_output();
    builder.connect_luma(source, feedback);
    builder.connect_luma(feedback, out);

    let graph = builder.build().expect("graph should build");
    let compiled = compile_graph(&graph).expect("graph should compile");

    let feedback_step = compiled
        .steps
        .iter()
        .find(|step| step.node_id == feedback)
        .expect("feedback node should have a compiled step");
    assert!(matches!(
        feedback_step.op,
        CompiledOp::StatefulFeedback(StatefulFeedbackNode { .. })
    ));
    assert_eq!(compiled.feedback_slots.get(&feedback).copied(), Some(0));
    assert!(compiled.resource_plan.gpu_lifetime_for(feedback).is_some());
}

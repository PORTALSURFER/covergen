use super::super::compile_stage::compiled_step;
use super::super::eval_stage::compiled_texture_source_for_param;
use super::super::{
    compiled_param_value_opt, param_schema, CompiledStepKind, GuiCompiledRuntime, TexRuntimeOp,
};
use crate::gui::project::{GuiProject, ProjectNodeKind, SignalEvalPath, SignalEvalStack};

#[test]
fn compiled_param_slots_match_keyed_param_values() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    assert!(project.set_param_value(solid, 0, 0.15));
    assert!(project.set_param_value(solid, 1, 0.35));
    assert!(project.set_param_value(solid, 2, 0.55));
    assert!(project.set_param_value(solid, 3, 0.75));
    let step = compiled_step(
        &project,
        solid,
        CompiledStepKind::Solid,
        &param_schema::solid::KEYS,
    );
    let mut eval_stack = SignalEvalStack::default();
    for (slot_index, key) in param_schema::solid::KEYS.iter().enumerate() {
        let keyed = project.node_param_value(solid, key, 0.0, &mut eval_stack);
        eval_stack.clear_nodes();
        let indexed = compiled_param_value_opt(&project, &step, slot_index, 0.0, &mut eval_stack);
        assert_eq!(
            indexed, keyed,
            "compiled slot {slot_index} should match keyed read for {key}"
        );
        eval_stack.clear_nodes();
    }
}

#[test]
fn compiled_feedback_history_slot_matches_texture_binding() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 40, 420, 480);
    assert!(project.connect_texture_link_to_param(solid, feedback, 0));
    let step = compiled_step(
        &project,
        feedback,
        CompiledStepKind::Feedback,
        &param_schema::feedback::RUNTIME_KEYS,
    );
    assert_eq!(
        compiled_texture_source_for_param(
            &project,
            &step,
            param_schema::feedback::RUNTIME_HISTORY_INDEX
        ),
        Some(solid)
    );
}

#[test]
fn transform_defaults_are_identity() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let transform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, transform));
    assert!(project.connect_image_link(transform, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 2);
    assert!(matches!(
        ops[1],
        TexRuntimeOp::Transform {
            brightness,
            gain_r,
            gain_g,
            gain_b,
            alpha_mul
        } if brightness == 1.0
            && gain_r == 1.0
            && gain_g == 1.0
            && gain_b == 1.0
            && alpha_mul == 1.0
    ));
}

#[test]
fn level_defaults_are_identity() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let level = project.add_node(ProjectNodeKind::TexLevel, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, level));
    assert!(project.connect_image_link(level, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 2);
    assert!(matches!(
        ops[1],
        TexRuntimeOp::Level {
            in_low,
            in_high,
            gamma,
            out_low,
            out_high
        } if in_low == 0.0
            && in_high == 1.0
            && gamma == 1.0
            && out_low == 0.0
            && out_high == 1.0
    ));
}

#[test]
fn blend_pipeline_compiles_to_store_and_blend_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 120, 40, 420, 480);
    let blend = project.add_node(ProjectNodeKind::TexBlend, 280, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 440, 40, 420, 480);
    assert!(project.connect_image_link(solid, blend));
    assert!(project.connect_texture_link_to_param(circle, blend, 0));
    assert!(project.connect_image_link(blend, out));
    assert!(project.set_param_dropdown_index(blend, 1, 1));
    assert!(project.set_param_value(blend, 2, 0.75));
    assert!(project.set_param_value(blend, 3, 0.2));
    assert!(project.set_param_value(blend, 4, 0.3));
    assert!(project.set_param_value(blend, 5, 0.4));
    assert!(project.set_param_value(blend, 6, 0.5));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert!(matches!(ops[0], TexRuntimeOp::Solid { .. }));
    assert!(matches!(
        ops[1],
        TexRuntimeOp::StoreTexture { texture_node_id } if texture_node_id == solid
    ));
    assert!(matches!(ops[2], TexRuntimeOp::Circle { .. }));
    assert!(matches!(
        ops[3],
        TexRuntimeOp::StoreTexture { texture_node_id } if texture_node_id == circle
    ));
    assert!(matches!(
        ops[4],
        TexRuntimeOp::Blend {
            mode,
            opacity,
            bg_r,
            bg_g,
            bg_b,
            bg_a,
            base_texture_node_id,
            layer_texture_node_id: Some(layer_id),
        } if mode == 1.0
            && (opacity - 0.75).abs() < 1e-6
            && (bg_r - 0.2).abs() < 1e-6
            && (bg_g - 0.3).abs() < 1e-6
            && (bg_b - 0.4).abs() < 1e-6
            && (bg_a - 0.5).abs() < 1e-6
            && base_texture_node_id == solid
            && layer_id == circle
    ));
}

#[test]
fn blend_pipeline_orders_branch_compilation_for_dependent_inputs() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
    let blend = project.add_node(ProjectNodeKind::TexBlend, 340, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 500, 40, 420, 480);
    assert!(project.connect_image_link(solid, xform));
    assert!(project.connect_image_link(xform, blend));
    assert!(project.connect_texture_link_to_param(solid, blend, 0));
    assert!(project.connect_image_link(blend, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert!(matches!(ops[0], TexRuntimeOp::Solid { .. }));
    assert!(matches!(
        ops[1],
        TexRuntimeOp::StoreTexture { texture_node_id } if texture_node_id == solid
    ));
    assert!(matches!(ops[2], TexRuntimeOp::Transform { .. }));
    assert!(matches!(
        ops[3],
        TexRuntimeOp::StoreTexture { texture_node_id } if texture_node_id == xform
    ));
    assert!(matches!(
        ops[4],
        TexRuntimeOp::Blend {
            base_texture_node_id,
            layer_texture_node_id: Some(layer_id),
            ..
        } if base_texture_node_id == xform && layer_id == solid
    ));
}

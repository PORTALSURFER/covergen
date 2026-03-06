use super::super::{
    param_schema, GuiCompiledRuntime, TexRuntimeFeedbackHistoryBinding, TexRuntimeOp,
};
use crate::gui::project::{GuiProject, ProjectNodeKind, SignalEvalStack};

#[test]
fn feedback_pipeline_compiles_to_feedback_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, feedback));
    assert!(project.connect_image_link(feedback, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], TexRuntimeOp::Solid { .. }));
    assert!(matches!(
        ops[1],
        TexRuntimeOp::Feedback {
            history: TexRuntimeFeedbackHistoryBinding::Internal { feedback_node_id },
            ..
        } if feedback_node_id == feedback
    ));
}

#[test]
fn feedback_pipeline_emits_configured_frame_gap() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, feedback));
    assert!(project.connect_image_link(feedback, out));
    let frame_gap_slot = project
        .node_param_slot_index(
            feedback,
            param_schema::feedback::RUNTIME_KEYS[param_schema::feedback::RUNTIME_FRAME_GAP_INDEX],
        )
        .expect("feedback frame_gap slot should exist");
    assert!(project.set_param_value(feedback, frame_gap_slot, 3.7));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert!(matches!(
        ops.get(1),
        Some(TexRuntimeOp::Feedback { frame_gap, .. }) if *frame_gap == 4
    ));
}

#[test]
fn feedback_pipeline_requires_primary_input_even_with_target_tex_binding() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_texture_link_to_param(solid, feedback, 0));
    assert!(project.connect_image_link(feedback, out));

    assert!(GuiCompiledRuntime::compile(&project).is_none());
}

#[test]
fn feedback_target_tex_binding_does_not_override_primary_input_source() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 120, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 280, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 440, 40, 420, 480);
    assert!(project.connect_image_link(solid, feedback));
    assert!(project.connect_texture_link_to_param(circle, feedback, 0));
    assert!(project.connect_image_link(feedback, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], TexRuntimeOp::Solid { .. }));
    assert!(matches!(
        ops[1],
        TexRuntimeOp::Feedback {
            history: TexRuntimeFeedbackHistoryBinding::External { texture_node_id },
            ..
        } if texture_node_id == circle
    ));
}

#[test]
fn feedback_accumulation_binding_allows_downstream_transform_source() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 40, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexColorAdjust, 340, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 500, 40, 420, 480);
    assert!(project.connect_image_link(solid, feedback));
    assert!(project.connect_image_link(feedback, xform));
    assert!(project.connect_image_link(xform, out));
    assert!(project.connect_texture_link_to_param(xform, feedback, 0));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 4);
    assert!(matches!(ops[0], TexRuntimeOp::Solid { .. }));
    assert!(matches!(
        ops[1],
        TexRuntimeOp::Feedback {
            history: TexRuntimeFeedbackHistoryBinding::External { texture_node_id },
            ..
        } if texture_node_id == xform
    ));
    assert!(matches!(ops[2], TexRuntimeOp::ColorAdjust { .. }));
    assert!(matches!(
        ops[3],
        TexRuntimeOp::StoreTexture { texture_node_id } if texture_node_id == xform
    ));
}

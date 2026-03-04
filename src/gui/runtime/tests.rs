use super::compile_stage::compiled_step;
use super::eval_stage::compiled_texture_source_for_param;
use super::{
    compiled_param_value_opt, param_schema, CompiledStepKind, GuiCompiledRuntime,
    PostProcessCategory, TexRuntimeFeedbackHistoryBinding, TexRuntimeFrameContext, TexRuntimeOp,
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
fn reaction_diffusion_emits_temporal_op_with_defaults() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let reaction = project.add_node(ProjectNodeKind::TexReactionDiffusion, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, reaction));
    assert!(project.connect_image_link(reaction, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 2);
    assert!(matches!(
        ops[1],
        TexRuntimeOp::ReactionDiffusion {
            diffusion_a,
            diffusion_b,
            feed,
            kill,
            dt,
            seed_mix,
            history: TexRuntimeFeedbackHistoryBinding::Internal { feedback_node_id },
        } if diffusion_a == 1.0
            && diffusion_b == 0.5
            && (feed - 0.055).abs() < 1e-6
            && (kill - 0.062).abs() < 1e-6
            && dt == 1.0
            && (seed_mix - 0.04).abs() < 1e-6
            && feedback_node_id == reaction
    ));
}

#[test]
fn post_temporal_emits_post_process_op_with_history() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let post = project.add_node(ProjectNodeKind::TexPostTemporal, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, post));
    assert!(project.connect_image_link(post, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 1.25, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 2);
    assert!(matches!(
        ops[1],
        TexRuntimeOp::PostProcess {
            category: PostProcessCategory::Temporal,
            amount,
            scale,
            threshold,
            speed,
            history: Some(TexRuntimeFeedbackHistoryBinding::Internal { feedback_node_id }),
            ..
        } if (amount - 0.5).abs() < 1e-6
            && (scale - 1.0).abs() < 1e-6
            && (threshold - 0.5).abs() < 1e-6
            && (speed - 1.0).abs() < 1e-6
            && feedback_node_id == post
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
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 340, 40, 420, 480);
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
    assert!(matches!(ops[2], TexRuntimeOp::Transform { .. }));
    assert!(matches!(
        ops[3],
        TexRuntimeOp::StoreTexture { texture_node_id } if texture_node_id == xform
    ));
}

#[test]
fn sphere_buffer_pipeline_compiles_to_sphere_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(project.connect_image_link(sphere, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], TexRuntimeOp::Sphere { .. }));
}

#[test]
fn scene_pass_bg_mode_controls_alpha_clip_flag() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(project.connect_image_link(sphere, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    match ops[0] {
        TexRuntimeOp::Sphere { alpha_clip, .. } => assert!(!alpha_clip),
        _ => panic!("expected sphere op"),
    }

    assert!(project.set_param_dropdown_index(pass, 2, 1));
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    match ops[0] {
        TexRuntimeOp::Sphere { alpha_clip, .. } => assert!(alpha_clip),
        _ => panic!("expected sphere op"),
    }
}

#[test]
fn tex_circle_op_keeps_alpha_clip_disabled() {
    let mut project = GuiProject::new_empty(640, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 20, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 180, 40, 420, 480);
    assert!(project.connect_image_link(circle, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    match ops[0] {
        TexRuntimeOp::Circle { alpha_clip, .. } => assert!(!alpha_clip),
        _ => panic!("expected circle op"),
    }
}

#[test]
fn camera_zoom_scales_scene_pass_radius() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let camera = project.add_node(ProjectNodeKind::RenderCamera, 500, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 660, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 820, 40, 420, 480);
    assert!(project.connect_image_link(sphere, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, camera));
    assert!(project.connect_image_link(camera, pass));
    assert!(project.connect_image_link(pass, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    let radius_default = match ops[0] {
        TexRuntimeOp::Sphere { radius, .. } => radius,
        _ => panic!("expected sphere op"),
    };

    assert!(project.set_param_value(camera, 0, 2.0));
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    let radius_zoomed = match ops[0] {
        TexRuntimeOp::Sphere { radius, .. } => radius,
        _ => panic!("expected sphere op"),
    };
    assert!(radius_zoomed > radius_default * 1.9);
}

#[test]
fn scene_pass_resolution_defaults_to_project_size_when_zero() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(project.connect_image_link(sphere, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));
    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let (w, h) = runtime.output_texture_size(&project, 0.0, &mut eval_stack);
    assert_eq!((w, h), (640, 480));
    assert!(project.set_param_value(pass, 0, 320.0));
    assert!(project.set_param_value(pass, 1, 200.0));
    let (w2, h2) = runtime.output_texture_size(&project, 0.0, &mut eval_stack);
    assert_eq!((w2, h2), (320, 200));
}

#[test]
fn output_texture_size_does_not_reuse_signal_memo_across_projects() {
    let mut project_a = GuiProject::new_empty(640, 480);
    let lfo_a = project_a.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
    let sphere_a = project_a.add_node(ProjectNodeKind::BufSphere, 180, 40, 420, 480);
    let entity_a = project_a.add_node(ProjectNodeKind::SceneEntity, 340, 40, 420, 480);
    let scene_a = project_a.add_node(ProjectNodeKind::SceneBuild, 500, 40, 420, 480);
    let pass_a = project_a.add_node(ProjectNodeKind::RenderScenePass, 660, 40, 420, 480);
    let out_a = project_a.add_node(ProjectNodeKind::IoWindowOut, 820, 40, 420, 480);
    assert!(project_a.connect_image_link(sphere_a, entity_a));
    assert!(project_a.connect_image_link(entity_a, scene_a));
    assert!(project_a.connect_image_link(scene_a, pass_a));
    assert!(project_a.connect_image_link(pass_a, out_a));
    assert!(project_a.connect_signal_link_to_param(
        lfo_a,
        pass_a,
        param_schema::render_scene_pass::RES_WIDTH_INDEX,
    ));
    assert!(project_a.set_param_value(lfo_a, param_schema::ctl_lfo::RATE_HZ_INDEX, 0.0));
    assert!(project_a.set_param_value(lfo_a, param_schema::ctl_lfo::AMPLITUDE_INDEX, 8.0));
    assert!(project_a.set_param_value(lfo_a, param_schema::ctl_lfo::PHASE_INDEX, 0.25));
    assert!(project_a.set_param_value(lfo_a, param_schema::ctl_lfo::BIAS_INDEX, 0.0));

    let mut project_b = GuiProject::new_empty(640, 480);
    let lfo_b = project_b.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
    let sphere_b = project_b.add_node(ProjectNodeKind::BufSphere, 180, 40, 420, 480);
    let entity_b = project_b.add_node(ProjectNodeKind::SceneEntity, 340, 40, 420, 480);
    let scene_b = project_b.add_node(ProjectNodeKind::SceneBuild, 500, 40, 420, 480);
    let pass_b = project_b.add_node(ProjectNodeKind::RenderScenePass, 660, 40, 420, 480);
    let out_b = project_b.add_node(ProjectNodeKind::IoWindowOut, 820, 40, 420, 480);
    assert!(project_b.connect_image_link(sphere_b, entity_b));
    assert!(project_b.connect_image_link(entity_b, scene_b));
    assert!(project_b.connect_image_link(scene_b, pass_b));
    assert!(project_b.connect_image_link(pass_b, out_b));
    assert!(project_b.connect_signal_link_to_param(
        lfo_b,
        pass_b,
        param_schema::render_scene_pass::RES_WIDTH_INDEX,
    ));
    assert!(project_b.set_param_value(lfo_b, param_schema::ctl_lfo::RATE_HZ_INDEX, 0.0));
    assert!(project_b.set_param_value(lfo_b, param_schema::ctl_lfo::AMPLITUDE_INDEX, 20.0));
    assert!(project_b.set_param_value(lfo_b, param_schema::ctl_lfo::PHASE_INDEX, 0.25));
    assert!(project_b.set_param_value(lfo_b, param_schema::ctl_lfo::BIAS_INDEX, 0.0));

    let runtime_a = GuiCompiledRuntime::compile(&project_a).expect("runtime A should compile");
    let runtime_b = GuiCompiledRuntime::compile(&project_b).expect("runtime B should compile");
    let mut eval_stack = SignalEvalStack::default();

    let (w_a, h_a) = runtime_a.output_texture_size(&project_a, 0.0, &mut eval_stack);
    let (w_b, h_b) = runtime_b.output_texture_size(&project_b, 0.0, &mut eval_stack);

    assert_eq!(h_a, 480);
    assert_eq!(h_b, 480);
    assert_eq!(w_a, 8);
    assert_eq!(w_b, 20);
}

#[test]
fn circle_nurbs_buffer_pipeline_compiles_to_circle_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(project.connect_image_link(circle, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], TexRuntimeOp::Circle { .. }));
}

#[test]
fn circle_nurbs_params_propagate_to_circle_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(project.connect_image_link(circle, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    assert!(project.set_param_value(circle, 1, 30.0));
    assert!(project.set_param_value(circle, 2, 150.0));
    assert!(project.set_param_value(circle, 3, 1.0));
    assert!(project.set_param_value(circle, 4, 0.006));
    assert!(project.set_param_value(circle, 5, 2.0));
    assert!(project.set_param_value(circle, 6, 12.0));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    match ops[0] {
        TexRuntimeOp::Circle {
            arc_start_deg,
            arc_end_deg,
            segment_count,
            arc_open,
            line_width,
            feather,
            ..
        } => {
            assert_eq!(arc_start_deg, 30.0);
            assert_eq!(arc_end_deg, 150.0);
            assert_eq!(segment_count, 12.0);
            assert_eq!(arc_open, 1.0);
            assert!(line_width <= 0.007);
            assert!(feather > 0.01);
        }
        _ => panic!("expected circle op"),
    }
}

#[test]
fn buffer_noise_deforms_mesh_shape_parameters() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
    let noise = project.add_node(ProjectNodeKind::BufNoise, 180, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 340, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 500, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 660, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 820, 40, 420, 480);
    assert!(project.connect_image_link(sphere, noise));
    assert!(project.connect_image_link(noise, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    assert!(project.set_param_value(noise, 0, 0.5));
    assert!(project.set_param_value(noise, 1, 3.0));
    assert!(project.set_param_value(noise, 2, 1.0));
    assert!(project.set_param_value(noise, 4, 17.0));
    assert!(project.set_param_value(noise, 5, 2.5));
    assert!(project.set_param_value(noise, 6, 0.4));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops_t0 = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops_t0);
    let mut ops_t1 = Vec::new();
    runtime.evaluate_ops(&project, 0.5, &mut eval_stack, &mut ops_t1);
    let (r0, phase0, twist0, stretch0) = match ops_t0[0] {
        TexRuntimeOp::Sphere {
            radius,
            noise_phase,
            noise_twist,
            noise_stretch,
            ..
        } => (radius, noise_phase, noise_twist, noise_stretch),
        _ => panic!("expected sphere op"),
    };
    let (r1, phase1, twist1, stretch1) = match ops_t1[0] {
        TexRuntimeOp::Sphere {
            radius,
            noise_phase,
            noise_twist,
            noise_stretch,
            ..
        } => (radius, noise_phase, noise_twist, noise_stretch),
        _ => panic!("expected sphere op"),
    };
    assert_eq!(r0, r1);
    assert_ne!(phase0, phase1);
    assert!(twist0 > 2.4 && twist1 > 2.4);
    assert!(stretch0 > 0.39 && stretch1 > 0.39);
}

#[test]
fn buffer_noise_loop_mode_matches_first_and_last_timeline_frame() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
    let noise = project.add_node(ProjectNodeKind::BufNoise, 180, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 340, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 500, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 660, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 820, 40, 420, 480);
    assert!(project.connect_image_link(sphere, noise));
    assert!(project.connect_image_link(noise, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    assert!(project.set_param_value(noise, 0, 0.5));
    assert!(project.set_param_value(noise, 1, 3.4));
    assert!(project.set_param_value(noise, 3, 0.2));
    assert!(project.set_param_value(noise, 4, 11.0));
    assert!(project.set_param_value(noise, 7, 9.0));
    assert!(project.set_param_dropdown_index(noise, 8, 1));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops_first = Vec::new();
    runtime.evaluate_ops_with_frame(
        &project,
        0.0,
        Some(TexRuntimeFrameContext {
            frame_index: 0,
            frame_total: 1_800,
        }),
        &mut eval_stack,
        &mut ops_first,
    );
    let mut ops_last = Vec::new();
    runtime.evaluate_ops_with_frame(
        &project,
        1_799.0 / 60.0,
        Some(TexRuntimeFrameContext {
            frame_index: 1_799,
            frame_total: 1_800,
        }),
        &mut eval_stack,
        &mut ops_last,
    );
    let mut ops_wrapped = Vec::new();
    runtime.evaluate_ops_with_frame(
        &project,
        1_800.0 / 60.0,
        Some(TexRuntimeFrameContext {
            frame_index: 1_800,
            frame_total: 1_800,
        }),
        &mut eval_stack,
        &mut ops_wrapped,
    );
    let phase_first = match ops_first[0] {
        TexRuntimeOp::Sphere { noise_phase, .. } => noise_phase,
        _ => panic!("expected sphere op"),
    };
    let phase_last = match ops_last[0] {
        TexRuntimeOp::Sphere { noise_phase, .. } => noise_phase,
        _ => panic!("expected sphere op"),
    };
    let phase_wrapped = match ops_wrapped[0] {
        TexRuntimeOp::Sphere { noise_phase, .. } => noise_phase,
        _ => panic!("expected sphere op"),
    };
    assert!(
        (phase_first - phase_last).abs() < 1e-4,
        "loop mode should match first/last frame phase: first={phase_first}, last={phase_last}"
    );
    assert!(
        (phase_first - phase_wrapped).abs() < 1e-4,
        "loop mode should wrap back to frame 0 phase: first={phase_first}, wrapped={phase_wrapped}"
    );
}

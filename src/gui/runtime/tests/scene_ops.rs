use super::super::{GuiCompiledRuntime, TexRuntimeFrameContext, TexRuntimeOp};
use crate::gui::project::{GuiProject, ProjectNodeKind, SignalEvalStack};

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
fn box_buffer_pipeline_compiles_to_box_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let box_node = project.add_node(ProjectNodeKind::BufBox, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(project.connect_image_link(box_node, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], TexRuntimeOp::Box { .. }));
}

#[test]
fn box_params_propagate_to_box_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let box_node = project.add_node(ProjectNodeKind::BufBox, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(project.connect_image_link(box_node, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));
    assert!(project.set_param_value(box_node, 0, 0.7));
    assert!(project.set_param_value(box_node, 1, 0.44));
    assert!(project.set_param_value(box_node, 2, 0.08));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    match ops[0] {
        TexRuntimeOp::Box {
            size_x,
            size_y,
            corner_radius,
            ..
        } => {
            assert_eq!(size_x, 0.7);
            assert_eq!(size_y, 0.44);
            assert_eq!(corner_radius, 0.08);
        }
        _ => panic!("expected box op"),
    }
}

#[test]
fn grid_buffer_pipeline_compiles_to_grid_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let grid = project.add_node(ProjectNodeKind::BufGrid, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(project.connect_image_link(grid, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], TexRuntimeOp::Grid { .. }));
}

#[test]
fn grid_params_propagate_to_grid_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let grid = project.add_node(ProjectNodeKind::BufGrid, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(project.connect_image_link(grid, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));
    assert!(project.set_param_value(grid, 0, 0.9));
    assert!(project.set_param_value(grid, 1, 0.6));
    assert!(project.set_param_value(grid, 2, 12.0));
    assert!(project.set_param_value(grid, 3, 5.0));
    assert!(project.set_param_value(grid, 4, 0.02));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    match ops[0] {
        TexRuntimeOp::Grid {
            size_x,
            size_y,
            cells_x,
            cells_y,
            line_width,
            ..
        } => {
            assert_eq!(size_x, 0.9);
            assert_eq!(size_y, 0.6);
            assert_eq!(cells_x, 12.0);
            assert_eq!(cells_y, 5.0);
            assert_eq!(line_width, 0.02);
        }
        _ => panic!("expected grid op"),
    }
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

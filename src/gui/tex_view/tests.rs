use super::{TexViewerGenerator, TexViewerOp, TexViewerPayload, TexViewerUpdate};
use crate::gui::project::{GuiProject, ProjectNodeKind};
use crate::gui::timeline::editor_panel_height;

#[test]
fn supported_graph_emits_gpu_ops_payload() {
    let mut project = GuiProject::new_empty(640, 480);
    let tex_source = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    assert!(project.connect_image_link(tex_source, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
}

#[test]
fn transform_chain_produces_solid_then_transform_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(solid, xform));
    assert!(project.connect_image_link(xform, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
    assert!(matches!(ops[1], TexViewerOp::Transform { .. }));
}

#[test]
fn level_chain_produces_solid_then_level_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
    let level = project.add_node(ProjectNodeKind::TexLevel, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(solid, level));
    assert!(project.connect_image_link(level, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
    assert!(matches!(ops[1], TexViewerOp::Level { .. }));
}

#[test]
fn feedback_chain_produces_solid_then_feedback_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(solid, feedback));
    assert!(project.connect_image_link(feedback, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
    assert!(matches!(ops[1], TexViewerOp::Feedback { .. }));
}

#[test]
fn reaction_diffusion_chain_produces_solid_then_reaction_diffusion_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
    let reaction = project.add_node(ProjectNodeKind::TexReactionDiffusion, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(solid, reaction));
    assert!(project.connect_image_link(reaction, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
    assert!(matches!(ops[1], TexViewerOp::ReactionDiffusion { .. }));
}

#[test]
fn post_process_chain_produces_post_process_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
    let post = project.add_node(ProjectNodeKind::TexPostDistortion, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(solid, post));
    assert!(project.connect_image_link(post, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
    assert!(matches!(ops[1], TexViewerOp::PostProcess { .. }));
}

#[test]
fn lfo_binding_changes_gpu_op_parameter_over_time() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(solid, out));
    assert!(project.toggle_node_expanded(solid, 420, 480));
    assert!(project.connect_image_link(lfo, solid));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let r0 = match viewer.frame().expect("frame0").payload {
        TexViewerPayload::GpuOps(ops) => match ops[0] {
            TexViewerOp::Solid { color_r, .. } => color_r,
            _ => panic!("first op should be solid"),
        },
    };
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 60,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let r1 = match viewer.frame().expect("frame1").payload {
        TexViewerPayload::GpuOps(ops) => match ops[0] {
            TexViewerOp::Solid { color_r, .. } => color_r,
            _ => panic!("first op should be solid"),
        },
    };
    assert_ne!(r0, r1);
}

#[test]
fn circle_node_emits_circle_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 60, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    assert!(project.connect_image_link(circle, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], TexViewerOp::Circle { .. }));
}

#[test]
fn sphere_buffer_pipeline_emits_sphere_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 60, 80, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 220, 80, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 380, 80, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 540, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 700, 80, 420, 480);
    assert!(project.connect_image_link(sphere, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], TexViewerOp::Sphere { .. }));
}

#[test]
fn scene_pass_resolution_overrides_output_texture_size() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 60, 80, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 220, 80, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 380, 80, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 540, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 700, 80, 420, 480);
    assert!(project.connect_image_link(sphere, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));
    assert!(project.set_param_value(pass, 0, 1024.0));
    assert!(project.set_param_value(pass, 1, 256.0));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 1200,
            viewport_height: 700,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    assert_eq!(frame.texture_width, 1024);
    assert_eq!(frame.texture_height, 256);
}

#[test]
fn circle_nurbs_buffer_pipeline_emits_circle_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 60, 80, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 220, 80, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 380, 80, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 540, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 700, 80, 420, 480);
    assert!(project.connect_image_link(circle, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], TexViewerOp::Circle { .. }));
}

#[test]
fn buffer_noise_chain_emits_scene_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 60, 80, 420, 480);
    let noise = project.add_node(ProjectNodeKind::BufNoise, 220, 80, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 380, 80, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 540, 80, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 700, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 860, 80, 420, 480);
    assert!(project.connect_image_link(sphere, noise));
    assert!(project.connect_image_link(noise, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 1200,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], TexViewerOp::Sphere { .. }));
}

#[test]
fn buffer_noise_chain_remains_time_dynamic_without_signal_bindings() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 60, 80, 420, 480);
    let noise = project.add_node(ProjectNodeKind::BufNoise, 220, 80, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 380, 80, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 540, 80, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 700, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 860, 80, 420, 480);
    assert!(project.connect_image_link(sphere, noise));
    assert!(project.connect_image_link(noise, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));
    assert!(!project.has_signal_bindings());

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 1200,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let phase_t0 = match viewer.frame().expect("frame0").payload {
        TexViewerPayload::GpuOps(ops) => match ops[0] {
            TexViewerOp::Sphere { noise_phase, .. } => noise_phase,
            _ => panic!("expected sphere op"),
        },
    };

    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 1200,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 60,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let phase_t1 = match viewer.frame().expect("frame1").payload {
        TexViewerPayload::GpuOps(ops) => match ops[0] {
            TexViewerOp::Sphere { noise_phase, .. } => noise_phase,
            _ => panic!("expected sphere op"),
        },
    };

    assert_ne!(phase_t0, phase_t1);
}

#[test]
fn ui_only_state_changes_do_not_invalidate_preview_cache_key() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    assert!(project.connect_image_link(solid, out));

    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let base_key = viewer.key;

    assert!(project.toggle_node_expanded(solid, 420, 480));
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    assert_eq!(viewer.key, base_key);

    assert!(project.select_next_param(solid));
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    assert_eq!(viewer.key, base_key);
}

#[test]
fn project_swap_with_same_tex_eval_epoch_rebuilds_cached_runtime() {
    let mut project_a = GuiProject::new_empty(640, 480);
    let solid = project_a.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
    let out_a = project_a.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    assert!(project_a.connect_image_link(solid, out_a));

    let mut project_b = GuiProject::new_empty(640, 480);
    let circle = project_b.add_node(ProjectNodeKind::TexCircle, 60, 80, 420, 480);
    let out_b = project_b.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    assert!(project_b.connect_image_link(circle, out_b));

    assert_eq!(
        project_a.invalidation().tex_eval,
        project_b.invalidation().tex_eval
    );

    let mut viewer = TexViewerGenerator::default();
    let update = TexViewerUpdate {
        viewport_width: 960,
        viewport_height: 540,
        panel_width: 420,
        frame_index: 0,
        timeline_total_frames: 1_800,
        timeline_fps: 60,
        tex_eval_epoch: project_a.invalidation().tex_eval,
    };
    viewer.update(&project_a, update);
    let frame_a = viewer.frame().expect("first frame should exist");
    let ops_a = match frame_a.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops_a.len(), 1);
    assert!(matches!(ops_a[0], TexViewerOp::Solid { .. }));

    viewer.update(
        &project_b,
        TexViewerUpdate {
            tex_eval_epoch: project_b.invalidation().tex_eval,
            ..update
        },
    );
    let frame_b = viewer.frame().expect("second frame should exist");
    let ops_b = match frame_b.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert_eq!(ops_b.len(), 1);
    assert!(matches!(ops_b[0], TexViewerOp::Circle { .. }));
}

#[test]
fn disconnected_graph_returns_empty_gpu_payload() {
    let project = GuiProject::new_empty(640, 480);
    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert!(ops.is_empty());
}

#[test]
fn viewer_frame_fits_texture_aspect_inside_output_panel() {
    let project = GuiProject::new_empty(1920, 1080);
    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 1200,
            viewport_height: 900,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    assert_eq!(frame.texture_width, 1920);
    assert_eq!(frame.texture_height, 1080);
    assert_eq!(frame.width, 780);
    assert_eq!(frame.height, 438);
    assert_eq!(frame.x, 420);
    let expected_y = ((editor_panel_height(900) as u32 - frame.height) / 2) as i32;
    assert_eq!(frame.y, expected_y);
}

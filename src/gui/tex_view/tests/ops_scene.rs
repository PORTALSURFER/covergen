use super::*;
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
fn box_buffer_pipeline_emits_box_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let box_node = project.add_node(ProjectNodeKind::BufBox, 60, 80, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 220, 80, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 380, 80, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 540, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 700, 80, 420, 480);
    assert!(project.connect_image_link(box_node, entity));
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
    assert!(matches!(ops[0], TexViewerOp::Box { .. }));
}

#[test]
fn grid_buffer_pipeline_emits_grid_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let grid = project.add_node(ProjectNodeKind::BufGrid, 60, 80, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 220, 80, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 380, 80, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 540, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 700, 80, 420, 480);
    assert!(project.connect_image_link(grid, entity));
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
    assert!(matches!(ops[0], TexViewerOp::Grid { .. }));
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

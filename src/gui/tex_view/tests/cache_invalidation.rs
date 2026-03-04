use super::*;
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
fn param_edit_updates_preview_ops_without_frame_advance() {
    let mut project = GuiProject::new_empty(640, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 60, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    assert!(project.connect_image_link(circle, out));

    let mut viewer = TexViewerGenerator::default();
    let update = TexViewerUpdate {
        viewport_width: 960,
        viewport_height: 540,
        panel_width: 420,
        frame_index: 0,
        timeline_total_frames: 1_800,
        timeline_fps: 60,
        tex_eval_epoch: project.invalidation().tex_eval,
    };
    viewer.update(&project, update);
    let baseline = viewer.frame().expect("baseline frame should exist");
    let baseline_center_x = match baseline.payload {
        TexViewerPayload::GpuOps(ops) => match ops[0] {
            TexViewerOp::Circle { center_x, .. } => center_x,
            _ => panic!("expected circle op"),
        },
    };
    let baseline_sig = baseline.ops_uniform_signature;

    assert!(project.set_param_value(circle, 0, 0.2));
    viewer.update(
        &project,
        TexViewerUpdate {
            tex_eval_epoch: project.invalidation().tex_eval,
            ..update
        },
    );
    let updated = viewer.frame().expect("updated frame should exist");
    let updated_center_x = match updated.payload {
        TexViewerPayload::GpuOps(ops) => match ops[0] {
            TexViewerOp::Circle { center_x, .. } => center_x,
            _ => panic!("expected circle op"),
        },
    };

    assert_ne!(baseline_center_x, updated_center_x);
    assert_ne!(baseline_sig, updated.ops_uniform_signature);
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

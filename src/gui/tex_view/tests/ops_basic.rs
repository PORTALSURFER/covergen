use super::*;
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
fn source_noise_node_emits_source_noise_op() {
    let mut project = GuiProject::new_empty(640, 480);
    let noise = project.add_node(ProjectNodeKind::TexSourceNoise, 60, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    assert!(project.connect_image_link(noise, out));

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
    assert!(matches!(ops[0], TexViewerOp::SourceNoise { .. }));
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
    assert!(matches!(ops[1], TexViewerOp::Transform2D { .. }));
}

#[test]
fn color_adjust_chain_produces_solid_then_color_adjust_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
    let color_adjust = project.add_node(ProjectNodeKind::TexColorAdjust, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(solid, color_adjust));
    assert!(project.connect_image_link(color_adjust, out));

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
    assert!(matches!(ops[1], TexViewerOp::ColorAdjust { .. }));
}

#[test]
fn mask_chain_produces_source_noise_then_mask_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let noise = project.add_node(ProjectNodeKind::TexSourceNoise, 40, 80, 420, 480);
    let mask = project.add_node(ProjectNodeKind::TexMask, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(noise, mask));
    assert!(project.connect_image_link(mask, out));

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
    assert!(matches!(ops[0], TexViewerOp::SourceNoise { .. }));
    assert!(matches!(ops[1], TexViewerOp::Mask { .. }));
}

#[test]
fn morphology_chain_produces_source_noise_then_morphology_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let noise = project.add_node(ProjectNodeKind::TexSourceNoise, 40, 80, 420, 480);
    let morphology = project.add_node(ProjectNodeKind::TexMorphology, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(noise, morphology));
    assert!(project.connect_image_link(morphology, out));

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
    assert!(matches!(ops[0], TexViewerOp::SourceNoise { .. }));
    assert!(matches!(ops[1], TexViewerOp::Morphology { .. }));
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
fn tone_map_chain_produces_source_noise_then_tone_map_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let noise = project.add_node(ProjectNodeKind::TexSourceNoise, 40, 80, 420, 480);
    let tone_map = project.add_node(ProjectNodeKind::TexToneMap, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(noise, tone_map));
    assert!(project.connect_image_link(tone_map, out));

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
    assert!(matches!(ops[0], TexViewerOp::SourceNoise { .. }));
    assert!(matches!(ops[1], TexViewerOp::ToneMap { .. }));
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
fn warp_transform_chain_produces_source_noise_then_warp_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let noise = project.add_node(ProjectNodeKind::TexSourceNoise, 40, 80, 420, 480);
    let warp = project.add_node(ProjectNodeKind::TexWarpTransform, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(noise, warp));
    assert!(project.connect_image_link(warp, out));

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
    assert!(matches!(ops[0], TexViewerOp::SourceNoise { .. }));
    assert!(matches!(ops[1], TexViewerOp::WarpTransform { .. }));
}

#[test]
fn directional_smear_chain_produces_source_noise_then_smear_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let noise = project.add_node(ProjectNodeKind::TexSourceNoise, 40, 80, 420, 480);
    let smear = project.add_node(ProjectNodeKind::TexDirectionalSmear, 180, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(noise, smear));
    assert!(project.connect_image_link(smear, out));

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
    assert!(matches!(ops[0], TexViewerOp::SourceNoise { .. }));
    assert!(matches!(ops[1], TexViewerOp::DirectionalSmear { .. }));
}

#[test]
fn domain_warp_chain_produces_store_and_domain_warp_ops() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
    let noise = project.add_node(ProjectNodeKind::TexSourceNoise, 180, 80, 420, 480);
    let domain_warp = project.add_node(ProjectNodeKind::TexDomainWarp, 320, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 460, 80, 420, 480);
    assert!(project.connect_image_link(solid, domain_warp));
    assert!(project.connect_texture_link_to_param(noise, domain_warp, 0));
    assert!(project.connect_image_link(domain_warp, out));

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
    assert_eq!(ops.len(), 5);
    assert!(matches!(ops[0], TexViewerOp::Solid { .. }));
    assert!(matches!(ops[1], TexViewerOp::StoreTexture { .. }));
    assert!(matches!(ops[2], TexViewerOp::SourceNoise { .. }));
    assert!(matches!(ops[3], TexViewerOp::StoreTexture { .. }));
    assert!(matches!(ops[4], TexViewerOp::DomainWarp { .. }));
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

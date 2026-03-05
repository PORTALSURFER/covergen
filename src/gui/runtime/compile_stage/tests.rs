use super::*;

fn compile_steps_for_node(project: &GuiProject, node_id: u32) -> Option<Vec<CompiledStep>> {
    let mut traversal = CompileTraversalState::default();
    let mut steps = Vec::new();
    if !compile_node(project, node_id, &mut traversal, &mut steps) {
        return None;
    }
    Some(steps)
}

#[test]
fn compile_node_rejects_when_node_is_already_on_traversal_stack() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 640, 480);
    let mut traversal = CompileTraversalState::default();
    traversal.visiting.insert(solid);
    let mut steps = Vec::new();

    assert!(!compile_node(&project, solid, &mut traversal, &mut steps));
    assert!(steps.is_empty());
}

#[test]
fn compile_leaf_nodes_emit_expected_step_kind_without_dependencies() {
    let cases = [
        (ProjectNodeKind::TexSolid, CompiledStepKind::Solid),
        (ProjectNodeKind::TexCircle, CompiledStepKind::Circle),
        (ProjectNodeKind::BufSphere, CompiledStepKind::SphereBuffer),
        (
            ProjectNodeKind::BufCircleNurbs,
            CompiledStepKind::CircleNurbsBuffer,
        ),
    ];
    for (kind, expected_step_kind) in cases {
        let mut project = GuiProject::new_empty(640, 480);
        let node_id = project.add_node(kind, 20, 40, 640, 480);
        let steps =
            compile_steps_for_node(&project, node_id).expect("leaf node should compile cleanly");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].node_id, node_id);
        assert_eq!(steps[0].kind, expected_step_kind);
    }
}

#[test]
fn compile_nodes_reject_missing_primary_input_sources() {
    let kinds = [
        ProjectNodeKind::BufNoise,
        ProjectNodeKind::TexTransform2D,
        ProjectNodeKind::TexLevel,
        ProjectNodeKind::TexFeedback,
        ProjectNodeKind::TexReactionDiffusion,
        ProjectNodeKind::TexPostTemporal,
        ProjectNodeKind::TexBlend,
        ProjectNodeKind::SceneEntity,
        ProjectNodeKind::SceneBuild,
        ProjectNodeKind::RenderCamera,
        ProjectNodeKind::RenderScenePass,
    ];
    for kind in kinds {
        let mut project = GuiProject::new_empty(640, 480);
        let node_id = project.add_node(kind, 40, 40, 640, 480);
        let mut traversal = CompileTraversalState::default();
        let mut steps = Vec::new();
        assert!(
            !compile_node(&project, node_id, &mut traversal, &mut steps),
            "{kind:?} should reject compilation without a primary source input"
        );
        assert!(steps.is_empty());
    }
}

#[test]
fn compile_post_process_nodes_emit_expected_category_step() {
    let cases = [
        (
            ProjectNodeKind::TexPostColorTone,
            PostProcessCategory::ColorTone,
        ),
        (
            ProjectNodeKind::TexPostEdgeStructure,
            PostProcessCategory::EdgeStructure,
        ),
        (
            ProjectNodeKind::TexPostBlurDiffusion,
            PostProcessCategory::BlurDiffusion,
        ),
        (
            ProjectNodeKind::TexPostDistortion,
            PostProcessCategory::Distortion,
        ),
        (
            ProjectNodeKind::TexPostTemporal,
            PostProcessCategory::Temporal,
        ),
        (
            ProjectNodeKind::TexPostNoiseTexture,
            PostProcessCategory::NoiseTexture,
        ),
        (
            ProjectNodeKind::TexPostLighting,
            PostProcessCategory::Lighting,
        ),
        (
            ProjectNodeKind::TexPostScreenSpace,
            PostProcessCategory::ScreenSpace,
        ),
        (
            ProjectNodeKind::TexPostExperimental,
            PostProcessCategory::Experimental,
        ),
    ];
    for (post_kind, expected_category) in cases {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 640, 480);
        let post = project.add_node(post_kind, 160, 40, 640, 480);
        assert!(project.connect_image_link(solid, post));

        let steps = compile_steps_for_node(&project, post)
            .expect("post-process node with source should compile");
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].kind, CompiledStepKind::Solid);
        assert_eq!(
            steps[1].kind,
            CompiledStepKind::PostProcess {
                category: expected_category,
            }
        );
    }
}

#[test]
fn compile_blend_node_emits_store_steps_for_base_and_layer_sources() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 640, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 120, 40, 640, 480);
    let blend = project.add_node(ProjectNodeKind::TexBlend, 220, 40, 640, 480);
    assert!(project.connect_image_link(solid, blend));
    assert!(project.connect_texture_link_to_param(circle, blend, 0));

    let mut traversal = CompileTraversalState::default();
    let mut steps = Vec::new();
    assert!(compile_node(&project, blend, &mut traversal, &mut steps));

    let mut saw_base_store = false;
    let mut saw_layer_store = false;
    let mut saw_blend = false;
    for step in steps {
        match step.kind {
            CompiledStepKind::StoreTexture if step.node_id == solid => saw_base_store = true,
            CompiledStepKind::StoreTexture if step.node_id == circle => saw_layer_store = true,
            CompiledStepKind::Blend {
                base_source_id,
                layer_source_id,
            } => {
                saw_blend = true;
                assert_eq!(base_source_id, solid);
                assert_eq!(layer_source_id, Some(circle));
            }
            _ => {}
        }
    }
    assert!(saw_base_store);
    assert!(saw_layer_store);
    assert!(saw_blend);
}

#[test]
fn compile_blend_orders_layer_before_base_when_base_depends_on_layer() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 640, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 120, 40, 640, 480);
    let blend = project.add_node(ProjectNodeKind::TexBlend, 220, 40, 640, 480);
    assert!(project.connect_image_link(solid, xform));
    assert!(project.connect_image_link(xform, blend));
    assert!(project.connect_texture_link_to_param(solid, blend, 0));

    let steps = compile_steps_for_node(&project, blend)
        .expect("blend with dependent base/layer should compile deterministically");
    assert_eq!(steps.len(), 5);
    assert_eq!(steps[0].kind, CompiledStepKind::Solid);
    assert_eq!(steps[0].node_id, solid);
    assert_eq!(steps[1].kind, CompiledStepKind::StoreTexture);
    assert_eq!(steps[1].node_id, solid);
    assert_eq!(steps[2].kind, CompiledStepKind::Transform);
    assert_eq!(steps[2].node_id, xform);
    assert_eq!(steps[3].kind, CompiledStepKind::StoreTexture);
    assert_eq!(steps[3].node_id, xform);
    assert_eq!(
        steps[4].kind,
        CompiledStepKind::Blend {
            base_source_id: xform,
            layer_source_id: Some(solid),
        }
    );
    assert_eq!(steps[4].node_id, blend);
}

#[test]
fn compile_feedback_blend_chain_emits_stable_topological_order() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 640, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 120, 40, 640, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 220, 40, 640, 480);
    let blend = project.add_node(ProjectNodeKind::TexBlend, 320, 40, 640, 480);

    assert!(project.connect_image_link(solid, feedback));
    assert!(project.connect_image_link(feedback, blend));
    assert!(project.connect_texture_link_to_param(circle, blend, 0));

    let steps = compile_steps_for_node(&project, blend)
        .expect("feedback+blend chain should compile into deterministic ordered steps");
    let kinds: Vec<CompiledStepKind> = steps.iter().map(|step| step.kind).collect();
    assert_eq!(
        kinds,
        vec![
            CompiledStepKind::Solid,
            CompiledStepKind::Feedback,
            CompiledStepKind::StoreTexture,
            CompiledStepKind::Circle,
            CompiledStepKind::StoreTexture,
            CompiledStepKind::Blend {
                base_source_id: feedback,
                layer_source_id: Some(circle),
            },
        ]
    );
    assert_eq!(steps[0].node_id, solid);
    assert_eq!(steps[1].node_id, feedback);
    assert_eq!(steps[2].node_id, feedback);
    assert_eq!(steps[3].node_id, circle);
    assert_eq!(steps[4].node_id, circle);
    assert_eq!(steps[5].node_id, blend);
}

#[test]
fn compile_scene_chain_emits_stable_topological_order() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 640, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 120, 40, 640, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 220, 40, 640, 480);
    let camera = project.add_node(ProjectNodeKind::RenderCamera, 320, 40, 640, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 420, 40, 640, 480);

    assert!(project.connect_image_link(sphere, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, camera));
    assert!(project.connect_image_link(camera, pass));

    let steps = compile_steps_for_node(&project, pass)
        .expect("scene chain should compile into deterministic ordered steps");
    let kinds: Vec<CompiledStepKind> = steps.iter().map(|step| step.kind).collect();
    assert_eq!(
        kinds,
        vec![
            CompiledStepKind::SphereBuffer,
            CompiledStepKind::SceneEntity,
            CompiledStepKind::SceneBuild,
            CompiledStepKind::Camera,
            CompiledStepKind::ScenePass,
        ]
    );
    assert_eq!(steps[0].node_id, sphere);
    assert_eq!(steps[1].node_id, entity);
    assert_eq!(steps[2].node_id, scene);
    assert_eq!(steps[3].node_id, camera);
    assert_eq!(steps[4].node_id, pass);
}

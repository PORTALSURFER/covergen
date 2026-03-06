use super::*;

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
fn compile_domain_warp_emits_store_steps_for_base_and_warp_sources() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 640, 480);
    let noise = project.add_node(ProjectNodeKind::TexSourceNoise, 120, 40, 640, 480);
    let domain_warp = project.add_node(ProjectNodeKind::TexDomainWarp, 220, 40, 640, 480);
    assert!(project.connect_image_link(solid, domain_warp));
    assert!(project.connect_texture_link_to_param(noise, domain_warp, 0));

    let mut traversal = CompileTraversalState::default();
    let mut steps = Vec::new();
    assert!(compile_node(
        &project,
        domain_warp,
        &mut traversal,
        &mut steps
    ));

    let mut saw_base_store = false;
    let mut saw_warp_store = false;
    let mut saw_domain_warp = false;
    for step in steps {
        match step.kind {
            CompiledStepKind::StoreTexture if step.node_id == solid => saw_base_store = true,
            CompiledStepKind::StoreTexture if step.node_id == noise => saw_warp_store = true,
            CompiledStepKind::DomainWarp {
                base_source_id,
                warp_source_id,
            } => {
                saw_domain_warp = true;
                assert_eq!(base_source_id, solid);
                assert_eq!(warp_source_id, Some(noise));
            }
            _ => {}
        }
    }
    assert!(saw_base_store);
    assert!(saw_warp_store);
    assert!(saw_domain_warp);
}

#[test]
fn compile_morphology_and_smear_emit_render_steps() {
    let mut project = GuiProject::new_empty(640, 480);
    let noise = project.add_node(ProjectNodeKind::TexSourceNoise, 20, 40, 640, 480);
    let morphology = project.add_node(ProjectNodeKind::TexMorphology, 120, 40, 640, 480);
    let smear = project.add_node(ProjectNodeKind::TexDirectionalSmear, 220, 40, 640, 480);
    assert!(project.connect_image_link(noise, morphology));
    assert!(project.connect_image_link(morphology, smear));

    let mut traversal = CompileTraversalState::default();
    let mut steps = Vec::new();
    assert!(compile_node(&project, smear, &mut traversal, &mut steps));
    assert!(steps
        .iter()
        .any(|step| step.kind == CompiledStepKind::Morphology));
    assert!(steps
        .iter()
        .any(|step| step.kind == CompiledStepKind::DirectionalSmear));
}

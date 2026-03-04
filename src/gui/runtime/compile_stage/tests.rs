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

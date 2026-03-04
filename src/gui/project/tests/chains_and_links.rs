use super::*;
#[test]
fn sphere_buffer_scene_chain_requires_typed_intermediate_nodes() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(!project.connect_image_link(sphere, out));
    assert!(project.connect_image_link(sphere, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));
    assert_eq!(
        project.link_resource_kind(sphere, entity),
        Some(ResourceKind::Buffer)
    );
    assert_eq!(
        project.link_resource_kind(entity, scene),
        Some(ResourceKind::Entity)
    );
    assert_eq!(
        project.link_resource_kind(scene, pass),
        Some(ResourceKind::Scene)
    );
    assert_eq!(
        project.link_resource_kind(pass, out),
        Some(ResourceKind::Texture2D)
    );
}

#[test]
fn camera_node_accepts_scene_and_outputs_scene() {
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
    assert_eq!(
        project.link_resource_kind(scene, camera),
        Some(ResourceKind::Scene)
    );
    assert_eq!(
        project.link_resource_kind(camera, pass),
        Some(ResourceKind::Scene)
    );
}

#[test]
fn circle_nurbs_buffer_scene_chain_requires_typed_intermediate_nodes() {
    let mut project = GuiProject::new_empty(640, 480);
    let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(!project.connect_image_link(circle, out));
    assert!(project.connect_image_link(circle, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));
    assert_eq!(
        project.link_resource_kind(circle, entity),
        Some(ResourceKind::Buffer)
    );
    assert_eq!(
        project.link_resource_kind(entity, scene),
        Some(ResourceKind::Entity)
    );
    assert_eq!(
        project.link_resource_kind(scene, pass),
        Some(ResourceKind::Scene)
    );
    assert_eq!(
        project.link_resource_kind(pass, out),
        Some(ResourceKind::Texture2D)
    );
}

#[test]
fn buffer_noise_chain_requires_buffer_input_and_outputs_buffer() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 120, 420, 480);
    let noise = project.add_node(ProjectNodeKind::BufNoise, 180, 120, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 340, 120, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 500, 120, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 660, 120, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 820, 120, 420, 480);

    assert!(!project.connect_image_link(solid, noise));
    assert!(project.connect_image_link(sphere, noise));
    assert!(project.connect_image_link(noise, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));
    assert_eq!(
        project.link_resource_kind(sphere, noise),
        Some(ResourceKind::Buffer)
    );
    assert_eq!(
        project.link_resource_kind(noise, entity),
        Some(ResourceKind::Buffer)
    );
}

#[test]
fn connect_image_link_rejects_cycle() {
    let mut project = GuiProject::new_empty(640, 480);
    let a = project.add_node(ProjectNodeKind::TexTransform2D, 20, 40, 420, 480);
    let b = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
    assert!(project.connect_image_link(a, b));
    assert!(!project.connect_image_link(b, a));
}

#[test]
fn insert_node_on_primary_link_rewires_texture_chain() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, out));
    assert!(project.insert_node_on_primary_link(xform, solid, out));
    assert_eq!(project.input_source_node_id(xform), Some(solid));
    assert_eq!(project.input_source_node_id(out), Some(xform));
}

#[test]
fn insert_node_on_primary_link_rejects_incompatible_node() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, out));
    assert!(!project.insert_node_on_primary_link(lfo, solid, out));
    assert_eq!(project.input_source_node_id(out), Some(solid));
    assert_eq!(project.input_source_node_id(lfo), None);
}

#[test]
fn disconnect_link_removes_texture_and_signal_bindings() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 160, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 40, 420, 480);
    assert!(project.connect_image_link(solid, out));
    assert!(project.toggle_node_expanded(solid, 420, 480));
    assert!(project.select_next_param(solid));
    assert!(project.connect_image_link(lfo, solid));
    assert!(project.edge_count() >= 2);
    assert!(project.disconnect_link(lfo, solid));
    assert!(project.disconnect_link(solid, out));
    assert_eq!(project.edge_count(), 0);
}

#[test]
fn connect_signal_link_to_specific_param_row() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 160, 40, 420, 480);
    assert!(project.connect_signal_link_to_param(lfo, circle, 5));
    let source = project.sample_signal_node(lfo, 0.25, &mut Vec::new());
    let value = project.node_param_value(circle, "color_g", 0.25, &mut Vec::new());
    assert_eq!(source, value);
    assert!(!project.connect_signal_link_to_param(lfo, circle, 5));
}

#[test]
fn disconnect_signal_link_from_param_only_unbinds_target_row() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 160, 40, 420, 480);
    assert!(project.connect_signal_link_to_param(lfo, circle, 0));
    assert!(project.connect_signal_link_to_param(lfo, circle, 1));
    assert_eq!(project.signal_source_for_param(circle, 0), Some(lfo));
    assert_eq!(project.signal_source_for_param(circle, 1), Some(lfo));

    assert!(project.disconnect_signal_link_from_param(circle, 0));
    assert_eq!(project.signal_source_for_param(circle, 0), None);
    assert_eq!(project.signal_source_for_param(circle, 1), Some(lfo));
    assert!(!project.disconnect_signal_link_from_param(circle, 0));
}

#[test]
fn manual_param_edit_detaches_signal_binding_on_target_row() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 160, 40, 420, 480);
    assert!(project.connect_signal_link_to_param(lfo, circle, 0));
    assert_eq!(project.signal_source_for_param(circle, 0), Some(lfo));

    assert!(project.set_param_value(circle, 0, 0.2));
    assert_eq!(project.signal_source_for_param(circle, 0), None);
    let value = project
        .node_param_raw_value(circle, 0)
        .expect("manual value should exist");
    assert!((value - 0.2).abs() < 1e-5);
}

#[test]
fn connect_texture_link_to_feedback_target_param_row() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 40, 420, 480);
    assert!(project.connect_texture_link_to_param(solid, feedback, 0));
    assert_eq!(project.texture_source_for_param(feedback, 0), Some(solid));
    assert_eq!(
        project.node_param_raw_text(feedback, 0),
        Some("tex.solid#1")
    );
    assert_eq!(
        project.param_link_source_for_param(feedback, 0),
        Some((solid, ResourceKind::Texture2D))
    );
    assert!(!project.connect_texture_link_to_param(solid, feedback, 0));
}

#[test]
fn connect_texture_link_to_feedback_accumulation_allows_downstream_source() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 40, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 340, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 500, 40, 420, 480);
    assert!(project.connect_image_link(solid, feedback));
    assert!(project.connect_image_link(feedback, xform));
    assert!(project.connect_image_link(xform, out));

    assert!(project.connect_texture_link_to_param(xform, feedback, 0));
    assert_eq!(project.texture_source_for_param(feedback, 0), Some(xform));
    let expected_label = format!("tex.transform_2d#{xform}");
    assert_eq!(
        project.node_param_raw_text(feedback, 0),
        Some(expected_label.as_str())
    );
}

#[test]
fn link_resource_kind_reports_texture_and_signal_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 120, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 200, 40, 420, 480);
    assert!(project.connect_image_link(solid, xform));
    assert!(project.connect_signal_link_to_param(lfo, xform, 3));
    assert_eq!(
        project.link_resource_kind(solid, xform),
        Some(ResourceKind::Texture2D)
    );
    assert_eq!(
        project.link_resource_kind(lfo, xform),
        Some(ResourceKind::Signal)
    );
    assert_eq!(project.signal_param_index_for_source(lfo, xform), Some(3));
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 360, 40, 420, 480);
    assert!(project.connect_texture_link_to_param(solid, feedback, 0));
    assert_eq!(
        project.link_resource_kind(solid, feedback),
        Some(ResourceKind::Texture2D)
    );
}

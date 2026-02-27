use super::{
    input_pin_center, node_expand_toggle_rect, node_param_value_rect, output_pin_center,
    GraphBounds, GuiProject, PersistedGuiProject, ProjectNodeKind, ResourceKind, NODE_HEIGHT,
    PARAM_LABEL_MAX_LEN,
};

#[test]
fn empty_project_has_no_nodes() {
    let project = GuiProject::new_empty(640, 480);
    assert_eq!(project.node_count(), 0);
}

#[test]
fn add_node_assigns_incrementing_ids() {
    let mut project = GuiProject::new_empty(640, 480);
    let a = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let b = project.add_node(ProjectNodeKind::IoWindowOut, 120, 120, 420, 480);
    assert_eq!(a, 1);
    assert_eq!(b, 2);
}

#[test]
fn node_hit_test_uses_topmost_order() {
    let mut project = GuiProject::new_empty(640, 480);
    let a = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let b = project.add_node(ProjectNodeKind::IoWindowOut, 80, 80, 420, 480);
    assert_eq!(project.node_at(90, 90), Some(b));
    assert_ne!(project.node_at(90, 90), Some(a));
}

#[test]
fn node_hit_test_updates_after_move_without_full_scan_state_drift() {
    let mut project = GuiProject::new_empty(640, 480);
    let node = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    assert_eq!(project.node_at(90, 90), Some(node));
    assert!(project.move_node(node, 260, 220, 420, 480));
    assert_eq!(project.node_at(90, 90), None);
    assert_eq!(project.node_at(270, 230), Some(node));
}

#[test]
fn expanded_node_hit_bounds_update_after_toggle() {
    let mut project = GuiProject::new_empty(640, 480);
    let node = project.add_node(ProjectNodeKind::TexSolid, 60, 60, 420, 480);
    let base_miss_y = 60 + NODE_HEIGHT + 4;
    assert_eq!(project.node_at(72, base_miss_y), None);
    assert!(project.toggle_node_expanded(node, 420, 480));
    assert_eq!(project.node_at(72, base_miss_y), Some(node));
}

#[test]
fn pin_hit_tests_work_through_spatial_bins() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 240, 80, 420, 480);
    let solid_node = project.node(solid).expect("solid node");
    let out_node = project.node(out).expect("output node");
    let (ox, oy) = output_pin_center(solid_node).expect("solid output");
    let (ix, iy) = input_pin_center(out_node).expect("output input");
    assert_eq!(project.output_pin_at(ox, oy, 10), Some(solid));
    assert_eq!(project.input_pin_at(ix, iy, 10, None), Some(out));
    assert_eq!(project.input_pin_at(ix, iy, 10, Some(out)), None);
}

#[test]
fn node_rect_query_returns_overlapping_nodes_only() {
    let mut project = GuiProject::new_empty(640, 480);
    let a = project.add_node(ProjectNodeKind::TexSolid, 40, 40, 420, 480);
    let b = project.add_node(ProjectNodeKind::TexCircle, 280, 180, 420, 480);
    let c = project.add_node(ProjectNodeKind::IoWindowOut, 360, 40, 420, 480);
    let overlaps = project.node_ids_overlapping_graph_rect(20, 20, 250, 170);
    assert_eq!(overlaps, vec![a]);
    let overlaps_multi = project.node_ids_overlapping_graph_rect(260, 20, 620, 260);
    assert_eq!(overlaps_multi, vec![b, c]);
}

#[test]
fn connect_image_link_wires_solid_to_window_out() {
    let mut project = GuiProject::new_empty(640, 480);
    let tex_source = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    assert!(project.connect_image_link(tex_source, out));
    assert_eq!(project.edge_count(), 1);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexSolid);
    assert!(!project.connect_image_link(tex_source, out));
}

#[test]
fn transform_node_supports_in_and_out_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 160, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
    assert!(project.connect_image_link(source, xform));
    assert!(project.connect_image_link(xform, out));
    assert_eq!(project.edge_count(), 2);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexTransform2D);
}

#[test]
fn feedback_node_supports_in_and_out_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 160, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
    assert!(project.connect_image_link(source, feedback));
    assert!(project.connect_image_link(feedback, out));
    assert_eq!(project.edge_count(), 2);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexFeedback);
}

#[test]
fn reaction_diffusion_node_supports_in_and_out_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let reaction = project.add_node(ProjectNodeKind::TexReactionDiffusion, 160, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
    assert!(project.connect_image_link(source, reaction));
    assert!(project.connect_image_link(reaction, out));
    assert_eq!(project.edge_count(), 2);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexReactionDiffusion);
}

#[test]
fn post_process_node_supports_in_and_out_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let post = project.add_node(ProjectNodeKind::TexPostColorTone, 160, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
    assert!(project.connect_image_link(source, post));
    assert!(project.connect_image_link(post, out));
    assert_eq!(project.edge_count(), 2);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexPostColorTone);
}

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

#[test]
fn delete_nodes_removes_nodes_and_clears_referenced_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 160, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 40, 420, 480);
    assert!(project.connect_image_link(solid, out));
    assert!(project.toggle_node_expanded(solid, 420, 480));
    assert!(project.select_next_param(solid));
    assert!(project.connect_image_link(lfo, solid));
    assert!(project.edge_count() >= 2);
    assert!(project.delete_nodes(&[solid]));
    assert!(project.node(solid).is_none());
    assert_eq!(project.edge_count(), 0);
    assert!(project.window_out_input_node_id().is_none());
}

#[test]
fn delete_nodes_rewires_single_texture_gap() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, xform));
    assert!(project.connect_image_link(xform, out));
    assert_eq!(project.window_out_input_node_id(), Some(xform));

    assert!(project.delete_nodes(&[xform]));
    assert!(project.node(xform).is_none());
    assert_eq!(project.window_out_input_node_id(), Some(solid));
    assert_eq!(project.edge_count(), 1);
}

#[test]
fn delete_nodes_rewires_multiple_deleted_texture_nodes() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let xform_a = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
    let xform_b = project.add_node(ProjectNodeKind::TexTransform2D, 340, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 500, 40, 420, 480);
    assert!(project.connect_image_link(solid, xform_a));
    assert!(project.connect_image_link(xform_a, xform_b));
    assert!(project.connect_image_link(xform_b, out));
    assert_eq!(project.window_out_input_node_id(), Some(xform_b));

    assert!(project.delete_nodes(&[xform_a, xform_b]));
    assert!(project.node(xform_a).is_none());
    assert!(project.node(xform_b).is_none());
    assert_eq!(project.window_out_input_node_id(), Some(solid));
    assert_eq!(project.edge_count(), 1);
}

#[test]
fn delete_nodes_rewires_texture_target_param_binding_gap() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, xform));
    assert!(project.connect_texture_link_to_param(xform, feedback, 0));
    assert_eq!(project.texture_source_for_param(feedback, 0), Some(xform));

    assert!(project.delete_nodes(&[xform]));
    assert_eq!(project.texture_source_for_param(feedback, 0), Some(solid));
}

#[test]
fn set_param_value_clamps_to_slot_range() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    assert!(project.set_param_value(solid, 0, 10.0));
    let value = project
        .node_param_raw_value(solid, 0)
        .expect("param value should exist");
    assert_eq!(value, 1.0);
    let value_text = project
        .node_param_raw_text(solid, 0)
        .expect("param text should exist");
    assert_eq!(value_text, "1.000");
}

#[test]
fn lfo_amplitude_accepts_higher_values() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
    assert!(project.set_param_value(lfo, 1, 12.5));
    let value = project
        .node_param_raw_value(lfo, 1)
        .expect("param value should exist");
    assert_eq!(value, 12.5);
}

#[test]
fn lfo_sync_mode_uses_dropdown_options() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
    assert!(project.param_is_dropdown(lfo, 4));
    assert!(!project.param_supports_text_edit(lfo, 4));
    let options = project
        .node_param_dropdown_options(lfo, 4)
        .expect("dropdown options should exist");
    assert_eq!(options.len(), 2);
    assert_eq!(project.node_param_raw_text(lfo, 4), Some("free"));
    assert!(project.set_param_dropdown_index(lfo, 4, 1));
    assert_eq!(project.node_param_raw_text(lfo, 4), Some("beat"));
    assert!(project.adjust_param(lfo, 4, -1.0));
    assert_eq!(project.node_param_raw_text(lfo, 4), Some("free"));
}

#[test]
fn beat_synced_lfo_follows_project_timeline_bpm() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
    assert!(project.set_param_dropdown_index(lfo, 4, 1));
    assert!(project.set_param_value(lfo, 5, 2.0));

    assert!(project.set_lfo_sync_bpm(90.0));
    let sample_90 = project
        .sample_signal_node(lfo, 0.125, &mut Vec::new())
        .expect("lfo should evaluate");

    assert!(project.set_lfo_sync_bpm(60.0));
    let sample_60 = project
        .sample_signal_node(lfo, 0.125, &mut Vec::new())
        .expect("lfo should evaluate");

    assert!(
        (sample_90 - sample_60).abs() > 0.05,
        "beat-synced lfo should change when bpm changes"
    );
}

#[test]
fn lfo_type_uses_dropdown_options() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
    assert!(project.param_is_dropdown(lfo, 6));
    assert!(!project.param_supports_text_edit(lfo, 6));
    let options = project
        .node_param_dropdown_options(lfo, 6)
        .expect("dropdown options should exist");
    assert_eq!(options.len(), 5);
    assert_eq!(project.node_param_raw_text(lfo, 6), Some("sine"));
    assert!(project.set_param_dropdown_index(lfo, 6, 3));
    assert_eq!(project.node_param_raw_text(lfo, 6), Some("pulse"));
}

#[test]
fn lfo_type_changes_signal_shape() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
    assert!(project.set_param_value(lfo, 1, 1.0));
    assert!(project.set_param_value(lfo, 3, 0.0));

    let sine = project
        .sample_signal_node(lfo, 0.25, &mut Vec::new())
        .expect("lfo should evaluate");
    assert!(project.set_param_dropdown_index(lfo, 6, 1));
    let saw = project
        .sample_signal_node(lfo, 0.25, &mut Vec::new())
        .expect("lfo should evaluate");
    assert!(
        (sine - saw).abs() > 0.1,
        "changing lfo type should change sampled output"
    );
}

#[test]
fn lfo_shape_modifies_waveform() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
    assert!(project.set_param_value(lfo, 1, 1.0));
    assert!(project.set_param_value(lfo, 3, 0.0));
    assert!(project.set_param_dropdown_index(lfo, 6, 3));
    assert!(project.set_param_value(lfo, 7, -1.0));
    let narrow = project
        .sample_signal_node(lfo, 1.0, &mut Vec::new())
        .expect("lfo should evaluate");
    assert!(project.set_param_value(lfo, 7, 1.0));
    let wide = project
        .sample_signal_node(lfo, 1.0, &mut Vec::new())
        .expect("lfo should evaluate");
    assert!(
        (narrow - wide).abs() > 1.0,
        "shape should significantly alter pulse duty"
    );
}

#[test]
fn circle_nurbs_arc_style_uses_dropdown_options() {
    let mut project = GuiProject::new_empty(640, 480);
    let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 40, 40, 420, 480);
    assert!(project.param_is_dropdown(circle, 3));
    assert!(!project.param_supports_text_edit(circle, 3));
    let options = project
        .node_param_dropdown_options(circle, 3)
        .expect("dropdown options should exist");
    assert_eq!(options.len(), 2);
    assert_eq!(project.node_param_raw_text(circle, 3), Some("closed"));
    assert!(project.set_param_dropdown_index(circle, 3, 1));
    assert_eq!(project.node_param_raw_text(circle, 3), Some("open_arc"));
    assert!(project.adjust_param(circle, 3, -1.0));
    assert_eq!(project.node_param_raw_text(circle, 3), Some("closed"));
}

#[test]
fn render_scene_pass_bg_mode_uses_dropdown_options() {
    let mut project = GuiProject::new_empty(640, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 40, 40, 420, 480);
    assert!(project.param_is_dropdown(pass, 2));
    assert!(!project.param_supports_text_edit(pass, 2));
    let options = project
        .node_param_dropdown_options(pass, 2)
        .expect("dropdown options should exist");
    assert_eq!(options.len(), 2);
    assert_eq!(project.node_param_raw_text(pass, 2), Some("with_bg"));
    assert!(project.set_param_dropdown_index(pass, 2, 1));
    assert_eq!(project.node_param_raw_text(pass, 2), Some("alpha_clip"));
    assert!(project.adjust_param(pass, 2, -1.0));
    assert_eq!(project.node_param_raw_text(pass, 2), Some("with_bg"));
}

#[test]
fn tex_blend_mode_uses_dropdown_options() {
    let mut project = GuiProject::new_empty(640, 480);
    let blend = project.add_node(ProjectNodeKind::TexBlend, 40, 40, 420, 480);
    assert!(project.param_is_dropdown(blend, 1));
    assert!(!project.param_supports_text_edit(blend, 1));
    let options = project
        .node_param_dropdown_options(blend, 1)
        .expect("dropdown options should exist");
    assert_eq!(options.len(), 9);
    assert_eq!(project.node_param_raw_text(blend, 1), Some("normal"));
    assert!(project.set_param_dropdown_index(blend, 1, 7));
    assert_eq!(project.node_param_raw_text(blend, 1), Some("lighten"));
    assert!(project.adjust_param(blend, 1, -1.0));
    assert_eq!(project.node_param_raw_text(blend, 1), Some("darken"));
}

#[test]
fn post_color_tone_effect_uses_dropdown_options() {
    let mut project = GuiProject::new_empty(640, 480);
    let post = project.add_node(ProjectNodeKind::TexPostColorTone, 40, 40, 420, 480);
    assert!(project.param_is_dropdown(post, 0));
    assert!(!project.param_supports_text_edit(post, 0));
    let options = project
        .node_param_dropdown_options(post, 0)
        .expect("dropdown options should exist");
    assert_eq!(options.len(), 10);
    assert_eq!(project.node_param_raw_text(post, 0), Some("bloom"));
    assert!(project.set_param_dropdown_index(post, 0, 9));
    assert_eq!(project.node_param_raw_text(post, 0), Some("duotone"));
}

#[test]
fn all_default_parameter_labels_fit_length_budget() {
    let mut project = GuiProject::new_empty(640, 480);
    let kinds = [
        ProjectNodeKind::TexSolid,
        ProjectNodeKind::TexCircle,
        ProjectNodeKind::BufSphere,
        ProjectNodeKind::BufCircleNurbs,
        ProjectNodeKind::BufNoise,
        ProjectNodeKind::TexTransform2D,
        ProjectNodeKind::TexLevel,
        ProjectNodeKind::TexFeedback,
        ProjectNodeKind::TexReactionDiffusion,
        ProjectNodeKind::TexPostColorTone,
        ProjectNodeKind::TexPostEdgeStructure,
        ProjectNodeKind::TexPostBlurDiffusion,
        ProjectNodeKind::TexPostDistortion,
        ProjectNodeKind::TexPostTemporal,
        ProjectNodeKind::TexPostNoiseTexture,
        ProjectNodeKind::TexPostLighting,
        ProjectNodeKind::TexPostScreenSpace,
        ProjectNodeKind::TexPostExperimental,
        ProjectNodeKind::TexBlend,
        ProjectNodeKind::SceneEntity,
        ProjectNodeKind::SceneBuild,
        ProjectNodeKind::RenderCamera,
        ProjectNodeKind::RenderScenePass,
        ProjectNodeKind::CtlLfo,
        ProjectNodeKind::IoWindowOut,
    ];
    let mut x = 20;
    for kind in kinds {
        let node_id = project.add_node(kind, x, 40, 420, 480);
        x += 20;
        let node = project.node(node_id).expect("node should exist");
        for row in node.param_views() {
            assert!(
                row.label.len() <= PARAM_LABEL_MAX_LEN,
                "label '{}' exceeds {} chars",
                row.label,
                PARAM_LABEL_MAX_LEN
            );
        }
    }
}

#[test]
fn signal_preview_is_limited_to_signal_nodes() {
    let kinds_without_preview = [
        ProjectNodeKind::TexSolid,
        ProjectNodeKind::TexCircle,
        ProjectNodeKind::BufSphere,
        ProjectNodeKind::BufCircleNurbs,
        ProjectNodeKind::BufNoise,
        ProjectNodeKind::TexTransform2D,
        ProjectNodeKind::TexLevel,
        ProjectNodeKind::TexFeedback,
        ProjectNodeKind::TexReactionDiffusion,
        ProjectNodeKind::TexPostColorTone,
        ProjectNodeKind::TexPostEdgeStructure,
        ProjectNodeKind::TexPostBlurDiffusion,
        ProjectNodeKind::TexPostDistortion,
        ProjectNodeKind::TexPostTemporal,
        ProjectNodeKind::TexPostNoiseTexture,
        ProjectNodeKind::TexPostLighting,
        ProjectNodeKind::TexPostScreenSpace,
        ProjectNodeKind::TexPostExperimental,
        ProjectNodeKind::TexBlend,
        ProjectNodeKind::SceneEntity,
        ProjectNodeKind::SceneBuild,
        ProjectNodeKind::RenderCamera,
        ProjectNodeKind::RenderScenePass,
        ProjectNodeKind::IoWindowOut,
    ];
    for kind in kinds_without_preview {
        assert!(
            !kind.shows_signal_preview(),
            "{} should not render signal preview",
            kind.stable_id()
        );
    }
    assert!(ProjectNodeKind::CtlLfo.shows_signal_preview());
}

#[test]
fn project_detects_when_signal_preview_nodes_exist() {
    let mut project = GuiProject::new_empty(640, 480);
    assert!(!project.has_signal_preview_nodes());
    project.add_node(ProjectNodeKind::TexSolid, 40, 40, 420, 480);
    assert!(!project.has_signal_preview_nodes());
    project.add_node(ProjectNodeKind::CtlLfo, 80, 40, 420, 480);
    assert!(project.has_signal_preview_nodes());
}

#[test]
#[should_panic(expected = "parameter label")]
fn parameter_constructor_rejects_labels_longer_than_budget() {
    let _ = super::params::param("key", "label_too_long", 0.0, 0.0, 1.0, 0.1);
}

#[test]
fn cached_param_text_updates_when_value_changes() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let initial = project
        .node_param_raw_text(solid, 0)
        .expect("param text should exist")
        .to_string();
    assert_eq!(initial, "0.900");

    assert!(project.set_param_value(solid, 0, 0.25));
    let after_set = project
        .node_param_raw_text(solid, 0)
        .expect("param text should exist");
    assert_eq!(after_set, "0.250");

    assert!(project.adjust_selected_param(solid, 1.0));
    let after_adjust = project
        .node_param_raw_text(solid, 0)
        .expect("param text should exist");
    assert_eq!(after_adjust, "0.260");
}

#[test]
fn adjust_param_changes_specific_row_value() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    assert!(project.adjust_param(solid, 1, 3.0));
    let value = project
        .node_param_raw_value(solid, 1)
        .expect("param value should exist");
    assert!((value - 0.93).abs() < 1e-5);
}

#[test]
fn render_signature_ignores_expand_and_param_selection_state() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 180, 40, 420, 480);
    assert!(project.connect_image_link(solid, out));
    let base = project.render_signature();

    assert!(project.toggle_node_expanded(solid, 420, 480));
    assert_eq!(project.render_signature(), base);

    assert!(project.select_next_param(solid));
    assert_eq!(project.render_signature(), base);
}

#[test]
fn ui_signature_changes_for_expand_or_param_selection() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let base = project.ui_signature();

    assert!(project.toggle_node_expanded(solid, 420, 480));
    let after_expand = project.ui_signature();
    assert_ne!(after_expand, base);

    assert!(project.select_next_param(solid));
    assert_ne!(project.ui_signature(), after_expand);
}

#[test]
fn scoped_invalidation_epochs_track_nodes_wires_and_tex_eval() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 40, 420, 480);
    let base = project.invalidation();

    assert!(project.toggle_node_expanded(solid, 420, 480));
    let after_expand = project.invalidation();
    assert_ne!(after_expand.nodes, base.nodes);
    assert_ne!(after_expand.wires, base.wires);
    assert_eq!(after_expand.tex_eval, base.tex_eval);

    assert!(project.set_param_value(solid, 0, 0.25));
    let after_param = project.invalidation();
    assert_ne!(after_param.nodes, after_expand.nodes);
    assert_eq!(after_param.wires, after_expand.wires);
    assert_ne!(after_param.tex_eval, after_expand.tex_eval);

    assert!(project.connect_image_link(solid, out));
    let after_link = project.invalidation();
    assert_ne!(after_link.nodes, after_param.nodes);
    assert_ne!(after_link.wires, after_param.wires);
    assert_ne!(after_link.tex_eval, after_param.tex_eval);
}

#[test]
fn render_signature_changes_when_render_param_changes() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let base = project.render_signature();

    assert!(project.set_param_value(solid, 0, 0.2));
    assert_ne!(project.render_signature(), base);
}

#[test]
fn param_row_hit_returns_index_for_expanded_node() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let node = project.node(solid).expect("node should exist");
    let row = super::node_param_row_rect(node, 2).expect("row rect");
    let hit = project.param_row_at(solid, row.x + 2, row.y + 2);
    assert_eq!(hit, Some(2));
    let value_rect = node_param_value_rect(node, 2).expect("value rect");
    let value_hit = project.param_value_box_contains(solid, 2, value_rect.x + 2, value_rect.y + 2);
    assert!(value_hit);
}

#[test]
fn expand_toggle_rect_exists_for_param_nodes_only() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    let solid_node = project.node(solid).expect("solid node");
    let out_node = project.node(out).expect("out node");
    let solid_rect = node_expand_toggle_rect(solid_node).expect("solid toggle");
    assert_eq!(solid_rect.x, solid_node.x() + super::NODE_TOGGLE_MARGIN);
    assert_eq!(solid_rect.y, solid_node.y() + super::NODE_TOGGLE_MARGIN);
    assert!(node_expand_toggle_rect(out_node).is_none());
}

#[test]
fn pin_centers_follow_node_kind_capabilities() {
    let mut project = GuiProject::new_empty(640, 480);
    let tex_source = project.add_node(ProjectNodeKind::TexSolid, 60, 70, 420, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 60, 140, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 70, 420, 480);
    let tex_node = project.node(tex_source).expect("tex node must exist");
    let lfo_node = project.node(lfo).expect("lfo node must exist");
    let out_node = project.node(out).expect("output node must exist");
    assert!(output_pin_center(tex_node).is_some());
    assert!(input_pin_center(tex_node).is_none());
    assert!(output_pin_center(lfo_node).is_some());
    assert!(input_pin_center(lfo_node).is_none());
    assert!(output_pin_center(out_node).is_none());
    assert!(input_pin_center(out_node).is_some());
}

#[test]
fn graph_bounds_span_all_nodes() {
    let mut project = GuiProject::new_empty(640, 480);
    project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
    project.add_node(ProjectNodeKind::IoWindowOut, 200, 160, 420, 480);
    assert_eq!(
        project.graph_bounds(),
        Some(GraphBounds {
            min_x: 40,
            min_y: 80,
            max_x: 200 + super::NODE_WIDTH,
            max_y: 204,
        })
    );
}

#[test]
fn persisted_roundtrip_restores_nodes_links_and_bindings() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
    let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 200, 40, 420, 480);
    let noise = project.add_node(ProjectNodeKind::BufNoise, 360, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 520, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 680, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 840, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 1000, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 1160, 40, 420, 480);
    assert!(project.connect_image_link(circle, noise));
    assert!(project.connect_image_link(noise, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));
    assert!(project.connect_texture_link_to_param(pass, feedback, 0));
    assert!(project.connect_signal_link_to_param(lfo, noise, 0));
    assert!(project.toggle_node_expanded(noise, 420, 480));
    assert!(project.set_param_value(noise, 1, 4.5));

    let persisted = project.to_persisted();
    let restored = GuiProject::from_persisted(persisted, 420, 480).expect("restore should work");
    assert_eq!(restored.node_count(), project.node_count());
    assert_eq!(restored.edge_count(), project.edge_count());
    assert_eq!(restored.render_signature(), project.render_signature());
    assert!(restored.has_signal_bindings());
}

#[test]
fn from_persisted_maps_legacy_feedback_target_tex_key() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 200, 40, 420, 480);
    assert!(project.connect_texture_link_to_param(solid, feedback, 0));

    let mut persisted = project.to_persisted();
    for node in &mut persisted.nodes {
        if node.kind != ProjectNodeKind::TexFeedback.stable_id() {
            continue;
        }
        for param in &mut node.params {
            if param.key == super::FEEDBACK_HISTORY_PARAM_KEY {
                param.key = super::LEGACY_FEEDBACK_HISTORY_PARAM_KEY.to_string();
            }
        }
    }

    let restored = GuiProject::from_persisted(persisted, 420, 480).expect("restore should work");
    let restored_solid = restored
        .nodes()
        .iter()
        .find(|node| node.kind() == ProjectNodeKind::TexSolid)
        .expect("solid should exist")
        .id();
    let restored_feedback = restored
        .nodes()
        .iter()
        .find(|node| node.kind() == ProjectNodeKind::TexFeedback)
        .expect("feedback should exist")
        .id();
    assert_eq!(
        restored.texture_source_for_param(restored_feedback, 0),
        Some(restored_solid)
    );
    let expected_label = format!("tex.solid#{restored_solid}");
    assert_eq!(
        restored.node_param_raw_text(restored_feedback, 0),
        Some(expected_label.as_str())
    );
}

#[test]
fn from_persisted_rejects_unsupported_version() {
    let persisted = PersistedGuiProject {
        version: 999,
        name: "broken".to_string(),
        preview_width: 640,
        preview_height: 480,
        nodes: Vec::new(),
    };
    assert!(GuiProject::from_persisted(persisted, 420, 480).is_err());
}

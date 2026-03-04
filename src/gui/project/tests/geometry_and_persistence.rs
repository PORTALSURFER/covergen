use super::*;
#[test]
fn render_signature_changes_when_links_change() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 180, 40, 420, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, out));
    let base = project.render_signature();

    assert!(project.connect_image_link(lfo, solid));
    let after_bind = project.render_signature();
    assert_ne!(after_bind, base);

    assert!(project.disconnect_link(lfo, solid));
    assert_ne!(project.render_signature(), after_bind);
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
    let restored = GuiProject::from_persisted_with_warnings(persisted, 420, 480)
        .expect("restore should work")
        .project;
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

    let restored = GuiProject::from_persisted_with_warnings(persisted, 420, 480)
        .expect("restore should work")
        .project;
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
    assert!(GuiProject::from_persisted_with_warnings(persisted, 420, 480).is_err());
}

#[test]
fn from_persisted_reports_dropped_unknown_params() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid_id = project.add_node(ProjectNodeKind::TexSolid, 40, 40, 420, 480);
    let mut persisted = project.to_persisted();
    let persisted_node = persisted
        .nodes
        .iter_mut()
        .find(|node| node.id == solid_id)
        .expect("persisted tex.solid node should exist");
    persisted_node.params.push(PersistedGuiParam {
        key: "unknown_param".to_string(),
        value: 0.5,
        signal_source: None,
        texture_source: None,
    });

    let loaded = GuiProject::from_persisted_with_warnings(persisted, 420, 480)
        .expect("restore should tolerate unknown params");
    assert_eq!(loaded.project.node_count(), 1);
    assert_eq!(loaded.warnings.len(), 1);

    let warning = &loaded.warnings[0];
    assert_eq!(warning.persisted_node_id, solid_id);
    assert_eq!(warning.node_kind, ProjectNodeKind::TexSolid.stable_id());
    assert_eq!(warning.param_key, "unknown_param");
    assert_eq!(
        warning.to_string(),
        format!(
            "dropped unknown persisted param 'unknown_param' on node {}#{}",
            ProjectNodeKind::TexSolid.stable_id(),
            solid_id
        )
    );
}

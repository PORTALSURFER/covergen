use super::*;
#[test]
fn runtime_param_schema_matches_default_editor_param_order() {
    fn assert_kind_keys(
        project: &GuiProject,
        node_id: u32,
        expected: &[&'static str],
        kind: ProjectNodeKind,
    ) {
        let node = project.node(node_id).expect("node should exist");
        let keys: Vec<&str> = node.params.iter().map(|slot| slot.key).collect();
        assert_eq!(
            keys.as_slice(),
            expected,
            "default param order drifted for {}",
            kind.stable_id()
        );
    }

    let mut project = GuiProject::new_empty(640, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 40, 40, 420, 480);
    let source_noise = project.add_node(ProjectNodeKind::TexSourceNoise, 220, 40, 420, 480);
    let box_node = project.add_node(ProjectNodeKind::BufBox, 310, 40, 420, 480);
    let grid_node = project.add_node(ProjectNodeKind::BufGrid, 400, 40, 420, 480);
    let transform = project.add_node(ProjectNodeKind::TexTransform2D, 400, 40, 420, 480);
    let color_adjust = project.add_node(ProjectNodeKind::TexColorAdjust, 490, 40, 420, 480);
    let mask = project.add_node(ProjectNodeKind::TexMask, 580, 40, 420, 480);
    let morphology = project.add_node(ProjectNodeKind::TexMorphology, 760, 40, 420, 480);
    let tone_map = project.add_node(ProjectNodeKind::TexToneMap, 940, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 1120, 40, 420, 480);
    let domain_warp = project.add_node(ProjectNodeKind::TexDomainWarp, 1300, 40, 420, 480);
    let smear = project.add_node(ProjectNodeKind::TexDirectionalSmear, 1480, 40, 420, 480);
    let warp = project.add_node(ProjectNodeKind::TexWarpTransform, 1660, 40, 420, 480);
    let scene_pass = project.add_node(ProjectNodeKind::RenderScenePass, 1840, 40, 420, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 2020, 40, 420, 480);
    assert_kind_keys(
        &project,
        circle,
        &param_schema::circle::KEYS,
        ProjectNodeKind::TexCircle,
    );
    assert_kind_keys(
        &project,
        source_noise,
        &param_schema::source_noise::KEYS,
        ProjectNodeKind::TexSourceNoise,
    );
    assert_kind_keys(
        &project,
        box_node,
        &param_schema::box_buffer::KEYS,
        ProjectNodeKind::BufBox,
    );
    assert_kind_keys(
        &project,
        grid_node,
        &param_schema::grid_buffer::KEYS,
        ProjectNodeKind::BufGrid,
    );
    assert_kind_keys(
        &project,
        transform,
        &param_schema::transform_2d::KEYS,
        ProjectNodeKind::TexTransform2D,
    );
    assert_kind_keys(
        &project,
        color_adjust,
        &param_schema::color_adjust::KEYS,
        ProjectNodeKind::TexColorAdjust,
    );
    assert_kind_keys(
        &project,
        mask,
        &param_schema::mask::KEYS,
        ProjectNodeKind::TexMask,
    );
    assert_kind_keys(
        &project,
        morphology,
        &param_schema::morphology::KEYS,
        ProjectNodeKind::TexMorphology,
    );
    assert_kind_keys(
        &project,
        tone_map,
        &param_schema::tone_map::KEYS,
        ProjectNodeKind::TexToneMap,
    );
    assert_kind_keys(
        &project,
        feedback,
        &param_schema::feedback::KEYS,
        ProjectNodeKind::TexFeedback,
    );
    assert_kind_keys(
        &project,
        domain_warp,
        &param_schema::domain_warp::KEYS,
        ProjectNodeKind::TexDomainWarp,
    );
    assert_kind_keys(
        &project,
        smear,
        &param_schema::directional_smear::KEYS,
        ProjectNodeKind::TexDirectionalSmear,
    );
    assert_kind_keys(
        &project,
        warp,
        &param_schema::warp_transform::KEYS,
        ProjectNodeKind::TexWarpTransform,
    );
    assert_kind_keys(
        &project,
        scene_pass,
        &param_schema::render_scene_pass::KEYS,
        ProjectNodeKind::RenderScenePass,
    );
    assert_kind_keys(
        &project,
        lfo,
        &param_schema::ctl_lfo::KEYS,
        ProjectNodeKind::CtlLfo,
    );
}

#[test]
fn all_default_parameter_labels_fit_length_budget() {
    let mut project = GuiProject::new_empty(640, 480);
    let kinds = [
        ProjectNodeKind::TexSolid,
        ProjectNodeKind::TexCircle,
        ProjectNodeKind::TexSourceNoise,
        ProjectNodeKind::BufSphere,
        ProjectNodeKind::BufBox,
        ProjectNodeKind::BufGrid,
        ProjectNodeKind::BufCircleNurbs,
        ProjectNodeKind::BufNoise,
        ProjectNodeKind::TexTransform2D,
        ProjectNodeKind::TexColorAdjust,
        ProjectNodeKind::TexLevel,
        ProjectNodeKind::TexMask,
        ProjectNodeKind::TexMorphology,
        ProjectNodeKind::TexToneMap,
        ProjectNodeKind::TexFeedback,
        ProjectNodeKind::TexReactionDiffusion,
        ProjectNodeKind::TexDomainWarp,
        ProjectNodeKind::TexDirectionalSmear,
        ProjectNodeKind::TexWarpTransform,
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
        ProjectNodeKind::TexSourceNoise,
        ProjectNodeKind::BufSphere,
        ProjectNodeKind::BufBox,
        ProjectNodeKind::BufGrid,
        ProjectNodeKind::BufCircleNurbs,
        ProjectNodeKind::BufNoise,
        ProjectNodeKind::TexTransform2D,
        ProjectNodeKind::TexColorAdjust,
        ProjectNodeKind::TexLevel,
        ProjectNodeKind::TexMask,
        ProjectNodeKind::TexMorphology,
        ProjectNodeKind::TexToneMap,
        ProjectNodeKind::TexFeedback,
        ProjectNodeKind::TexReactionDiffusion,
        ProjectNodeKind::TexDomainWarp,
        ProjectNodeKind::TexDirectionalSmear,
        ProjectNodeKind::TexWarpTransform,
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
fn project_detects_temporal_node_kinds() {
    let mut project = GuiProject::new_empty(640, 480);
    assert!(!project.has_temporal_nodes());

    project.add_node(ProjectNodeKind::TexSolid, 40, 40, 420, 480);
    assert!(!project.has_temporal_nodes());

    project.add_node(ProjectNodeKind::BufNoise, 80, 40, 420, 480);
    assert!(project.has_temporal_nodes());
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

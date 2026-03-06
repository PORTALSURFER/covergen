use super::*;
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
fn signal_sampling_with_memo_matches_direct_sampling() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
    let direct = project
        .sample_signal_node(lfo, 0.125, &mut Vec::new())
        .expect("lfo should evaluate");

    let mut memo = SignalSampleMemo::default();
    let mut eval_stack = Vec::new();
    let memoized = project
        .sample_signal_node_with_memo(lfo, 0.125, &mut eval_stack, &mut memo)
        .expect("memoized lfo should evaluate");
    assert!((memoized - direct).abs() <= 1e-6);
    assert!(
        !memo.is_empty(),
        "memoized sampling should cache node/time evaluation results"
    );

    let memo_len = memo.len();
    eval_stack.clear();
    let repeated = project
        .sample_signal_node_with_memo(lfo, 0.125, &mut eval_stack, &mut memo)
        .expect("repeated memoized lfo should evaluate");
    assert!((repeated - direct).abs() <= 1e-6);
    assert_eq!(memo.len(), memo_len);
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
fn drift_lfo_is_slow_soft_and_non_repeating() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
    assert!(project.set_param_dropdown_index(lfo, 6, 4));
    assert!(project.set_param_value(lfo, 0, 0.2));
    assert!(project.set_param_value(lfo, 1, 1.0));
    assert!(project.set_param_value(lfo, 3, 0.0));
    assert!(project.set_param_value(lfo, 7, -0.5));

    let mut max_delta = 0.0f32;
    let mut prev = project
        .sample_signal_node(lfo, 0.0, &mut Vec::new())
        .expect("drift lfo should evaluate");
    for step in 1..=40 {
        let t = step as f32 * 0.05;
        let current = project
            .sample_signal_node(lfo, t, &mut Vec::new())
            .expect("drift lfo should evaluate");
        max_delta = max_delta.max((current - prev).abs());
        prev = current;
    }
    assert!(
        max_delta < 0.25,
        "drift should move smoothly without sharp jumps (max delta {max_delta})"
    );

    let a = project
        .sample_signal_node(lfo, 0.30, &mut Vec::new())
        .expect("drift lfo should evaluate");
    let b = project
        .sample_signal_node(lfo, 5.30, &mut Vec::new())
        .expect("drift lfo should evaluate");
    assert!(
        (a - b).abs() > 0.02,
        "drift should not lock to exact short periodic repetition"
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
    assert_eq!(options.len(), 11);
    assert_eq!(project.node_param_raw_text(post, 0), Some("bloom"));
    assert!(project.set_param_dropdown_index(post, 0, 10));
    assert_eq!(project.node_param_raw_text(post, 0), Some("mono"));
}

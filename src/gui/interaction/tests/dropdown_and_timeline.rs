use super::*;
#[test]
fn dropdown_click_selects_correct_option_at_low_zoom() {
    let mut project = GuiProject::new_empty(640, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(pass, 420, 480));
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.zoom = 0.5;

    let value_rect = {
        let node = project.node(pass).expect("scene-pass node should exist");
        node_param_value_rect(node, 2).expect("bg_mode value rect should exist")
    };
    let value_panel = super::graph_rect_to_panel(value_rect, &state);
    let open_dropdown = InputSnapshot {
        mouse_pos: Some((value_panel.x + 2, value_panel.y + 2)),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    let (_, consumed_open) =
        handle_param_edit_input(&open_dropdown, &mut project, 420, 480, &mut state);
    assert!(consumed_open);
    assert_eq!(state.param_dropdown.map(|d| d.node_id), Some(pass));

    let second_row_panel = {
        let node = project.node(pass).expect("scene-pass node should exist");
        let options = project
            .node_param_dropdown_options(pass, 2)
            .expect("bg_mode dropdown options");
        let list_world = node_param_dropdown_rect(node, 2, options.len()).expect("dropdown rect");
        let second_row_world = Rect::new(
            list_world.x,
            list_world.y + NODE_PARAM_DROPDOWN_ROW_HEIGHT,
            list_world.w,
            NODE_PARAM_DROPDOWN_ROW_HEIGHT,
        );
        super::graph_rect_to_panel(second_row_world, &state)
    };
    let select_second_option = InputSnapshot {
        mouse_pos: Some((second_row_panel.x + 2, second_row_panel.y + 1)),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    let (_, consumed_select) =
        handle_param_edit_input(&select_second_option, &mut project, 420, 480, &mut state);
    assert!(consumed_select);
    assert_eq!(project.node_param_raw_text(pass, 2), Some("alpha_clip"));
}

#[test]
fn apply_preview_actions_keeps_dropdown_open_after_value_click() {
    let config = V2Config::parse(Vec::new()).expect("config");
    let mut project = GuiProject::new_empty(640, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(pass, 420, 480));
    let mut state = PreviewState::new(&config);
    let value_rect = {
        let node = project.node(pass).expect("scene-pass node should exist");
        node_param_value_rect(node, 2).expect("bg_mode value rect should exist")
    };
    let value_panel = super::graph_rect_to_panel(value_rect, &state);
    let input = InputSnapshot {
        mouse_pos: Some((value_panel.x + 2, value_panel.y + 2)),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    assert!(apply_preview_actions(
        InteractionFrameContext::new(&config, 640, 420, 480),
        input,
        &mut project,
        &mut state,
    ));
    assert_eq!(
        state
            .param_dropdown
            .map(|dropdown| (dropdown.node_id, dropdown.param_index)),
        Some((pass, 2))
    );
}

#[test]
fn feedback_reset_button_click_queues_reset_action() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(feedback, 420, 480));
    assert!(project.connect_texture_link_to_param(solid, feedback, 0));
    let value_rect = {
        let node = project.node(feedback).expect("feedback node should exist");
        node_param_value_rect(node, 3).expect("reset value rect should exist")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let click = InputSnapshot {
        mouse_pos: Some((value_rect.x + 2, value_rect.y + 2)),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = handle_param_edit_input(&click, &mut project, 420, 480, &mut state);
    assert!(!changed);
    assert!(consumed);
    assert!(matches!(
        state.pending_app_action,
        Some(PendingAppAction::ResetFeedback {
            feedback_node_id,
            accumulation_texture_node_id,
        }) if feedback_node_id == feedback && accumulation_texture_node_id == Some(solid)
    ));
    assert!(state.param_edit.is_none());
    assert!(state.param_dropdown.is_none());
}

#[test]
fn timeline_volume_slider_updates_audio_volume() {
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let timeline = timeline_rect(640, 480);
    let controls = timeline_control_layout(timeline);
    let input = InputSnapshot {
        mouse_pos: Some((
            controls.volume_slider.x + controls.volume_slider.w - 1,
            controls.volume_slider.y + controls.volume_slider.h / 2,
        )),
        left_clicked: true,
        left_down: true,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = super::handle_timeline_input(&input, 640, 480, 60, &mut state);
    assert!(changed);
    assert!(consumed);
    assert!((state.export_menu.parsed_audio_volume() - 2.0).abs() < 0.01);
}

#[test]
fn timeline_bpm_buttons_update_bpm() {
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let initial = state.export_menu.parsed_bpm();
    let timeline = timeline_rect(640, 480);
    let controls = timeline_control_layout(timeline);
    let input = InputSnapshot {
        mouse_pos: Some((
            controls.bpm_up.x + controls.bpm_up.w / 2,
            controls.bpm_up.y + controls.bpm_up.h / 2,
        )),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = super::handle_timeline_input(&input, 640, 480, 60, &mut state);
    assert!(changed);
    assert!(consumed);
    assert!((state.export_menu.parsed_bpm() - (initial + 1.0)).abs() < 0.01);
}

#[test]
fn timeline_bpm_value_click_starts_text_edit() {
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let timeline = timeline_rect(640, 480);
    let controls = timeline_control_layout(timeline);
    let input = InputSnapshot {
        mouse_pos: Some((
            controls.bpm_value.x + controls.bpm_value.w / 2,
            controls.bpm_value.y + controls.bpm_value.h / 2,
        )),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = super::handle_timeline_input(&input, 640, 480, 60, &mut state);
    assert!(changed);
    assert!(consumed);
    let edit = state
        .timeline_bpm_edit
        .as_ref()
        .expect("bpm edit should be active");
    assert_eq!(edit.buffer, state.export_menu.bpm);
    assert_eq!(edit.cursor, state.export_menu.bpm.len());
    assert_eq!(edit.anchor, 0);
}

#[test]
fn timeline_bpm_text_commit_updates_bpm() {
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let timeline = timeline_rect(640, 480);
    let controls = timeline_control_layout(timeline);
    let click = InputSnapshot {
        mouse_pos: Some((controls.bpm_value.x + 2, controls.bpm_value.y + 2)),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    let _ = super::handle_timeline_input(&click, 640, 480, 60, &mut state);

    let commit = InputSnapshot {
        typed_text: "96".to_string(),
        param_commit: true,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = super::handle_timeline_input(&commit, 640, 480, 60, &mut state);
    assert!(changed);
    assert!(!consumed);
    assert!(state.timeline_bpm_edit.is_none());
    assert!((state.export_menu.parsed_bpm() - 96.0).abs() < 0.01);
}

#[test]
fn timeline_bpm_invalid_commit_keeps_edit_open() {
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let timeline = timeline_rect(640, 480);
    let controls = timeline_control_layout(timeline);
    let click = InputSnapshot {
        mouse_pos: Some((controls.bpm_value.x + 2, controls.bpm_value.y + 2)),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    let _ = super::handle_timeline_input(&click, 640, 480, 60, &mut state);

    let commit = InputSnapshot {
        typed_text: ".".to_string(),
        param_commit: true,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = super::handle_timeline_input(&commit, 640, 480, 60, &mut state);
    assert!(changed);
    assert!(!consumed);
    let edit = state
        .timeline_bpm_edit
        .as_ref()
        .expect("invalid bpm should keep edit active");
    assert_eq!(edit.buffer, ".");
    assert!((state.export_menu.parsed_bpm() - 120.0).abs() < 0.01);
}

#[test]
fn timeline_bar_value_click_starts_text_edit() {
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let timeline = timeline_rect(640, 480);
    let controls = timeline_control_layout(timeline);
    let input = InputSnapshot {
        mouse_pos: Some((
            controls.bar_value.x + controls.bar_value.w / 2,
            controls.bar_value.y + controls.bar_value.h / 2,
        )),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = super::handle_timeline_input(&input, 640, 480, 60, &mut state);
    assert!(changed);
    assert!(consumed);
    let edit = state
        .timeline_bar_edit
        .as_ref()
        .expect("bar edit should be active");
    assert_eq!(edit.buffer, state.export_menu.bar_length);
    assert_eq!(edit.cursor, state.export_menu.bar_length.len());
    assert_eq!(edit.anchor, 0);
}

#[test]
fn timeline_bar_text_commit_updates_bar_length() {
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let timeline = timeline_rect(640, 480);
    let controls = timeline_control_layout(timeline);
    let click = InputSnapshot {
        mouse_pos: Some((controls.bar_value.x + 2, controls.bar_value.y + 2)),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    let _ = super::handle_timeline_input(&click, 640, 480, 60, &mut state);

    let commit = InputSnapshot {
        typed_text: "12".to_string(),
        param_commit: true,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = super::handle_timeline_input(&commit, 640, 480, 60, &mut state);
    assert!(changed);
    assert!(!consumed);
    assert!(state.timeline_bar_edit.is_none());
    assert!((state.export_menu.parsed_bar_length() - 12.0).abs() < 0.01);
}

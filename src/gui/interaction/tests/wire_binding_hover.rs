use super::*;
#[test]
fn wheel_over_param_value_box_does_not_adjust_parameter() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let value_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_value_rect(node, 0).expect("value rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let input = InputSnapshot {
        mouse_pos: Some((value_rect.x + 2, value_rect.y + 2)),
        wheel_lines_y: 2.0,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = handle_param_wheel_input(&input, &mut project, 420, 480, &mut state);
    assert!(!changed);
    assert!(!consumed);
    let value = project
        .node_param_raw_value(solid, 0)
        .expect("param value should exist");
    assert!((value - 0.9).abs() < 1e-5);
}

#[test]
fn alt_drag_over_param_value_scrubs_parameter_value() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let value_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_value_rect(node, 0).expect("value rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));

    let start = InputSnapshot {
        alt_down: true,
        left_clicked: true,
        left_down: true,
        mouse_pos: Some((value_rect.x + 4, value_rect.y + value_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (changed_start, consumed_start) =
        handle_alt_param_drag(&start, &mut project, 420, 480, &mut state);
    assert!(consumed_start);
    assert!(changed_start || state.active_node == Some(solid));
    assert!(state.param_scrub.is_some());

    let drag = InputSnapshot {
        alt_down: true,
        left_down: true,
        mouse_pos: Some((value_rect.x + 4, value_rect.y + value_rect.h / 2 - 40)),
        ..InputSnapshot::default()
    };
    let (changed_drag, consumed_drag) =
        handle_alt_param_drag(&drag, &mut project, 420, 480, &mut state);
    assert!(consumed_drag);
    assert!(changed_drag);
    let value = project
        .node_param_raw_value(solid, 0)
        .expect("param value should exist");
    assert!(
        value > 0.9,
        "expected scrubbing to increase value, got {value}"
    );

    let release = InputSnapshot {
        alt_down: false,
        left_down: false,
        mouse_pos: Some((value_rect.x + 4, value_rect.y + value_rect.h / 2 - 40)),
        ..InputSnapshot::default()
    };
    let (_changed_release, consumed_release) =
        handle_alt_param_drag(&release, &mut project, 420, 480, &mut state);
    assert!(consumed_release);
    assert!(state.param_scrub.is_none());
}

#[test]
fn scrub_continues_while_left_down_after_alt_release() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let value_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_value_rect(node, 0).expect("value rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let start = InputSnapshot {
        alt_down: true,
        left_clicked: true,
        left_down: true,
        mouse_pos: Some((value_rect.x + 4, value_rect.y + value_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (_, consumed_start) = handle_alt_param_drag(&start, &mut project, 420, 480, &mut state);
    assert!(consumed_start);
    assert!(state.param_scrub.is_some());

    let drag = InputSnapshot {
        alt_down: false,
        left_down: true,
        mouse_pos: Some((value_rect.x + 4, value_rect.y + value_rect.h / 2 - 40)),
        ..InputSnapshot::default()
    };
    let (changed_drag, consumed_drag) =
        handle_alt_param_drag(&drag, &mut project, 420, 480, &mut state);
    assert!(consumed_drag);
    assert!(changed_drag);
}

#[test]
fn scrub_stops_when_mouse_released_even_if_alt_still_down() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let value_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_value_rect(node, 0).expect("value rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let start = InputSnapshot {
        alt_down: true,
        left_clicked: true,
        left_down: true,
        mouse_pos: Some((value_rect.x + 4, value_rect.y + value_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (_, consumed_start) = handle_alt_param_drag(&start, &mut project, 420, 480, &mut state);
    assert!(consumed_start);
    assert!(state.param_scrub.is_some());

    let release = InputSnapshot {
        alt_down: true,
        left_down: false,
        mouse_pos: Some((value_rect.x + 4, value_rect.y + value_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (changed_release, consumed_release) =
        handle_alt_param_drag(&release, &mut project, 420, 480, &mut state);
    assert!(consumed_release);
    assert!(changed_release);
    assert!(state.param_scrub.is_none());
}

#[test]
fn alt_drag_over_param_value_starts_when_param_edit_is_active() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let value_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_value_rect(node, 0).expect("value rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.param_edit = Some(ParamEditState {
        node_id: solid,
        param_index: 0,
        buffer: "0.90".to_string(),
        cursor: 4,
        anchor: 4,
    });

    let start = InputSnapshot {
        alt_down: true,
        left_clicked: true,
        left_down: true,
        mouse_pos: Some((value_rect.x + 4, value_rect.y + value_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (changed_start, consumed_start) =
        handle_alt_param_drag(&start, &mut project, 420, 480, &mut state);
    assert!(consumed_start);
    assert!(changed_start || state.active_node == Some(solid));
    assert!(state.param_scrub.is_some());
    assert!(state.param_edit.is_none());

    let drag = InputSnapshot {
        alt_down: true,
        left_down: true,
        mouse_pos: Some((value_rect.x + 4, value_rect.y + value_rect.h / 2 - 40)),
        ..InputSnapshot::default()
    };
    let (changed_drag, consumed_drag) =
        handle_alt_param_drag(&drag, &mut project, 420, 480, &mut state);
    assert!(consumed_drag);
    assert!(changed_drag);
    let value = project
        .node_param_raw_value(solid, 0)
        .expect("param value should exist");
    assert!(
        value > 0.9,
        "expected scrubbing to increase value, got {value}"
    );
}

#[test]
fn alt_drag_over_param_row_label_scrubs_parameter_value() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let (row_rect, value_rect) = {
        let node = project.node(solid).expect("solid node exists");
        (
            node_param_row_rect(node, 0).expect("row rect exists"),
            node_param_value_rect(node, 0).expect("value rect exists"),
        )
    };
    let row_x = row_rect.x + 8;
    assert!(
        row_x < value_rect.x,
        "test expects label-side x to be left of value box"
    );
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));

    let start = InputSnapshot {
        alt_down: true,
        left_clicked: true,
        left_down: true,
        mouse_pos: Some((row_x, row_rect.y + row_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (changed_start, consumed_start) =
        handle_alt_param_drag(&start, &mut project, 420, 480, &mut state);
    assert!(consumed_start);
    assert!(changed_start || state.active_node == Some(solid));
    assert!(state.param_scrub.is_some());

    let drag = InputSnapshot {
        alt_down: true,
        left_down: true,
        mouse_pos: Some((row_x, row_rect.y + row_rect.h / 2 - 40)),
        ..InputSnapshot::default()
    };
    let (changed_drag, consumed_drag) =
        handle_alt_param_drag(&drag, &mut project, 420, 480, &mut state);
    assert!(consumed_drag);
    assert!(changed_drag);
    let value = project
        .node_param_raw_value(solid, 0)
        .expect("param value should exist");
    assert!(
        value > 0.9,
        "expected scrubbing to increase value, got {value}"
    );
}

#[test]
fn alt_drag_starts_without_fresh_click_when_left_is_already_down() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let row_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_row_rect(node, 0).expect("row rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.prev_left_down = true;

    let start = InputSnapshot {
        alt_down: true,
        left_clicked: false,
        left_down: true,
        mouse_pos: Some((row_rect.x + 8, row_rect.y + row_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (changed_start, consumed_start) =
        handle_alt_param_drag(&start, &mut project, 420, 480, &mut state);
    assert!(consumed_start);
    assert!(changed_start || state.active_node == Some(solid));
    assert!(state.param_scrub.is_some());
}

#[test]
fn alt_drag_starts_from_active_param_edit_when_cursor_is_off_row() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let row_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_row_rect(node, 0).expect("row rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.param_edit = Some(ParamEditState {
        node_id: solid,
        param_index: 0,
        buffer: "0.90".to_string(),
        cursor: 4,
        anchor: 4,
    });

    let start = InputSnapshot {
        alt_down: true,
        left_down: true,
        left_clicked: false,
        mouse_pos: Some((row_rect.x + row_rect.w + 40, row_rect.y + row_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (changed_start, consumed_start) =
        handle_alt_param_drag(&start, &mut project, 420, 480, &mut state);
    assert!(consumed_start);
    assert!(changed_start || state.active_node == Some(solid));
    assert!(state.param_scrub.is_some());
    assert!(state.param_edit.is_none());
}

#[test]
fn alt_drag_starts_from_hover_target_when_cursor_is_slightly_off_row() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let row_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_row_rect(node, 0).expect("row rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.hover_alt_param = Some(HoverParamTarget {
        node_id: solid,
        param_index: 0,
    });

    let start = InputSnapshot {
        alt_down: true,
        left_down: true,
        left_clicked: true,
        mouse_pos: Some((row_rect.x + row_rect.w + 24, row_rect.y + row_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (changed_start, consumed_start) =
        handle_alt_param_drag(&start, &mut project, 420, 480, &mut state);
    assert!(consumed_start);
    assert!(changed_start || state.active_node == Some(solid));
    assert!(state.param_scrub.is_some());
}

#[test]
fn alt_hover_latched_click_starts_scrub_when_alt_flag_drops_on_click() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let row_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_row_rect(node, 0).expect("row rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.hover_alt_param = Some(HoverParamTarget {
        node_id: solid,
        param_index: 0,
    });

    let start = InputSnapshot {
        alt_down: false,
        left_down: true,
        left_clicked: true,
        mouse_pos: Some((row_rect.x + row_rect.w + 24, row_rect.y + row_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (changed_start, consumed_start) =
        handle_alt_param_drag(&start, &mut project, 420, 480, &mut state);
    assert!(consumed_start);
    assert!(changed_start || state.active_node == Some(solid));
    assert!(state.param_scrub.is_some());
}

#[test]
fn alt_drag_starts_with_right_button_fallback() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let row_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_row_rect(node, 0).expect("row rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.hover_alt_param = Some(HoverParamTarget {
        node_id: solid,
        param_index: 0,
    });

    let start = InputSnapshot {
        alt_down: true,
        right_down: true,
        right_clicked: true,
        mouse_pos: Some((row_rect.x + row_rect.w + 24, row_rect.y + row_rect.h / 2)),
        ..InputSnapshot::default()
    };
    let (changed_start, consumed_start) =
        handle_alt_param_drag(&start, &mut project, 420, 480, &mut state);
    assert!(consumed_start);
    assert!(changed_start || state.active_node == Some(solid));
    assert!(state.param_scrub.is_some());
}

#[test]
fn alt_hover_marks_scrubbable_param_target() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let value_rect = {
        let node = project.node(solid).expect("solid node exists");
        node_param_value_rect(node, 0).expect("value rect exists")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let input = InputSnapshot {
        alt_down: true,
        mouse_pos: Some((value_rect.x + 2, value_rect.y + 2)),
        ..InputSnapshot::default()
    };
    assert!(update_hover_state(
        &input,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert_eq!(
        state.hover_alt_param,
        Some(HoverParamTarget {
            node_id: solid,
            param_index: 0
        })
    );
}

#[test]
fn signal_wire_hover_does_not_auto_expand_node() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(!project.node_expanded(solid));
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: lfo,
        cursor_x: 0,
        cursor_y: 0,
    });

    let expand_hover = InputSnapshot {
        mouse_pos: Some((225, 85)),
        ..InputSnapshot::default()
    };
    assert!(update_hover_state(
        &expand_hover,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert!(!project.node_expanded(solid));
    assert!(state.hover_param_target.is_none());
}

#[test]
fn tab_opened_bind_hover_node_collapses_on_exit() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(!project.node_expanded(solid));
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: lfo,
        cursor_x: 0,
        cursor_y: 0,
    });

    let hover_node = InputSnapshot {
        mouse_pos: Some((225, 85)),
        ..InputSnapshot::default()
    };
    assert!(update_hover_state(
        &hover_node,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert!(!project.node_expanded(solid));
    let toggle = InputSnapshot {
        toggle_node_open: true,
        ..InputSnapshot::default()
    };
    assert!(handle_node_open_toggle(
        &toggle,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert!(project.node_expanded(solid));

    let hover_away = InputSnapshot {
        mouse_pos: Some((16, 16)),
        ..InputSnapshot::default()
    };
    assert!(update_hover_state(
        &hover_away,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert!(!project.node_expanded(solid));
}

#[test]
fn signal_wire_hover_does_not_collapse_user_expanded_node() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));

    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: lfo,
        cursor_x: 0,
        cursor_y: 0,
    });

    let hover_node = InputSnapshot {
        mouse_pos: Some((225, 85)),
        ..InputSnapshot::default()
    };
    let _ = update_hover_state(&hover_node, &mut project, 420, 480, &mut state);
    let hover_away = InputSnapshot {
        mouse_pos: Some((16, 16)),
        ..InputSnapshot::default()
    };
    let _ = update_hover_state(&hover_away, &mut project, 420, 480, &mut state);
    assert!(project.node_expanded(solid));
}

#[test]
fn texture_wire_hover_over_feedback_does_not_auto_expand_node() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
    assert!(!project.node_expanded(feedback));

    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: solid,
        cursor_x: 0,
        cursor_y: 0,
    });
    let hover_node = InputSnapshot {
        mouse_pos: Some((225, 85)),
        ..InputSnapshot::default()
    };
    assert!(update_hover_state(
        &hover_node,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert!(!project.node_expanded(feedback));
    assert!(state.hover_param_target.is_none());
}

#[test]
fn texture_wire_hover_still_targets_input_pin_for_regular_nodes() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 220, 80, 420, 480);
    let (in_x, in_y) = {
        let node = project.node(xform).expect("transform node should exist");
        input_pin_center(node).expect("input pin should exist")
    };

    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: solid,
        cursor_x: 0,
        cursor_y: 0,
    });
    let hover_input = InputSnapshot {
        mouse_pos: Some((in_x, in_y)),
        ..InputSnapshot::default()
    };
    assert!(update_hover_state(
        &hover_input,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert_eq!(state.hover_input_pin, Some(xform));
}

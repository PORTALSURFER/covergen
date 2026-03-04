use super::*;
#[test]
fn dropping_signal_wire_binds_hovered_parameter() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 220, 80, 420, 480);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: lfo,
        cursor_x: 0,
        cursor_y: 0,
    });
    state.hover_param_target = Some(HoverParamTarget {
        node_id: circle,
        param_index: 2,
    });
    let input = InputSnapshot {
        left_down: false,
        ..InputSnapshot::default()
    };
    assert!(handle_wire_input(
        &input,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert!(state.wire_drag.is_none());
    assert_eq!(project.signal_source_for_param(circle, 2), Some(lfo));
}

#[test]
fn dropping_texture_wire_binds_feedback_target_parameter() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: solid,
        cursor_x: 0,
        cursor_y: 0,
    });
    state.hover_param_target = Some(HoverParamTarget {
        node_id: feedback,
        param_index: 0,
    });
    let input = InputSnapshot {
        left_down: false,
        ..InputSnapshot::default()
    };
    assert!(handle_wire_input(
        &input,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert!(state.wire_drag.is_none());
    assert_eq!(project.texture_source_for_param(feedback, 0), Some(solid));
}

#[test]
fn dropping_texture_wire_on_feedback_target_value_box_binds_parameter() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(feedback, 420, 480));
    let value_rect = {
        let node = project.node(feedback).expect("feedback node should exist");
        node_param_value_rect(node, 0).expect("feedback target value rect should exist")
    };
    let cursor = (value_rect.x + 2, value_rect.y + 2);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: solid,
        cursor_x: cursor.0,
        cursor_y: cursor.1,
    });

    let hover = InputSnapshot {
        mouse_pos: Some(cursor),
        left_down: true,
        ..InputSnapshot::default()
    };
    assert!(update_hover_state(
        &hover,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert_eq!(
        state.hover_param_target,
        Some(HoverParamTarget {
            node_id: feedback,
            param_index: 0,
        })
    );

    let release = InputSnapshot {
        mouse_pos: Some(cursor),
        left_down: false,
        ..InputSnapshot::default()
    };
    assert!(handle_wire_input(
        &release,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert_eq!(project.texture_source_for_param(feedback, 0), Some(solid));
}

#[test]
fn texture_drop_release_hit_test_binds_feedback_target_without_hover_target() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(feedback, 420, 480));
    let value_rect = {
        let node = project.node(feedback).expect("feedback node should exist");
        node_param_value_rect(node, 0).expect("feedback target value rect should exist")
    };
    let cursor = (value_rect.x + 2, value_rect.y + 2);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: solid,
        cursor_x: cursor.0,
        cursor_y: cursor.1,
    });

    let release = InputSnapshot {
        mouse_pos: Some(cursor),
        left_down: false,
        ..InputSnapshot::default()
    };
    assert!(handle_wire_input(
        &release,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert_eq!(project.texture_source_for_param(feedback, 0), Some(solid));
}

#[test]
fn texture_drop_on_collapsed_feedback_card_does_not_create_implicit_binding() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
    assert!(!project.node_expanded(feedback));

    let center = (220 + NODE_WIDTH / 2, 80 + 22);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: solid,
        cursor_x: center.0,
        cursor_y: center.1,
    });
    let release = InputSnapshot {
        mouse_pos: Some(center),
        left_down: false,
        ..InputSnapshot::default()
    };
    assert!(handle_wire_input(
        &release,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert_eq!(project.texture_source_for_param(feedback, 0), None);
    assert_eq!(project.input_source_node_id(feedback), None);
}

#[test]
fn texture_drop_on_feedback_input_pin_keeps_primary_input_wiring() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 80, 420, 480);
    let input_pin = {
        let node = project.node(feedback).expect("feedback node should exist");
        input_pin_center(node).expect("feedback input pin should exist")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: solid,
        cursor_x: input_pin.0,
        cursor_y: input_pin.1,
    });
    state.hover_input_pin = Some(feedback);
    let release = InputSnapshot {
        mouse_pos: Some(input_pin),
        left_down: false,
        ..InputSnapshot::default()
    };
    assert!(handle_wire_input(
        &release,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert_eq!(project.input_source_node_id(feedback), Some(solid));
    assert_eq!(project.texture_source_for_param(feedback, 0), None);
}

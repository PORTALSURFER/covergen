use super::{
    apply_preview_actions, backspace_param_text, can_append_param_char, handle_add_menu_input,
    handle_delete_selected_nodes, handle_drag_input, handle_help_input, handle_link_cut,
    handle_main_export_menu_input, handle_node_open_toggle, handle_param_edit_input,
    handle_param_wheel_input, handle_right_selection, handle_wire_input, insert_param_char,
    marquee_moved, move_param_cursor_left, move_param_cursor_right, rects_overlap,
    segments_intersect, update_hover_state, AddNodeMenuEntry, RightMarqueeState,
};
use crate::gui::geometry::Rect;
use crate::gui::project::{
    input_pin_center, node_param_dropdown_rect, node_param_row_rect, node_param_value_rect,
    output_pin_center, GuiProject, ProjectNodeKind, NODE_PARAM_DROPDOWN_ROW_HEIGHT, NODE_WIDTH,
};
use crate::gui::state::{
    AddNodeMenuState, DragState, ExportMenuState, HoverInsertLink, HoverParamTarget, InputSnapshot,
    LinkCutState, ParamEditState, PreviewState, WireDragState,
};
use crate::gui::timeline::{editor_panel_height, timeline_control_layout, timeline_rect};
use crate::runtime_config::V2Config;

#[test]
fn segments_intersect_detects_crossing_lines() {
    assert!(segments_intersect(0, 0, 10, 10, 0, 10, 10, 0));
}

#[test]
fn segments_intersect_detects_non_crossing_lines() {
    assert!(!segments_intersect(0, 0, 10, 0, 0, 5, 10, 5));
}

#[test]
fn can_append_param_char_limits_numeric_input_shape() {
    assert!(can_append_param_char("", '1'));
    assert!(can_append_param_char("", '-'));
    assert!(!can_append_param_char("1", '-'));
    assert!(can_append_param_char("1", '.'));
    assert!(!can_append_param_char("1.2", '.'));
    assert!(!can_append_param_char("", 'a'));
}

#[test]
fn marquee_moved_requires_drag_threshold() {
    assert!(!marquee_moved(RightMarqueeState {
        start_x: 10,
        start_y: 10,
        cursor_x: 13,
        cursor_y: 12,
    }));
    assert!(marquee_moved(RightMarqueeState {
        start_x: 10,
        start_y: 10,
        cursor_x: 18,
        cursor_y: 10,
    }));
}

#[test]
fn rects_overlap_detects_intersection() {
    assert!(rects_overlap(0, 0, 10, 10, 8, 8, 16, 16));
    assert!(!rects_overlap(0, 0, 10, 10, 11, 11, 20, 20));
}

#[test]
fn insert_param_char_replaces_selection() {
    let mut edit = ParamEditState {
        node_id: 7,
        param_index: 0,
        buffer: "1.000".to_string(),
        cursor: 5,
        anchor: 0,
    };
    assert!(insert_param_char(&mut edit, '2'));
    assert_eq!(edit.buffer, "2");
    assert_eq!(edit.cursor, 1);
    assert_eq!(edit.anchor, 1);
}

#[test]
fn backspace_param_text_deletes_selected_range() {
    let mut edit = ParamEditState {
        node_id: 7,
        param_index: 0,
        buffer: "12.34".to_string(),
        cursor: 4,
        anchor: 1,
    };
    assert!(backspace_param_text(&mut edit));
    assert_eq!(edit.buffer, "14");
    assert_eq!(edit.cursor, 1);
    assert_eq!(edit.anchor, 1);
}

#[test]
fn cursor_moves_collapse_selection_when_not_extending() {
    let mut edit = ParamEditState {
        node_id: 7,
        param_index: 0,
        buffer: "12.34".to_string(),
        cursor: 4,
        anchor: 1,
    };
    assert!(move_param_cursor_left(&mut edit, false));
    assert_eq!(edit.cursor, 1);
    assert_eq!(edit.anchor, 1);
    assert!(move_param_cursor_right(&mut edit, false));
    assert_eq!(edit.cursor, 2);
    assert_eq!(edit.anchor, 2);
}

#[test]
fn delete_hotkey_removes_selected_nodes() {
    let mut project = GuiProject::new_empty(640, 480);
    let tex_source = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    assert!(project.connect_image_link(tex_source, out));
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.selected_nodes.push(tex_source);
    state.active_node = Some(tex_source);
    let input = InputSnapshot {
        param_delete: true,
        ..InputSnapshot::default()
    };
    assert!(handle_delete_selected_nodes(
        &input,
        &mut project,
        &mut state
    ));
    assert!(project.node(tex_source).is_none());
    assert_eq!(project.edge_count(), 0);
    assert!(state.selected_nodes.is_empty());
    assert!(state.active_node.is_none());
}

#[test]
fn f1_over_node_opens_help_modal() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let input = InputSnapshot {
        mouse_pos: Some((90, 90)),
        open_help: true,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = handle_help_input(&input, &project, 420, 480, &mut state);
    assert!(changed);
    assert!(consumed);
    let modal = state.help_modal.as_ref().expect("help modal should open");
    assert!(modal.title.starts_with("Node Help:"));
    assert!(modal
        .lines
        .iter()
        .any(|line| line.contains(&format!("#{solid}"))));
}

#[test]
fn help_modal_closes_on_click() {
    let mut project = GuiProject::new_empty(640, 480);
    let _solid = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.help_modal = Some(crate::gui::help::build_global_help_modal());
    let close = InputSnapshot {
        left_clicked: true,
        ..InputSnapshot::default()
    };
    let (changed, consumed) = handle_help_input(&close, &project, 420, 480, &mut state);
    assert!(changed);
    assert!(consumed);
    assert!(state.help_modal.is_none());
}

#[test]
fn export_panel_stays_open_on_outside_click() {
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.export_menu = ExportMenuState::open_at(80, 80, 420, 480);
    let outside_click = InputSnapshot {
        left_clicked: true,
        mouse_pos: Some((12, 12)),
        ..InputSnapshot::default()
    };
    assert!(!handle_main_export_menu_input(
        &outside_click,
        420,
        480,
        &mut state
    ));
    assert!(state.export_menu.open);
}

#[test]
fn export_panel_close_button_closes_panel() {
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.export_menu = ExportMenuState::open_at(80, 80, 420, 480);
    let close = state.export_menu.close_button_rect();
    let click_close = InputSnapshot {
        left_clicked: true,
        mouse_pos: Some((close.x + close.w / 2, close.y + close.h / 2)),
        ..InputSnapshot::default()
    };
    assert!(handle_main_export_menu_input(
        &click_close,
        420,
        480,
        &mut state
    ));
    assert!(!state.export_menu.open);
}

#[test]
fn export_panel_title_drag_moves_popup() {
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.export_menu = ExportMenuState::open_at(80, 80, 420, 480);
    let initial_x = state.export_menu.x;
    let initial_y = state.export_menu.y;
    let start_drag = InputSnapshot {
        left_clicked: true,
        left_down: true,
        mouse_pos: Some((100, 92)),
        ..InputSnapshot::default()
    };
    assert!(handle_main_export_menu_input(
        &start_drag,
        420,
        480,
        &mut state
    ));
    assert_eq!(
        state.export_menu_drag.map(|drag| drag.offset_x),
        Some(100 - initial_x)
    );
    assert_eq!(
        state.export_menu_drag.map(|drag| drag.offset_y),
        Some(92 - initial_y)
    );

    let drag_move = InputSnapshot {
        left_down: true,
        mouse_pos: Some((180, 160)),
        ..InputSnapshot::default()
    };
    assert!(handle_main_export_menu_input(
        &drag_move, 420, 480, &mut state
    ));
    let expected_max_y = (editor_panel_height(480) as i32 - state.export_menu.rect().h - 8).max(8);
    assert_eq!(state.export_menu.x, 8);
    assert_eq!(state.export_menu.y, expected_max_y);

    let release = InputSnapshot {
        left_down: false,
        mouse_pos: Some((180, 160)),
        ..InputSnapshot::default()
    };
    assert!(!handle_main_export_menu_input(
        &release, 420, 480, &mut state
    ));
    assert!(state.export_menu_drag.is_none());
    assert!(state.export_menu.open);
}

#[test]
fn wheel_over_param_value_box_adjusts_value_and_consumes_zoom() {
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
    assert!(changed);
    assert!(consumed);
    let value = project
        .node_param_raw_value(solid, 0)
        .expect("param value should exist");
    assert!((value - 0.92).abs() < 1e-5);
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

#[test]
fn dragging_node_over_wire_highlights_insert_candidate() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 120, 160, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(solid, out));
    let (from_x, from_y) = {
        let node = project.node(solid).expect("solid should exist");
        output_pin_center(node).expect("solid output pin")
    };
    let (to_x, to_y) = {
        let node = project.node(out).expect("out should exist");
        input_pin_center(node).expect("out input pin")
    };
    let mid = ((from_x + to_x) / 2, (from_y + to_y) / 2);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.drag = Some(DragState {
        node_id: xform,
        offset_x: 0,
        offset_y: 0,
        origin_x: 120,
        origin_y: 160,
    });
    let drag = InputSnapshot {
        mouse_pos: Some(mid),
        left_down: true,
        ..InputSnapshot::default()
    };
    assert!(handle_drag_input(&drag, &mut project, 420, 480, &mut state));
    assert_eq!(
        state.hover_insert_link,
        Some(HoverInsertLink {
            source_id: solid,
            target_id: out,
        })
    );
}

#[test]
fn dropping_dragged_node_on_wire_inserts_node_between_link() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 120, 160, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 420, 480);
    assert!(project.connect_image_link(solid, out));
    let (from_x, from_y) = {
        let node = project.node(solid).expect("solid should exist");
        output_pin_center(node).expect("solid output pin")
    };
    let (to_x, to_y) = {
        let node = project.node(out).expect("out should exist");
        input_pin_center(node).expect("out input pin")
    };
    let mid = ((from_x + to_x) / 2, (from_y + to_y) / 2);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.drag = Some(DragState {
        node_id: xform,
        offset_x: 0,
        offset_y: 0,
        origin_x: 120,
        origin_y: 160,
    });
    state.hover_insert_link = Some(HoverInsertLink {
        source_id: solid,
        target_id: out,
    });
    let drop = InputSnapshot {
        mouse_pos: Some(mid),
        left_down: false,
        ..InputSnapshot::default()
    };
    assert!(handle_drag_input(&drop, &mut project, 420, 480, &mut state));
    assert!(state.drag.is_none());
    assert!(state.hover_insert_link.is_none());
    assert_eq!(project.input_source_node_id(xform), Some(solid));
    assert_eq!(project.input_source_node_id(out), Some(xform));
}

#[test]
fn dragging_selected_nodes_moves_selection_as_one_group() {
    let mut project = GuiProject::new_empty(640, 480);
    let first = project.add_node(ProjectNodeKind::TexTransform2D, 40, 80, 420, 480);
    let second = project.add_node(ProjectNodeKind::TexSolid, 180, 120, 420, 480);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.selected_nodes = vec![first, second];
    state.drag = Some(DragState {
        node_id: first,
        offset_x: 0,
        offset_y: 0,
        origin_x: 40,
        origin_y: 80,
    });

    let drag = InputSnapshot {
        mouse_pos: Some((90, 130)),
        left_down: true,
        ..InputSnapshot::default()
    };
    assert!(handle_drag_input(&drag, &mut project, 420, 480, &mut state));
    let first_node = project.node(first).expect("first node should exist");
    let second_node = project.node(second).expect("second node should exist");
    assert_eq!(first_node.x(), 90);
    assert_eq!(first_node.y(), 130);
    assert_eq!(second_node.x(), 230);
    assert_eq!(second_node.y(), 170);
}

#[test]
fn dropping_node_on_top_of_other_snaps_to_side_from_drag_origin() {
    let mut project = GuiProject::new_empty(640, 480);
    let dragged = project.add_node(ProjectNodeKind::TexTransform2D, 40, 80, 420, 480);
    let target = project.add_node(ProjectNodeKind::TexSolid, 260, 80, 420, 480);
    // Simulate release while dragged node overlaps target card.
    assert!(project.move_node(dragged, 260, 80, 420, 480));
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.drag = Some(DragState {
        node_id: dragged,
        offset_x: 0,
        offset_y: 0,
        origin_x: 40,
        origin_y: 80,
    });
    let drop = InputSnapshot {
        mouse_pos: Some((300, 100)),
        left_down: false,
        ..InputSnapshot::default()
    };
    assert!(handle_drag_input(&drop, &mut project, 420, 480, &mut state));
    let dragged_node = project.node(dragged).expect("dragged node should exist");
    let target_node = project.node(target).expect("target node should exist");
    assert_eq!(
        dragged_node.x(),
        target_node.x() - NODE_WIDTH - super::NODE_OVERLAP_SNAP_GAP_PX
    );
}

#[test]
fn right_click_on_bound_param_value_unbinds_parameter() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(circle, 420, 480));
    assert!(project.connect_signal_link_to_param(lfo, circle, 2));
    let value_rect = {
        let node = project.node(circle).expect("circle node should exist");
        node_param_value_rect(node, 2).expect("value rect should exist")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let input = InputSnapshot {
        mouse_pos: Some((value_rect.x + 2, value_rect.y + 2)),
        right_clicked: true,
        ..InputSnapshot::default()
    };
    assert!(handle_right_selection(
        &input,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert_eq!(project.signal_source_for_param(circle, 2), None);
}

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
    let input = InputSnapshot {
        mouse_pos: Some((value_rect.x + 2, value_rect.y + 2)),
        left_clicked: true,
        ..InputSnapshot::default()
    };
    assert!(apply_preview_actions(
        &config,
        input,
        &mut project,
        640,
        420,
        480,
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
fn alt_cut_unbinds_parameter_link_when_cut_crosses_param_wire() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(circle, 420, 480));
    assert!(project.connect_signal_link_to_param(lfo, circle, 2));

    let (from_x, from_y) = {
        let source = project.node(lfo).expect("lfo node should exist");
        output_pin_center(source).expect("source output pin should exist")
    };
    let (to_x, to_y) = {
        let target = project.node(circle).expect("circle node should exist");
        let row = node_param_row_rect(target, 2).expect("row rect should exist");
        (row.x + row.w - 4, row.y + row.h / 2)
    };
    let cut_x = (from_x + to_x) / 2;
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.link_cut = Some(LinkCutState {
        start_x: cut_x,
        start_y: from_y.min(to_y) - 24,
        cursor_x: cut_x,
        cursor_y: from_y.max(to_y) + 24,
    });
    let input = InputSnapshot {
        left_down: false,
        ..InputSnapshot::default()
    };
    assert!(handle_link_cut(&input, &mut project, 420, 480, &mut state));
    assert_eq!(project.signal_source_for_param(circle, 2), None);
}

#[test]
fn alt_cut_unbinds_parameter_link_when_cut_crosses_routed_param_wire() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
    let _blocker = project.add_node(ProjectNodeKind::TexSolid, 210, 70, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 420, 80, 420, 480);
    assert!(project.toggle_node_expanded(circle, 420, 480));
    assert!(project.connect_signal_link_to_param(lfo, circle, 2));

    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let (from_x, from_y) = {
        let source = project.node(lfo).expect("lfo node should exist");
        output_pin_center(source).expect("source output pin should exist")
    };
    let (to_x, to_y) = {
        let target = project.node(circle).expect("circle node should exist");
        let row = node_param_row_rect(target, 2).expect("row rect should exist");
        (row.x + row.w - 4, row.y + row.h / 2)
    };
    let obstacles = super::collect_panel_node_obstacles(&project, &state);
    let exit_x = from_x.saturating_add(super::PARAM_WIRE_EXIT_TAIL_PX);
    let entry_x = to_x.saturating_add(super::PARAM_WIRE_ENTRY_TAIL_PX);
    let route = crate::gui::scene::wire_route::route_param_path(
        (exit_x, from_y),
        (entry_x, to_y),
        obstacles.as_slice(),
    );
    let cut = route
        .windows(2)
        .find_map(|segment| {
            let (ax, ay) = segment[0];
            let (bx, by) = segment[1];
            let (start_x, start_y, cursor_x, cursor_y) = if ax == bx {
                (ax - 24, (ay + by) / 2, ax + 24, (ay + by) / 2)
            } else {
                ((ax + bx) / 2, ay - 24, (ax + bx) / 2, ay + 24)
            };
            if !segments_intersect(start_x, start_y, cursor_x, cursor_y, ax, ay, bx, by)
                || segments_intersect(
                    start_x, start_y, cursor_x, cursor_y, from_x, from_y, to_x, to_y,
                )
            {
                return None;
            }
            Some(LinkCutState {
                start_x,
                start_y,
                cursor_x,
                cursor_y,
            })
        })
        .expect("expected routed segment that is distinct from source-to-target straight wire");
    state.link_cut = Some(cut);
    let input = InputSnapshot {
        left_down: false,
        ..InputSnapshot::default()
    };
    assert!(handle_link_cut(&input, &mut project, 420, 480, &mut state));
    assert_eq!(project.signal_source_for_param(circle, 2), None);
}

#[test]
fn add_menu_category_then_secondary_picker_spawns_node() {
    let mut project = GuiProject::new_empty(640, 480);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.pan_x = 48.0;
    state.pan_y = 30.0;
    state.zoom = 2.0;
    state.menu = AddNodeMenuState::open_at(120, 100, 420, 480);
    let mut control_index = None;
    for index in 0..state.menu.visible_entry_count() {
        let Some(entry) = state.menu.visible_entry(index) else {
            continue;
        };
        if matches!(
            entry,
            AddNodeMenuEntry::Category(category) if category.label() == "Control"
        ) {
            control_index = Some(index);
            break;
        }
    }
    let control_index = control_index.expect("control category should exist");
    state.menu.selected = control_index;
    let open_category = InputSnapshot {
        menu_accept: true,
        ..InputSnapshot::default()
    };
    assert!(handle_add_menu_input(
        &open_category,
        &mut project,
        420,
        480,
        &mut state
    ));
    assert!(state.menu.active_category.is_some());
    let query = InputSnapshot {
        typed_text: "lfo".to_string(),
        ..InputSnapshot::default()
    };
    assert!(handle_add_menu_input(
        &query,
        &mut project,
        420,
        480,
        &mut state
    ));
    state.menu.selected = 1;
    let spawn = InputSnapshot {
        menu_accept: true,
        ..InputSnapshot::default()
    };
    assert!(handle_add_menu_input(
        &spawn,
        &mut project,
        420,
        480,
        &mut state
    ));
    let mut spawned_lfo = None;
    for node in project.nodes() {
        if node.kind() == ProjectNodeKind::CtlLfo {
            spawned_lfo = Some((node.x(), node.y()));
            break;
        }
    }
    assert_eq!(spawned_lfo, Some((36, 35)));
    assert!(!state.menu.open);
}

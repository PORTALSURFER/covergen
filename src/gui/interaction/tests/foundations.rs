use super::*;
#[test]
fn segments_intersect_detects_crossing_lines() {
    assert!(segments_intersect((0, 0), (10, 10), (0, 10), (10, 0)));
}

#[test]
fn segments_intersect_detects_non_crossing_lines() {
    assert!(!segments_intersect((0, 0), (10, 0), (0, 5), (10, 5)));
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
    assert!(rects_overlap((0, 0, 10, 10), (8, 8, 16, 16)));
    assert!(!rects_overlap((0, 0, 10, 10), (11, 11, 20, 20)));
}

#[test]
fn timeline_step_handles_large_delta_without_iterative_catchup() {
    let config = V2Config::parse(Vec::new()).expect("config");
    let mut state = PreviewState::new(&config);
    state.paused = false;
    state.frame_index = 5;
    state.timeline_accum_secs = 0.0;

    let advanced = step_timeline_if_running(&mut state, Duration::from_secs(10), 60, 180);
    assert!(advanced);
    assert_eq!(state.frame_index, 65);
    assert!(state.timeline_accum_secs < (1.0 / 60.0));
}

#[test]
fn apply_preview_actions_toggle_pause_invalidates_timeline() {
    let config = V2Config::parse(Vec::new()).expect("config");
    let mut project = GuiProject::new_empty(640, 480);
    let mut state = PreviewState::new(&config);
    let before = state.invalidation;
    let input = InputSnapshot {
        toggle_pause: true,
        ..InputSnapshot::default()
    };
    assert!(apply_preview_actions(
        InteractionFrameContext::new(&config, 640, 420, 480),
        input,
        &mut project,
        &mut state,
    ));
    assert!(
        state.invalidation.timeline != before.timeline,
        "pause toggle should invalidate timeline layer"
    );
}

#[test]
fn apply_preview_actions_hover_updates_invalidate_only_nodes_when_overlay_state_is_unchanged() {
    let config = V2Config::parse(Vec::new()).expect("config");
    let mut project = GuiProject::new_empty(640, 480);
    let _solid = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let mut state = PreviewState::new(&config);
    let before = state.invalidation;
    let input = InputSnapshot {
        mouse_pos: Some((90, 90)),
        ..InputSnapshot::default()
    };
    assert!(apply_preview_actions(
        InteractionFrameContext::new(&config, 640, 420, 480),
        input,
        &mut project,
        &mut state,
    ));
    assert!(
        state.invalidation.nodes != before.nodes,
        "hover updates should invalidate nodes layer"
    );
    assert!(
        state.invalidation.wires == before.wires,
        "hover-only updates should not invalidate wires layer"
    );
    assert!(
        state.invalidation.overlays == before.overlays,
        "hover updates should not invalidate overlays when overlay state is unchanged"
    );
}

#[test]
fn apply_preview_actions_debug_input_flag_change_invalidates_overlays() {
    let config = V2Config::parse(Vec::new()).expect("config");
    let mut project = GuiProject::new_empty(640, 480);
    let mut state = PreviewState::new(&config);
    let before = state.invalidation;
    let input = InputSnapshot {
        alt_down: true,
        ..InputSnapshot::default()
    };
    assert!(apply_preview_actions(
        InteractionFrameContext::new(&config, 640, 420, 480),
        input,
        &mut project,
        &mut state,
    ));
    assert!(state.debug_input_alt_down);
    assert!(
        state.invalidation.overlays != before.overlays,
        "debug HUD input flags should invalidate overlays when they change"
    );
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

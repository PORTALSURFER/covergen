use super::*;
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
fn dragging_node_over_routed_wire_segment_highlights_insert_candidate() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let _blocker = project.add_node(ProjectNodeKind::TexFeedback, 420, 40, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 120, 220, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 700, 60, 420, 480);
    assert!(project.connect_image_link(solid, out));
    let (from_x, from_y) = {
        let node = project.node(solid).expect("solid should exist");
        output_pin_center(node).expect("solid output pin")
    };
    let (to_x, to_y) = {
        let node = project.node(out).expect("out should exist");
        input_pin_center(node).expect("out input pin")
    };
    let obstacles = super::collect_graph_node_obstacles(&project);
    let route_map = wire_route::RouteObstacleMap::from_obstacles(obstacles.as_slice());
    let route = wire_route::route_wire_path_with_tails_with_map(
        wire_route::RouteEndpoint {
            point: (from_x, from_y),
            corridor_dir: wire_route::RouteDirection::East,
        },
        wire_route::RouteEndpoint {
            point: (to_x, to_y),
            corridor_dir: wire_route::RouteDirection::West,
        },
        &route_map,
    );
    let direct_dist_sq = |x: i32, y: i32| -> f32 {
        super::point_to_segment_distance_sq(
            x as f32,
            y as f32,
            from_x as f32,
            from_y as f32,
            to_x as f32,
            to_y as f32,
        )
    };
    let hover_radius_sq = 100.0_f32;
    let Some(cursor) = route.windows(2).find_map(|segment| {
        let px = (segment[0].0 + segment[1].0) / 2;
        let py = (segment[0].1 + segment[1].1) / 2;
        (direct_dist_sq(px, py) > hover_radius_sq).then_some((px, py))
    }) else {
        panic!("expected routed path to include one detour segment: {route:?}");
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.drag = Some(DragState {
        node_id: xform,
        offset_x: 0,
        offset_y: 0,
        origin_x: 120,
        origin_y: 220,
    });
    let drag = InputSnapshot {
        mouse_pos: Some(cursor),
        left_down: true,
        ..InputSnapshot::default()
    };
    assert!(handle_drag_input(
        &drag,
        &mut project,
        1400,
        480,
        &mut state
    ));
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
fn add_menu_drop_on_wire_inserts_new_node_between_link() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
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
    let drop_point = ((from_x + to_x) / 2, (from_y + to_y) / 2);
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.menu =
        AddNodeMenuState::open_at(drop_point.0, drop_point.1, 420, editor_panel_height(480));
    assert!(state.menu.open_category(AddNodeCategory::Texture));
    let option_index = add_node_options()
        .iter()
        .position(|option| option.kind == ProjectNodeKind::TexTransform2D)
        .expect("transform option should exist");
    let visible_entry_index = (0..state.menu.visible_entry_count())
        .find(|index| {
            state.menu.visible_entry(*index) == Some(AddNodeMenuEntry::Option(option_index))
        })
        .expect("menu should expose transform option");
    assert!(state.menu.select_index(visible_entry_index));
    let input = InputSnapshot {
        menu_accept: true,
        ..InputSnapshot::default()
    };
    assert!(handle_add_menu_input(
        &input,
        &mut project,
        420,
        480,
        &mut state
    ));
    let inserted = project
        .nodes()
        .iter()
        .find(|node| node.kind() == ProjectNodeKind::TexTransform2D)
        .expect("transform node should be created")
        .id();
    assert_eq!(project.input_source_node_id(inserted), Some(solid));
    assert_eq!(project.input_source_node_id(out), Some(inserted));
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
    assert_eq!(first_node.x(), 88);
    assert_eq!(first_node.y(), 128);
    assert_eq!(second_node.x(), 228);
    assert_eq!(second_node.y(), 168);
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

use super::*;
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
fn alt_cut_unbinds_parameter_link_when_target_node_is_collapsed() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 220, 80, 420, 480);
    assert!(project.connect_signal_link_to_param(lfo, circle, 2));

    let (from_x, from_y) = {
        let source = project.node(lfo).expect("lfo node should exist");
        output_pin_center(source).expect("source output pin should exist")
    };
    let (to_x, to_y) = {
        let target = project.node(circle).expect("circle node should exist");
        collapsed_param_entry_pin_center(target).expect("collapsed param pin should exist")
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
fn hover_param_target_uses_collapsed_param_pin() {
    let mut project = GuiProject::new_empty(640, 480);
    let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 60, 420, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 180, 80, 420, 480);
    assert!(project.select_param(circle, 3));
    let (pin_x, pin_y) = {
        let target = project.node(circle).expect("circle node should exist");
        collapsed_param_entry_pin_center(target).expect("collapsed param pin should exist")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.wire_drag = Some(WireDragState {
        source_node_id: lfo,
        cursor_x: pin_x,
        cursor_y: pin_y,
    });
    let input = InputSnapshot {
        mouse_pos: Some((pin_x, pin_y)),
        left_down: true,
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
        state.hover_param_target,
        Some(HoverParamTarget {
            node_id: circle,
            param_index: 3,
        })
    );
}

#[test]
fn hovering_expanded_param_row_sets_soft_param_hover_state() {
    let mut project = GuiProject::new_empty(640, 480);
    let circle = project.add_node(ProjectNodeKind::TexCircle, 180, 80, 420, 480);
    assert!(project.toggle_node_expanded(circle, 420, 480));
    let (hover_x, hover_y) = {
        let node = project.node(circle).expect("circle node should exist");
        let row = node_param_row_rect(node, 1).expect("param row should exist");
        (row.x + 8, row.y + row.h / 2)
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    let input = InputSnapshot {
        mouse_pos: Some((hover_x, hover_y)),
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
        state.hover_param,
        Some(HoverParamTarget {
            node_id: circle,
            param_index: 1,
        })
    );
    assert!(state.hover_param_target.is_none());
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
    let obstacles = super::collect_graph_node_obstacles(&project);
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
            if !segments_intersect((start_x, start_y), (cursor_x, cursor_y), (ax, ay), (bx, by))
                || segments_intersect(
                    (start_x, start_y),
                    (cursor_x, cursor_y),
                    (from_x, from_y),
                    (to_x, to_y),
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
fn collect_cut_links_dedupes_fallback_hits_for_same_link() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 240, 80, 420, 480);
    assert!(project.connect_image_link(solid, out));

    let (from_x, from_y) = {
        let source = project.node(solid).expect("solid node should exist");
        output_pin_center(source).expect("source output pin should exist")
    };
    let (to_x, to_y) = {
        let target = project.node(out).expect("out node should exist");
        input_pin_center(target).expect("target input pin should exist")
    };
    let cut_x = (from_x + to_x) / 2;
    let cut = LinkCutState {
        start_x: cut_x,
        start_y: from_y.min(to_y) - 24,
        cursor_x: cut_x,
        cursor_y: from_y.max(to_y) + 24,
    };
    let state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));

    let links = collect_cut_links(
        &project,
        420,
        480,
        &state,
        LinkCutState { start_y: -8, ..cut },
    );
    assert_eq!(
        links.len(),
        1,
        "fallback scan should not duplicate cut hits"
    );
    assert_eq!(
        links[0],
        CutLink {
            source_id: solid,
            target_id: out,
            param_index: None,
        }
    );
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
    assert_eq!(spawned_lfo, Some((36, 36)));
    assert!(!state.menu.open);
}

#[test]
fn alt_cut_does_not_start_while_param_edit_is_active() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let value_rect = {
        let node = project.node(solid).expect("solid node should exist");
        node_param_value_rect(node, 0).expect("value rect should exist")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.param_edit = Some(ParamEditState {
        node_id: solid,
        param_index: 0,
        buffer: "0.90".to_string(),
        cursor: 4,
        anchor: 4,
    });
    let input = InputSnapshot {
        alt_down: true,
        left_clicked: true,
        left_down: true,
        mouse_pos: Some((value_rect.x + 2, value_rect.y + 2)),
        ..InputSnapshot::default()
    };
    assert!(!handle_link_cut(&input, &mut project, 420, 480, &mut state));
    assert!(state.link_cut.is_none());
}

#[test]
fn alt_cut_does_not_start_while_alt_hover_targets_scrubbable_param() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 220, 80, 420, 480);
    assert!(project.toggle_node_expanded(solid, 420, 480));
    let row_rect = {
        let node = project.node(solid).expect("solid node should exist");
        node_param_row_rect(node, 0).expect("row rect should exist")
    };
    let mut state = PreviewState::new(&V2Config::parse(Vec::new()).expect("config"));
    state.hover_alt_param = Some(HoverParamTarget {
        node_id: solid,
        param_index: 0,
    });
    let input = InputSnapshot {
        alt_down: true,
        left_clicked: true,
        left_down: true,
        mouse_pos: Some((row_rect.x + row_rect.w + 24, row_rect.y + row_rect.h / 2)),
        ..InputSnapshot::default()
    };
    assert!(!handle_link_cut(&input, &mut project, 420, 480, &mut state));
    assert!(state.link_cut.is_none());
}

#[test]
fn add_menu_toggle_shortcut_does_not_seed_query_text() {
    let config = V2Config::parse(Vec::new()).expect("config");
    let mut project = GuiProject::new_empty(640, 480);
    let mut state = PreviewState::new(&config);
    let input = InputSnapshot {
        toggle_add_menu: true,
        typed_text: "A".to_string(),
        ..InputSnapshot::default()
    };
    assert!(apply_preview_actions(
        InteractionFrameContext::new(&config, 640, 420, 480),
        input,
        &mut project,
        &mut state
    ));
    assert!(state.menu.open);
    assert!(state.menu.query.is_empty());
    assert!(state.menu.visible_entry_count() > 1);
}

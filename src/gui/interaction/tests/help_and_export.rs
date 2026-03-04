use super::*;
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

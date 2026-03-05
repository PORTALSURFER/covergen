//! Overlay-layer geometry composition helpers for [`SceneBuilder`].

use super::*;
use crate::gui::state::HoverParamTarget;
use std::fmt::Write as _;

impl SceneBuilder {
    pub(super) fn rebuild_overlays_layer(
        &mut self,
        project: &GuiProject,
        state: &PreviewState,
        panel_width: usize,
        panel_height: usize,
    ) {
        let before = self.layer_capacity(ActiveLayer::Overlays);
        self.set_active_layer(ActiveLayer::Overlays);
        self.set_active_space(CoordSpace::Graph);
        self.clear_active_layer();
        self.push_param_dropdown(project, state);
        self.set_active_space(CoordSpace::Screen);
        self.push_wire_drag(project, state);
        self.push_right_marquee(state);
        self.push_link_cut(state);
        self.push_menu(state);
        self.push_main_menu(state);
        self.push_export_menu(state);
        self.push_help_modal(state, panel_width, panel_height);
        self.push_interaction_debug_hud(state);
        self.bump_layer_alloc_growth(before, self.layer_capacity(ActiveLayer::Overlays));
    }

    fn push_param_dropdown(&mut self, project: &GuiProject, state: &PreviewState) {
        let Some(dropdown) = state.param_dropdown else {
            return;
        };
        let Some(node) = project.node(dropdown.node_id) else {
            return;
        };
        let Some(options) =
            project.node_param_dropdown_options(dropdown.node_id, dropdown.param_index)
        else {
            return;
        };
        if options.is_empty() {
            return;
        }
        let Some(list_world) = node_param_dropdown_rect(node, dropdown.param_index, options.len())
        else {
            return;
        };
        let list_panel = graph_rect_to_panel(list_world, state);
        self.push_rect(list_panel, PARAM_DROPDOWN_BG);
        self.push_border(list_panel, PARAM_VALUE_BORDER);
        let selected = project
            .node_param_dropdown_selected_index(dropdown.node_id, dropdown.param_index)
            .unwrap_or(0);
        for (index, option) in options.iter().enumerate() {
            let row_world = Rect::new(
                list_world.x,
                list_world.y + index as i32 * crate::gui::project::NODE_PARAM_DROPDOWN_ROW_HEIGHT,
                list_world.w,
                crate::gui::project::NODE_PARAM_DROPDOWN_ROW_HEIGHT,
            );
            let row_panel = graph_rect_to_panel(row_world, state);
            if index == selected {
                self.push_rect(row_panel, PARAM_DROPDOWN_SELECTED);
            }
            if state.hover_dropdown_item == Some(index) {
                self.push_rect(row_panel, PARAM_DROPDOWN_HOVER);
            }
            self.push_graph_text_in_rect(row_panel, 4, option.label, NODE_TEXT, state);
        }
    }

    fn push_menu(&mut self, state: &PreviewState) {
        if !state.menu.open {
            return;
        }
        let rect = state.menu.rect();
        self.push_rect(rect, MENU_BG);
        self.push_border(rect, MENU_BORDER);
        self.push_text(
            rect.x + MENU_INNER_PADDING + 6,
            rect.y + 6,
            "Create Node",
            MENU_TEXT,
        );
        let search_rect = state.menu.search_rect();
        let search_text = if state.menu.query.is_empty() {
            if state.menu.is_category_picker() {
                "Search categories..."
            } else {
                "Search nodes..."
            }
        } else {
            state.menu.query.as_str()
        };
        self.push_rect(search_rect, MENU_SEARCH_BG);
        self.push_border(search_rect, MENU_BORDER);
        self.push_text(search_rect.x + 6, search_rect.y + 7, search_text, MENU_TEXT);
        let entry_count = state.menu.visible_entry_count();
        if entry_count == 0 {
            self.push_text(
                rect.x + MENU_INNER_PADDING + 6,
                search_rect.y + search_rect.h + MENU_BLOCK_GAP + 6,
                "No matching nodes",
                MENU_CATEGORY_TEXT,
            );
            return;
        }
        let mut menu_label_scratch = std::mem::take(&mut self.label_scratch);
        for entry_index in 0..entry_count {
            let Some(entry) = state.menu.visible_entry(entry_index) else {
                continue;
            };
            let Some(item) = state.menu.entry_rect(entry_index) else {
                continue;
            };
            if (state.menu.selected == entry_index || state.hover_menu_item == Some(entry_index))
                && !matches!(entry, AddNodeMenuEntry::Category(_))
            {
                self.push_rect(item, MENU_SELECTED);
            }
            let (text, color) = match entry {
                AddNodeMenuEntry::Category(category) => {
                    let chip = category_chip_rect(item);
                    self.push_rect(chip, category_menu_color(category));
                    if state.menu.selected == entry_index
                        || state.hover_menu_item == Some(entry_index)
                    {
                        self.push_border(
                            Rect::new(chip.x - 1, chip.y - 1, chip.w + 2, chip.h + 2),
                            MENU_SELECTED,
                        );
                    }
                    self.push_border(chip, MENU_CATEGORY_CHIP_BORDER);
                    menu_label_scratch.clear();
                    menu_label_scratch.push_str(category.label());
                    self.push_text(
                        chip.x + 8,
                        chip.y + 2,
                        menu_label_scratch.as_str(),
                        MENU_CATEGORY_CHIP_TEXT,
                    );
                    (menu_label_scratch.as_str(), MENU_CATEGORY_TEXT)
                }
                AddNodeMenuEntry::Back => ("< Categories", MENU_CATEGORY_TEXT),
                AddNodeMenuEntry::Option(option_index) => {
                    let option = ADD_NODE_OPTIONS[option_index];
                    if state.menu.query.is_empty() {
                        (option.label(), MENU_TEXT)
                    } else {
                        menu_label_scratch.clear();
                        menu_label_scratch.push_str(option.category.label());
                        menu_label_scratch.push_str(" / ");
                        menu_label_scratch.push_str(option.label());
                        (menu_label_scratch.as_str(), MENU_TEXT)
                    }
                }
            };
            if !matches!(entry, AddNodeMenuEntry::Category(_)) {
                self.push_text(item.x + 6, item.y + 6, text, color);
            }
        }
        self.label_scratch = menu_label_scratch;
    }

    fn push_main_menu(&mut self, state: &PreviewState) {
        if !state.main_menu.open {
            return;
        }
        let rect = state.main_menu.rect();
        self.push_rect(rect, MENU_BG);
        self.push_border(rect, MENU_BORDER);
        self.push_text(
            rect.x + MENU_INNER_PADDING + 6,
            rect.y + 6,
            "Main Menu",
            MENU_TEXT,
        );
        for (entry_index, item) in state.main_menu.items().iter().copied().enumerate() {
            let Some(row) = state.main_menu.entry_rect(entry_index) else {
                continue;
            };
            if state.main_menu.selected == entry_index
                || state.hover_main_menu_item == Some(entry_index)
            {
                self.push_rect(row, MENU_SELECTED);
            }
            let label = if item == MainMenuItem::Export && state.export_menu.open {
                "Export >"
            } else {
                item.label()
            };
            self.push_text(row.x + 6, row.y + 6, label, MENU_TEXT);
        }
    }

    fn push_export_menu(&mut self, state: &PreviewState) {
        if !state.export_menu.open {
            return;
        }
        let rect = state.export_menu.rect();
        self.push_rect(rect, MENU_BG);
        self.push_border(rect, MENU_BORDER);
        self.push_text(
            rect.x + MENU_INNER_PADDING + 6,
            rect.y + 6,
            "Export H.264",
            MENU_TEXT,
        );
        let close_rect = state.export_menu.close_button_rect();
        if state.hover_export_menu_close {
            self.push_rect(close_rect, MENU_SELECTED);
        }
        self.push_border(close_rect, MENU_BORDER);
        self.push_line(
            close_rect.x + 3,
            close_rect.y + 3,
            close_rect.x + close_rect.w - 4,
            close_rect.y + close_rect.h - 4,
            MENU_TEXT,
        );
        self.push_line(
            close_rect.x + close_rect.w - 4,
            close_rect.y + 3,
            close_rect.x + 3,
            close_rect.y + close_rect.h - 4,
            MENU_TEXT,
        );
        let mut menu_label_scratch = std::mem::take(&mut self.label_scratch);
        for (entry_index, item) in state.export_menu.items().iter().copied().enumerate() {
            let Some(row) = state.export_menu.entry_rect(entry_index) else {
                continue;
            };
            if state.export_menu.selected == entry_index
                || state.hover_export_menu_item == Some(entry_index)
            {
                self.push_rect(row, MENU_SELECTED);
            }
            menu_label_scratch.clear();
            match item {
                ExportMenuItem::Directory => {
                    menu_label_scratch.push_str("Directory: ");
                    menu_label_scratch.push_str(state.export_menu.directory.as_str());
                }
                ExportMenuItem::FileName => {
                    menu_label_scratch.push_str("File Name: ");
                    menu_label_scratch.push_str(state.export_menu.file_name.as_str());
                }
                ExportMenuItem::BeatsPerBar => {
                    menu_label_scratch.push_str("Beats / Bar: ");
                    menu_label_scratch.push_str(state.export_menu.beats_per_bar.as_str());
                }
                ExportMenuItem::Codec => {
                    menu_label_scratch.push_str("Video: H.264 (OpenH264)");
                }
                ExportMenuItem::StartStop => {
                    if state.export_menu.exporting {
                        menu_label_scratch.push_str("Stop Export");
                    } else {
                        menu_label_scratch.push_str("Start Export");
                    }
                }
                ExportMenuItem::Preview => {
                    let _ = write!(
                        &mut menu_label_scratch,
                        "Preview: {}/{} frames",
                        state.export_menu.preview_frame, state.export_menu.preview_total
                    );
                }
            }
            self.push_text(row.x + 6, row.y + 6, menu_label_scratch.as_str(), MENU_TEXT);
        }
        let preview = state.export_menu.preview_viewport_rect();
        let preview_label_y = (preview.y - 14).max(rect.y + 8);
        self.push_text(
            preview.x,
            preview_label_y,
            "Export Preview",
            MENU_CATEGORY_TEXT,
        );
        self.push_rect(preview, PARAM_VALUE_BG);
        self.push_border(preview, MENU_BORDER);
        if !state.export_menu.status.is_empty() {
            self.push_text(
                rect.x + MENU_INNER_PADDING + 6,
                rect.y + rect.h - 16,
                state.export_menu.status.as_str(),
                MENU_CATEGORY_TEXT,
            );
        }
        self.label_scratch = menu_label_scratch;
    }

    fn push_help_modal(&mut self, state: &PreviewState, panel_width: usize, panel_height: usize) {
        let Some(help) = state.help_modal.as_ref() else {
            return;
        };
        let editor_h = editor_panel_height(panel_height) as i32;
        if panel_width == 0 || editor_h <= 0 {
            return;
        }
        let panel_rect = Rect::new(0, 0, panel_width as i32, editor_h);
        self.push_rect(panel_rect, HELP_BACKDROP);

        let max_modal_w = (panel_width as i32 - 32).max(280);
        let modal_w = max_modal_w.clamp(280, 560);
        let title_h = 18;
        let line_h = 14;
        let footer_h = 16;
        let pad = 10;
        let min_modal_h = 112;
        let desired_h = min_modal_h + (help.lines.len() as i32 * line_h);
        let max_modal_h = (editor_h - 28).max(min_modal_h);
        let modal_h = desired_h.min(max_modal_h);
        let modal_x = ((panel_width as i32 - modal_w) / 2).max(8);
        let modal_y = ((editor_h - modal_h) / 2).max(8);
        let modal = Rect::new(modal_x, modal_y, modal_w, modal_h);
        self.push_rect(modal, HELP_PANEL_BG);
        self.push_border(modal, MENU_BORDER);

        self.push_text(
            modal.x + pad,
            modal.y + pad,
            help.title.as_str(),
            HELP_TITLE,
        );
        let hint = "F1/click to close";
        self.push_text(
            modal.x + modal.w - self.text_renderer.measure_text_width(hint, 1.0) - pad,
            modal.y + pad,
            hint,
            HELP_HINT,
        );

        let body_y = modal.y + pad + title_h;
        let body_h = modal.h - title_h - footer_h - (pad * 2);
        let visible_lines = (body_h / line_h).max(0) as usize;
        let mut y = body_y;
        for line in help.lines.iter().take(visible_lines) {
            self.push_text(modal.x + pad, y, line.as_str(), HELP_TEXT);
            y += line_h;
        }
        if help.lines.len() > visible_lines && visible_lines > 0 {
            self.push_text(
                modal.x + pad,
                modal.y + modal.h - pad - footer_h,
                "...",
                HELP_HINT,
            );
        }
    }

    fn push_interaction_debug_hud(&mut self, state: &PreviewState) {
        let mode = if state.param_scrub.is_some() {
            "SCRUB"
        } else if state.link_cut.is_some() {
            "CUT"
        } else {
            "NONE"
        };
        let hover_alt = format_hover_param_target(state.hover_alt_param);
        let scrub = state.param_scrub.map_or_else(
            || "-".to_string(),
            |scrub| format!("n{}:p{}", scrub.node_id, scrub.param_index),
        );
        let mut debug_line = String::new();
        let _ = write!(
            &mut debug_line,
            "DBG mode={mode} alt={} lmb={} click={} rmb={} rclick={} hover_alt={} scrub={} cut={} edit={}",
            bool_flag(state.debug_input_alt_down),
            bool_flag(state.debug_input_left_down),
            bool_flag(state.debug_input_left_clicked),
            bool_flag(state.debug_input_right_down),
            bool_flag(state.debug_input_right_clicked),
            hover_alt,
            scrub,
            bool_flag(state.link_cut.is_some()),
            bool_flag(state.param_edit.is_some()),
        );
        let rect = Rect::new(8, 8, 700, 20);
        self.push_rect(rect, MENU_BG);
        self.push_border(rect, MENU_BORDER);
        self.push_text(rect.x + 6, rect.y + 6, debug_line.as_str(), HELP_HINT);
    }

    fn push_link_cut(&mut self, state: &PreviewState) {
        let Some(cut) = state.link_cut else {
            return;
        };
        self.push_line(
            cut.start_x,
            cut.start_y,
            cut.cursor_x,
            cut.cursor_y,
            CUT_LINE_COLOR,
        );
    }

    fn push_right_marquee(&mut self, state: &PreviewState) {
        let Some(marquee) = state.right_marquee else {
            return;
        };
        let Some(rect) = marquee_panel_rect(marquee) else {
            return;
        };
        self.push_rect(rect, MARQUEE_FILL);
        self.push_border(rect, MARQUEE_BORDER);
    }

    fn push_wire_drag(&mut self, project: &GuiProject, state: &PreviewState) {
        let Some(wire) = state.wire_drag else {
            return;
        };
        let Some(source) = project.node(wire.source_node_id) else {
            return;
        };
        let Some((x0, y0)) = output_pin_center(source) else {
            return;
        };
        let (x0, y0) = graph_point_to_panel(x0, y0, state);
        let (x1, y1) = if wire_drag_source_kind(project, wire) == Some(ResourceKind::Signal) {
            if let Some(target) = state.hover_param_target {
                if let Some(target_node) = project.node(target.node_id) {
                    if let Some(row) = node_param_row_rect(target_node, target.param_index) {
                        graph_point_to_panel(row.x + row.w - 4, row.y + row.h / 2, state)
                    } else if let Some((pin_x, pin_y)) =
                        collapsed_param_entry_pin_center(target_node)
                    {
                        graph_point_to_panel(pin_x, pin_y, state)
                    } else {
                        (wire.cursor_x, wire.cursor_y)
                    }
                } else {
                    (wire.cursor_x, wire.cursor_y)
                }
            } else {
                (wire.cursor_x, wire.cursor_y)
            }
        } else if let Some(target_id) = state.hover_input_pin {
            if let Some(target_node) = project.node(target_id) {
                input_pin_center(target_node)
                    .map(|(x, y)| graph_point_to_panel(x, y, state))
                    .unwrap_or((wire.cursor_x, wire.cursor_y))
            } else {
                (wire.cursor_x, wire.cursor_y)
            }
        } else {
            (wire.cursor_x, wire.cursor_y)
        };
        if wire_drag_source_kind(project, wire) == Some(ResourceKind::Signal) {
            if state.hover_param_target.is_some() {
                self.push_signal_wire_right_exit_entry(x0, y0, x1, y1, PARAM_EDGE_COLOR);
                self.push_param_target_marker(x1, y1, PARAM_EDGE_COLOR);
            } else {
                self.push_signal_wire_right_exit(x0, y0, x1, y1, PARAM_EDGE_COLOR);
            }
        } else {
            self.push_straight_wire_with_round_caps(x0, y0, x1, y1, PIN_HOVER);
        }
    }
}

fn bool_flag(value: bool) -> u8 {
    if value {
        1
    } else {
        0
    }
}

fn format_hover_param_target(target: Option<HoverParamTarget>) -> String {
    target.map_or_else(
        || "-".to_string(),
        |target| format!("n{}:p{}", target.node_id, target.param_index),
    )
}

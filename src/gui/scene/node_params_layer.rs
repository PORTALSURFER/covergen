use super::*;

impl SceneBuilder {
    pub(super) fn push_node_params(&mut self, node: &ProjectNode, state: &PreviewState) {
        if node.param_count() == 0 {
            return;
        }
        let mut label_scratch = std::mem::take(&mut self.label_scratch);
        let mut fitted_label_scratch = std::mem::take(&mut self.fitted_label_scratch);
        for (index, row) in node.param_views().enumerate() {
            let Some(row_world) = node_param_row_rect(node, index) else {
                continue;
            };
            let row_rect = graph_rect_to_panel(row_world, state);
            let Some(value_world) = node_param_value_rect(node, index) else {
                continue;
            };
            let value_rect = graph_rect_to_panel(value_world, state);
            if row.selected {
                self.push_rect(
                    Rect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h),
                    PARAM_SELECTED,
                );
            }
            let bind_hover = state
                .hover_param_target
                .map(|target| target.node_id == node.id() && target.param_index == index)
                .unwrap_or(false);
            let soft_hover = state
                .hover_param
                .map(|target| target.node_id == node.id() && target.param_index == index)
                .unwrap_or(false);
            if bind_hover {
                self.push_rect(
                    Rect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h),
                    PARAM_BIND_HOVER,
                );
            } else if soft_hover {
                self.push_rect(
                    Rect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h),
                    PARAM_SOFT_HOVER,
                );
            }
            label_scratch.clear();
            label_scratch.push_str(row.label);
            if row.bound {
                label_scratch.push_str(" *");
            }
            let label_x = row_rect.x + 4;
            let label_max_w = (value_rect.x - label_x - 4).max(0);
            let fitted_label = self.fit_graph_text_into(
                label_scratch.as_str(),
                label_max_w,
                state,
                &mut fitted_label_scratch,
            );
            let label_rect = Rect::new(label_x, row_rect.y, label_max_w, row_rect.h);
            let bound_color = if row.bound {
                PARAM_EDGE_COLOR
            } else {
                NODE_TEXT
            };
            self.push_graph_text_in_rect(label_rect, 0, fitted_label, bound_color, state);
            self.push_rect(
                value_rect,
                if row.action_button {
                    if soft_hover {
                        PARAM_ACTION_BG_HOVER
                    } else {
                        PARAM_ACTION_BG
                    }
                } else {
                    PARAM_VALUE_BG
                },
            );
            let alt_hover = state
                .hover_alt_param
                .map(|target| target.node_id == node.id() && target.param_index == index)
                .unwrap_or(false);
            let editing = state
                .param_edit
                .as_ref()
                .map(|edit| edit.node_id == node.id() && edit.param_index == index)
                .unwrap_or(false);
            if row.action_button {
                self.push_graph_text_in_rect(value_rect, 4, row.value_text, NODE_TEXT, state);
            } else {
                if alt_hover {
                    self.push_rect(value_rect, PARAM_VALUE_ALT_HOVER);
                }
                if soft_hover && !alt_hover && !editing {
                    self.push_rect(value_rect, PARAM_VALUE_SOFT_HOVER);
                }
                let active_edit = state
                    .param_edit
                    .as_ref()
                    .filter(|edit| edit.node_id == node.id() && edit.param_index == index);
                let value_text = active_edit
                    .map(|edit| edit.buffer.as_str())
                    .unwrap_or(row.value_text);
                self.push_value_editor_text(
                    value_rect,
                    value_text,
                    active_edit,
                    bound_color,
                    state,
                );
                if row.dropdown {
                    let arrow_y = value_rect.y + value_rect.h / 2;
                    let arrow_x = value_rect.x + value_rect.w - 8;
                    self.push_line(arrow_x - 3, arrow_y - 1, arrow_x, arrow_y + 2, bound_color);
                    self.push_line(arrow_x, arrow_y + 2, arrow_x + 3, arrow_y - 1, bound_color);
                }
            }
            self.push_border(
                value_rect,
                if row.action_button {
                    if soft_hover {
                        PARAM_VALUE_ACTIVE
                    } else {
                        PARAM_VALUE_BORDER
                    }
                } else if editing || alt_hover {
                    PARAM_VALUE_ACTIVE
                } else if soft_hover {
                    PARAM_VALUE_SOFT_BORDER
                } else if row.bound {
                    PARAM_EDGE_COLOR
                } else {
                    PARAM_VALUE_BORDER
                },
            );
        }
        self.label_scratch = label_scratch;
        self.fitted_label_scratch = fitted_label_scratch;
    }
}

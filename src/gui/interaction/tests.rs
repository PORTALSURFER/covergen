use super::collect_graph_node_obstacles;
use super::drag::point_to_segment_distance_sq;
use super::graph_rect_to_panel;
use super::{
    apply_preview_actions, backspace_param_text, can_append_param_char, handle_add_menu_input,
    handle_alt_param_drag, handle_delete_selected_nodes, handle_drag_input, handle_help_input,
    handle_link_cut, handle_main_export_menu_input, handle_node_open_toggle,
    handle_param_edit_input, handle_param_wheel_input, handle_right_selection,
    handle_timeline_input, handle_wire_input, insert_param_char, marquee_moved,
    move_param_cursor_left, move_param_cursor_right, rects_overlap, segments_intersect,
    step_timeline_if_running, update_hover_state, AddNodeMenuEntry, InteractionFrameContext,
    RightMarqueeState, NODE_OVERLAP_SNAP_GAP_PX, PARAM_WIRE_ENTRY_TAIL_PX, PARAM_WIRE_EXIT_TAIL_PX,
};
use crate::gui::geometry::Rect;
use crate::gui::project::{
    collapsed_param_entry_pin_center, input_pin_center, node_param_dropdown_rect,
    node_param_row_rect, node_param_value_rect, output_pin_center, AddNodeCategory, GuiProject,
    ProjectNodeKind, NODE_PARAM_DROPDOWN_ROW_HEIGHT, NODE_WIDTH,
};
use crate::gui::scene::wire_route;
use crate::gui::state::{
    add_node_options, AddNodeMenuState, DragState, ExportMenuState, HoverInsertLink,
    HoverParamTarget, InputSnapshot, LinkCutState, ParamEditState, PendingAppAction, PreviewState,
    WireDragState,
};
use crate::gui::timeline::{editor_panel_height, timeline_control_layout, timeline_rect};
use crate::runtime_config::V2Config;
use std::time::Duration;

mod drag_and_insert;
mod dropdown_and_timeline;
mod foundations;
mod help_and_export;
mod link_cut_and_add_menu;
mod wire_binding_drop;
mod wire_binding_hover;

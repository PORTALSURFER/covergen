//! Shared GUI session state and per-frame input models.

use crate::runtime_config::V2Config;

mod add_menu;

pub(crate) use add_menu::{
    AddNodeCategory, AddNodeMenuEntry, AddNodeMenuState, ADD_NODE_OPTIONS, MENU_BLOCK_GAP,
    MENU_INNER_PADDING,
};

/// Snapshot of one frame's input state.
#[derive(Clone, Debug, Default)]
pub(crate) struct InputSnapshot {
    pub(crate) mouse_pos: Option<(i32, i32)>,
    pub(crate) left_down: bool,
    pub(crate) left_clicked: bool,
    pub(crate) right_down: bool,
    pub(crate) right_clicked: bool,
    pub(crate) alt_down: bool,
    pub(crate) shift_down: bool,
    pub(crate) middle_down: bool,
    pub(crate) middle_clicked: bool,
    pub(crate) wheel_lines_y: f32,
    pub(crate) toggle_pause: bool,
    pub(crate) new_project: bool,
    pub(crate) focus_all: bool,
    pub(crate) toggle_node_open: bool,
    pub(crate) toggle_add_menu: bool,
    pub(crate) menu_up: bool,
    pub(crate) menu_down: bool,
    pub(crate) param_dec: bool,
    pub(crate) param_inc: bool,
    pub(crate) menu_accept: bool,
    pub(crate) typed_text: String,
    pub(crate) param_backspace: bool,
    pub(crate) param_delete: bool,
    pub(crate) param_select_all: bool,
    pub(crate) param_commit: bool,
    pub(crate) param_cancel: bool,
}

/// Active node drag state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DragState {
    pub(crate) node_id: u32,
    pub(crate) offset_x: i32,
    pub(crate) offset_y: i32,
}

/// Active wire-drag state from a source output pin.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WireDragState {
    pub(crate) source_node_id: u32,
    pub(crate) cursor_x: i32,
    pub(crate) cursor_y: i32,
}

/// Hovered parameter target while dragging a signal-binding wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HoverParamTarget {
    pub(crate) node_id: u32,
    pub(crate) param_index: usize,
}

/// Active alt-drag line used to cut links that intersect it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LinkCutState {
    pub(crate) start_x: i32,
    pub(crate) start_y: i32,
    pub(crate) cursor_x: i32,
    pub(crate) cursor_y: i32,
}

/// Active middle-mouse panning state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PanDragState {
    pub(crate) last_x: i32,
    pub(crate) last_y: i32,
}

/// Active right-drag marquee selection box.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RightMarqueeState {
    pub(crate) start_x: i32,
    pub(crate) start_y: i32,
    pub(crate) cursor_x: i32,
    pub(crate) cursor_y: i32,
}

/// Active parameter text-edit session for one node parameter.
#[derive(Clone, Debug)]
pub(crate) struct ParamEditState {
    pub(crate) node_id: u32,
    pub(crate) param_index: usize,
    pub(crate) buffer: String,
    pub(crate) cursor: usize,
    pub(crate) anchor: usize,
}

/// Active dropdown session for one node parameter.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ParamDropdownState {
    pub(crate) node_id: u32,
    pub(crate) param_index: usize,
}

/// Runtime animation/editor state for one GUI session.
#[derive(Clone, Debug)]
pub(crate) struct PreviewState {
    pub(crate) frame_index: u32,
    pub(crate) timeline_accum_secs: f32,
    pub(crate) paused: bool,
    pub(crate) avg_fps: f32,
    pub(crate) prev_left_down: bool,
    pub(crate) drag: Option<DragState>,
    pub(crate) wire_drag: Option<WireDragState>,
    pub(crate) link_cut: Option<LinkCutState>,
    pub(crate) pan_drag: Option<PanDragState>,
    pub(crate) right_marquee: Option<RightMarqueeState>,
    pub(crate) param_edit: Option<ParamEditState>,
    pub(crate) param_dropdown: Option<ParamDropdownState>,
    pub(crate) selected_nodes: Vec<u32>,
    pub(crate) pan_x: f32,
    pub(crate) pan_y: f32,
    pub(crate) zoom: f32,
    pub(crate) menu: AddNodeMenuState,
    pub(crate) active_node: Option<u32>,
    pub(crate) hover_node: Option<u32>,
    pub(crate) hover_output_pin: Option<u32>,
    pub(crate) hover_input_pin: Option<u32>,
    pub(crate) hover_param_target: Option<HoverParamTarget>,
    pub(crate) hover_dropdown_item: Option<usize>,
    /// Node ids auto-expanded while dragging a signal bind wire.
    pub(crate) auto_expanded_binding_nodes: Vec<u32>,
    pub(crate) hover_menu_item: Option<usize>,
}

impl PreviewState {
    /// Create initial GUI state for one run.
    pub(crate) fn new(_config: &V2Config) -> Self {
        Self {
            frame_index: 0,
            timeline_accum_secs: 0.0,
            paused: false,
            avg_fps: 0.0,
            prev_left_down: false,
            drag: None,
            wire_drag: None,
            link_cut: None,
            pan_drag: None,
            right_marquee: None,
            param_edit: None,
            param_dropdown: None,
            selected_nodes: Vec::new(),
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            menu: AddNodeMenuState::closed(),
            active_node: None,
            hover_node: None,
            hover_output_pin: None,
            hover_input_pin: None,
            hover_param_target: None,
            hover_dropdown_item: None,
            auto_expanded_binding_nodes: Vec::new(),
            hover_menu_item: None,
        }
    }
}

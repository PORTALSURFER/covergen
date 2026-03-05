//! Shared GUI session state and per-frame input models.

use super::help::HelpModalContent;
use crate::runtime_config::V2Config;

mod add_menu;
mod main_menu;
mod popup_list;

pub(crate) use add_menu::{
    AddNodeCategory, AddNodeMenuEntry, AddNodeMenuState, ADD_NODE_OPTIONS, MENU_BLOCK_GAP,
    MENU_INNER_PADDING,
};
pub(crate) use main_menu::{
    ExportMenuItem, ExportMenuState, MainMenuItem, MainMenuState, MAIN_MENU_WIDTH,
};

/// Pending app-level action requested by menu interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingAppAction {
    SaveProject,
    LoadProject,
    StartExport,
    StopExport,
    ResetFeedback {
        feedback_node_id: u32,
        accumulation_texture_node_id: Option<u32>,
    },
    Exit,
}

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
    pub(crate) open_help: bool,
    pub(crate) toggle_node_open: bool,
    pub(crate) toggle_add_menu: bool,
    pub(crate) toggle_main_menu: bool,
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

/// Scoped invalidation epochs for retained GUI subtrees.
///
/// Each epoch bumps only when its subtree dependencies changed, so retained
/// scene layers and tex evaluation can skip hash polling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GuiInvalidation {
    pub(crate) nodes: u64,
    pub(crate) wires: u64,
    pub(crate) overlays: u64,
    pub(crate) timeline: u64,
    pub(crate) tex_eval: u64,
}

impl GuiInvalidation {
    /// Return initial epochs with all retained subtrees marked dirty once.
    pub(crate) const fn initial_dirty() -> Self {
        Self {
            nodes: 1,
            wires: 1,
            overlays: 1,
            timeline: 1,
            tex_eval: 1,
        }
    }

    /// Mark all retained scene subtrees and tex evaluation as dirty.
    pub(crate) fn invalidate_all(&mut self) {
        self.invalidate_nodes();
        self.invalidate_wires();
        self.invalidate_overlays();
        self.invalidate_timeline();
        self.invalidate_tex_eval();
    }

    /// Mark node-card subtree dirty.
    pub(crate) fn invalidate_nodes(&mut self) {
        self.nodes = self.nodes.wrapping_add(1);
    }

    /// Mark wire/edge subtree dirty.
    pub(crate) fn invalidate_wires(&mut self) {
        self.wires = self.wires.wrapping_add(1);
    }

    /// Mark overlay/menu/dropdown subtree dirty.
    pub(crate) fn invalidate_overlays(&mut self) {
        self.overlays = self.overlays.wrapping_add(1);
    }

    /// Mark timeline subtree dirty.
    pub(crate) fn invalidate_timeline(&mut self) {
        self.timeline = self.timeline.wrapping_add(1);
    }

    /// Mark tex evaluation subtree dirty.
    pub(crate) fn invalidate_tex_eval(&mut self) {
        self.tex_eval = self.tex_eval.wrapping_add(1);
    }
}

/// Active node drag state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DragState {
    pub(crate) node_id: u32,
    pub(crate) offset_x: i32,
    pub(crate) offset_y: i32,
    pub(crate) origin_x: i32,
    pub(crate) origin_y: i32,
}

/// Active wire-drag state from a source output pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Hovered primary link target while dragging a node to insert on a wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HoverInsertLink {
    pub(crate) source_id: u32,
    pub(crate) target_id: u32,
}

/// Active alt-drag line used to cut links that intersect it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LinkCutState {
    pub(crate) start_x: i32,
    pub(crate) start_y: i32,
    pub(crate) cursor_x: i32,
    pub(crate) cursor_y: i32,
}

/// Active middle-mouse panning state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PanDragState {
    pub(crate) last_x: i32,
    pub(crate) last_y: i32,
}

/// Active export-popup drag state anchored to the title bar grab offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PopupDragState {
    pub(crate) offset_x: i32,
    pub(crate) offset_y: i32,
}

/// Active right-drag marquee selection box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RightMarqueeState {
    pub(crate) start_x: i32,
    pub(crate) start_y: i32,
    pub(crate) cursor_x: i32,
    pub(crate) cursor_y: i32,
}

/// Active parameter text-edit session for one node parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParamEditState {
    pub(crate) node_id: u32,
    pub(crate) param_index: usize,
    pub(crate) buffer: String,
    pub(crate) cursor: usize,
    pub(crate) anchor: usize,
}

/// Active Alt+drag parameter scrub session for one numeric parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ParamScrubState {
    pub(crate) node_id: u32,
    pub(crate) param_index: usize,
    pub(crate) last_mouse_y: i32,
    pub(crate) pixel_remainder: f32,
}

/// Active dropdown session for one node parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParamDropdownState {
    pub(crate) node_id: u32,
    pub(crate) param_index: usize,
}

/// Active text-edit session for timeline value widgets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TimelineBpmEditState {
    pub(crate) buffer: String,
    pub(crate) cursor: usize,
    pub(crate) anchor: usize,
}

/// Runtime animation/editor state for one GUI session.
#[derive(Clone, Debug)]
pub(crate) struct PreviewState {
    pub(crate) frame_index: u32,
    pub(crate) timeline_accum_secs: f32,
    pub(crate) paused: bool,
    pub(crate) timeline_scrub_active: bool,
    pub(crate) timeline_volume_drag_active: bool,
    pub(crate) avg_fps: f32,
    pub(crate) prev_left_down: bool,
    pub(crate) drag: Option<DragState>,
    pub(crate) wire_drag: Option<WireDragState>,
    pub(crate) link_cut: Option<LinkCutState>,
    pub(crate) pan_drag: Option<PanDragState>,
    pub(crate) export_menu_drag: Option<PopupDragState>,
    pub(crate) right_marquee: Option<RightMarqueeState>,
    pub(crate) param_edit: Option<ParamEditState>,
    pub(crate) param_scrub: Option<ParamScrubState>,
    pub(crate) timeline_bpm_edit: Option<TimelineBpmEditState>,
    pub(crate) timeline_bar_edit: Option<TimelineBpmEditState>,
    pub(crate) param_dropdown: Option<ParamDropdownState>,
    pub(crate) selected_nodes: Vec<u32>,
    pub(crate) pan_x: f32,
    pub(crate) pan_y: f32,
    pub(crate) zoom: f32,
    pub(crate) menu: AddNodeMenuState,
    pub(crate) main_menu: MainMenuState,
    pub(crate) export_menu: ExportMenuState,
    pub(crate) active_node: Option<u32>,
    pub(crate) hover_node: Option<u32>,
    pub(crate) hover_output_pin: Option<u32>,
    pub(crate) hover_input_pin: Option<u32>,
    pub(crate) hover_param: Option<HoverParamTarget>,
    pub(crate) hover_param_target: Option<HoverParamTarget>,
    pub(crate) hover_alt_param: Option<HoverParamTarget>,
    pub(crate) hover_insert_link: Option<HoverInsertLink>,
    pub(crate) hover_dropdown_item: Option<usize>,
    /// Node ids auto-expanded while dragging signal/texture parameter bind wires.
    pub(crate) auto_expanded_binding_nodes: Vec<u32>,
    pub(crate) hover_menu_item: Option<usize>,
    pub(crate) hover_main_menu_item: Option<usize>,
    pub(crate) hover_export_menu_item: Option<usize>,
    pub(crate) hover_export_menu_close: bool,
    pub(crate) pending_app_action: Option<PendingAppAction>,
    pub(crate) request_new_project: bool,
    pub(crate) help_modal: Option<HelpModalContent>,
    /// Last processed `Alt` modifier state for interaction debug overlay.
    pub(crate) debug_input_alt_down: bool,
    /// Last processed left-button held state for interaction debug overlay.
    pub(crate) debug_input_left_down: bool,
    /// Last processed left-button edge click state for interaction debug overlay.
    pub(crate) debug_input_left_clicked: bool,
    pub(crate) invalidation: GuiInvalidation,
}

impl PreviewState {
    /// Create initial GUI state for one run.
    pub(crate) fn new(_config: &V2Config) -> Self {
        Self {
            frame_index: 0,
            timeline_accum_secs: 0.0,
            paused: false,
            timeline_scrub_active: false,
            timeline_volume_drag_active: false,
            avg_fps: 0.0,
            prev_left_down: false,
            drag: None,
            wire_drag: None,
            link_cut: None,
            pan_drag: None,
            export_menu_drag: None,
            right_marquee: None,
            param_edit: None,
            param_scrub: None,
            timeline_bpm_edit: None,
            timeline_bar_edit: None,
            param_dropdown: None,
            selected_nodes: Vec::new(),
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            menu: AddNodeMenuState::closed(),
            main_menu: MainMenuState::closed(),
            export_menu: ExportMenuState::closed(),
            active_node: None,
            hover_node: None,
            hover_output_pin: None,
            hover_input_pin: None,
            hover_param: None,
            hover_param_target: None,
            hover_alt_param: None,
            hover_insert_link: None,
            hover_dropdown_item: None,
            auto_expanded_binding_nodes: Vec::new(),
            hover_menu_item: None,
            hover_main_menu_item: None,
            hover_export_menu_item: None,
            hover_export_menu_close: false,
            pending_app_action: None,
            request_new_project: false,
            help_modal: None,
            debug_input_alt_down: false,
            debug_input_left_down: false,
            debug_input_left_clicked: false,
            invalidation: GuiInvalidation::initial_dirty(),
        }
    }
}

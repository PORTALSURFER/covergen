//! GUI theme tokens for scene rendering.
//!
//! The `AGIO` theme keeps core surfaces grayscale and reserves saturated
//! highlights for interaction and status signals.

/// Immutable color token set for GUI scene rendering.
///
/// All values are encoded as `0xAARRGGBB`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GuiTheme {
    /// Darkest preview clear color.
    pub(crate) preview_bg: u32,
    /// Main panel surface.
    pub(crate) panel_bg: u32,
    /// Default border and divider color.
    pub(crate) border: u32,
    /// Default node card body color.
    pub(crate) node_body: u32,
    /// Add-node popup background.
    pub(crate) menu_bg: u32,
    /// Header strip background.
    pub(crate) header_bg: u32,
    /// Header text color.
    pub(crate) header_text: u32,
    /// Node label text color.
    pub(crate) node_text: u32,
    /// Menu label text color.
    pub(crate) menu_text: u32,
    /// Highlight for warning and drag states.
    pub(crate) highlight_warning: u32,
    /// Highlight for error-critical states.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) highlight_error: u32,
    /// Highlight for success and healthy status.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) highlight_success: u32,
    /// Highlight for active selection.
    pub(crate) highlight_selection: u32,
    /// Highlight for hover/focus indications.
    pub(crate) highlight_focus: u32,
    /// Highlight for accent strokes and links.
    pub(crate) highlight_accent: u32,
    /// Header color for `tex.solid` nodes.
    pub(crate) node_header_tex_solid: u32,
    /// Header color for `tex.circle` nodes.
    pub(crate) node_header_tex_circle: u32,
    /// Header color for `tex.transform_2d` nodes.
    pub(crate) node_header_tex_transform_2d: u32,
    /// Header color for `ctl.lfo` nodes.
    pub(crate) node_header_ctl_lfo: u32,
    /// Header color for `io.window_out` nodes.
    pub(crate) node_header_io_window_out: u32,
}

/// `AGIO` theme: grayscale foundation plus six semantic highlights.
pub(crate) const AGIO: GuiTheme = GuiTheme {
    preview_bg: 0xFF010101,
    panel_bg: 0xFF030303,
    border: 0xFF111111,
    node_body: 0xFF050505,
    menu_bg: 0xFF080808,
    header_bg: 0xFF0D0D0D,
    header_text: 0xFFCECECE,
    node_text: 0xFFB8B8B8,
    menu_text: 0xFFDADADA,
    highlight_warning: 0xFFF59E0B,
    highlight_error: 0xFFEF4444,
    highlight_success: 0xFF22C55E,
    highlight_selection: 0xFF3B82F6,
    highlight_focus: 0xFF06B6D4,
    highlight_accent: 0xFFA855F7,
    node_header_tex_solid: 0xFF355C7D,
    node_header_tex_circle: 0xFF2C4F6D,
    node_header_tex_transform_2d: 0xFF2A9D8F,
    node_header_ctl_lfo: 0xFFB7791F,
    node_header_io_window_out: 0xFFB16286,
};

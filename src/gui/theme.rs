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
    pub(crate) highlight_error: u32,
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
    /// Header color for `tex.source_noise` nodes.
    pub(crate) node_header_tex_source_noise: u32,
    /// Header color for `buf.sphere` nodes.
    pub(crate) node_header_buf_sphere: u32,
    /// Header color for `buf.circle_nurbs` nodes.
    pub(crate) node_header_buf_circle_nurbs: u32,
    /// Header color for `buf.noise` nodes.
    pub(crate) node_header_buf_noise: u32,
    /// Header color for `tex.transform_2d` nodes.
    pub(crate) node_header_tex_transform_2d: u32,
    /// Header color for `tex.level` nodes.
    pub(crate) node_header_tex_level: u32,
    /// Header color for `tex.mask` nodes.
    pub(crate) node_header_tex_mask: u32,
    /// Header color for `tex.tone_map` nodes.
    pub(crate) node_header_tex_tone_map: u32,
    /// Header color for `tex.feedback` nodes.
    pub(crate) node_header_tex_feedback: u32,
    /// Header color for `tex.reaction_diffusion` nodes.
    pub(crate) node_header_tex_reaction_diffusion: u32,
    /// Header color for `tex.domain_warp` nodes.
    pub(crate) node_header_tex_domain_warp: u32,
    /// Header color for `tex.warp_transform` nodes.
    pub(crate) node_header_tex_warp_transform: u32,
    /// Header color for `tex.post_color_tone` nodes.
    pub(crate) node_header_tex_post_color_tone: u32,
    /// Header color for `tex.post_edge_structure` nodes.
    pub(crate) node_header_tex_post_edge_structure: u32,
    /// Header color for `tex.post_blur_diffusion` nodes.
    pub(crate) node_header_tex_post_blur_diffusion: u32,
    /// Header color for `tex.post_distortion` nodes.
    pub(crate) node_header_tex_post_distortion: u32,
    /// Header color for `tex.post_temporal` nodes.
    pub(crate) node_header_tex_post_temporal: u32,
    /// Header color for `tex.post_noise_texture` nodes.
    pub(crate) node_header_tex_post_noise_texture: u32,
    /// Header color for `tex.post_lighting` nodes.
    pub(crate) node_header_tex_post_lighting: u32,
    /// Header color for `tex.post_screen_space` nodes.
    pub(crate) node_header_tex_post_screen_space: u32,
    /// Header color for `tex.post_experimental` nodes.
    pub(crate) node_header_tex_post_experimental: u32,
    /// Header color for `tex.blend` nodes.
    pub(crate) node_header_tex_blend: u32,
    /// Header color for `scene.entity` nodes.
    pub(crate) node_header_scene_entity: u32,
    /// Header color for `scene.build` nodes.
    pub(crate) node_header_scene_build: u32,
    /// Header color for `render.camera` nodes.
    pub(crate) node_header_render_camera: u32,
    /// Header color for `render.scene_pass` nodes.
    pub(crate) node_header_render_scene_pass: u32,
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
    highlight_selection: 0xFF3B82F6,
    highlight_focus: 0xFF06B6D4,
    highlight_accent: 0xFFA855F7,
    node_header_tex_solid: 0xFF355C7D,
    node_header_tex_circle: 0xFF2C4F6D,
    node_header_tex_source_noise: 0xFF435C78,
    node_header_buf_sphere: 0xFF5B4A78,
    node_header_buf_circle_nurbs: 0xFF6A557C,
    node_header_buf_noise: 0xFF7A4E68,
    node_header_tex_transform_2d: 0xFF2A9D8F,
    node_header_tex_level: 0xFF2E8B6F,
    node_header_tex_mask: 0xFF2F7463,
    node_header_tex_tone_map: 0xFF367D61,
    node_header_tex_feedback: 0xFF2F7A7A,
    node_header_tex_reaction_diffusion: 0xFF2A6F5A,
    node_header_tex_domain_warp: 0xFF365E8A,
    node_header_tex_warp_transform: 0xFF3B6F96,
    node_header_tex_post_color_tone: 0xFF4A7E66,
    node_header_tex_post_edge_structure: 0xFF4B6D77,
    node_header_tex_post_blur_diffusion: 0xFF467A8F,
    node_header_tex_post_distortion: 0xFF6A5A8F,
    node_header_tex_post_temporal: 0xFF7A5A6F,
    node_header_tex_post_noise_texture: 0xFF6A6A5A,
    node_header_tex_post_lighting: 0xFF8A6A44,
    node_header_tex_post_screen_space: 0xFF4C5E78,
    node_header_tex_post_experimental: 0xFF5A4C8A,
    node_header_tex_blend: 0xFF3A7F6C,
    node_header_scene_entity: 0xFF3F6F8F,
    node_header_scene_build: 0xFF4C6C4F,
    node_header_render_camera: 0xFF5E6A78,
    node_header_render_scene_pass: 0xFF8A5A44,
    node_header_ctl_lfo: 0xFFB7791F,
    node_header_io_window_out: 0xFFB16286,
};

//! Scene color/style constants and category/node palette helpers.

use crate::gui::geometry::Rect;
use crate::gui::project::ProjectNodeKind;
use crate::gui::state::AddNodeCategory;
use crate::gui::theme::AGIO;

use super::Color;

pub(super) const PREVIEW_BG: Color = Color::argb(AGIO.preview_bg);
pub(super) const PANEL_BG: Color = Color::argb(AGIO.panel_bg);
pub(super) const BORDER_COLOR: Color = Color::argb(AGIO.border);
pub(super) const EDGE_COLOR: Color = Color::argb(AGIO.highlight_accent);
pub(super) const EDGE_INSERT_HOVER: Color = Color::argb(AGIO.highlight_focus);
pub(super) const PARAM_EDGE_COLOR: Color = Color::argb(AGIO.highlight_error);
pub(super) const NODE_BODY: Color = Color::argb(AGIO.node_body);
pub(super) const NODE_DRAG: Color = Color::argb(AGIO.highlight_warning);
pub(super) const NODE_HOVER: Color = Color::argb(AGIO.highlight_focus);
pub(super) const NODE_SELECTED: Color = Color::argb(AGIO.highlight_selection);
pub(super) const MENU_BG: Color = Color::argb(AGIO.menu_bg);
pub(super) const MENU_SELECTED: Color = Color::argb(AGIO.highlight_selection);
pub(super) const MENU_BORDER: Color = Color::argb(AGIO.border);
pub(super) const HEADER_BG: Color = Color::argb(AGIO.header_bg);
pub(super) const HEADER_TEXT: Color = Color::argb(AGIO.header_text);
pub(super) const NODE_TEXT: Color = Color::argb(AGIO.node_text);
pub(super) const MENU_TEXT: Color = Color::argb(AGIO.menu_text);
pub(super) const MENU_CATEGORY_TEXT: Color = Color::argb(0xFFBEBEBE);
pub(super) const MENU_CATEGORY_CHIP_TEXT: Color = Color::argb(0xFF111111);
pub(super) const MENU_CATEGORY_CHIP_BORDER: Color = Color::argb(0xFF0A0A0A);
pub(super) const MENU_SEARCH_BG: Color = Color::argb(0xFF121212);
pub(super) const HELP_BACKDROP: Color = Color::argb(0x88000000);
pub(super) const HELP_PANEL_BG: Color = Color::argb(0xFF111111);
pub(super) const HELP_TITLE: Color = Color::argb(0xFFEAEAEA);
pub(super) const HELP_TEXT: Color = Color::argb(0xFFD0D0D0);
pub(super) const HELP_HINT: Color = Color::argb(0xFFA7A7A7);
pub(super) const PIN_BODY: Color = Color::argb(AGIO.highlight_selection);
pub(super) const PIN_HOVER: Color = Color::argb(AGIO.highlight_focus);
pub(super) const PARAM_SELECTED: Color = Color::argb(0x33262F3A);
pub(super) const PARAM_BIND_HOVER: Color = Color::argb(0x3342A5F5);
pub(super) const PARAM_SOFT_HOVER: Color = Color::argb(0x1A79AEE3);
pub(super) const TOGGLE_BG: Color = Color::argb(0xFF121212);
pub(super) const TOGGLE_BORDER: Color = Color::argb(AGIO.border);
pub(super) const TOGGLE_ACTIVE_BG: Color = Color::argb(0x663B82F6);
pub(super) const TOGGLE_ICON: Color = Color::argb(AGIO.menu_text);
pub(super) const PARAM_VALUE_BG: Color = Color::argb(0xFF101010);
pub(super) const PARAM_VALUE_BORDER: Color = Color::argb(AGIO.border);
pub(super) const PARAM_VALUE_ACTIVE: Color = Color::argb(AGIO.highlight_focus);
pub(super) const PARAM_VALUE_SOFT_HOVER: Color = Color::argb(0x166AA7D8);
pub(super) const PARAM_VALUE_SOFT_BORDER: Color = Color::argb(0xFF4D6175);
pub(super) const PARAM_VALUE_ALT_HOVER: Color = Color::argb(0x3342A5F5);
pub(super) const PARAM_ACTION_BG: Color = Color::argb(0xFF152029);
pub(super) const PARAM_ACTION_BG_HOVER: Color = Color::argb(0xFF1E3140);
pub(super) const PARAM_VALUE_SELECTION: Color = Color::argb(0x664A88D9);
pub(super) const PARAM_VALUE_CARET: Color = Color::argb(0xFFE2E2E2);
pub(super) const PARAM_DROPDOWN_BG: Color = Color::argb(0xFF0E0E0E);
pub(super) const PARAM_DROPDOWN_SELECTED: Color = Color::argb(0x663B82F6);
pub(super) const PARAM_DROPDOWN_HOVER: Color = Color::argb(0x3342A5F5);
pub(super) const NODE_SIGNAL_SCOPE_BG: Color = Color::argb(0x1A4A88D9);
pub(super) const NODE_SIGNAL_SCOPE_BORDER: Color = Color::argb(0x664A88D9);
pub(super) const NODE_SIGNAL_SCOPE_GUIDE_ZERO: Color = Color::argb(0x4466A2D9);
pub(super) const NODE_SIGNAL_SCOPE_GUIDE_ONE: Color = Color::argb(0x3381C784);
pub(super) const NODE_SIGNAL_SCOPE_WAVE: Color = Color::argb(0xFF9ED0FF);
pub(super) const CUT_EDGE_COLOR: Color = Color::argb(AGIO.highlight_warning);
pub(super) const CUT_LINE_COLOR: Color = Color::argb(AGIO.highlight_warning);
pub(super) const MARQUEE_FILL: Color = Color::argb(0x223B82F6);
pub(super) const MARQUEE_BORDER: Color = Color::argb(AGIO.highlight_selection);
pub(super) const TIMELINE_BG: Color = Color::argb(0xFF101010);
pub(super) const TIMELINE_BORDER: Color = Color::argb(AGIO.border);
pub(super) const TIMELINE_TRACK_BG: Color = Color::argb(0xFF171717);
pub(super) const TIMELINE_TRACK_FILL: Color = Color::argb(AGIO.highlight_selection);
pub(super) const TIMELINE_BTN_ACTIVE: Color = Color::argb(0x553B82F6);
pub(super) const TIMELINE_BTN_IDLE: Color = Color::argb(0xFF171717);
pub(super) const TIMELINE_TEXT: Color = Color::argb(0xFFD5D5D5);
pub(super) const TIMELINE_TEXT_MUTED: Color = Color::argb(0xFF8D8D8D);
pub(super) const TIMELINE_TRACK_BG_MUTED: Color = Color::argb(0xFF131313);
pub(super) const TIMELINE_BEAT_ON: Color = Color::argb(0xFF63E06C);
pub(super) const GRAPH_TEXT_HIDE_ZOOM: f32 = 0.58;

/// Return the header color assigned to one node kind.
pub(super) fn node_top_color(kind: ProjectNodeKind) -> Color {
    match kind {
        ProjectNodeKind::TexSolid => Color::argb(AGIO.node_header_tex_solid),
        ProjectNodeKind::TexCircle => Color::argb(AGIO.node_header_tex_circle),
        ProjectNodeKind::TexSourceNoise => Color::argb(AGIO.node_header_tex_source_noise),
        ProjectNodeKind::BufSphere => Color::argb(AGIO.node_header_buf_sphere),
        ProjectNodeKind::BufCircleNurbs => Color::argb(AGIO.node_header_buf_circle_nurbs),
        ProjectNodeKind::BufNoise => Color::argb(AGIO.node_header_buf_noise),
        ProjectNodeKind::TexTransform2D => Color::argb(AGIO.node_header_tex_transform_2d),
        ProjectNodeKind::TexLevel => Color::argb(AGIO.node_header_tex_level),
        ProjectNodeKind::TexMask => Color::argb(AGIO.node_header_tex_mask),
        ProjectNodeKind::TexToneMap => Color::argb(AGIO.node_header_tex_tone_map),
        ProjectNodeKind::TexFeedback => Color::argb(AGIO.node_header_tex_feedback),
        ProjectNodeKind::TexReactionDiffusion => {
            Color::argb(AGIO.node_header_tex_reaction_diffusion)
        }
        ProjectNodeKind::TexDomainWarp => Color::argb(AGIO.node_header_tex_domain_warp),
        ProjectNodeKind::TexWarpTransform => Color::argb(AGIO.node_header_tex_warp_transform),
        ProjectNodeKind::TexPostColorTone => Color::argb(AGIO.node_header_tex_post_color_tone),
        ProjectNodeKind::TexPostEdgeStructure => {
            Color::argb(AGIO.node_header_tex_post_edge_structure)
        }
        ProjectNodeKind::TexPostBlurDiffusion => {
            Color::argb(AGIO.node_header_tex_post_blur_diffusion)
        }
        ProjectNodeKind::TexPostDistortion => Color::argb(AGIO.node_header_tex_post_distortion),
        ProjectNodeKind::TexPostTemporal => Color::argb(AGIO.node_header_tex_post_temporal),
        ProjectNodeKind::TexPostNoiseTexture => {
            Color::argb(AGIO.node_header_tex_post_noise_texture)
        }
        ProjectNodeKind::TexPostLighting => Color::argb(AGIO.node_header_tex_post_lighting),
        ProjectNodeKind::TexPostScreenSpace => Color::argb(AGIO.node_header_tex_post_screen_space),
        ProjectNodeKind::TexPostExperimental => Color::argb(AGIO.node_header_tex_post_experimental),
        ProjectNodeKind::TexBlend => Color::argb(AGIO.node_header_tex_blend),
        ProjectNodeKind::SceneEntity => Color::argb(AGIO.node_header_scene_entity),
        ProjectNodeKind::SceneBuild => Color::argb(AGIO.node_header_scene_build),
        ProjectNodeKind::RenderCamera => Color::argb(AGIO.node_header_render_camera),
        ProjectNodeKind::RenderScenePass => Color::argb(AGIO.node_header_render_scene_pass),
        ProjectNodeKind::CtlLfo => Color::argb(AGIO.node_header_ctl_lfo),
        ProjectNodeKind::IoWindowOut => Color::argb(AGIO.node_header_io_window_out),
    }
}

/// Return one menu accent color for an add-node category.
pub(super) fn category_menu_color(category: AddNodeCategory) -> Color {
    match category {
        AddNodeCategory::Texture => Color::argb(AGIO.node_header_tex_solid),
        AddNodeCategory::Buffer => Color::argb(AGIO.node_header_buf_sphere),
        AddNodeCategory::Scene => Color::argb(AGIO.node_header_scene_entity),
        AddNodeCategory::Render => Color::argb(AGIO.node_header_render_scene_pass),
        AddNodeCategory::Control => Color::argb(AGIO.node_header_ctl_lfo),
        AddNodeCategory::Io => Color::argb(AGIO.node_header_io_window_out),
    }
}

/// Return the rounded chip rectangle inside one category menu row.
pub(super) fn category_chip_rect(item: Rect) -> Rect {
    let chip_w = (item.w - 12).max(58);
    let chip_h = (item.h - 2).max(16);
    Rect::new(item.x + 6, item.y + ((item.h - chip_h) / 2), chip_w, chip_h)
}

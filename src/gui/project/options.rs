//! Static dropdown option tables for GUI node parameters.
//!
//! These constants are kept separate from the main project model to keep
//! `project.rs` focused on graph data structures and state transitions.

use super::NodeParamOption;

/// Arc style options exposed by the `buf.circle_nurbs` node.
pub(super) const BUF_CIRCLE_ARC_STYLE_OPTIONS: [NodeParamOption; 2] = [
    NodeParamOption {
        label: "closed",
        value: 0.0,
    },
    NodeParamOption {
        label: "open_arc",
        value: 1.0,
    },
];
/// Temporal animation modes exposed by the `buf.noise` node.
pub(super) const BUF_NOISE_LOOP_MODE_OPTIONS: [NodeParamOption; 2] = [
    NodeParamOption {
        label: "free",
        value: 0.0,
    },
    NodeParamOption {
        label: "loop",
        value: 1.0,
    },
];
/// Background compositing modes exposed by the `render.scene_pass` node.
pub(super) const SCENE_PASS_BG_MODE_OPTIONS: [NodeParamOption; 2] = [
    NodeParamOption {
        label: "with_bg",
        value: 0.0,
    },
    NodeParamOption {
        label: "alpha_clip",
        value: 1.0,
    },
];
/// Timing modes exposed by the `ctl.lfo` node.
pub(super) const LFO_SYNC_MODE_OPTIONS: [NodeParamOption; 2] = [
    NodeParamOption {
        label: "free",
        value: 0.0,
    },
    NodeParamOption {
        label: "beat",
        value: 1.0,
    },
];
/// Waveform types exposed by the `ctl.lfo` node.
pub(super) const LFO_TYPE_OPTIONS: [NodeParamOption; 5] = [
    NodeParamOption {
        label: "sine",
        value: 0.0,
    },
    NodeParamOption {
        label: "saw",
        value: 1.0,
    },
    NodeParamOption {
        label: "triangle",
        value: 2.0,
    },
    NodeParamOption {
        label: "pulse",
        value: 3.0,
    },
    NodeParamOption {
        label: "drift",
        value: 4.0,
    },
];
/// Blend/composite modes exposed by the `tex.blend` node.
pub(super) const TEX_BLEND_MODE_OPTIONS: [NodeParamOption; 9] = [
    NodeParamOption {
        label: "normal",
        value: 0.0,
    },
    NodeParamOption {
        label: "add",
        value: 1.0,
    },
    NodeParamOption {
        label: "subtract",
        value: 2.0,
    },
    NodeParamOption {
        label: "multiply",
        value: 3.0,
    },
    NodeParamOption {
        label: "screen",
        value: 4.0,
    },
    NodeParamOption {
        label: "overlay",
        value: 5.0,
    },
    NodeParamOption {
        label: "darken",
        value: 6.0,
    },
    NodeParamOption {
        label: "lighten",
        value: 7.0,
    },
    NodeParamOption {
        label: "difference",
        value: 8.0,
    },
];
/// Effect options exposed by the `tex.post_color_tone` node.
pub(super) const POST_COLOR_TONE_EFFECT_OPTIONS: [NodeParamOption; 10] = [
    NodeParamOption {
        label: "bloom",
        value: 0.0,
    },
    NodeParamOption {
        label: "tone_map",
        value: 1.0,
    },
    NodeParamOption {
        label: "grading",
        value: 2.0,
    },
    NodeParamOption {
        label: "wb_shift",
        value: 3.0,
    },
    NodeParamOption {
        label: "exposure",
        value: 4.0,
    },
    NodeParamOption {
        label: "contrast",
        value: 5.0,
    },
    NodeParamOption {
        label: "gamma",
        value: 6.0,
    },
    NodeParamOption {
        label: "vibrance",
        value: 7.0,
    },
    NodeParamOption {
        label: "posterize",
        value: 8.0,
    },
    NodeParamOption {
        label: "duotone",
        value: 9.0,
    },
];
/// Effect options exposed by the `tex.post_edge_structure` node.
pub(super) const POST_EDGE_STRUCTURE_EFFECT_OPTIONS: [NodeParamOption; 6] = [
    NodeParamOption {
        label: "edge_detect",
        value: 0.0,
    },
    NodeParamOption {
        label: "toon_edge",
        value: 1.0,
    },
    NodeParamOption {
        label: "emboss",
        value: 2.0,
    },
    NodeParamOption {
        label: "sharpen",
        value: 3.0,
    },
    NodeParamOption {
        label: "kuwahara",
        value: 4.0,
    },
    NodeParamOption {
        label: "depth_edge",
        value: 5.0,
    },
];
/// Effect options exposed by the `tex.post_blur_diffusion` node.
pub(super) const POST_BLUR_DIFFUSION_EFFECT_OPTIONS: [NodeParamOption; 7] = [
    NodeParamOption {
        label: "gaussian",
        value: 0.0,
    },
    NodeParamOption {
        label: "box",
        value: 1.0,
    },
    NodeParamOption {
        label: "kawase",
        value: 2.0,
    },
    NodeParamOption {
        label: "radial",
        value: 3.0,
    },
    NodeParamOption {
        label: "motion",
        value: 4.0,
    },
    NodeParamOption {
        label: "dof",
        value: 5.0,
    },
    NodeParamOption {
        label: "tilt_shift",
        value: 6.0,
    },
];
/// Effect options exposed by the `tex.post_distortion` node.
pub(super) const POST_DISTORTION_EFFECT_OPTIONS: [NodeParamOption; 6] = [
    NodeParamOption {
        label: "chrom_ab",
        value: 0.0,
    },
    NodeParamOption {
        label: "lens_warp",
        value: 1.0,
    },
    NodeParamOption {
        label: "heat",
        value: 2.0,
    },
    NodeParamOption {
        label: "shockwave",
        value: 3.0,
    },
    NodeParamOption {
        label: "refract",
        value: 4.0,
    },
    NodeParamOption {
        label: "glitch",
        value: 5.0,
    },
];
/// Effect options exposed by the `tex.post_temporal` node.
pub(super) const POST_TEMPORAL_EFFECT_OPTIONS: [NodeParamOption; 5] = [
    NodeParamOption {
        label: "trails",
        value: 0.0,
    },
    NodeParamOption {
        label: "feedback",
        value: 1.0,
    },
    NodeParamOption {
        label: "datamosh",
        value: 2.0,
    },
    NodeParamOption {
        label: "afterimg",
        value: 3.0,
    },
    NodeParamOption {
        label: "echo",
        value: 4.0,
    },
];
/// Effect options exposed by the `tex.post_noise_texture` node.
pub(super) const POST_NOISE_TEXTURE_EFFECT_OPTIONS: [NodeParamOption; 6] = [
    NodeParamOption {
        label: "grain",
        value: 0.0,
    },
    NodeParamOption {
        label: "dither",
        value: 1.0,
    },
    NodeParamOption {
        label: "scanline",
        value: 2.0,
    },
    NodeParamOption {
        label: "vhs",
        value: 3.0,
    },
    NodeParamOption {
        label: "pixelate",
        value: 4.0,
    },
    NodeParamOption {
        label: "mosaic",
        value: 5.0,
    },
];
/// Effect options exposed by the `tex.post_lighting` node.
pub(super) const POST_LIGHTING_EFFECT_OPTIONS: [NodeParamOption; 6] = [
    NodeParamOption {
        label: "god_rays",
        value: 0.0,
    },
    NodeParamOption {
        label: "lens_flare",
        value: 1.0,
    },
    NodeParamOption {
        label: "vignette",
        value: 2.0,
    },
    NodeParamOption {
        label: "leaks",
        value: 3.0,
    },
    NodeParamOption {
        label: "anamorph",
        value: 4.0,
    },
    NodeParamOption {
        label: "halation",
        value: 5.0,
    },
];
/// Effect options exposed by the `tex.post_screen_space` node.
pub(super) const POST_SCREEN_SPACE_EFFECT_OPTIONS: [NodeParamOption; 6] = [
    NodeParamOption {
        label: "ssao",
        value: 0.0,
    },
    NodeParamOption {
        label: "ssr",
        value: 1.0,
    },
    NodeParamOption {
        label: "ss_refract",
        value: 2.0,
    },
    NodeParamOption {
        label: "depth_fog",
        value: 3.0,
    },
    NodeParamOption {
        label: "height_fade",
        value: 4.0,
    },
    NodeParamOption {
        label: "curvature",
        value: 5.0,
    },
];
/// Effect options exposed by the `tex.post_experimental` node.
pub(super) const POST_EXPERIMENTAL_EFFECT_OPTIONS: [NodeParamOption; 9] = [
    NodeParamOption {
        label: "rd_filter",
        value: 0.0,
    },
    NodeParamOption {
        label: "cell_auto",
        value: 1.0,
    },
    NodeParamOption {
        label: "zoom_fb",
        value: 2.0,
    },
    NodeParamOption {
        label: "kaleido",
        value: 3.0,
    },
    NodeParamOption {
        label: "polar",
        value: 4.0,
    },
    NodeParamOption {
        label: "sdf_remap",
        value: 5.0,
    },
    NodeParamOption {
        label: "flow_adv",
        value: 6.0,
    },
    NodeParamOption {
        label: "fourier",
        value: 7.0,
    },
    NodeParamOption {
        label: "grad_style",
        value: 8.0,
    },
];

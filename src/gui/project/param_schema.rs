//! Canonical parameter-key schema shared by GUI project and runtime compiler.
//!
//! The editor (`project::params`) and runtime (`gui::runtime`) both rely on
//! these ordered key lists so slot resolution stays consistent.

/// Canonical keys for `tex.solid`.
pub(crate) mod solid {
    pub(crate) const KEYS: [&str; 4] = ["color_r", "color_g", "color_b", "alpha"];
}

/// Canonical keys for `tex.circle`.
pub(crate) mod circle {
    pub(crate) const CENTER_X: &str = "center_x";
    pub(crate) const CENTER_Y: &str = "center_y";
    pub(crate) const RADIUS: &str = "radius";
    pub(crate) const FEATHER: &str = "feather";
    pub(crate) const COLOR_R: &str = "color_r";
    pub(crate) const COLOR_G: &str = "color_g";
    pub(crate) const COLOR_B: &str = "color_b";
    pub(crate) const ALPHA: &str = "alpha";

    pub(crate) const KEYS: [&str; 8] = [
        CENTER_X, CENTER_Y, RADIUS, FEATHER, COLOR_R, COLOR_G, COLOR_B, ALPHA,
    ];
}

/// Canonical keys for `buf.sphere`.
pub(crate) mod sphere_buffer {
    pub(crate) const KEYS: [&str; 1] = ["radius"];
}

/// Canonical keys for `buf.circle_nurbs`.
pub(crate) mod circle_nurbs_buffer {
    pub(crate) const KEYS: [&str; 7] = [
        "radius",
        "arc_start",
        "arc_end",
        "line_width",
        "order",
        "divisions",
        "arc_style",
    ];
}

/// Canonical keys for `buf.noise`.
pub(crate) mod buffer_noise {
    pub(crate) const KEYS: [&str; 9] = [
        "amplitude",
        "frequency",
        "speed_hz",
        "phase",
        "seed",
        "twist",
        "stretch",
        "loop_cyc",
        "loop_mode",
    ];
}

/// Canonical keys for `scene.entity`.
pub(crate) mod scene_entity {
    pub(crate) const KEYS: [&str; 8] = [
        "pos_x", "pos_y", "scale", "ambient", "color_r", "color_g", "color_b", "alpha",
    ];
}

/// Canonical keys for `render.camera`.
pub(crate) mod render_camera {
    pub(crate) const KEYS: [&str; 1] = ["zoom"];
}

/// Canonical keys for `render.scene_pass`.
pub(crate) mod render_scene_pass {
    pub(crate) const RES_WIDTH: &str = "res_width";
    pub(crate) const RES_HEIGHT: &str = "res_height";
    pub(crate) const BG_MODE: &str = "bg_mode";
    pub(crate) const EDGE_SOFTNESS: &str = "edge_softness";
    pub(crate) const LIGHT_X: &str = "light_x";
    pub(crate) const LIGHT_Y: &str = "light_y";
    pub(crate) const LIGHT_Z: &str = "light_z";

    pub(crate) const KEYS: [&str; 7] = [
        RES_WIDTH,
        RES_HEIGHT,
        BG_MODE,
        EDGE_SOFTNESS,
        LIGHT_X,
        LIGHT_Y,
        LIGHT_Z,
    ];
}

/// Canonical keys for `tex.transform_2d`.
pub(crate) mod transform_2d {
    pub(crate) const BRIGHTNESS: &str = "brightness";
    pub(crate) const GAIN_R: &str = "gain_r";
    pub(crate) const GAIN_G: &str = "gain_g";
    pub(crate) const GAIN_B: &str = "gain_b";
    pub(crate) const ALPHA_MUL: &str = "alpha_mul";

    pub(crate) const KEYS: [&str; 5] = [BRIGHTNESS, GAIN_R, GAIN_G, GAIN_B, ALPHA_MUL];
}

/// Canonical keys for `tex.level`.
pub(crate) mod level {
    pub(crate) const KEYS: [&str; 5] = ["in_low", "in_high", "gamma", "out_low", "out_high"];
}

/// Canonical keys for `tex.feedback`.
pub(crate) mod feedback {
    use super::super::{
        FEEDBACK_FRAME_GAP_PARAM_KEY, FEEDBACK_HISTORY_PARAM_KEY, FEEDBACK_RESET_PARAM_KEY,
    };

    pub(crate) const MIX: &str = "feedback";

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const KEYS: [&str; 4] = [
        FEEDBACK_HISTORY_PARAM_KEY,
        MIX,
        FEEDBACK_FRAME_GAP_PARAM_KEY,
        FEEDBACK_RESET_PARAM_KEY,
    ];
}

/// Canonical keys for `tex.reaction_diffusion`.
pub(crate) mod reaction_diffusion {
    pub(crate) const KEYS: [&str; 6] = ["diff_a", "diff_b", "feed", "kill", "dt", "seed_mix"];
}

/// Canonical keys for `tex.post_*` nodes.
pub(crate) mod post_process {
    pub(crate) const KEYS: [&str; 5] = ["effect", "amount", "scale", "thresh", "speed"];
}

/// Canonical keys for `tex.blend`.
pub(crate) mod blend {
    pub(crate) const KEYS: [&str; 6] = ["blend_mode", "opacity", "bg_r", "bg_g", "bg_b", "bg_a"];
}

/// Canonical keys for `ctl.lfo`.
pub(crate) mod ctl_lfo {
    pub(crate) const RATE_HZ: &str = "rate_hz";
    pub(crate) const AMPLITUDE: &str = "amplitude";
    pub(crate) const PHASE: &str = "phase";
    pub(crate) const BIAS: &str = "bias";
    pub(crate) const SYNC_MODE: &str = "sync_mode";
    pub(crate) const BEAT_MUL: &str = "beat_mul";
    pub(crate) const LFO_TYPE: &str = "lfo_type";
    pub(crate) const SHAPE: &str = "shape";

    pub(crate) const RATE_HZ_INDEX: usize = 0;
    pub(crate) const AMPLITUDE_INDEX: usize = 1;
    pub(crate) const PHASE_INDEX: usize = 2;
    pub(crate) const BIAS_INDEX: usize = 3;
    pub(crate) const SYNC_MODE_INDEX: usize = 4;
    pub(crate) const BEAT_MUL_INDEX: usize = 5;
    pub(crate) const LFO_TYPE_INDEX: usize = 6;
    pub(crate) const SHAPE_INDEX: usize = 7;

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const KEYS: [&str; 8] = [
        RATE_HZ, AMPLITUDE, PHASE, BIAS, SYNC_MODE, BEAT_MUL, LFO_TYPE, SHAPE,
    ];
}

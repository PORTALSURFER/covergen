//! Canonical parameter-key schema shared by GUI project and runtime compiler.
//!
//! The editor (`project::params`) and runtime (`gui::runtime`) both rely on
//! these ordered key lists so slot resolution stays consistent.

/// Canonical keys for `tex.solid`.
pub(crate) mod solid {
    pub(crate) const COLOR_R: &str = "color_r";
    pub(crate) const COLOR_G: &str = "color_g";
    pub(crate) const COLOR_B: &str = "color_b";
    pub(crate) const ALPHA: &str = "alpha";

    pub(crate) const COLOR_R_INDEX: usize = 0;
    pub(crate) const COLOR_G_INDEX: usize = 1;
    pub(crate) const COLOR_B_INDEX: usize = 2;
    pub(crate) const ALPHA_INDEX: usize = 3;

    pub(crate) const KEYS: [&str; 4] = [COLOR_R, COLOR_G, COLOR_B, ALPHA];
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

    pub(crate) const CENTER_X_INDEX: usize = 0;
    pub(crate) const CENTER_Y_INDEX: usize = 1;
    pub(crate) const RADIUS_INDEX: usize = 2;
    pub(crate) const FEATHER_INDEX: usize = 3;
    pub(crate) const COLOR_R_INDEX: usize = 4;
    pub(crate) const COLOR_G_INDEX: usize = 5;
    pub(crate) const COLOR_B_INDEX: usize = 6;
    pub(crate) const ALPHA_INDEX: usize = 7;
}

/// Canonical keys for `buf.sphere`.
pub(crate) mod sphere_buffer {
    pub(crate) const RADIUS: &str = "radius";
    pub(crate) const SEGMENTS: &str = "segments";
    pub(crate) const RINGS: &str = "rings";

    pub(crate) const RADIUS_INDEX: usize = 0;
    pub(crate) const KEYS: [&str; 3] = [RADIUS, SEGMENTS, RINGS];
}

/// Canonical keys for `buf.circle_nurbs`.
pub(crate) mod circle_nurbs_buffer {
    pub(crate) const RADIUS: &str = "radius";
    pub(crate) const ARC_START: &str = "arc_start";
    pub(crate) const ARC_END: &str = "arc_end";
    pub(crate) const LINE_WIDTH: &str = "line_width";
    pub(crate) const ORDER: &str = "order";
    pub(crate) const DIVISIONS: &str = "divisions";
    pub(crate) const ARC_STYLE: &str = "arc_style";

    pub(crate) const RADIUS_INDEX: usize = 0;
    pub(crate) const ARC_START_INDEX: usize = 1;
    pub(crate) const ARC_END_INDEX: usize = 2;
    pub(crate) const LINE_WIDTH_INDEX: usize = 3;
    pub(crate) const ORDER_INDEX: usize = 4;
    pub(crate) const DIVISIONS_INDEX: usize = 5;
    pub(crate) const ARC_STYLE_INDEX: usize = 6;

    pub(crate) const KEYS: [&str; 7] = [
        RADIUS, ARC_START, ARC_END, LINE_WIDTH, ORDER, DIVISIONS, ARC_STYLE,
    ];
}

/// Canonical keys for `buf.noise`.
pub(crate) mod buffer_noise {
    pub(crate) const AMPLITUDE: &str = "amplitude";
    pub(crate) const FREQUENCY: &str = "frequency";
    pub(crate) const SPEED_HZ: &str = "speed_hz";
    pub(crate) const PHASE: &str = "phase";
    pub(crate) const SEED: &str = "seed";
    pub(crate) const TWIST: &str = "twist";
    pub(crate) const STRETCH: &str = "stretch";
    pub(crate) const LOOP_CYC: &str = "loop_cyc";
    pub(crate) const LOOP_MODE: &str = "loop_mode";

    pub(crate) const AMPLITUDE_INDEX: usize = 0;
    pub(crate) const FREQUENCY_INDEX: usize = 1;
    pub(crate) const SPEED_HZ_INDEX: usize = 2;
    pub(crate) const PHASE_INDEX: usize = 3;
    pub(crate) const SEED_INDEX: usize = 4;
    pub(crate) const TWIST_INDEX: usize = 5;
    pub(crate) const STRETCH_INDEX: usize = 6;
    pub(crate) const LOOP_CYC_INDEX: usize = 7;
    pub(crate) const LOOP_MODE_INDEX: usize = 8;

    pub(crate) const KEYS: [&str; 9] = [
        AMPLITUDE, FREQUENCY, SPEED_HZ, PHASE, SEED, TWIST, STRETCH, LOOP_CYC, LOOP_MODE,
    ];
}

/// Canonical keys for `scene.entity`.
pub(crate) mod scene_entity {
    pub(crate) const POS_X: &str = "pos_x";
    pub(crate) const POS_Y: &str = "pos_y";
    pub(crate) const SCALE: &str = "scale";
    pub(crate) const AMBIENT: &str = "ambient";
    pub(crate) const COLOR_R: &str = "color_r";
    pub(crate) const COLOR_G: &str = "color_g";
    pub(crate) const COLOR_B: &str = "color_b";
    pub(crate) const ALPHA: &str = "alpha";

    pub(crate) const POS_X_INDEX: usize = 0;
    pub(crate) const POS_Y_INDEX: usize = 1;
    pub(crate) const SCALE_INDEX: usize = 2;
    pub(crate) const AMBIENT_INDEX: usize = 3;
    pub(crate) const COLOR_R_INDEX: usize = 4;
    pub(crate) const COLOR_G_INDEX: usize = 5;
    pub(crate) const COLOR_B_INDEX: usize = 6;
    pub(crate) const ALPHA_INDEX: usize = 7;

    pub(crate) const KEYS: [&str; 8] = [
        POS_X, POS_Y, SCALE, AMBIENT, COLOR_R, COLOR_G, COLOR_B, ALPHA,
    ];
}

/// Canonical keys for `render.camera`.
pub(crate) mod render_camera {
    pub(crate) const ZOOM: &str = "zoom";
    pub(crate) const ZOOM_INDEX: usize = 0;
    pub(crate) const KEYS: [&str; 1] = [ZOOM];
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

    pub(crate) const RES_WIDTH_INDEX: usize = 0;
    pub(crate) const RES_HEIGHT_INDEX: usize = 1;
    pub(crate) const BG_MODE_INDEX: usize = 2;
    pub(crate) const EDGE_SOFTNESS_INDEX: usize = 3;
    pub(crate) const LIGHT_X_INDEX: usize = 4;
    pub(crate) const LIGHT_Y_INDEX: usize = 5;
    pub(crate) const LIGHT_Z_INDEX: usize = 6;
}

/// Canonical keys for `tex.transform_2d`.
pub(crate) mod transform_2d {
    pub(crate) const BRIGHTNESS: &str = "brightness";
    pub(crate) const GAIN_R: &str = "gain_r";
    pub(crate) const GAIN_G: &str = "gain_g";
    pub(crate) const GAIN_B: &str = "gain_b";
    pub(crate) const ALPHA_MUL: &str = "alpha_mul";

    pub(crate) const KEYS: [&str; 5] = [BRIGHTNESS, GAIN_R, GAIN_G, GAIN_B, ALPHA_MUL];

    pub(crate) const BRIGHTNESS_INDEX: usize = 0;
    pub(crate) const GAIN_R_INDEX: usize = 1;
    pub(crate) const GAIN_G_INDEX: usize = 2;
    pub(crate) const GAIN_B_INDEX: usize = 3;
    pub(crate) const ALPHA_MUL_INDEX: usize = 4;
}

/// Canonical keys for `tex.level`.
pub(crate) mod level {
    pub(crate) const IN_LOW: &str = "in_low";
    pub(crate) const IN_HIGH: &str = "in_high";
    pub(crate) const GAMMA: &str = "gamma";
    pub(crate) const OUT_LOW: &str = "out_low";
    pub(crate) const OUT_HIGH: &str = "out_high";

    pub(crate) const IN_LOW_INDEX: usize = 0;
    pub(crate) const IN_HIGH_INDEX: usize = 1;
    pub(crate) const GAMMA_INDEX: usize = 2;
    pub(crate) const OUT_LOW_INDEX: usize = 3;
    pub(crate) const OUT_HIGH_INDEX: usize = 4;

    pub(crate) const KEYS: [&str; 5] = [IN_LOW, IN_HIGH, GAMMA, OUT_LOW, OUT_HIGH];
}

/// Canonical keys for `tex.feedback`.
pub(crate) mod feedback {
    use super::super::{
        FEEDBACK_FRAME_GAP_PARAM_KEY, FEEDBACK_HISTORY_PARAM_KEY, FEEDBACK_RESET_PARAM_KEY,
        LEGACY_FEEDBACK_HISTORY_PARAM_KEY,
    };

    /// Canonical persisted key for explicit external history source bindings.
    pub(crate) const HISTORY: &str = FEEDBACK_HISTORY_PARAM_KEY;
    /// Legacy persisted key accepted only for backward-compatible loads.
    pub(crate) const LEGACY_HISTORY: &str = LEGACY_FEEDBACK_HISTORY_PARAM_KEY;
    pub(crate) const MIX: &str = "feedback";
    pub(crate) const FRAME_GAP: &str = FEEDBACK_FRAME_GAP_PARAM_KEY;
    pub(crate) const RESET: &str = FEEDBACK_RESET_PARAM_KEY;

    #[cfg(test)]
    pub(crate) const KEYS: [&str; 4] = [HISTORY, MIX, FRAME_GAP, RESET];

    /// Runtime-compiled slot order for `tex.feedback`.
    ///
    /// The canonical `HISTORY` key is evaluated first and the legacy
    /// `LEGACY_HISTORY` key is retained only as fallback compatibility.
    pub(crate) const RUNTIME_KEYS: [&str; 4] = [MIX, HISTORY, LEGACY_HISTORY, FRAME_GAP];
    pub(crate) const RUNTIME_MIX_INDEX: usize = 0;
    pub(crate) const RUNTIME_HISTORY_INDEX: usize = 1;
    pub(crate) const RUNTIME_LEGACY_HISTORY_INDEX: usize = 2;
    pub(crate) const RUNTIME_FRAME_GAP_INDEX: usize = 3;

    /// History-binding slot resolution order (canonical first, legacy second).
    pub(crate) const RUNTIME_HISTORY_INDEX_FALLBACK: [usize; 2] =
        [RUNTIME_HISTORY_INDEX, RUNTIME_LEGACY_HISTORY_INDEX];

    /// Return true when one key identifies feedback-history binding state.
    pub(crate) fn is_history_key(key: &str) -> bool {
        key == HISTORY || key == LEGACY_HISTORY
    }
}

/// Canonical keys for `tex.reaction_diffusion`.
pub(crate) mod reaction_diffusion {
    pub(crate) const DIFF_A: &str = "diff_a";
    pub(crate) const DIFF_B: &str = "diff_b";
    pub(crate) const FEED: &str = "feed";
    pub(crate) const KILL: &str = "kill";
    pub(crate) const DT: &str = "dt";
    pub(crate) const SEED_MIX: &str = "seed_mix";

    pub(crate) const DIFF_A_INDEX: usize = 0;
    pub(crate) const DIFF_B_INDEX: usize = 1;
    pub(crate) const FEED_INDEX: usize = 2;
    pub(crate) const KILL_INDEX: usize = 3;
    pub(crate) const DT_INDEX: usize = 4;
    pub(crate) const SEED_MIX_INDEX: usize = 5;

    pub(crate) const KEYS: [&str; 6] = [DIFF_A, DIFF_B, FEED, KILL, DT, SEED_MIX];
}

/// Canonical keys for `tex.post_*` nodes.
pub(crate) mod post_process {
    pub(crate) const EFFECT: &str = "effect";
    pub(crate) const AMOUNT: &str = "amount";
    pub(crate) const SCALE: &str = "scale";
    pub(crate) const THRESH: &str = "thresh";
    pub(crate) const SPEED: &str = "speed";

    pub(crate) const EFFECT_INDEX: usize = 0;
    pub(crate) const AMOUNT_INDEX: usize = 1;
    pub(crate) const SCALE_INDEX: usize = 2;
    pub(crate) const THRESH_INDEX: usize = 3;
    pub(crate) const SPEED_INDEX: usize = 4;

    pub(crate) const KEYS: [&str; 5] = [EFFECT, AMOUNT, SCALE, THRESH, SPEED];
}

/// Canonical keys for `tex.blend`.
pub(crate) mod blend {
    use super::super::BLEND_LAYER_PARAM_KEY;

    pub(crate) const LAYER: &str = BLEND_LAYER_PARAM_KEY;
    pub(crate) const MODE: &str = "blend_mode";
    pub(crate) const OPACITY: &str = "opacity";
    pub(crate) const BG_R: &str = "bg_r";
    pub(crate) const BG_G: &str = "bg_g";
    pub(crate) const BG_B: &str = "bg_b";
    pub(crate) const BG_A: &str = "bg_a";

    pub(crate) const MODE_INDEX: usize = 0;
    pub(crate) const OPACITY_INDEX: usize = 1;
    pub(crate) const BG_R_INDEX: usize = 2;
    pub(crate) const BG_G_INDEX: usize = 3;
    pub(crate) const BG_B_INDEX: usize = 4;
    pub(crate) const BG_A_INDEX: usize = 5;

    /// Editor/default-slot key order (includes optional secondary texture binding).
    #[cfg(test)]
    pub(crate) const KEYS: [&str; 7] = [LAYER, MODE, OPACITY, BG_R, BG_G, BG_B, BG_A];
    /// Runtime compile-time key order (numeric controls only, no texture target).
    pub(crate) const RUNTIME_KEYS: [&str; 6] = [MODE, OPACITY, BG_R, BG_G, BG_B, BG_A];
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

    pub(crate) const KEYS: [&str; 8] = [
        RATE_HZ, AMPLITUDE, PHASE, BIAS, SYNC_MODE, BEAT_MUL, LFO_TYPE, SHAPE,
    ];
}

#[cfg(test)]
mod tests;

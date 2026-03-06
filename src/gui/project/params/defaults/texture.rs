use super::*;

pub(super) fn tex_solid_params() -> Vec<NodeParamSlot> {
    vec![
        param("color_r", "color_r", 0.9, 0.0, 1.0, 0.01),
        param("color_g", "color_g", 0.9, 0.0, 1.0, 0.01),
        param("color_b", "color_b", 0.9, 0.0, 1.0, 0.01),
        param("alpha", "alpha", 1.0, 0.0, 1.0, 0.01),
    ]
}

pub(super) fn tex_circle_params() -> Vec<NodeParamSlot> {
    vec![
        param(
            param_schema::circle::CENTER_X,
            "center_x",
            0.5,
            0.0,
            1.0,
            0.01,
        ),
        param(
            param_schema::circle::CENTER_Y,
            "center_y",
            0.5,
            0.0,
            1.0,
            0.01,
        ),
        param(
            param_schema::circle::RADIUS,
            "radius",
            0.24,
            0.02,
            0.5,
            0.005,
        ),
        param(
            param_schema::circle::FEATHER,
            "feather",
            0.06,
            0.0,
            0.25,
            0.005,
        ),
        param(
            param_schema::circle::COLOR_R,
            "color_r",
            0.9,
            0.0,
            1.0,
            0.01,
        ),
        param(
            param_schema::circle::COLOR_G,
            "color_g",
            0.9,
            0.0,
            1.0,
            0.01,
        ),
        param(
            param_schema::circle::COLOR_B,
            "color_b",
            0.9,
            0.0,
            1.0,
            0.01,
        ),
        param(param_schema::circle::ALPHA, "alpha", 1.0, 0.0, 1.0, 0.01),
    ]
}

pub(super) fn tex_source_noise_params() -> Vec<NodeParamSlot> {
    vec![
        param(
            param_schema::source_noise::SEED,
            "seed",
            1.0,
            0.0,
            65535.0,
            1.0,
        ),
        param(
            param_schema::source_noise::SCALE,
            "scale",
            4.0,
            0.05,
            32.0,
            0.05,
        ),
        param(
            param_schema::source_noise::OCTAVES,
            "octaves",
            4.0,
            1.0,
            8.0,
            1.0,
        ),
        param(
            param_schema::source_noise::AMPLITUDE,
            "amplitude",
            1.0,
            0.0,
            2.0,
            0.01,
        ),
        param_dropdown(
            param_schema::source_noise::MODE,
            "mode",
            0,
            &TEX_SOURCE_NOISE_MODE_OPTIONS,
        ),
    ]
}

pub(super) fn tex_transform_2d_params() -> Vec<NodeParamSlot> {
    vec![
        // Keep transform as identity by default so inserting this node
        // never changes output until the user edits parameters.
        param(
            param_schema::transform_2d::BRIGHTNESS,
            "brightness",
            1.0,
            0.0,
            64.0,
            0.1,
        ),
        param(
            param_schema::transform_2d::GAIN_R,
            "gain_r",
            1.0,
            0.0,
            64.0,
            0.1,
        ),
        param(
            param_schema::transform_2d::GAIN_G,
            "gain_g",
            1.0,
            0.0,
            64.0,
            0.1,
        ),
        param(
            param_schema::transform_2d::GAIN_B,
            "gain_b",
            1.0,
            0.0,
            64.0,
            0.1,
        ),
        param(
            param_schema::transform_2d::ALPHA_MUL,
            "alpha_mul",
            1.0,
            0.0,
            64.0,
            0.1,
        ),
    ]
}

pub(super) fn tex_level_params() -> Vec<NodeParamSlot> {
    vec![
        // Keep level as identity by default so inserting this node
        // never changes output until the user edits parameters.
        param("in_low", "in_low", 0.0, 0.0, 1.0, 0.01),
        param("in_high", "in_high", 1.0, 0.0, 1.0, 0.01),
        param("gamma", "gamma", 1.0, 0.1, 8.0, 0.01),
        param("out_low", "out_low", 0.0, 0.0, 1.0, 0.01),
        param("out_high", "out_high", 1.0, 0.0, 1.0, 0.01),
    ]
}

pub(super) fn tex_mask_params() -> Vec<NodeParamSlot> {
    vec![
        param(
            param_schema::mask::THRESHOLD,
            "threshold",
            0.5,
            0.0,
            1.0,
            0.01,
        ),
        param(
            param_schema::mask::SOFTNESS,
            "softness",
            0.1,
            0.0,
            1.0,
            0.01,
        ),
        param_dropdown(
            param_schema::mask::INVERT,
            "invert",
            0,
            &TEX_MASK_INVERT_OPTIONS,
        ),
    ]
}

pub(super) fn tex_morphology_params() -> Vec<NodeParamSlot> {
    vec![
        param_dropdown(
            param_schema::morphology::MODE,
            "mode",
            0,
            &TEX_MORPHOLOGY_MODE_OPTIONS,
        ),
        param(
            param_schema::morphology::RADIUS,
            "radius",
            1.0,
            0.0,
            8.0,
            0.1,
        ),
        param(
            param_schema::morphology::AMOUNT,
            "amount",
            1.0,
            0.0,
            1.0,
            0.01,
        ),
    ]
}

pub(super) fn tex_tone_map_params() -> Vec<NodeParamSlot> {
    vec![
        param(
            param_schema::tone_map::CONTRAST,
            "contrast",
            1.0,
            1.0,
            3.0,
            0.01,
        ),
        param(
            param_schema::tone_map::LOW_PCT,
            "low_pct",
            0.0,
            0.0,
            0.9,
            0.01,
        ),
        param(
            param_schema::tone_map::HIGH_PCT,
            "high_pct",
            1.0,
            0.01,
            1.0,
            0.01,
        ),
    ]
}

pub(super) fn tex_feedback_params() -> Vec<NodeParamSlot> {
    vec![
        // Optional external accumulation-history binding for feedback.
        param_texture_target(FEEDBACK_HISTORY_PARAM_KEY, FEEDBACK_HISTORY_PARAM_LABEL),
        // History output gain for delayed feedback (`history * feedback`).
        param(param_schema::feedback::MIX, "feedback", 1.0, 0.0, 1.0, 0.01),
        // Number of skipped history-write frames between updates.
        // `0` keeps classic one-frame feedback behavior.
        param(
            FEEDBACK_FRAME_GAP_PARAM_KEY,
            FEEDBACK_FRAME_GAP_PARAM_LABEL,
            0.0,
            0.0,
            32.0,
            1.0,
        ),
        // Clears this node's feedback history buffer.
        param_action_button(
            param_schema::feedback::RESET,
            FEEDBACK_RESET_PARAM_LABEL,
            "reset",
        ),
    ]
}

pub(super) fn tex_reaction_diffusion_params() -> Vec<NodeParamSlot> {
    vec![
        // Gray-Scott diffusion coefficient for reagent A.
        param("diff_a", "diff_a", 1.0, 0.0, 2.0, 0.01),
        // Gray-Scott diffusion coefficient for reagent B.
        param("diff_b", "diff_b", 0.5, 0.0, 2.0, 0.01),
        // Feed rate that replenishes reagent A.
        param("feed", "feed", 0.055, 0.0, 0.12, 0.001),
        // Kill rate that removes reagent B.
        param("kill", "kill", 0.062, 0.0, 0.12, 0.001),
        // Integration step multiplier per frame.
        param("dt", "dt", 1.0, 0.0, 2.0, 0.01),
        // Blend amount for injecting source texture concentrations.
        param("seed_mix", "seed_mix", 0.04, 0.0, 1.0, 0.01),
    ]
}

pub(super) fn tex_domain_warp_params() -> Vec<NodeParamSlot> {
    vec![
        // Optional warp field sampled to derive domain offsets.
        param_texture_target(
            DOMAIN_WARP_TEXTURE_PARAM_KEY,
            DOMAIN_WARP_TEXTURE_PARAM_LABEL,
        ),
        param(
            param_schema::domain_warp::STRENGTH,
            "strength",
            0.28,
            0.0,
            2.0,
            0.01,
        ),
        param(
            param_schema::domain_warp::FREQUENCY,
            "freq",
            2.5,
            0.05,
            16.0,
            0.05,
        ),
        param(
            param_schema::domain_warp::ROTATION,
            "rotate",
            0.0,
            -180.0,
            180.0,
            1.0,
        ),
        param(
            param_schema::domain_warp::OCTAVES,
            "octaves",
            3.0,
            1.0,
            6.0,
            1.0,
        ),
    ]
}

pub(super) fn tex_directional_smear_params() -> Vec<NodeParamSlot> {
    vec![
        param(
            param_schema::directional_smear::ANGLE,
            "angle",
            90.0,
            -180.0,
            180.0,
            1.0,
        ),
        param(
            param_schema::directional_smear::LENGTH,
            "length",
            18.0,
            0.0,
            96.0,
            1.0,
        ),
        param(
            param_schema::directional_smear::JITTER,
            "jitter",
            0.2,
            0.0,
            1.0,
            0.01,
        ),
        param(
            param_schema::directional_smear::AMOUNT,
            "amount",
            0.5,
            0.0,
            1.0,
            0.01,
        ),
    ]
}

pub(super) fn tex_warp_transform_params() -> Vec<NodeParamSlot> {
    vec![
        param(
            param_schema::warp_transform::STRENGTH,
            "strength",
            0.5,
            0.0,
            2.4,
            0.01,
        ),
        param(
            param_schema::warp_transform::FREQUENCY,
            "freq",
            2.0,
            0.05,
            12.0,
            0.05,
        ),
        param(
            param_schema::warp_transform::PHASE,
            "phase",
            0.0,
            -64.0,
            64.0,
            0.01,
        ),
    ]
}

pub(super) fn tex_post_color_tone_params() -> Vec<NodeParamSlot> {
    post_process_params("effect", &POST_COLOR_TONE_EFFECT_OPTIONS)
}

pub(super) fn tex_post_edge_structure_params() -> Vec<NodeParamSlot> {
    post_process_params("effect", &POST_EDGE_STRUCTURE_EFFECT_OPTIONS)
}

pub(super) fn tex_post_blur_diffusion_params() -> Vec<NodeParamSlot> {
    post_process_params("effect", &POST_BLUR_DIFFUSION_EFFECT_OPTIONS)
}

pub(super) fn tex_post_distortion_params() -> Vec<NodeParamSlot> {
    post_process_params("effect", &POST_DISTORTION_EFFECT_OPTIONS)
}

pub(super) fn tex_post_temporal_params() -> Vec<NodeParamSlot> {
    post_process_params("effect", &POST_TEMPORAL_EFFECT_OPTIONS)
}

pub(super) fn tex_post_noise_texture_params() -> Vec<NodeParamSlot> {
    post_process_params("effect", &POST_NOISE_TEXTURE_EFFECT_OPTIONS)
}

pub(super) fn tex_post_lighting_params() -> Vec<NodeParamSlot> {
    post_process_params("effect", &POST_LIGHTING_EFFECT_OPTIONS)
}

pub(super) fn tex_post_screen_space_params() -> Vec<NodeParamSlot> {
    post_process_params("effect", &POST_SCREEN_SPACE_EFFECT_OPTIONS)
}

pub(super) fn tex_post_experimental_params() -> Vec<NodeParamSlot> {
    post_process_params("effect", &POST_EXPERIMENTAL_EFFECT_OPTIONS)
}

pub(super) fn tex_blend_params() -> Vec<NodeParamSlot> {
    vec![
        // Optional secondary composite input for blend operations.
        param_texture_target(BLEND_LAYER_PARAM_KEY, BLEND_LAYER_PARAM_LABEL),
        param_dropdown("blend_mode", "blend_mode", 0, &TEX_BLEND_MODE_OPTIONS),
        // Keep blend as identity by default until users increase opacity.
        param("opacity", "opacity", 0.0, 0.0, 1.0, 0.01),
        // Optional post-composite background fill color.
        param("bg_r", "bg_r", 0.0, 0.0, 1.0, 0.01),
        param("bg_g", "bg_g", 0.0, 0.0, 1.0, 0.01),
        param("bg_b", "bg_b", 0.0, 0.0, 1.0, 0.01),
        // `0` keeps the output alpha unchanged; `1` fully fills background.
        param("bg_a", "bg_a", 0.0, 0.0, 1.0, 0.01),
    ]
}

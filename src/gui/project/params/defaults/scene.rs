use super::*;

pub(super) fn scene_entity_params() -> Vec<NodeParamSlot> {
    vec![
        param("pos_x", "pos_x", 0.5, 0.0, 1.0, 0.01),
        param("pos_y", "pos_y", 0.5, 0.0, 1.0, 0.01),
        param("scale", "scale", 1.0, 0.1, 2.0, 0.01),
        param("ambient", "ambient", 0.2, 0.0, 1.0, 0.01),
        param("color_r", "color_r", 0.9, 0.0, 1.0, 0.01),
        param("color_g", "color_g", 0.9, 0.0, 1.0, 0.01),
        param("color_b", "color_b", 0.9, 0.0, 1.0, 0.01),
        param("alpha", "alpha", 1.0, 0.0, 1.0, 0.01),
    ]
}

pub(super) fn scene_build_params() -> Vec<NodeParamSlot> {
    Vec::new()
}

pub(super) fn render_camera_params() -> Vec<NodeParamSlot> {
    vec![param("zoom", "zoom", 1.0, 0.1, 8.0, 0.05)]
}

pub(super) fn render_scene_pass_params() -> Vec<NodeParamSlot> {
    vec![
        // `0` keeps project preview resolution.
        param(
            param_schema::render_scene_pass::RES_WIDTH,
            "res_width",
            0.0,
            0.0,
            8192.0,
            1.0,
        ),
        // `0` keeps project preview resolution.
        param(
            param_schema::render_scene_pass::RES_HEIGHT,
            "res_height",
            0.0,
            0.0,
            8192.0,
            1.0,
        ),
        // `with_bg` preserves the preview background clear; `alpha_clip`
        // clears transparent so only rendered scene objects remain.
        param_dropdown(
            param_schema::render_scene_pass::BG_MODE,
            "bg_mode",
            0,
            &SCENE_PASS_BG_MODE_OPTIONS,
        ),
        param(
            param_schema::render_scene_pass::EDGE_SOFTNESS,
            "edge_soft",
            0.01,
            0.0,
            0.25,
            0.005,
        ),
        param(
            param_schema::render_scene_pass::LIGHT_X,
            "light_x",
            0.4,
            -1.0,
            1.0,
            0.02,
        ),
        param(
            param_schema::render_scene_pass::LIGHT_Y,
            "light_y",
            -0.5,
            -1.0,
            1.0,
            0.02,
        ),
        param(
            param_schema::render_scene_pass::LIGHT_Z,
            "light_z",
            1.0,
            0.0,
            2.0,
            0.02,
        ),
    ]
}

pub(super) fn io_window_out_params() -> Vec<NodeParamSlot> {
    Vec::new()
}

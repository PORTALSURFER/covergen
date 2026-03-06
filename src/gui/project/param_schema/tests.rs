use super::*;
use std::collections::HashSet;

fn assert_index_map(keys: &[&str], mapping: &[(usize, &str)]) {
    for (index, key) in mapping.iter().copied() {
        assert_eq!(
            keys.get(index),
            Some(&key),
            "index {index} should map to {key}"
        );
    }
}

fn assert_unique_keys(keys: &[&str]) {
    let unique: HashSet<&str> = keys.iter().copied().collect();
    assert_eq!(unique.len(), keys.len(), "schema keys must be unique");
}

#[test]
fn schema_key_arrays_have_unique_entries() {
    let key_sets: &[&[&str]] = &[
        &solid::KEYS,
        &circle::KEYS,
        &sphere_buffer::KEYS,
        &circle_nurbs_buffer::KEYS,
        &buffer_noise::KEYS,
        &scene_entity::KEYS,
        &render_camera::KEYS,
        &render_scene_pass::KEYS,
        &transform_2d::KEYS,
        &level::KEYS,
        &source_noise::KEYS,
        &feedback::KEYS,
        &mask::KEYS,
        &morphology::KEYS,
        &feedback::RUNTIME_KEYS,
        &reaction_diffusion::KEYS,
        &domain_warp::KEYS,
        &directional_smear::KEYS,
        &warp_transform::KEYS,
        &post_process::KEYS,
        &blend::KEYS,
        &ctl_lfo::KEYS,
    ];
    for keys in key_sets.iter().copied() {
        assert_unique_keys(keys);
    }
}

#[test]
fn index_constants_match_declared_key_order() {
    assert_index_map(
        &solid::KEYS,
        &[
            (solid::COLOR_R_INDEX, solid::COLOR_R),
            (solid::COLOR_G_INDEX, solid::COLOR_G),
            (solid::COLOR_B_INDEX, solid::COLOR_B),
            (solid::ALPHA_INDEX, solid::ALPHA),
        ],
    );
    assert_index_map(
        &circle::KEYS,
        &[
            (circle::CENTER_X_INDEX, circle::CENTER_X),
            (circle::CENTER_Y_INDEX, circle::CENTER_Y),
            (circle::RADIUS_INDEX, circle::RADIUS),
            (circle::FEATHER_INDEX, circle::FEATHER),
            (circle::COLOR_R_INDEX, circle::COLOR_R),
            (circle::COLOR_G_INDEX, circle::COLOR_G),
            (circle::COLOR_B_INDEX, circle::COLOR_B),
            (circle::ALPHA_INDEX, circle::ALPHA),
        ],
    );
    assert_index_map(
        &source_noise::KEYS,
        &[
            (source_noise::SEED_INDEX, source_noise::SEED),
            (source_noise::SCALE_INDEX, source_noise::SCALE),
            (source_noise::OCTAVES_INDEX, source_noise::OCTAVES),
            (source_noise::AMPLITUDE_INDEX, source_noise::AMPLITUDE),
            (source_noise::MODE_INDEX, source_noise::MODE),
        ],
    );
    assert_index_map(
        &mask::KEYS,
        &[
            (mask::THRESHOLD_INDEX, mask::THRESHOLD),
            (mask::SOFTNESS_INDEX, mask::SOFTNESS),
            (mask::INVERT_INDEX, mask::INVERT),
        ],
    );
    assert_index_map(
        &morphology::KEYS,
        &[
            (morphology::MODE_INDEX, morphology::MODE),
            (morphology::RADIUS_INDEX, morphology::RADIUS),
            (morphology::AMOUNT_INDEX, morphology::AMOUNT),
        ],
    );
    assert_index_map(
        &render_scene_pass::KEYS,
        &[
            (
                render_scene_pass::RES_WIDTH_INDEX,
                render_scene_pass::RES_WIDTH,
            ),
            (
                render_scene_pass::RES_HEIGHT_INDEX,
                render_scene_pass::RES_HEIGHT,
            ),
            (render_scene_pass::BG_MODE_INDEX, render_scene_pass::BG_MODE),
            (
                render_scene_pass::EDGE_SOFTNESS_INDEX,
                render_scene_pass::EDGE_SOFTNESS,
            ),
            (render_scene_pass::LIGHT_X_INDEX, render_scene_pass::LIGHT_X),
            (render_scene_pass::LIGHT_Y_INDEX, render_scene_pass::LIGHT_Y),
            (render_scene_pass::LIGHT_Z_INDEX, render_scene_pass::LIGHT_Z),
        ],
    );
    assert_index_map(
        &domain_warp::KEYS,
        &[
            (domain_warp::WARP_TEXTURE_INDEX, domain_warp::WARP_TEXTURE),
            (domain_warp::STRENGTH_INDEX, domain_warp::STRENGTH),
            (domain_warp::FREQUENCY_INDEX, domain_warp::FREQUENCY),
            (domain_warp::ROTATION_INDEX, domain_warp::ROTATION),
            (domain_warp::OCTAVES_INDEX, domain_warp::OCTAVES),
        ],
    );
    assert_index_map(
        &directional_smear::KEYS,
        &[
            (directional_smear::ANGLE_INDEX, directional_smear::ANGLE),
            (directional_smear::LENGTH_INDEX, directional_smear::LENGTH),
            (directional_smear::JITTER_INDEX, directional_smear::JITTER),
            (directional_smear::AMOUNT_INDEX, directional_smear::AMOUNT),
        ],
    );
    assert_index_map(
        &warp_transform::KEYS,
        &[
            (warp_transform::STRENGTH_INDEX, warp_transform::STRENGTH),
            (warp_transform::FREQUENCY_INDEX, warp_transform::FREQUENCY),
            (warp_transform::PHASE_INDEX, warp_transform::PHASE),
        ],
    );
    assert_index_map(
        &blend::KEYS,
        &[
            (blend::MODE_INDEX, blend::MODE),
            (blend::OPACITY_INDEX, blend::OPACITY),
            (blend::BG_R_INDEX, blend::BG_R),
            (blend::BG_G_INDEX, blend::BG_G),
            (blend::BG_B_INDEX, blend::BG_B),
            (blend::BG_A_INDEX, blend::BG_A),
        ],
    );
    assert_index_map(
        &ctl_lfo::KEYS,
        &[
            (ctl_lfo::RATE_HZ_INDEX, ctl_lfo::RATE_HZ),
            (ctl_lfo::AMPLITUDE_INDEX, ctl_lfo::AMPLITUDE),
            (ctl_lfo::PHASE_INDEX, ctl_lfo::PHASE),
            (ctl_lfo::BIAS_INDEX, ctl_lfo::BIAS),
            (ctl_lfo::SYNC_MODE_INDEX, ctl_lfo::SYNC_MODE),
            (ctl_lfo::BEAT_MUL_INDEX, ctl_lfo::BEAT_MUL),
            (ctl_lfo::LFO_TYPE_INDEX, ctl_lfo::LFO_TYPE),
            (ctl_lfo::SHAPE_INDEX, ctl_lfo::SHAPE),
        ],
    );
}

#[test]
fn feedback_runtime_history_fallback_order_is_stable() {
    assert_index_map(
        &feedback::RUNTIME_KEYS,
        &[
            (feedback::RUNTIME_MIX_INDEX, feedback::MIX),
            (feedback::RUNTIME_HISTORY_INDEX, feedback::HISTORY),
            (
                feedback::RUNTIME_LEGACY_HISTORY_INDEX,
                feedback::LEGACY_HISTORY,
            ),
            (feedback::RUNTIME_FRAME_GAP_INDEX, feedback::FRAME_GAP),
        ],
    );
    assert_eq!(
        feedback::RUNTIME_HISTORY_INDEX_FALLBACK,
        [
            feedback::RUNTIME_HISTORY_INDEX,
            feedback::RUNTIME_LEGACY_HISTORY_INDEX,
        ],
    );
}

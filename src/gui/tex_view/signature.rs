//! Tex-view operation payload and structure signature hashing.

use super::TexViewerOp;

/// Hash operation structure for cache invalidation that depends on op shape/order only.
pub(super) fn ops_plan_signature(ops: &[TexViewerOp]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for op in ops {
        match *op {
            TexViewerOp::Solid { .. } => hash = fnv1a(hash, 1),
            TexViewerOp::Circle { .. } => hash = fnv1a(hash, 2),
            TexViewerOp::Sphere { .. } => hash = fnv1a(hash, 3),
            TexViewerOp::Transform { .. } => hash = fnv1a(hash, 4),
            TexViewerOp::Level { .. } => hash = fnv1a(hash, 5),
            TexViewerOp::Feedback { history, .. } => {
                hash = fnv1a(hash, 6);
                hash = hash_feedback_binding(hash, history);
            }
            TexViewerOp::ReactionDiffusion { history, .. } => {
                hash = fnv1a(hash, 7);
                hash = hash_feedback_binding(hash, history);
            }
            TexViewerOp::PostProcess {
                category, history, ..
            } => {
                hash = fnv1a(hash, 8);
                hash = fnv1a(hash, category as u64);
                hash = match history {
                    Some(binding) => hash_feedback_binding(fnv1a(hash, 1), binding),
                    None => fnv1a(hash, 0),
                };
            }
            TexViewerOp::StoreTexture { texture_node_id } => {
                hash = fnv1a(hash, 9);
                hash = fnv1a(hash, texture_node_id as u64);
            }
            TexViewerOp::Blend {
                base_texture_node_id,
                layer_texture_node_id,
                ..
            } => {
                hash = fnv1a(hash, 10);
                hash = fnv1a(hash, base_texture_node_id as u64);
                hash = fnv1a(hash, layer_texture_node_id.unwrap_or(0) as u64);
            }
        }
    }
    hash
}

/// Hash full operation payload values for uniform/bind-group update invalidation.
pub(super) fn ops_uniform_signature(ops: &[TexViewerOp]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for op in ops {
        match *op {
            TexViewerOp::Solid {
                color_r,
                color_g,
                color_b,
                alpha,
            } => {
                hash = fnv1a(hash, 1);
                hash = hash_f32(hash, color_r);
                hash = hash_f32(hash, color_g);
                hash = hash_f32(hash, color_b);
                hash = hash_f32(hash, alpha);
            }
            TexViewerOp::Circle {
                center_x,
                center_y,
                radius,
                feather,
                line_width,
                noise_amount,
                noise_freq,
                noise_phase,
                noise_twist,
                noise_stretch,
                arc_start_deg,
                arc_end_deg,
                segment_count,
                arc_open,
                color_r,
                color_g,
                color_b,
                alpha,
                alpha_clip,
            } => {
                hash = fnv1a(hash, 2);
                for value in [
                    center_x,
                    center_y,
                    radius,
                    feather,
                    line_width,
                    noise_amount,
                    noise_freq,
                    noise_phase,
                    noise_twist,
                    noise_stretch,
                    arc_start_deg,
                    arc_end_deg,
                    segment_count,
                    arc_open,
                    color_r,
                    color_g,
                    color_b,
                    alpha,
                ] {
                    hash = hash_f32(hash, value);
                }
                hash = fnv1a(hash, alpha_clip as u64);
            }
            TexViewerOp::Sphere {
                center_x,
                center_y,
                radius,
                edge_softness,
                noise_amount,
                noise_freq,
                noise_phase,
                noise_twist,
                noise_stretch,
                light_x,
                light_y,
                light_z,
                ambient,
                color_r,
                color_g,
                color_b,
                alpha,
                alpha_clip,
            } => {
                hash = fnv1a(hash, 3);
                for value in [
                    center_x,
                    center_y,
                    radius,
                    edge_softness,
                    noise_amount,
                    noise_freq,
                    noise_phase,
                    noise_twist,
                    noise_stretch,
                    light_x,
                    light_y,
                    light_z,
                    ambient,
                    color_r,
                    color_g,
                    color_b,
                    alpha,
                ] {
                    hash = hash_f32(hash, value);
                }
                hash = fnv1a(hash, alpha_clip as u64);
            }
            TexViewerOp::Transform {
                brightness,
                gain_r,
                gain_g,
                gain_b,
                alpha_mul,
            } => {
                hash = fnv1a(hash, 4);
                for value in [brightness, gain_r, gain_g, gain_b, alpha_mul] {
                    hash = hash_f32(hash, value);
                }
            }
            TexViewerOp::Level {
                in_low,
                in_high,
                gamma,
                out_low,
                out_high,
            } => {
                hash = fnv1a(hash, 5);
                for value in [in_low, in_high, gamma, out_low, out_high] {
                    hash = hash_f32(hash, value);
                }
            }
            TexViewerOp::Feedback {
                mix,
                frame_gap,
                history,
            } => {
                hash = fnv1a(hash, 6);
                hash = hash_f32(hash, mix);
                hash = fnv1a(hash, frame_gap as u64);
                hash = hash_feedback_binding(hash, history);
            }
            TexViewerOp::ReactionDiffusion {
                diffusion_a,
                diffusion_b,
                feed,
                kill,
                dt,
                seed_mix,
                history,
            } => {
                hash = fnv1a(hash, 7);
                for value in [diffusion_a, diffusion_b, feed, kill, dt, seed_mix] {
                    hash = hash_f32(hash, value);
                }
                hash = hash_feedback_binding(hash, history);
            }
            TexViewerOp::PostProcess {
                category,
                effect,
                amount,
                scale,
                threshold,
                speed,
                time,
                history,
            } => {
                hash = fnv1a(hash, 8);
                hash = fnv1a(hash, category as u64);
                for value in [effect, amount, scale, threshold, speed, time] {
                    hash = hash_f32(hash, value);
                }
                hash = match history {
                    Some(binding) => hash_feedback_binding(fnv1a(hash, 1), binding),
                    None => fnv1a(hash, 0),
                };
            }
            TexViewerOp::StoreTexture { texture_node_id } => {
                hash = fnv1a(hash, 9);
                hash = fnv1a(hash, texture_node_id as u64);
            }
            TexViewerOp::Blend {
                mode,
                opacity,
                bg_r,
                bg_g,
                bg_b,
                bg_a,
                base_texture_node_id,
                layer_texture_node_id,
            } => {
                hash = fnv1a(hash, 10);
                for value in [mode, opacity, bg_r, bg_g, bg_b, bg_a] {
                    hash = hash_f32(hash, value);
                }
                hash = fnv1a(hash, base_texture_node_id as u64);
                hash = fnv1a(hash, layer_texture_node_id.unwrap_or(0) as u64);
            }
        }
    }
    hash
}

fn hash_feedback_binding(
    mut hash: u64,
    binding: crate::gui::runtime::TexRuntimeFeedbackHistoryBinding,
) -> u64 {
    hash = match binding {
        crate::gui::runtime::TexRuntimeFeedbackHistoryBinding::Internal { feedback_node_id } => {
            fnv1a(hash, 1 ^ feedback_node_id as u64)
        }
        crate::gui::runtime::TexRuntimeFeedbackHistoryBinding::External { texture_node_id } => {
            fnv1a(hash, 2 ^ texture_node_id as u64)
        }
    };
    hash
}

fn hash_f32(hash: u64, value: f32) -> u64 {
    fnv1a(hash, value.to_bits() as u64)
}

fn fnv1a(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(0x100000001b3)
}

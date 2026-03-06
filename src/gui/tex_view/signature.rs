//! Tex-view operation payload and structure signature hashing.

use super::TexViewerOp;

/// Hash tex-view operations once and return both cache signatures.
///
/// The plan signature tracks op structure/order for execution-plan reuse while
/// the uniform signature tracks full payload values for uniform/bind-group
/// uploads. Returning both from one traversal avoids redundant hot-path scans.
pub(super) fn ops_signatures(ops: &[TexViewerOp]) -> (u64, u64) {
    let mut plan_hash = 0xcbf29ce484222325_u64;
    let mut uniform_hash = 0xcbf29ce484222325_u64;
    for op in ops {
        match *op {
            TexViewerOp::Solid {
                color_r,
                color_g,
                color_b,
                alpha,
            } => {
                plan_hash = fnv1a(plan_hash, 1);
                uniform_hash = fnv1a(uniform_hash, 1);
                uniform_hash = hash_f32(uniform_hash, color_r);
                uniform_hash = hash_f32(uniform_hash, color_g);
                uniform_hash = hash_f32(uniform_hash, color_b);
                uniform_hash = hash_f32(uniform_hash, alpha);
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
                plan_hash = fnv1a(plan_hash, 2);
                uniform_hash = fnv1a(uniform_hash, 2);
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
                    uniform_hash = hash_f32(uniform_hash, value);
                }
                uniform_hash = fnv1a(uniform_hash, alpha_clip as u64);
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
                plan_hash = fnv1a(plan_hash, 3);
                uniform_hash = fnv1a(uniform_hash, 3);
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
                    uniform_hash = hash_f32(uniform_hash, value);
                }
                uniform_hash = fnv1a(uniform_hash, alpha_clip as u64);
            }
            TexViewerOp::SourceNoise {
                seed,
                scale,
                octaves,
                amplitude,
                mode,
            } => {
                plan_hash = fnv1a(plan_hash, 11);
                uniform_hash = fnv1a(uniform_hash, 11);
                for value in [seed, scale, octaves, amplitude, mode] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
            }
            TexViewerOp::Transform {
                brightness,
                gain_r,
                gain_g,
                gain_b,
                alpha_mul,
            } => {
                plan_hash = fnv1a(plan_hash, 4);
                uniform_hash = fnv1a(uniform_hash, 4);
                for value in [brightness, gain_r, gain_g, gain_b, alpha_mul] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
            }
            TexViewerOp::Level {
                in_low,
                in_high,
                gamma,
                out_low,
                out_high,
            } => {
                plan_hash = fnv1a(plan_hash, 5);
                uniform_hash = fnv1a(uniform_hash, 5);
                for value in [in_low, in_high, gamma, out_low, out_high] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
            }
            TexViewerOp::Mask {
                threshold,
                softness,
                invert,
            } => {
                plan_hash = fnv1a(plan_hash, 12);
                uniform_hash = fnv1a(uniform_hash, 12);
                for value in [threshold, softness, invert] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
            }
            TexViewerOp::Morphology {
                mode,
                radius,
                amount,
            } => {
                plan_hash = fnv1a(plan_hash, 16);
                uniform_hash = fnv1a(uniform_hash, 16);
                for value in [mode, radius, amount] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
            }
            TexViewerOp::ToneMap {
                contrast,
                low_pct,
                high_pct,
            } => {
                plan_hash = fnv1a(plan_hash, 13);
                uniform_hash = fnv1a(uniform_hash, 13);
                for value in [contrast, low_pct, high_pct] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
            }
            TexViewerOp::Feedback {
                mix,
                frame_gap,
                history,
            } => {
                plan_hash = fnv1a(plan_hash, 6);
                plan_hash = hash_feedback_binding(plan_hash, history);
                uniform_hash = fnv1a(uniform_hash, 6);
                uniform_hash = hash_f32(uniform_hash, mix);
                uniform_hash = fnv1a(uniform_hash, frame_gap as u64);
                uniform_hash = hash_feedback_binding(uniform_hash, history);
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
                plan_hash = fnv1a(plan_hash, 7);
                plan_hash = hash_feedback_binding(plan_hash, history);
                uniform_hash = fnv1a(uniform_hash, 7);
                for value in [diffusion_a, diffusion_b, feed, kill, dt, seed_mix] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
                uniform_hash = hash_feedback_binding(uniform_hash, history);
            }
            TexViewerOp::DomainWarp {
                strength,
                frequency,
                rotation,
                octaves,
                base_texture_node_id,
                warp_texture_node_id,
            } => {
                plan_hash = fnv1a(plan_hash, 15);
                plan_hash = fnv1a(plan_hash, base_texture_node_id as u64);
                plan_hash = fnv1a(plan_hash, warp_texture_node_id.unwrap_or(0) as u64);
                uniform_hash = fnv1a(uniform_hash, 15);
                for value in [strength, frequency, rotation, octaves] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
                uniform_hash = fnv1a(uniform_hash, base_texture_node_id as u64);
                uniform_hash = fnv1a(uniform_hash, warp_texture_node_id.unwrap_or(0) as u64);
            }
            TexViewerOp::DirectionalSmear {
                angle,
                length,
                jitter,
                amount,
            } => {
                plan_hash = fnv1a(plan_hash, 17);
                uniform_hash = fnv1a(uniform_hash, 17);
                for value in [angle, length, jitter, amount] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
            }
            TexViewerOp::WarpTransform {
                strength,
                frequency,
                phase,
            } => {
                plan_hash = fnv1a(plan_hash, 14);
                uniform_hash = fnv1a(uniform_hash, 14);
                for value in [strength, frequency, phase] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
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
                plan_hash = fnv1a(plan_hash, 8);
                plan_hash = fnv1a(plan_hash, category as u64);
                uniform_hash = fnv1a(uniform_hash, 8);
                uniform_hash = fnv1a(uniform_hash, category as u64);
                for value in [effect, amount, scale, threshold, speed, time] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
                plan_hash = match history {
                    Some(binding) => hash_feedback_binding(fnv1a(plan_hash, 1), binding),
                    None => fnv1a(plan_hash, 0),
                };
                uniform_hash = match history {
                    Some(binding) => hash_feedback_binding(fnv1a(uniform_hash, 1), binding),
                    None => fnv1a(uniform_hash, 0),
                };
            }
            TexViewerOp::StoreTexture { texture_node_id } => {
                plan_hash = fnv1a(plan_hash, 9);
                plan_hash = fnv1a(plan_hash, texture_node_id as u64);
                uniform_hash = fnv1a(uniform_hash, 9);
                uniform_hash = fnv1a(uniform_hash, texture_node_id as u64);
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
                plan_hash = fnv1a(plan_hash, 10);
                plan_hash = fnv1a(plan_hash, base_texture_node_id as u64);
                plan_hash = fnv1a(plan_hash, layer_texture_node_id.unwrap_or(0) as u64);
                uniform_hash = fnv1a(uniform_hash, 10);
                for value in [mode, opacity, bg_r, bg_g, bg_b, bg_a] {
                    uniform_hash = hash_f32(uniform_hash, value);
                }
                uniform_hash = fnv1a(uniform_hash, base_texture_node_id as u64);
                uniform_hash = fnv1a(uniform_hash, layer_texture_node_id.unwrap_or(0) as u64);
            }
        }
    }
    (plan_hash, uniform_hash)
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

#[cfg(test)]
mod tests {
    use super::ops_signatures;
    use crate::gui::runtime::{
        PostProcessCategory, TexRuntimeFeedbackHistoryBinding, TexRuntimeOp,
    };

    #[test]
    fn one_pass_signature_hashing_distinguishes_structure_and_uniform_changes() {
        let base = [TexRuntimeOp::PostProcess {
            category: PostProcessCategory::Temporal,
            effect: 0.5,
            amount: 0.25,
            scale: 1.0,
            threshold: 0.3,
            speed: 0.75,
            time: 2.0,
            history: Some(TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 7,
            }),
        }];
        let (base_plan, base_uniform) = ops_signatures(&base);

        let uniform_changed = [TexRuntimeOp::PostProcess {
            category: PostProcessCategory::Temporal,
            effect: 0.5,
            amount: 0.75,
            scale: 1.0,
            threshold: 0.3,
            speed: 0.75,
            time: 2.0,
            history: Some(TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 7,
            }),
        }];
        let (uniform_plan, uniform_sig) = ops_signatures(&uniform_changed);
        assert_eq!(
            base_plan, uniform_plan,
            "uniform-only changes should preserve the plan signature"
        );
        assert_ne!(
            base_uniform, uniform_sig,
            "uniform-only changes should invalidate the uniform signature"
        );

        let structure_changed = [TexRuntimeOp::Feedback {
            mix: 0.4,
            frame_gap: 2,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 7,
            },
        }];
        let (structure_plan, structure_uniform) = ops_signatures(&structure_changed);
        assert_ne!(
            base_plan, structure_plan,
            "op-shape changes must invalidate the plan signature"
        );
        assert_ne!(
            base_uniform, structure_uniform,
            "op-shape changes must also invalidate the uniform signature"
        );
    }
}

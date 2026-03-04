//! Runtime emit-stage helpers extracted from `runtime.rs`.

use super::*;

impl GuiCompiledRuntime {
    pub(super) fn update_noise_mesh(
        step: &CompiledStep,
        step_state: &mut RuntimeEvalState,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        let Some(mut mesh_state) = step_state.mesh else {
            return;
        };
        let amplitude = ctx
            .param(step, param_schema::buffer_noise::AMPLITUDE_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let frequency = ctx
            .param(step, param_schema::buffer_noise::FREQUENCY_INDEX)
            .unwrap_or(2.0)
            .max(0.01);
        let speed_hz = ctx
            .param(step, param_schema::buffer_noise::SPEED_HZ_INDEX)
            .unwrap_or(0.35)
            .max(0.0);
        let phase = ctx
            .param(step, param_schema::buffer_noise::PHASE_INDEX)
            .unwrap_or(0.0);
        let seed = ctx
            .param(step, param_schema::buffer_noise::SEED_INDEX)
            .unwrap_or(1.0);
        let twist = ctx
            .param(step, param_schema::buffer_noise::TWIST_INDEX)
            .unwrap_or(0.0)
            .clamp(-8.0, 8.0);
        let stretch = ctx
            .param(step, param_schema::buffer_noise::STRETCH_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let loop_cycles = ctx
            .param(step, param_schema::buffer_noise::LOOP_CYC_INDEX)
            .unwrap_or(12.0)
            .clamp(0.0, 256.0);
        let loop_mode = ctx
            .param(step, param_schema::buffer_noise::LOOP_MODE_INDEX)
            .unwrap_or(0.0)
            >= 0.5;
        let (base_phase, warp_freq, warp_input) = if loop_mode {
            let loop_phase = timeline_loop_phase(ctx.frame, ctx.time_secs);
            (
                loop_phase * loop_cycles.round(),
                frequency.round().clamp(1.0, 64.0),
                loop_phase,
            )
        } else {
            (
                ctx.time_secs * speed_hz * std::f32::consts::TAU,
                frequency,
                ctx.time_secs * speed_hz * std::f32::consts::TAU * 0.37,
            )
        };
        let phase_warp = if loop_mode {
            layered_loop_sine_noise(warp_input, warp_freq, phase, seed)
        } else {
            layered_sine_noise(warp_input, warp_freq, phase, seed)
        };
        let mut noise_phase =
            base_phase + phase * std::f32::consts::TAU + seed * 0.173 + phase_warp * 0.65;
        if loop_mode {
            noise_phase = noise_phase.rem_euclid(std::f32::consts::TAU);
        }
        mesh_state.noise_amount = amplitude;
        mesh_state.noise_freq = frequency;
        mesh_state.noise_phase = noise_phase;
        mesh_state.noise_twist = twist;
        mesh_state.noise_stretch = stretch;
        step_state.mesh = Some(mesh_state);
        step_state.scene_ready = false;
    }

    pub(super) fn emit_scene_pass(
        step: &CompiledStep,
        step_state: &RuntimeEvalState,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        if !step_state.scene_ready {
            return;
        }
        let (Some(mesh_state), Some(entity_state)) = (step_state.mesh, step_state.entity) else {
            return;
        };
        let alpha_clip = ctx
            .param(step, param_schema::render_scene_pass::BG_MODE_INDEX)
            .unwrap_or(0.0)
            >= 0.5;
        let zoom = step_state.camera_zoom.max(0.1);
        let center_x = (entity_state.pos_x - 0.5) * zoom + 0.5;
        let center_y = (entity_state.pos_y - 0.5) * zoom + 0.5;
        let edge_softness = ctx
            .param(step, param_schema::render_scene_pass::EDGE_SOFTNESS_INDEX)
            .unwrap_or(0.01)
            .max(0.0);
        match mesh_state.profile {
            SceneMeshProfile::Sphere => {
                let light_x = ctx
                    .param(step, param_schema::render_scene_pass::LIGHT_X_INDEX)
                    .unwrap_or(0.4);
                let light_y = ctx
                    .param(step, param_schema::render_scene_pass::LIGHT_Y_INDEX)
                    .unwrap_or(-0.5);
                let light_z = ctx
                    .param(step, param_schema::render_scene_pass::LIGHT_Z_INDEX)
                    .unwrap_or(1.0);
                ctx.out_ops.push(TexRuntimeOp::Sphere {
                    center_x,
                    center_y,
                    radius: (mesh_state.radius * entity_state.scale * zoom).max(0.01),
                    edge_softness: edge_softness * zoom,
                    noise_amount: mesh_state.noise_amount,
                    noise_freq: mesh_state.noise_freq,
                    noise_phase: mesh_state.noise_phase,
                    noise_twist: mesh_state.noise_twist,
                    noise_stretch: mesh_state.noise_stretch,
                    light_x,
                    light_y,
                    light_z,
                    ambient: entity_state.ambient,
                    color_r: entity_state.color_r,
                    color_g: entity_state.color_g,
                    color_b: entity_state.color_b,
                    alpha: entity_state.alpha,
                    alpha_clip,
                });
            }
            SceneMeshProfile::CircleNurbs => ctx.out_ops.push(TexRuntimeOp::Circle {
                center_x,
                center_y,
                radius: (mesh_state.radius * entity_state.scale * zoom).max(0.01),
                feather: edge_softness * (1.0 + (5.0 - mesh_state.order).max(0.0) * 0.35) * zoom,
                line_width: (mesh_state.line_width * entity_state.scale * zoom).max(0.0005),
                noise_amount: mesh_state.noise_amount,
                noise_freq: mesh_state.noise_freq,
                noise_phase: mesh_state.noise_phase,
                noise_twist: mesh_state.noise_twist,
                noise_stretch: mesh_state.noise_stretch,
                arc_start_deg: mesh_state.arc_start_deg,
                arc_end_deg: mesh_state.arc_end_deg,
                segment_count: mesh_state.segment_count,
                arc_open: mesh_state.arc_open as u32 as f32,
                color_r: entity_state.color_r,
                color_g: entity_state.color_g,
                color_b: entity_state.color_b,
                alpha: entity_state.alpha,
                alpha_clip,
            }),
        }
    }

    pub(super) fn emit_feedback(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let history = compiled_feedback_history_source(ctx.project, step).map_or(
            TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: step.node_id,
            },
            |texture_node_id| TexRuntimeFeedbackHistoryBinding::External { texture_node_id },
        );
        let mix = ctx
            .param(step, param_schema::feedback::RUNTIME_MIX_INDEX)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let frame_gap = ctx
            .param(step, param_schema::feedback::RUNTIME_FRAME_GAP_INDEX)
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, 32.0) as u32;
        ctx.out_ops.push(TexRuntimeOp::Feedback {
            mix,
            frame_gap,
            history,
        });
    }

    pub(super) fn emit_reaction_diffusion(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let diffusion_a = ctx
            .param(step, param_schema::reaction_diffusion::DIFF_A_INDEX)
            .unwrap_or(1.0)
            .clamp(0.0, 2.0);
        let diffusion_b = ctx
            .param(step, param_schema::reaction_diffusion::DIFF_B_INDEX)
            .unwrap_or(0.5)
            .clamp(0.0, 2.0);
        let feed = ctx
            .param(step, param_schema::reaction_diffusion::FEED_INDEX)
            .unwrap_or(0.055)
            .clamp(0.0, 0.12);
        let kill = ctx
            .param(step, param_schema::reaction_diffusion::KILL_INDEX)
            .unwrap_or(0.062)
            .clamp(0.0, 0.12);
        let dt = ctx
            .param(step, param_schema::reaction_diffusion::DT_INDEX)
            .unwrap_or(1.0)
            .clamp(0.0, 2.0);
        let seed_mix = ctx
            .param(step, param_schema::reaction_diffusion::SEED_MIX_INDEX)
            .unwrap_or(0.04)
            .clamp(0.0, 1.0);
        ctx.out_ops.push(TexRuntimeOp::ReactionDiffusion {
            diffusion_a,
            diffusion_b,
            feed,
            kill,
            dt,
            seed_mix,
            history: TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: step.node_id,
            },
        });
    }

    pub(super) fn emit_post_process(
        step: &CompiledStep,
        category: PostProcessCategory,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        let history = if post_process_uses_history(category) {
            Some(TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: step.node_id,
            })
        } else {
            None
        };
        let effect = ctx
            .param(step, param_schema::post_process::EFFECT_INDEX)
            .unwrap_or(0.0);
        let amount = ctx
            .param(step, param_schema::post_process::AMOUNT_INDEX)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        let scale = ctx
            .param(step, param_schema::post_process::SCALE_INDEX)
            .unwrap_or(1.0)
            .clamp(0.0, 8.0);
        let threshold = ctx
            .param(step, param_schema::post_process::THRESH_INDEX)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        let speed = ctx
            .param(step, param_schema::post_process::SPEED_INDEX)
            .unwrap_or(1.0)
            .clamp(0.0, 8.0);
        ctx.out_ops.push(TexRuntimeOp::PostProcess {
            category,
            effect,
            amount,
            scale,
            threshold,
            speed,
            time: ctx.time_secs,
            history,
        });
    }
}

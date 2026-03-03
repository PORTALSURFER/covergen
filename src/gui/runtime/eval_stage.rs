//! Runtime evaluation helpers for compiled GUI steps.

use super::*;

/// Resolve one compiled scalar parameter value for one node step.
pub(super) fn compiled_param_value_opt(
    project: &GuiProject,
    step: &CompiledStep,
    param_slot: usize,
    time_secs: f32,
    eval_stack: &mut SignalEvalStack,
) -> Option<f32> {
    let index = step.param_slots.get(param_slot).copied().flatten()?.0;
    RUNTIME_SIGNAL_SAMPLE_MEMO.with(|memo| {
        let mut borrow = memo.borrow_mut();
        project.node_param_value_by_index_with_memo(
            step.node_id,
            index,
            time_secs,
            eval_stack,
            &mut borrow,
        )
    })
}

/// Resolve one compiled texture source id for one node step parameter.
pub(super) fn compiled_texture_source_for_param(
    project: &GuiProject,
    step: &CompiledStep,
    param_slot: usize,
) -> Option<u32> {
    let index = step.param_slots.get(param_slot).copied().flatten()?.0;
    project.texture_source_for_param(step.node_id, index)
}

pub(super) fn layered_sine_noise(t: f32, frequency: f32, phase: f32, seed: f32) -> f32 {
    let s0 = seed * 0.13 + phase;
    let s1 = seed * 0.73 + phase * 1.9;
    let s2 = seed * 1.37 + phase * 0.47;
    let n0 = (t * frequency + s0).sin();
    let n1 = (t * frequency * 2.11 + s1).sin();
    let n2 = (t * frequency * 4.37 + s2).sin();
    (n0 * 0.62 + n1 * 0.28 + n2 * 0.10).clamp(-1.0, 1.0)
}

pub(super) fn layered_loop_sine_noise(
    loop_phase: f32,
    frequency: f32,
    phase: f32,
    seed: f32,
) -> f32 {
    let freq_cycles = frequency.round().clamp(1.0, 64.0);
    let s0 = seed * 0.13 + phase;
    let s1 = seed * 0.73 + phase * 1.9;
    let s2 = seed * 1.37 + phase * 0.47;
    let n0 = (loop_phase * freq_cycles + s0).sin();
    let n1 = (loop_phase * freq_cycles * 2.0 + s1).sin();
    let n2 = (loop_phase * freq_cycles * 4.0 + s2).sin();
    (n0 * 0.62 + n1 * 0.28 + n2 * 0.10).clamp(-1.0, 1.0)
}

pub(super) fn timeline_loop_phase(frame: Option<TexRuntimeFrameContext>, time_secs: f32) -> f32 {
    let progress = match frame {
        Some(ctx) => normalized_loop_progress(ctx.frame_index, ctx.frame_total),
        None => {
            let frame_total = 30 * DEFAULT_LOOP_FPS;
            let loop_secs = frame_total as f32 / DEFAULT_LOOP_FPS as f32;
            let wrapped_secs = time_secs.max(0.0).rem_euclid(loop_secs);
            let frame_index = (wrapped_secs * DEFAULT_LOOP_FPS as f32).floor() as u32;
            normalized_loop_progress(frame_index, frame_total)
        }
    };
    progress * std::f32::consts::TAU
}

pub(super) fn normalized_loop_progress(frame_index: u32, frame_total: u32) -> f32 {
    if frame_total <= 1 {
        return 0.0;
    }
    let max_index = frame_total - 1;
    let wrapped = frame_index % frame_total;
    wrapped as f32 / max_index as f32
}

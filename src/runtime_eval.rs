//! Graph evaluation for V2 runtime execution.
//!
//! This module evaluates compiled nodes on CPU for deterministic test snapshots
//! and legacy cross-checking. Production V2 execution uses retained GPU flow in
//! `runtime_gpu`.

mod helpers;

use std::error::Error;
use std::time::Instant;

use crate::gpu_render::GpuLayerRenderer;
use crate::image_ops::{apply_contrast, blend_layer_stack, stretch_to_percentile};
use crate::proc_graph::{
    apply_sop_geometry, eval_chop_lfo, eval_chop_math, eval_chop_remap, eval_source_noise_scalar,
    render_top_camera, SopPrimitive,
};
use crate::telemetry;

use super::compiler::{CompiledGraph, CompiledNodeStep, CompiledOp};
use super::node::{GraphTimeInput, PortType};
use super::runtime::RuntimeBuffers;
use super::runtime_ops::{blend_with_mask, build_mask, generate_source_noise, warp_luma};

use helpers::{
    op_scope, optional_scalar_input, pixel_count, release_step_values, require_luma_input,
    require_mask_input, require_scalar_input, require_sop_input, required_lifetime,
    AliasedResourceArena, DenseRuntimeValues, RuntimeValue,
};

/// Render raw graph luma into `buffers.layered` before output post-processing.
pub(crate) fn render_graph_luma(
    compiled: &CompiledGraph,
    renderer: Option<&mut GpuLayerRenderer>,
    buffers: &mut RuntimeBuffers,
    seed_offset: u32,
    modulation: Option<GraphTimeInput>,
) -> Result<(), Box<dyn Error>> {
    if compiled.can_use_retained_layer_path {
        evaluate_retained_layer_graph(compiled, renderer, buffers, seed_offset, modulation)
    } else {
        evaluate_mixed_graph(compiled, renderer, buffers, seed_offset, modulation)
    }
}

fn evaluate_retained_layer_graph(
    compiled: &CompiledGraph,
    renderer: Option<&mut GpuLayerRenderer>,
    _buffers: &mut RuntimeBuffers,
    seed_offset: u32,
    modulation: Option<GraphTimeInput>,
) -> Result<(), Box<dyn Error>> {
    let renderer = renderer.ok_or("graph requires GPU renderer but none was initialized")?;
    renderer.begin_retained_image()?;

    for step in &compiled.steps {
        let node_start = Instant::now();
        match step.op {
            CompiledOp::GenerateLayer(layer) => {
                let effective = modulation.map_or(layer, |time| layer.with_time(time));
                let params = effective.to_params(compiled.width, compiled.height, seed_offset);
                let gpu_start = Instant::now();
                renderer.submit_retained_layer(
                    &params,
                    effective.opacity,
                    effective.blend_mode.as_u32(),
                    effective.contrast,
                )?;
                telemetry::record_timing(
                    "v2.gpu.node.generate_layer.retained",
                    gpu_start.elapsed(),
                );
            }
            CompiledOp::Output(_) => {}
            _ => {
                return Err("non-layer node found in retained-layer execution path".into());
            }
        }
        telemetry::record_timing(op_scope(step.op), node_start.elapsed());
    }

    Ok(())
}

fn evaluate_mixed_graph(
    compiled: &CompiledGraph,
    mut renderer: Option<&mut GpuLayerRenderer>,
    buffers: &mut RuntimeBuffers,
    seed_offset: u32,
    modulation: Option<GraphTimeInput>,
) -> Result<(), Box<dyn Error>> {
    let pixels = pixel_count(compiled.width, compiled.height)?;
    let mut values = DenseRuntimeValues::new(compiled.steps.len());
    // Test-only fallback feedback memory. Production persistent feedback uses GPU buffers.
    let mut feedback_state = DenseFeedbackState::new(compiled.steps.len());
    let mut arena = AliasedResourceArena::new(&compiled.resource_plan, pixels);
    let mut output_written = false;

    for (step_index, step) in compiled.steps.iter().enumerate() {
        let node_start = Instant::now();
        match step.op {
            CompiledOp::GenerateLayer(layer) => {
                let effective = modulation.map_or(layer, |time| layer.with_time(time));
                execute_generate_layer(
                    step,
                    effective,
                    compiled,
                    seed_offset,
                    renderer.as_deref_mut(),
                    buffers,
                    &mut values,
                    &mut arena,
                )?;
            }
            CompiledOp::SourceNoise(spec) => {
                let effective = modulation.map_or(spec, |time| spec.with_time(time));
                let effective_seed = effective.seed.wrapping_add(seed_offset);
                match effective.output_port {
                    PortType::LumaTexture => {
                        let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                        let mut out = arena.acquire_for(lifetime);
                        generate_source_noise(
                            compiled.width,
                            compiled.height,
                            effective_seed,
                            effective.scale,
                            effective.octaves,
                            effective.amplitude,
                            &mut out,
                        );
                        values.insert_step(step, RuntimeValue::Luma(out));
                    }
                    PortType::MaskTexture => {
                        let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                        let mut out = arena.acquire_for(lifetime);
                        generate_source_noise(
                            compiled.width,
                            compiled.height,
                            effective_seed,
                            effective.scale,
                            effective.octaves,
                            effective.amplitude,
                            &mut out,
                        );
                        values.insert_step(step, RuntimeValue::Mask(out));
                    }
                    PortType::ChannelScalar => {
                        let phase = modulation.map(|time| time.normalized).unwrap_or(0.0);
                        values.insert_step(
                            step,
                            RuntimeValue::Scalar(eval_source_noise_scalar(
                                effective.seed,
                                effective.scale,
                                effective.octaves,
                                effective.amplitude,
                                phase,
                            )),
                        );
                    }
                    PortType::SopPrimitive => {
                        return Err("source-noise output port cannot be SOP".into());
                    }
                }
            }
            CompiledOp::Mask(spec) => {
                let effective = modulation.map_or(spec, |time| spec.with_time(time));
                let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                let mut out = arena.acquire_for(lifetime);
                let input = require_luma_input(&values, step, 0)?;
                build_mask(
                    input,
                    effective.threshold,
                    effective.softness,
                    effective.invert,
                    &mut out,
                );
                values.insert_step(step, RuntimeValue::Mask(out));
            }
            CompiledOp::Blend(spec) => {
                let effective = modulation.map_or(spec, |time| spec.with_time(time));
                let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                let mut blended = arena.acquire_for(lifetime);
                let base = require_luma_input(&values, step, 0)?;
                blended.copy_from_slice(base);
                let top = require_luma_input(&values, step, 1)?;
                let mask = if step.inputs.len() > 2 {
                    Some(require_mask_input(&values, step, 2)?)
                } else {
                    None
                };
                blend_with_mask(&mut blended, top, effective.mode, effective.opacity, mask);
                values.insert_step(step, RuntimeValue::Luma(blended));
            }
            CompiledOp::ToneMap(spec) => {
                let effective = modulation.map_or(spec, |time| spec.with_time(time));
                let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                let mut out = arena.acquire_for(lifetime);
                let input = require_luma_input(&values, step, 0)?;
                let channel = optional_scalar_input(&values, step, 1)?;
                out.copy_from_slice(input);
                apply_contrast(
                    &mut out,
                    (effective.contrast * channel.unwrap_or(1.0)).clamp(0.5, 3.0),
                );
                stretch_to_percentile(
                    &mut out,
                    &mut buffers.percentile,
                    effective.low_pct,
                    effective.high_pct,
                    false,
                );
                values.insert_step(step, RuntimeValue::Luma(out));
            }
            CompiledOp::WarpTransform(spec) => {
                let mut effective = modulation.map_or(spec, |time| spec.with_time(time));
                let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                let mut out = arena.acquire_for(lifetime);
                let input = require_luma_input(&values, step, 0)?;
                let channel = optional_scalar_input(&values, step, 1)?;
                if let Some(value) = channel {
                    effective.strength = (effective.strength * value).clamp(0.0, 2.4);
                }
                warp_luma(input, compiled.width, compiled.height, effective, &mut out);
                values.insert_step(step, RuntimeValue::Luma(out));
            }
            CompiledOp::StatefulFeedback(spec) => {
                let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                let mut out = arena.acquire_for(lifetime);
                let input = require_luma_input(&values, step, 0)?;
                let state = feedback_state.get_or_insert(step, pixels);
                let mix = spec.mix.clamp(0.0, 1.0);
                for ((dst, current), previous) in out.iter_mut().zip(input.iter()).zip(state.iter())
                {
                    *dst = ((1.0 - mix) * *current + mix * *previous).clamp(0.0, 1.0);
                }
                state.copy_from_slice(&out);
                values.insert_step(step, RuntimeValue::Luma(out));
            }
            CompiledOp::ChopLfo(spec) => {
                values.insert_step(step, RuntimeValue::Scalar(eval_chop_lfo(spec, modulation)));
            }
            CompiledOp::ChopMath(spec) => {
                let a = require_scalar_input(&values, step, 0)?;
                let b = optional_scalar_input(&values, step, 1)?;
                values.insert_step(step, RuntimeValue::Scalar(eval_chop_math(spec, a, b)));
            }
            CompiledOp::ChopRemap(spec) => {
                let input = require_scalar_input(&values, step, 0)?;
                values.insert_step(step, RuntimeValue::Scalar(eval_chop_remap(spec, input)));
            }
            CompiledOp::SopCircle(spec) => {
                values.insert_step(step, RuntimeValue::Sop(SopPrimitive::Circle(spec)));
            }
            CompiledOp::SopSphere(spec) => {
                values.insert_step(step, RuntimeValue::Sop(SopPrimitive::Sphere(spec)));
            }
            CompiledOp::SopGeometry(spec) => {
                let input = require_sop_input(&values, step, 0)?;
                let modulation = optional_scalar_input(&values, step, 1)?;
                values.insert_step(
                    step,
                    RuntimeValue::Sop(apply_sop_geometry(input, spec, modulation)),
                );
            }
            CompiledOp::TopCameraRender(spec) => {
                let primitive = require_sop_input(&values, step, 0)?;
                let channel = optional_scalar_input(&values, step, 1)?;
                let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                let mut out = arena.acquire_for(lifetime);
                render_top_camera(
                    primitive,
                    spec,
                    channel,
                    compiled.width,
                    compiled.height,
                    &mut out,
                );
                values.insert_step(step, RuntimeValue::Luma(out));
            }
            CompiledOp::Output(_) => {
                let output = require_luma_input(&values, step, 0)?;
                if output.len() != pixels {
                    return Err("compiled output buffer size mismatch".into());
                }
                buffers.layered.copy_from_slice(output);
                output_written = true;
            }
        }
        telemetry::record_timing(op_scope(step.op), node_start.elapsed());
        release_step_values(step_index, compiled, &mut values, &mut arena)?;
    }

    if !output_written {
        return Err("compiled output node produced no value".into());
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_generate_layer(
    step: &CompiledNodeStep,
    layer: super::node::GenerateLayerNode,
    compiled: &CompiledGraph,
    seed_offset: u32,
    renderer: Option<&mut GpuLayerRenderer>,
    buffers: &mut RuntimeBuffers,
    values: &mut DenseRuntimeValues,
    arena: &mut AliasedResourceArena,
) -> Result<(), Box<dyn Error>> {
    let renderer = renderer.ok_or("generate-layer node requires GPU renderer")?;
    let params = layer.to_params(compiled.width, compiled.height, seed_offset);
    let gpu_start = Instant::now();
    renderer.render_layer(&params, &mut buffers.layer_scratch)?;
    telemetry::record_timing("v2.gpu.node.generate_layer", gpu_start.elapsed());
    apply_contrast(&mut buffers.layer_scratch, layer.contrast);

    let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
    let mut out = arena.acquire_for(lifetime);
    if step.inputs.is_empty() {
        out.copy_from_slice(&buffers.layer_scratch);
    } else {
        let base = require_luma_input(values, step, 0)?;
        out.copy_from_slice(base);
        blend_layer_stack(
            &mut out,
            &buffers.layer_scratch,
            layer.opacity,
            layer.blend_mode,
        );
    }

    values.insert_step(step, RuntimeValue::Luma(out));
    Ok(())
}

/// Dense feedback scratch buffers keyed by compile-time node index.
struct DenseFeedbackState {
    slots: Vec<Option<Vec<f32>>>,
}

impl DenseFeedbackState {
    /// Allocate one optional feedback slot per compiled step.
    fn new(node_count: usize) -> Self {
        Self {
            slots: (0..node_count).map(|_| None).collect(),
        }
    }

    /// Return persistent feedback storage for this step, creating it on first use.
    fn get_or_insert(&mut self, step: &CompiledNodeStep, pixels: usize) -> &mut Vec<f32> {
        let slot = &mut self.slots[step.node_index];
        if slot.is_none() {
            *slot = Some(vec![0.0; pixels]);
        }
        slot.as_mut().expect("feedback slot initialized")
    }
}

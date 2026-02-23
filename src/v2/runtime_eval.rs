//! Graph evaluation for V2 runtime execution.
//!
//! This module runs compiled nodes either through retained GPU layering or
//! through mixed CPU/GPU execution when explicit node operators are present.

use std::collections::HashMap;
use std::error::Error;

use crate::gpu_render::GpuLayerRenderer;
use crate::image_ops::{apply_contrast, blend_layer_stack, stretch_to_percentile};

use super::animation::modulate_layer_for_frame;
use super::compiler::{
    CompiledGraph, CompiledNodeStep, CompiledOp, CompiledResourcePlan, CompiledValueKind,
    CompiledValueLifetime,
};
use super::graph::NodeId;
use super::node::PortType;
use super::runtime::RuntimeBuffers;
use super::runtime_ops::{blend_with_mask, build_mask, generate_source_noise, warp_luma};

enum RuntimeValue {
    Luma(Vec<f32>),
    Mask(Vec<f32>),
}

/// Optional deterministic animation modulation context.
#[derive(Clone, Copy)]
pub(crate) struct FrameModulation {
    pub frame_index: u32,
    pub total_frames: u32,
}

/// Render raw graph luma into `buffers.layered` before output post-processing.
pub(crate) fn render_graph_luma(
    compiled: &CompiledGraph,
    renderer: Option<&mut GpuLayerRenderer>,
    buffers: &mut RuntimeBuffers,
    seed_offset: u32,
    modulation: Option<FrameModulation>,
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
    buffers: &mut RuntimeBuffers,
    seed_offset: u32,
    modulation: Option<FrameModulation>,
) -> Result<(), Box<dyn Error>> {
    let renderer = renderer.ok_or("graph requires GPU renderer but none was initialized")?;
    renderer.begin_retained_image()?;

    let mut layer_index = 0u32;
    for step in &compiled.steps {
        match step.op {
            CompiledOp::GenerateLayer(layer) => {
                let effective = apply_modulation(layer, modulation, layer_index);
                let params = effective.to_params(compiled.width, compiled.height, seed_offset);
                renderer.submit_retained_layer(
                    &params,
                    effective.opacity,
                    effective.blend_mode.as_u32(),
                    effective.contrast,
                )?;
                layer_index = layer_index.wrapping_add(1);
            }
            CompiledOp::Output => {}
            _ => {
                return Err("non-layer node found in retained-layer execution path".into());
            }
        }
    }

    renderer.collect_retained_image(&mut buffers.layered)?;
    Ok(())
}

fn evaluate_mixed_graph(
    compiled: &CompiledGraph,
    mut renderer: Option<&mut GpuLayerRenderer>,
    buffers: &mut RuntimeBuffers,
    seed_offset: u32,
    modulation: Option<FrameModulation>,
) -> Result<(), Box<dyn Error>> {
    let pixels = pixel_count(compiled.width, compiled.height)?;
    let mut values: HashMap<NodeId, RuntimeValue> = HashMap::with_capacity(compiled.steps.len());
    let mut arena = AliasedResourceArena::new(&compiled.resource_plan, pixels);
    let mut layer_index = 0u32;
    let mut output_written = false;

    for (step_index, step) in compiled.steps.iter().enumerate() {
        match step.op {
            CompiledOp::GenerateLayer(layer) => {
                let effective = apply_modulation(layer, modulation, layer_index);
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
                layer_index = layer_index.wrapping_add(1);
            }
            CompiledOp::SourceNoise(spec) => {
                let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                let mut out = arena.acquire_for(lifetime);
                generate_source_noise(
                    compiled.width,
                    compiled.height,
                    spec.seed.wrapping_add(seed_offset),
                    spec.scale,
                    spec.octaves,
                    spec.amplitude,
                    &mut out,
                );
                match spec.output_port {
                    PortType::LumaTexture => {
                        values.insert(step.node_id, RuntimeValue::Luma(out));
                    }
                    PortType::MaskTexture => {
                        values.insert(step.node_id, RuntimeValue::Mask(out));
                    }
                }
            }
            CompiledOp::Mask(spec) => {
                let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                let mut out = arena.acquire_for(lifetime);
                let input = require_luma_input(&values, step, 0)?;
                build_mask(input, spec.threshold, spec.softness, spec.invert, &mut out);
                values.insert(step.node_id, RuntimeValue::Mask(out));
            }
            CompiledOp::Blend(spec) => {
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
                blend_with_mask(&mut blended, top, spec.mode, spec.opacity, mask);
                values.insert(step.node_id, RuntimeValue::Luma(blended));
            }
            CompiledOp::ToneMap(spec) => {
                let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                let mut out = arena.acquire_for(lifetime);
                let input = require_luma_input(&values, step, 0)?;
                out.copy_from_slice(input);
                apply_contrast(&mut out, spec.contrast);
                stretch_to_percentile(
                    &mut out,
                    &mut buffers.percentile,
                    spec.low_pct,
                    spec.high_pct,
                    false,
                );
                values.insert(step.node_id, RuntimeValue::Luma(out));
            }
            CompiledOp::WarpTransform(spec) => {
                let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
                let mut out = arena.acquire_for(lifetime);
                let input = require_luma_input(&values, step, 0)?;
                warp_luma(input, compiled.width, compiled.height, spec, &mut out);
                values.insert(step.node_id, RuntimeValue::Luma(out));
            }
            CompiledOp::Output => {
                let output = require_luma_input(&values, step, 0)?;
                if output.len() != pixels {
                    return Err("compiled output buffer size mismatch".into());
                }
                buffers.layered.copy_from_slice(output);
                output_written = true;
            }
        }

        release_step_values(step_index, compiled, &mut values, &mut arena)?;
    }

    if !output_written {
        return Err("compiled output node produced no value".into());
    }

    Ok(())
}

fn execute_generate_layer(
    step: &CompiledNodeStep,
    layer: super::node::GenerateLayerNode,
    compiled: &CompiledGraph,
    seed_offset: u32,
    renderer: Option<&mut GpuLayerRenderer>,
    buffers: &mut RuntimeBuffers,
    values: &mut HashMap<NodeId, RuntimeValue>,
    arena: &mut AliasedResourceArena,
) -> Result<(), Box<dyn Error>> {
    let renderer = renderer.ok_or("generate-layer node requires GPU renderer")?;
    let params = layer.to_params(compiled.width, compiled.height, seed_offset);
    renderer.render_layer(&params, &mut buffers.layer_scratch)?;
    apply_contrast(&mut buffers.layer_scratch, layer.contrast);

    let lifetime = required_lifetime(&compiled.resource_plan, step.node_id)?;
    let mut out = arena.acquire_for(lifetime);
    if step.inputs.is_empty() {
        out.copy_from_slice(&buffers.layer_scratch);
    } else {
        let base = require_luma(values, step.inputs[0])?;
        out.copy_from_slice(base);
        blend_layer_stack(
            &mut out,
            &buffers.layer_scratch,
            layer.opacity,
            layer.blend_mode,
        );
    }

    values.insert(step.node_id, RuntimeValue::Luma(out));
    Ok(())
}

fn release_step_values(
    step_index: usize,
    compiled: &CompiledGraph,
    values: &mut HashMap<NodeId, RuntimeValue>,
    arena: &mut AliasedResourceArena,
) -> Result<(), Box<dyn Error>> {
    let releases = compiled
        .resource_plan
        .releases_by_step
        .get(step_index)
        .ok_or("invalid release schedule index")?;

    for node_id in releases {
        let value = values
            .remove(node_id)
            .ok_or_else(|| format!("missing transient value for release node {:?}", node_id))?;
        let lifetime = required_lifetime(&compiled.resource_plan, *node_id)?;
        arena.recycle(lifetime, value)?;
    }

    Ok(())
}

fn required_lifetime(
    plan: &CompiledResourcePlan,
    node_id: NodeId,
) -> Result<CompiledValueLifetime, Box<dyn Error>> {
    plan.lifetime_for(node_id)
        .ok_or_else(|| format!("missing resource lifetime for node {:?}", node_id).into())
}

fn require_luma_input<'a>(
    values: &'a HashMap<NodeId, RuntimeValue>,
    step: &CompiledNodeStep,
    slot: usize,
) -> Result<&'a [f32], Box<dyn Error>> {
    let node_id = *step
        .inputs
        .get(slot)
        .ok_or_else(|| format!("node {:?} missing required input slot {slot}", step.node_id))?;
    require_luma(values, node_id)
}

fn require_mask_input<'a>(
    values: &'a HashMap<NodeId, RuntimeValue>,
    step: &CompiledNodeStep,
    slot: usize,
) -> Result<&'a [f32], Box<dyn Error>> {
    let node_id = *step
        .inputs
        .get(slot)
        .ok_or_else(|| format!("node {:?} missing required input slot {slot}", step.node_id))?;
    require_mask(values, node_id)
}

fn require_luma<'a>(
    values: &'a HashMap<NodeId, RuntimeValue>,
    node_id: NodeId,
) -> Result<&'a [f32], Box<dyn Error>> {
    match values.get(&node_id) {
        Some(RuntimeValue::Luma(value)) => Ok(value),
        Some(RuntimeValue::Mask(_)) => {
            Err(format!("node {:?} output is mask but luma was required", node_id).into())
        }
        None => Err(format!("missing input value for node {:?}", node_id).into()),
    }
}

fn require_mask<'a>(
    values: &'a HashMap<NodeId, RuntimeValue>,
    node_id: NodeId,
) -> Result<&'a [f32], Box<dyn Error>> {
    match values.get(&node_id) {
        Some(RuntimeValue::Mask(value)) => Ok(value),
        Some(RuntimeValue::Luma(_)) => {
            Err(format!("node {:?} output is luma but mask was required", node_id).into())
        }
        None => Err(format!("missing input value for node {:?}", node_id).into()),
    }
}

fn apply_modulation(
    layer: super::node::GenerateLayerNode,
    modulation: Option<FrameModulation>,
    layer_index: u32,
) -> super::node::GenerateLayerNode {
    if let Some(modulation) = modulation {
        modulate_layer_for_frame(
            layer,
            modulation.frame_index,
            modulation.total_frames,
            layer_index,
        )
    } else {
        layer
    }
}

fn pixel_count(width: u32, height: u32) -> Result<usize, Box<dyn Error>> {
    width
        .checked_mul(height)
        .map(|count| count as usize)
        .ok_or("invalid pixel dimensions".into())
}

struct AliasedResourceArena {
    pixel_count: usize,
    luma_slots: Vec<Option<Vec<f32>>>,
    mask_slots: Vec<Option<Vec<f32>>>,
}

impl AliasedResourceArena {
    fn new(plan: &CompiledResourcePlan, pixel_count: usize) -> Self {
        Self {
            pixel_count,
            luma_slots: (0..plan.peak_luma_slots)
                .map(|_| Some(vec![0.0f32; pixel_count]))
                .collect(),
            mask_slots: (0..plan.peak_mask_slots)
                .map(|_| Some(vec![0.0f32; pixel_count]))
                .collect(),
        }
    }

    fn acquire_for(&mut self, lifetime: CompiledValueLifetime) -> Vec<f32> {
        let slots = match lifetime.kind {
            CompiledValueKind::Luma => &mut self.luma_slots,
            CompiledValueKind::Mask => &mut self.mask_slots,
        };

        slots
            .get_mut(lifetime.alias_slot)
            .and_then(Option::take)
            .unwrap_or_else(|| vec![0.0f32; self.pixel_count])
    }

    fn recycle(
        &mut self,
        lifetime: CompiledValueLifetime,
        value: RuntimeValue,
    ) -> Result<(), Box<dyn Error>> {
        let (kind, mut buffer) = match value {
            RuntimeValue::Luma(buffer) => (CompiledValueKind::Luma, buffer),
            RuntimeValue::Mask(buffer) => (CompiledValueKind::Mask, buffer),
        };

        if kind != lifetime.kind {
            return Err("resource kind mismatch while recycling aliased buffer".into());
        }
        if buffer.len() != self.pixel_count {
            buffer.resize(self.pixel_count, 0.0);
        }

        let slots = match lifetime.kind {
            CompiledValueKind::Luma => &mut self.luma_slots,
            CompiledValueKind::Mask => &mut self.mask_slots,
        };
        let slot = slots
            .get_mut(lifetime.alias_slot)
            .ok_or("alias slot index out of bounds")?;
        *slot = Some(buffer);
        Ok(())
    }
}

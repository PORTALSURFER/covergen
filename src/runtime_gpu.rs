//! GPU-native graph evaluator for compiled V2 node graphs.

use std::collections::HashMap;
use std::error::Error;
use std::time::Instant;

use crate::gpu_render::GpuLayerRenderer;
use crate::proc_graph::{
    apply_sop_geometry, eval_chop_lfo, eval_chop_math, eval_chop_remap, eval_source_noise_scalar,
    SopPrimitive,
};
use crate::telemetry;

use super::compiler::{CompiledGraph, CompiledNodeStep, CompiledOp, CompiledValueKind};
use super::graph::NodeId;
use super::node::{GraphTimeInput, PortType};

/// Render one compiled graph image/frame fully on GPU and stage final output.
pub(crate) fn render_graph_luma_gpu(
    compiled: &CompiledGraph,
    renderer: &mut GpuLayerRenderer,
    seed_offset: u32,
    modulation: Option<GraphTimeInput>,
) -> Result<(), Box<dyn Error>> {
    renderer.begin_retained_image()?;

    let mut scalar_values: HashMap<NodeId, f32> = HashMap::new();
    let mut sop_values: HashMap<NodeId, SopPrimitive> = HashMap::new();

    for (step_index, step) in compiled.steps.iter().enumerate() {
        let node_start = Instant::now();
        match step.op {
            CompiledOp::GenerateLayer(layer) => {
                let effective = modulation.map_or(layer, |time| layer.with_time(time));
                let params = effective.to_params(compiled.width, compiled.height, seed_offset);
                let input = optional_luma_input_slot(compiled, step, 0)?;
                let output = output_luma_slot(compiled, step.node_id)?;
                let gpu_start = Instant::now();
                renderer.render_generate_layer_to_alias(
                    &params,
                    input,
                    output,
                    effective.opacity,
                    effective.blend_mode.as_u32(),
                    effective.contrast,
                )?;
                telemetry::record_timing(
                    "v2.gpu.node.generate_layer.retained",
                    gpu_start.elapsed(),
                );
            }
            CompiledOp::SourceNoise(spec) => {
                let effective = modulation.map_or(spec, |time| spec.with_time(time));
                let effective_seed = effective.seed.wrapping_add(seed_offset);
                match effective.output_port {
                    PortType::LumaTexture | PortType::MaskTexture => {
                        let output_mask = matches!(effective.output_port, PortType::MaskTexture);
                        let output = if output_mask {
                            output_mask_slot(compiled, step.node_id)?
                        } else {
                            output_luma_slot(compiled, step.node_id)?
                        };
                        renderer.render_source_noise_to_alias(
                            output_mask,
                            output,
                            effective_seed,
                            effective.scale,
                            effective.octaves,
                            effective.amplitude,
                        )?;
                    }
                    PortType::ChannelScalar => {
                        let phase = modulation.map(|time| time.normalized).unwrap_or(0.0);
                        scalar_values.insert(
                            step.node_id,
                            eval_source_noise_scalar(
                                effective.seed,
                                effective.scale,
                                effective.octaves,
                                effective.amplitude,
                                phase,
                            ),
                        );
                    }
                    PortType::SopPrimitive => {
                        return Err("source-noise output port cannot be SOP".into());
                    }
                }
            }
            CompiledOp::Mask(spec) => {
                let effective = modulation.map_or(spec, |time| spec.with_time(time));
                let input = luma_input_slot(compiled, step, 0)?;
                let output = output_mask_slot(compiled, step.node_id)?;
                renderer.render_mask_to_alias(
                    input,
                    output,
                    effective.threshold,
                    effective.softness,
                    effective.invert,
                )?;
            }
            CompiledOp::Blend(spec) => {
                let effective = modulation.map_or(spec, |time| spec.with_time(time));
                let base = luma_input_slot(compiled, step, 0)?;
                let top = luma_input_slot(compiled, step, 1)?;
                let mask = if step.inputs.len() > 2 {
                    Some(mask_input_slot(compiled, step, 2)?)
                } else {
                    None
                };
                let output = output_luma_slot(compiled, step.node_id)?;
                renderer.render_blend_to_alias(
                    base,
                    top,
                    mask,
                    output,
                    effective.mode.as_u32(),
                    effective.opacity,
                )?;
            }
            CompiledOp::ToneMap(spec) => {
                let mut effective = modulation.map_or(spec, |time| spec.with_time(time));
                let channel = optional_scalar_input(step, 1, &scalar_values)?;
                if let Some(value) = channel {
                    effective.contrast = (effective.contrast * value).clamp(0.5, 3.0);
                }
                let input = luma_input_slot(compiled, step, 0)?;
                let output = output_luma_slot(compiled, step.node_id)?;
                renderer.render_tone_map_to_alias(
                    input,
                    output,
                    effective.contrast,
                    effective.low_pct,
                    effective.high_pct,
                )?;
            }
            CompiledOp::WarpTransform(spec) => {
                let mut effective = modulation.map_or(spec, |time| spec.with_time(time));
                let channel = optional_scalar_input(step, 1, &scalar_values)?;
                if let Some(value) = channel {
                    effective.strength = (effective.strength * value).clamp(0.0, 2.4);
                }
                let input = luma_input_slot(compiled, step, 0)?;
                let output = output_luma_slot(compiled, step.node_id)?;
                renderer.render_warp_to_alias(
                    input,
                    output,
                    effective.strength,
                    effective.frequency,
                    effective.phase,
                )?;
            }
            CompiledOp::StatefulFeedback(spec) => {
                let input = luma_input_slot(compiled, step, 0)?;
                let output = output_luma_slot(compiled, step.node_id)?;
                let feedback_slot = stateful_feedback_slot(compiled, step.node_id)?;
                renderer.render_stateful_feedback_to_alias(
                    input,
                    output,
                    feedback_slot,
                    spec.mix,
                )?;
            }
            CompiledOp::ChopLfo(spec) => {
                scalar_values.insert(step.node_id, eval_chop_lfo(spec, modulation));
            }
            CompiledOp::ChopMath(spec) => {
                let a = require_scalar_input(step, 0, &scalar_values)?;
                let b = optional_scalar_input(step, 1, &scalar_values)?;
                scalar_values.insert(step.node_id, eval_chop_math(spec, a, b));
            }
            CompiledOp::ChopRemap(spec) => {
                let input = require_scalar_input(step, 0, &scalar_values)?;
                scalar_values.insert(step.node_id, eval_chop_remap(spec, input));
            }
            CompiledOp::SopCircle(spec) => {
                sop_values.insert(step.node_id, SopPrimitive::Circle(spec));
            }
            CompiledOp::SopSphere(spec) => {
                sop_values.insert(step.node_id, SopPrimitive::Sphere(spec));
            }
            CompiledOp::SopGeometry(spec) => {
                let input = require_sop_input(step, 0, &sop_values)?;
                let modulation = optional_scalar_input(step, 1, &scalar_values)?;
                sop_values.insert(step.node_id, apply_sop_geometry(input, spec, modulation));
            }
            CompiledOp::TopCameraRender(spec) => {
                let primitive = require_sop_input(step, 0, &sop_values)?;
                let channel = optional_scalar_input(step, 1, &scalar_values)?;
                let output = output_luma_slot(compiled, step.node_id)?;
                renderer.render_top_camera_to_alias(primitive, spec, channel, output)?;
            }
            CompiledOp::Output(output) => {
                let _ = output;
            }
        }
        telemetry::record_timing(op_scope(step.op), node_start.elapsed());

        validate_release_index(compiled, step_index)?;
    }

    let compositor_start = Instant::now();
    let compositor_plan = build_final_compositor_plan(compiled)?;
    renderer.compose_outputs_to_retained(compositor_plan.primary_slot, &compositor_plan.taps)?;
    telemetry::record_timing("v2.gpu.final_compositor", compositor_start.elapsed());

    Ok(())
}

struct FinalCompositorPlan {
    primary_slot: usize,
    taps: Vec<(u8, usize)>,
}

fn build_final_compositor_plan(
    compiled: &CompiledGraph,
) -> Result<FinalCompositorPlan, Box<dyn Error>> {
    let mut primary_slot = None;
    let mut taps = Vec::new();
    for binding in &compiled.output_bindings {
        let source_slot = output_luma_slot(compiled, binding.source_node)?;
        match binding.role {
            super::node::OutputRole::Primary => {
                primary_slot = Some(source_slot);
            }
            super::node::OutputRole::Tap => taps.push((binding.slot, source_slot)),
        }
    }
    let primary_slot = primary_slot.ok_or("compiled graph has no primary output binding")?;
    taps.sort_by_key(|(slot, _)| *slot);
    Ok(FinalCompositorPlan { primary_slot, taps })
}

fn output_luma_slot(compiled: &CompiledGraph, node_id: NodeId) -> Result<usize, Box<dyn Error>> {
    let lifetime = compiled
        .resource_plan
        .gpu_lifetime_for(node_id)
        .ok_or_else(|| format!("missing gpu lifetime for luma node {:?}", node_id))?;
    if lifetime.kind != CompiledValueKind::Luma {
        return Err(format!("node {:?} is not a gpu luma producer", node_id).into());
    }
    Ok(lifetime.alias_slot)
}

fn output_mask_slot(compiled: &CompiledGraph, node_id: NodeId) -> Result<usize, Box<dyn Error>> {
    let lifetime = compiled
        .resource_plan
        .gpu_lifetime_for(node_id)
        .ok_or_else(|| format!("missing gpu lifetime for mask node {:?}", node_id))?;
    if lifetime.kind != CompiledValueKind::Mask {
        return Err(format!("node {:?} is not a gpu mask producer", node_id).into());
    }
    Ok(lifetime.alias_slot)
}

fn luma_input_slot(
    compiled: &CompiledGraph,
    step: &CompiledNodeStep,
    slot: usize,
) -> Result<usize, Box<dyn Error>> {
    let node_id = input_node(step, slot)?;
    output_luma_slot(compiled, node_id)
}

fn mask_input_slot(
    compiled: &CompiledGraph,
    step: &CompiledNodeStep,
    slot: usize,
) -> Result<usize, Box<dyn Error>> {
    let node_id = input_node(step, slot)?;
    output_mask_slot(compiled, node_id)
}

fn optional_luma_input_slot(
    compiled: &CompiledGraph,
    step: &CompiledNodeStep,
    slot: usize,
) -> Result<Option<usize>, Box<dyn Error>> {
    if slot >= step.inputs.len() {
        return Ok(None);
    }
    Ok(Some(luma_input_slot(compiled, step, slot)?))
}

fn stateful_feedback_slot(
    compiled: &CompiledGraph,
    node_id: NodeId,
) -> Result<usize, Box<dyn Error>> {
    compiled
        .feedback_slots
        .get(&node_id)
        .copied()
        .ok_or_else(|| format!("missing feedback slot mapping for node {:?}", node_id).into())
}

fn require_scalar_input(
    step: &CompiledNodeStep,
    slot: usize,
    values: &HashMap<NodeId, f32>,
) -> Result<f32, Box<dyn Error>> {
    let node = input_node(step, slot)?;
    values
        .get(&node)
        .copied()
        .ok_or_else(|| format!("missing scalar input value from node {:?}", node).into())
}

fn optional_scalar_input(
    step: &CompiledNodeStep,
    slot: usize,
    values: &HashMap<NodeId, f32>,
) -> Result<Option<f32>, Box<dyn Error>> {
    if slot >= step.inputs.len() {
        return Ok(None);
    }
    Ok(Some(require_scalar_input(step, slot, values)?))
}

fn require_sop_input(
    step: &CompiledNodeStep,
    slot: usize,
    values: &HashMap<NodeId, SopPrimitive>,
) -> Result<SopPrimitive, Box<dyn Error>> {
    let node = input_node(step, slot)?;
    values
        .get(&node)
        .copied()
        .ok_or_else(|| format!("missing SOP input value from node {:?}", node).into())
}

fn input_node(step: &CompiledNodeStep, slot: usize) -> Result<NodeId, Box<dyn Error>> {
    step.inputs
        .get(slot)
        .copied()
        .ok_or_else(|| format!("node {:?} missing required input slot {slot}", step.node_id).into())
}

fn op_scope(op: CompiledOp) -> &'static str {
    match op {
        CompiledOp::GenerateLayer(_) => "v2.node.generate_layer",
        CompiledOp::SourceNoise(_) => "v2.node.source_noise",
        CompiledOp::Mask(_) => "v2.node.mask",
        CompiledOp::Blend(_) => "v2.node.blend",
        CompiledOp::ToneMap(_) => "v2.node.tonemap",
        CompiledOp::WarpTransform(_) => "v2.node.warp_transform",
        CompiledOp::StatefulFeedback(_) => "v2.node.stateful_feedback",
        CompiledOp::ChopLfo(_) => "v2.node.chop_lfo",
        CompiledOp::ChopMath(_) => "v2.node.chop_math",
        CompiledOp::ChopRemap(_) => "v2.node.chop_remap",
        CompiledOp::SopCircle(_) => "v2.node.sop_circle",
        CompiledOp::SopSphere(_) => "v2.node.sop_sphere",
        CompiledOp::SopGeometry(_) => "v2.node.sop_geometry",
        CompiledOp::TopCameraRender(_) => "v2.node.top_camera_render",
        CompiledOp::Output(_) => "v2.node.output",
    }
}

fn validate_release_index(
    compiled: &CompiledGraph,
    step_index: usize,
) -> Result<(), Box<dyn Error>> {
    let releases = compiled
        .resource_plan
        .gpu_releases_by_step
        .get(step_index)
        .ok_or("invalid gpu release schedule index")?;

    for node_id in releases {
        let _ = compiled
            .resource_plan
            .gpu_lifetime_for(*node_id)
            .ok_or_else(|| format!("missing gpu lifetime for release node {:?}", node_id))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_final_compositor_plan, output_luma_slot};
    use crate::compiler::compile_graph;
    use crate::graph::{GenerateLayerNode, GraphBuilder};
    use crate::model::LayerBlendMode;
    use crate::node::GenerateLayerTemporal;

    fn sample_layer(seed: u32) -> GenerateLayerNode {
        GenerateLayerNode {
            symmetry: 4,
            symmetry_style: 1,
            iterations: 180,
            seed,
            fill_scale: 1.0,
            fractal_zoom: 0.9,
            art_style: 2,
            art_style_secondary: 3,
            art_style_mix: 0.5,
            bend_strength: 0.3,
            warp_strength: 0.2,
            warp_frequency: 1.8,
            tile_scale: 1.0,
            tile_phase: 0.0,
            center_x: 0.0,
            center_y: 0.0,
            shader_layer_count: 3,
            blend_mode: LayerBlendMode::Normal,
            opacity: 1.0,
            contrast: 1.0,
            temporal: GenerateLayerTemporal::default(),
        }
    }

    #[test]
    fn compositor_plan_uses_primary_and_sorted_taps() {
        let mut builder = GraphBuilder::new(256, 256, 99);
        let a = builder.add_generate_layer(sample_layer(1));
        let b = builder.add_generate_layer(sample_layer(2));
        let primary = builder.add_output();
        let tap3 = builder.add_output_tap(3);
        let tap1 = builder.add_output_tap(1);
        builder.connect_luma(a, tap3);
        builder.connect_luma(b, tap1);
        builder.connect_luma(b, primary);

        let graph = builder.build().expect("graph");
        let compiled = compile_graph(&graph).expect("compiled");
        let plan = build_final_compositor_plan(&compiled).expect("plan");

        assert_eq!(
            plan.primary_slot,
            output_luma_slot(&compiled, b).expect("slot")
        );
        assert_eq!(plan.taps.len(), 2);
        assert_eq!(plan.taps[0].0, 1);
        assert_eq!(plan.taps[1].0, 3);
        assert_eq!(
            plan.taps[0].1,
            output_luma_slot(&compiled, b).expect("tap slot")
        );
        assert_eq!(
            plan.taps[1].1,
            output_luma_slot(&compiled, a).expect("tap slot")
        );
    }
}

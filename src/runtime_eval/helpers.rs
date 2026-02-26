use std::collections::HashMap;
use std::error::Error;

use crate::compiler::{
    CompiledGraph, CompiledNodeStep, CompiledOp, CompiledResourcePlan, CompiledValueKind,
    CompiledValueLifetime,
};
use crate::graph::NodeId;
use crate::proc_graph::SopPrimitive;

pub(super) enum RuntimeValue {
    Luma(Vec<f32>),
    Mask(Vec<f32>),
    Scalar(f32),
    Sop(SopPrimitive),
}

pub(super) fn op_scope(op: CompiledOp) -> &'static str {
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

pub(super) fn release_step_values(
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

pub(super) fn required_lifetime(
    plan: &CompiledResourcePlan,
    node_id: NodeId,
) -> Result<CompiledValueLifetime, Box<dyn Error>> {
    plan.lifetime_for(node_id)
        .ok_or_else(|| format!("missing resource lifetime for node {:?}", node_id).into())
}

pub(super) fn require_luma_input<'a>(
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

pub(super) fn require_mask_input<'a>(
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

pub(super) fn require_luma(
    values: &HashMap<NodeId, RuntimeValue>,
    node_id: NodeId,
) -> Result<&[f32], Box<dyn Error>> {
    match values.get(&node_id) {
        Some(RuntimeValue::Luma(value)) => Ok(value),
        Some(RuntimeValue::Mask(_)) => {
            Err(format!("node {:?} output is mask but luma was required", node_id).into())
        }
        Some(RuntimeValue::Scalar(_)) => {
            Err(format!("node {:?} output is scalar but luma was required", node_id).into())
        }
        Some(RuntimeValue::Sop(_)) => {
            Err(format!("node {:?} output is SOP but luma was required", node_id).into())
        }
        None => Err(format!("missing input value for node {:?}", node_id).into()),
    }
}

fn require_mask(
    values: &HashMap<NodeId, RuntimeValue>,
    node_id: NodeId,
) -> Result<&[f32], Box<dyn Error>> {
    match values.get(&node_id) {
        Some(RuntimeValue::Mask(value)) => Ok(value),
        Some(RuntimeValue::Luma(_)) => {
            Err(format!("node {:?} output is luma but mask was required", node_id).into())
        }
        Some(RuntimeValue::Scalar(_)) => {
            Err(format!("node {:?} output is scalar but mask was required", node_id).into())
        }
        Some(RuntimeValue::Sop(_)) => {
            Err(format!("node {:?} output is SOP but mask was required", node_id).into())
        }
        None => Err(format!("missing input value for node {:?}", node_id).into()),
    }
}

pub(super) fn require_scalar_input(
    values: &HashMap<NodeId, RuntimeValue>,
    step: &CompiledNodeStep,
    slot: usize,
) -> Result<f32, Box<dyn Error>> {
    let node_id = *step
        .inputs
        .get(slot)
        .ok_or_else(|| format!("node {:?} missing required input slot {slot}", step.node_id))?;
    match values.get(&node_id) {
        Some(RuntimeValue::Scalar(value)) => Ok(*value),
        Some(RuntimeValue::Luma(_)) => {
            Err(format!("node {:?} output is luma but scalar was required", node_id).into())
        }
        Some(RuntimeValue::Mask(_)) => {
            Err(format!("node {:?} output is mask but scalar was required", node_id).into())
        }
        Some(RuntimeValue::Sop(_)) => {
            Err(format!("node {:?} output is SOP but scalar was required", node_id).into())
        }
        None => Err(format!("missing input value for node {:?}", node_id).into()),
    }
}

pub(super) fn optional_scalar_input(
    values: &HashMap<NodeId, RuntimeValue>,
    step: &CompiledNodeStep,
    slot: usize,
) -> Result<Option<f32>, Box<dyn Error>> {
    if slot >= step.inputs.len() {
        return Ok(None);
    }
    Ok(Some(require_scalar_input(values, step, slot)?))
}

pub(super) fn require_sop_input(
    values: &HashMap<NodeId, RuntimeValue>,
    step: &CompiledNodeStep,
    slot: usize,
) -> Result<SopPrimitive, Box<dyn Error>> {
    let node_id = *step
        .inputs
        .get(slot)
        .ok_or_else(|| format!("node {:?} missing required input slot {slot}", step.node_id))?;
    match values.get(&node_id) {
        Some(RuntimeValue::Sop(value)) => Ok(*value),
        Some(RuntimeValue::Luma(_)) => {
            Err(format!("node {:?} output is luma but SOP was required", node_id).into())
        }
        Some(RuntimeValue::Mask(_)) => {
            Err(format!("node {:?} output is mask but SOP was required", node_id).into())
        }
        Some(RuntimeValue::Scalar(_)) => {
            Err(format!("node {:?} output is scalar but SOP was required", node_id).into())
        }
        None => Err(format!("missing input value for node {:?}", node_id).into()),
    }
}

pub(super) fn pixel_count(width: u32, height: u32) -> Result<usize, Box<dyn Error>> {
    width
        .checked_mul(height)
        .map(|count| count as usize)
        .ok_or("invalid pixel dimensions".into())
}

pub(super) struct AliasedResourceArena {
    pixel_count: usize,
    luma_slots: Vec<Option<Vec<f32>>>,
    mask_slots: Vec<Option<Vec<f32>>>,
}

impl AliasedResourceArena {
    pub(super) fn new(plan: &CompiledResourcePlan, pixel_count: usize) -> Self {
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

    pub(super) fn acquire_for(&mut self, lifetime: CompiledValueLifetime) -> Vec<f32> {
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
            RuntimeValue::Scalar(_) | RuntimeValue::Sop(_) => return Ok(()),
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

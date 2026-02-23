//! Resource lifetime planning for compiled V2 graph steps.

use std::collections::HashMap;

use crate::v2::graph::{GraphBuildError, NodeId};
use crate::v2::node::PortType;

use super::{
    CompiledNodeStep, CompiledOp, CompiledResourcePlan, CompiledValueKind, CompiledValueLifetime,
};

pub(super) fn build_resource_plan(
    steps: &[CompiledNodeStep],
) -> Result<CompiledResourcePlan, GraphBuildError> {
    let mut host_lifetimes = collect_lifetimes(steps, output_kind);
    let mut gpu_lifetimes = collect_lifetimes(steps, gpu_output_kind);

    let peak_luma_slots = assign_alias_slots(&mut host_lifetimes, CompiledValueKind::Luma);
    let peak_mask_slots = assign_alias_slots(&mut host_lifetimes, CompiledValueKind::Mask);
    let gpu_peak_luma_slots = assign_alias_slots(&mut gpu_lifetimes, CompiledValueKind::Luma);
    let gpu_peak_mask_slots = assign_alias_slots(&mut gpu_lifetimes, CompiledValueKind::Mask);

    let releases_by_step = build_releases(steps.len(), &host_lifetimes);
    let gpu_releases_by_step = build_releases(steps.len(), &gpu_lifetimes);

    Ok(CompiledResourcePlan {
        lifetimes: host_lifetimes,
        gpu_lifetimes,
        releases_by_step,
        gpu_releases_by_step,
        peak_luma_slots,
        peak_mask_slots,
        gpu_peak_luma_slots,
        gpu_peak_mask_slots,
    })
}

fn collect_lifetimes(
    steps: &[CompiledNodeStep],
    output_selector: fn(CompiledOp) -> Option<CompiledValueKind>,
) -> HashMap<NodeId, CompiledValueLifetime> {
    let mut lifetimes = HashMap::with_capacity(steps.len());
    for (step_index, step) in steps.iter().enumerate() {
        if let Some(kind) = output_selector(step.op) {
            lifetimes.insert(
                step.node_id,
                CompiledValueLifetime {
                    kind,
                    first_step: step_index,
                    last_step: step_index,
                    alias_slot: 0,
                },
            );
        }
    }

    for (step_index, step) in steps.iter().enumerate() {
        for input in &step.inputs {
            if let Some(lifetime) = lifetimes.get_mut(input) {
                lifetime.last_step = lifetime.last_step.max(step_index);
            }
        }
    }
    lifetimes
}

fn build_releases(
    step_count: usize,
    lifetimes: &HashMap<NodeId, CompiledValueLifetime>,
) -> Vec<Vec<NodeId>> {
    let mut releases_by_step = vec![Vec::new(); step_count];
    for (node_id, lifetime) in lifetimes {
        releases_by_step[lifetime.last_step].push(*node_id);
    }
    for releases in &mut releases_by_step {
        releases.sort_by_key(|node_id| node_id.0);
    }
    releases_by_step
}

fn assign_alias_slots(
    lifetimes: &mut HashMap<NodeId, CompiledValueLifetime>,
    kind: CompiledValueKind,
) -> usize {
    let mut values: Vec<(usize, NodeId, usize)> = lifetimes
        .iter()
        .filter_map(|(node_id, lifetime)| {
            (lifetime.kind == kind).then_some((lifetime.first_step, *node_id, lifetime.last_step))
        })
        .collect();
    values.sort_by_key(|(first_step, node_id, _)| (*first_step, node_id.0));

    let mut active: Vec<(usize, usize)> = Vec::new();
    let mut free_slots = Vec::new();
    let mut next_slot = 0usize;

    for (first_step, node_id, last_step) in values {
        let mut index = 0usize;
        while index < active.len() {
            if active[index].1 < first_step {
                free_slots.push(active.swap_remove(index).0);
            } else {
                index += 1;
            }
        }

        let alias_slot = if let Some(slot) = free_slots.pop() {
            slot
        } else {
            let slot = next_slot;
            next_slot += 1;
            slot
        };

        if let Some(lifetime) = lifetimes.get_mut(&node_id) {
            lifetime.alias_slot = alias_slot;
        }
        active.push((alias_slot, last_step));
    }

    next_slot
}

fn output_kind(op: CompiledOp) -> Option<CompiledValueKind> {
    match op {
        CompiledOp::GenerateLayer(_)
        | CompiledOp::Blend(_)
        | CompiledOp::ToneMap(_)
        | CompiledOp::WarpTransform(_) => Some(CompiledValueKind::Luma),
        CompiledOp::SourceNoise(spec) => match spec.output_port {
            PortType::LumaTexture => Some(CompiledValueKind::Luma),
            PortType::MaskTexture => Some(CompiledValueKind::Mask),
        },
        CompiledOp::Mask(_) => Some(CompiledValueKind::Mask),
        CompiledOp::Output => None,
    }
}

fn gpu_output_kind(op: CompiledOp) -> Option<CompiledValueKind> {
    match op {
        CompiledOp::GenerateLayer(_) => Some(CompiledValueKind::Luma),
        CompiledOp::SourceNoise(spec) => match spec.output_port {
            PortType::LumaTexture => Some(CompiledValueKind::Luma),
            PortType::MaskTexture => Some(CompiledValueKind::Mask),
        },
        CompiledOp::Mask(_) => Some(CompiledValueKind::Mask),
        CompiledOp::Blend(_) | CompiledOp::ToneMap(_) | CompiledOp::WarpTransform(_) => {
            Some(CompiledValueKind::Luma)
        }
        CompiledOp::Output => None,
    }
}

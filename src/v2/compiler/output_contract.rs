//! Output binding helpers for compiled V2 graphs.

#[cfg(test)]
use std::collections::{HashMap, HashSet, VecDeque};

use crate::v2::graph::GraphBuildError;
#[cfg(test)]
use crate::v2::graph::NodeId;
#[cfg(test)]
use crate::v2::node::OutputNode;
use crate::v2::node::OutputRole;

use super::{CompiledNodeStep, CompiledOp, CompiledOutputBinding};

/// Collect output bindings from compiled steps and validate output contract.
pub(super) fn collect_output_bindings(
    steps: &[CompiledNodeStep],
) -> Result<Vec<CompiledOutputBinding>, GraphBuildError> {
    let mut bindings = Vec::new();
    for step in steps {
        if let CompiledOp::Output(output) = step.op {
            if step.inputs.len() != 1 {
                return Err(GraphBuildError::new(format!(
                    "output node {:?} requires exactly one input",
                    step.node_id
                )));
            }
            bindings.push(CompiledOutputBinding {
                output_node: step.node_id,
                source_node: step.inputs[0],
                role: output.role,
                slot: output.slot,
            });
        }
    }

    if bindings.is_empty() {
        return Err(GraphBuildError::new("compiled graph has no output node"));
    }
    let primary_count = bindings
        .iter()
        .filter(|binding| matches!(binding.role, OutputRole::Primary))
        .count();
    if primary_count != 1 {
        return Err(GraphBuildError::new(format!(
            "compiled graph requires exactly one primary output, got {}",
            primary_count
        )));
    }
    Ok(bindings)
}

/// Detect whether a graph remains a simple retained linear layer path.
#[cfg(test)]
pub(super) fn detect_linear_layer_path(
    steps: &[CompiledNodeStep],
    incoming: &HashMap<NodeId, Vec<(u8, NodeId)>>,
    primary_output_node: NodeId,
    has_non_layer_nodes: bool,
) -> Result<bool, GraphBuildError> {
    if has_non_layer_nodes {
        return Ok(false);
    }

    let mut reachable = HashSet::with_capacity(steps.len());
    let mut queue = VecDeque::from([primary_output_node]);
    while let Some(current) = queue.pop_front() {
        if !reachable.insert(current) {
            continue;
        }
        let parents = incoming.get(&current).ok_or_else(|| {
            GraphBuildError::new("missing incoming table entry during path analysis")
        })?;
        for (_, source) in parents {
            queue.push_back(*source);
        }
    }

    let tap_output_count = steps
        .iter()
        .filter(|step| {
            matches!(
                step.op,
                CompiledOp::Output(OutputNode {
                    role: OutputRole::Tap,
                    ..
                })
            )
        })
        .count();
    if reachable.len().saturating_add(tap_output_count) != steps.len() {
        return Ok(false);
    }

    let tap_output_nodes: HashSet<NodeId> = steps
        .iter()
        .filter_map(|step| {
            matches!(
                step.op,
                CompiledOp::Output(OutputNode {
                    role: OutputRole::Tap,
                    ..
                })
            )
            .then_some(step.node_id)
        })
        .collect();

    let mut outgoing_reachable = HashMap::with_capacity(steps.len());
    for step in steps {
        outgoing_reachable.insert(step.node_id, 0usize);
    }
    for step in steps {
        if tap_output_nodes.contains(&step.node_id) {
            continue;
        }
        for input in &step.inputs {
            if let Some(count) = outgoing_reachable.get_mut(input) {
                *count += 1;
            }
        }
    }

    let mut roots = 0usize;
    for step in steps {
        match step.op {
            CompiledOp::GenerateLayer(_) => {
                if step.inputs.len() > 1 {
                    return Ok(false);
                }
                if step.inputs.is_empty() {
                    roots += 1;
                }
                if outgoing_reachable.get(&step.node_id).copied().unwrap_or(0) != 1 {
                    return Ok(false);
                }
            }
            CompiledOp::Output(output) => match output.role {
                OutputRole::Primary => {
                    if step.node_id != primary_output_node || step.inputs.len() != 1 {
                        return Ok(false);
                    }
                }
                OutputRole::Tap => {
                    if step.inputs.len() != 1 {
                        return Ok(false);
                    }
                }
            },
            _ => return Ok(false),
        }
    }

    Ok(roots == 1)
}

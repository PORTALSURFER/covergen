//! Graph compiler for the V2 GPU node runtime.

use std::collections::{HashMap, HashSet, VecDeque};

use super::graph::{EdgeSpec, GpuGraph, GraphBuildError, NodeId, NodeSpec};
use super::node::{
    BlendNode, GenerateLayerNode, MaskNode, NodeKind, PortType, SourceNoiseNode, ToneMapNode,
    WarpTransformNode,
};

/// Executable node operation in compiled graph order.
#[derive(Clone, Copy, Debug)]
pub enum CompiledOp {
    GenerateLayer(GenerateLayerNode),
    SourceNoise(SourceNoiseNode),
    Mask(MaskNode),
    Blend(BlendNode),
    ToneMap(ToneMapNode),
    WarpTransform(WarpTransformNode),
    Output,
}

/// One scheduled node in a compiled graph.
#[derive(Clone, Debug)]
pub struct CompiledNodeStep {
    pub node_id: NodeId,
    pub op: CompiledOp,
    pub inputs: Vec<NodeId>,
}

/// Runtime value kind used for transient resource planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompiledValueKind {
    Luma,
    Mask,
}

/// Lifetime and alias slot for one produced node value.
#[derive(Clone, Copy, Debug)]
pub struct CompiledValueLifetime {
    pub kind: CompiledValueKind,
    pub first_step: usize,
    pub last_step: usize,
    pub alias_slot: usize,
}

#[derive(Clone, Debug)]
pub struct CompiledResourcePlan {
    pub lifetimes: HashMap<NodeId, CompiledValueLifetime>,
    pub releases_by_step: Vec<Vec<NodeId>>,
    pub peak_luma_slots: usize,
    pub peak_mask_slots: usize,
}

impl CompiledResourcePlan {
    pub fn lifetime_for(&self, node_id: NodeId) -> Option<CompiledValueLifetime> {
        self.lifetimes.get(&node_id).copied()
    }
}

#[derive(Clone, Debug)]
pub struct CompiledGraph {
    pub width: u32,
    pub height: u32,
    pub seed: u32,
    pub steps: Vec<CompiledNodeStep>,
    pub output_node: NodeId,
    pub has_non_layer_nodes: bool,
    pub can_use_retained_layer_path: bool,
    pub resource_plan: CompiledResourcePlan,
}

pub fn compile_graph(graph: &GpuGraph) -> Result<CompiledGraph, GraphBuildError> {
    let topo_order = topological_order(&graph.nodes, &graph.edges)?;

    let mut node_map = HashMap::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        node_map.insert(node.id, node.kind);
    }

    let mut incoming: HashMap<NodeId, Vec<(u8, NodeId)>> =
        HashMap::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        incoming.insert(node.id, Vec::new());
    }
    for edge in &graph.edges {
        incoming
            .get_mut(&edge.to)
            .ok_or_else(|| GraphBuildError::new("edge target missing during compilation"))?
            .push((edge.to_input, edge.from));
    }

    let mut output_node = None;
    let mut steps = Vec::with_capacity(graph.nodes.len());
    let mut has_non_layer_nodes = false;

    for node_id in topo_order {
        let kind = node_map
            .get(&node_id)
            .copied()
            .ok_or_else(|| GraphBuildError::new("topology references missing node"))?;

        let mut inputs = incoming.get(&node_id).cloned().unwrap_or_default();
        inputs.sort_by_key(|(slot, _)| *slot);
        let inputs: Vec<NodeId> = inputs.into_iter().map(|(_, source)| source).collect();

        let op = match kind {
            NodeKind::GenerateLayer(spec) => CompiledOp::GenerateLayer(spec),
            NodeKind::SourceNoise(spec) => {
                has_non_layer_nodes = true;
                CompiledOp::SourceNoise(spec)
            }
            NodeKind::Mask(spec) => {
                has_non_layer_nodes = true;
                CompiledOp::Mask(spec)
            }
            NodeKind::Blend(spec) => {
                has_non_layer_nodes = true;
                CompiledOp::Blend(spec)
            }
            NodeKind::ToneMap(spec) => {
                has_non_layer_nodes = true;
                CompiledOp::ToneMap(spec)
            }
            NodeKind::WarpTransform(spec) => {
                has_non_layer_nodes = true;
                CompiledOp::WarpTransform(spec)
            }
            NodeKind::Output => {
                if output_node.is_some() {
                    return Err(GraphBuildError::new(
                        "multiple output nodes are not supported by current V2 runtime",
                    ));
                }
                output_node = Some(node_id);
                CompiledOp::Output
            }
        };

        steps.push(CompiledNodeStep {
            node_id,
            op,
            inputs,
        });
    }

    if steps.is_empty() {
        return Err(GraphBuildError::new(
            "compiled graph contains no executable nodes",
        ));
    }

    let output_node =
        output_node.ok_or_else(|| GraphBuildError::new("compiled graph has no output node"))?;
    let can_use_retained_layer_path =
        detect_linear_layer_path(&steps, &incoming, output_node, has_non_layer_nodes)?;
    let resource_plan = build_resource_plan(&steps)?;

    Ok(CompiledGraph {
        width: graph.width,
        height: graph.height,
        seed: graph.seed,
        steps,
        output_node,
        has_non_layer_nodes,
        can_use_retained_layer_path,
        resource_plan,
    })
}

fn build_resource_plan(
    steps: &[CompiledNodeStep],
) -> Result<CompiledResourcePlan, GraphBuildError> {
    let mut lifetimes = HashMap::with_capacity(steps.len());

    for (step_index, step) in steps.iter().enumerate() {
        if let Some(kind) = output_kind(step.op) {
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
            let lifetime = lifetimes.get_mut(input).ok_or_else(|| {
                GraphBuildError::new(format!(
                    "resource lifetime analysis missing producer for node {:?}",
                    input
                ))
            })?;
            lifetime.last_step = lifetime.last_step.max(step_index);
        }
    }

    let peak_luma_slots = assign_alias_slots(&mut lifetimes, CompiledValueKind::Luma);
    let peak_mask_slots = assign_alias_slots(&mut lifetimes, CompiledValueKind::Mask);

    let mut releases_by_step = vec![Vec::new(); steps.len()];
    for (node_id, lifetime) in &lifetimes {
        releases_by_step[lifetime.last_step].push(*node_id);
    }
    for releases in &mut releases_by_step {
        releases.sort_by_key(|node_id| node_id.0);
    }

    Ok(CompiledResourcePlan {
        lifetimes,
        releases_by_step,
        peak_luma_slots,
        peak_mask_slots,
    })
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

fn detect_linear_layer_path(
    steps: &[CompiledNodeStep],
    incoming: &HashMap<NodeId, Vec<(u8, NodeId)>>,
    output_node: NodeId,
    has_non_layer_nodes: bool,
) -> Result<bool, GraphBuildError> {
    if has_non_layer_nodes {
        return Ok(false);
    }

    let mut reachable = HashSet::with_capacity(steps.len());
    let mut queue = VecDeque::from([output_node]);
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

    if reachable.len() != steps.len() {
        return Ok(false);
    }

    let mut outgoing_reachable = HashMap::with_capacity(steps.len());
    for step in steps {
        outgoing_reachable.insert(step.node_id, 0usize);
    }
    for step in steps {
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
            CompiledOp::Output => {
                if step.node_id != output_node || step.inputs.len() != 1 {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }

    Ok(roots == 1)
}

fn topological_order(
    nodes: &[NodeSpec],
    edges: &[EdgeSpec],
) -> Result<Vec<NodeId>, GraphBuildError> {
    let mut indegree = HashMap::with_capacity(nodes.len());
    let mut adjacency: HashMap<NodeId, Vec<NodeId>> = HashMap::with_capacity(nodes.len());

    for node in nodes {
        indegree.insert(node.id, 0usize);
        adjacency.insert(node.id, Vec::new());
    }

    for edge in edges {
        *indegree
            .get_mut(&edge.to)
            .ok_or_else(|| GraphBuildError::new("edge target missing in topological pass"))? += 1;
        adjacency
            .get_mut(&edge.from)
            .ok_or_else(|| GraphBuildError::new("edge source missing in topological pass"))?
            .push(edge.to);
    }

    let mut queue = VecDeque::new();
    for node in nodes {
        if indegree.get(&node.id).copied().unwrap_or(0) == 0 {
            queue.push_back(node.id);
        }
    }

    let mut order = Vec::with_capacity(nodes.len());
    while let Some(current) = queue.pop_front() {
        order.push(current);
        if let Some(next) = adjacency.get(&current) {
            for target in next {
                if let Some(value) = indegree.get_mut(target) {
                    *value -= 1;
                    if *value == 0 {
                        queue.push_back(*target);
                    }
                }
            }
        }
    }

    if order.len() != nodes.len() {
        return Err(GraphBuildError::new(
            "topological ordering failed because graph is cyclic",
        ));
    }

    Ok(order)
}

#[cfg(test)]
#[path = "compiler_tests.rs"]
mod tests;

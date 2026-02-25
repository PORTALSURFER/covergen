//! Graph compiler for the V2 GPU node runtime.

use std::collections::{HashMap, VecDeque};

use super::graph::{EdgeSpec, GpuGraph, GraphBuildError, NodeId, NodeSpec};
use super::node::{
    BlendNode, GenerateLayerNode, MaskNode, NodeKind, OutputNode, OutputRole, SourceNoiseNode,
    StatefulFeedbackNode, ToneMapNode, WarpTransformNode,
};
use crate::chop::{ChopLfoNode, ChopMathNode, ChopRemapNode};
use crate::sop::{SopCircleNode, SopGeometryNode, SopSphereNode, TopCameraRenderNode};
use output_contract::collect_output_bindings;
#[cfg(test)]
use output_contract::detect_linear_layer_path;
use resource_plan::build_resource_plan;

mod output_contract;
mod resource_plan;

/// Executable node operation in compiled graph order.
#[derive(Clone, Copy, Debug)]
pub enum CompiledOp {
    GenerateLayer(GenerateLayerNode),
    SourceNoise(SourceNoiseNode),
    Mask(MaskNode),
    Blend(BlendNode),
    ToneMap(ToneMapNode),
    WarpTransform(WarpTransformNode),
    StatefulFeedback(StatefulFeedbackNode),
    ChopLfo(ChopLfoNode),
    ChopMath(ChopMathNode),
    ChopRemap(ChopRemapNode),
    SopCircle(SopCircleNode),
    SopSphere(SopSphereNode),
    SopGeometry(SopGeometryNode),
    TopCameraRender(TopCameraRenderNode),
    Output(OutputNode),
}

/// One scheduled node in a compiled graph.
#[derive(Clone, Debug)]
pub struct CompiledNodeStep {
    pub node_id: NodeId,
    pub op: CompiledOp,
    pub inputs: Vec<NodeId>,
}

/// Output binding produced by one compiled output node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompiledOutputBinding {
    pub output_node: NodeId,
    pub source_node: NodeId,
    pub role: OutputRole,
    pub slot: u8,
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
    /// Host-side transient lifetimes used by current mixed CPU execution.
    #[cfg(test)]
    pub lifetimes: HashMap<NodeId, CompiledValueLifetime>,
    /// GPU-capable transient lifetimes used for buffer alias planning.
    pub gpu_lifetimes: HashMap<NodeId, CompiledValueLifetime>,
    /// Host-side release schedule keyed by producer last-use step.
    #[cfg(test)]
    pub releases_by_step: Vec<Vec<NodeId>>,
    /// GPU-side release schedule keyed by producer last-use step.
    pub gpu_releases_by_step: Vec<Vec<NodeId>>,
    #[cfg(test)]
    pub peak_luma_slots: usize,
    #[cfg(test)]
    pub peak_mask_slots: usize,
    pub gpu_peak_luma_slots: usize,
    pub gpu_peak_mask_slots: usize,
}

impl CompiledResourcePlan {
    #[cfg(test)]
    pub fn lifetime_for(&self, node_id: NodeId) -> Option<CompiledValueLifetime> {
        self.lifetimes.get(&node_id).copied()
    }

    pub fn gpu_lifetime_for(&self, node_id: NodeId) -> Option<CompiledValueLifetime> {
        self.gpu_lifetimes.get(&node_id).copied()
    }
}

#[derive(Clone, Debug)]
pub struct CompiledGraph {
    pub width: u32,
    pub height: u32,
    pub seed: u32,
    pub steps: Vec<CompiledNodeStep>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub primary_output_node: NodeId,
    pub output_bindings: Vec<CompiledOutputBinding>,
    /// Persistent GPU feedback slot index for each stateful feedback node.
    pub feedback_slots: HashMap<NodeId, usize>,
    #[cfg(test)]
    pub has_non_layer_nodes: bool,
    #[cfg(test)]
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

    let mut steps = Vec::with_capacity(graph.nodes.len());
    #[cfg(test)]
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
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::SourceNoise(spec)
            }
            NodeKind::Mask(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::Mask(spec)
            }
            NodeKind::Blend(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::Blend(spec)
            }
            NodeKind::ToneMap(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::ToneMap(spec)
            }
            NodeKind::WarpTransform(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::WarpTransform(spec)
            }
            NodeKind::StatefulFeedback(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::StatefulFeedback(spec)
            }
            NodeKind::ChopLfo(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::ChopLfo(spec)
            }
            NodeKind::ChopMath(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::ChopMath(spec)
            }
            NodeKind::ChopRemap(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::ChopRemap(spec)
            }
            NodeKind::SopCircle(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::SopCircle(spec)
            }
            NodeKind::SopSphere(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::SopSphere(spec)
            }
            NodeKind::SopGeometry(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::SopGeometry(spec)
            }
            NodeKind::TopCameraRender(spec) => {
                #[cfg(test)]
                {
                    has_non_layer_nodes = true;
                }
                CompiledOp::TopCameraRender(spec)
            }
            NodeKind::Output(output) => CompiledOp::Output(output),
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

    let output_bindings = collect_output_bindings(&steps)?;
    let feedback_slots = collect_feedback_slots(&steps);
    let primary_output = output_bindings
        .iter()
        .copied()
        .find(|binding| matches!(binding.role, OutputRole::Primary))
        .ok_or_else(|| GraphBuildError::new("compiled graph has no primary output node"))?;
    let primary_output_node = primary_output.output_node;
    #[cfg(test)]
    let can_use_retained_layer_path =
        detect_linear_layer_path(&steps, &incoming, primary_output_node, has_non_layer_nodes)?;
    let resource_plan = build_resource_plan(&steps)?;

    Ok(CompiledGraph {
        width: graph.width,
        height: graph.height,
        seed: graph.seed,
        steps,
        primary_output_node,
        output_bindings,
        feedback_slots,
        #[cfg(test)]
        has_non_layer_nodes,
        #[cfg(test)]
        can_use_retained_layer_path,
        resource_plan,
    })
}

fn collect_feedback_slots(steps: &[CompiledNodeStep]) -> HashMap<NodeId, usize> {
    let mut slots = HashMap::new();
    for step in steps {
        if matches!(step.op, CompiledOp::StatefulFeedback(_)) {
            let next_slot = slots.len();
            slots.insert(step.node_id, next_slot);
        }
    }
    slots
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

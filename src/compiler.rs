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
#[allow(clippy::large_enum_variant)]
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
    /// Dense slot index for this node in runtime-side transient value vectors.
    pub node_index: usize,
    pub op: CompiledOp,
    pub inputs: Vec<NodeId>,
    /// Dense slot index per input node, aligned with `inputs`.
    pub input_indices: Vec<usize>,
}

/// Output binding produced by one compiled output node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompiledOutputBinding {
    pub output_node: NodeId,
    pub source_node: NodeId,
    pub role: OutputRole,
    pub slot: u8,
}

/// Precomputed final-output compositor bindings for runtime submission.
///
/// This plan is immutable for one compiled graph and avoids per-frame output
/// binding scans and tap sorting in the GPU runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledFinalCompositorPlan {
    /// Alias slot used as the retained primary output source.
    pub primary_slot: usize,
    /// Sorted tap bindings as `(tap_slot, alias_slot)`.
    pub taps: Vec<(u8, usize)>,
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

    #[cfg(test)]
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
    /// Precomputed final-output compositor plan reused by every runtime frame.
    pub final_compositor_plan: CompiledFinalCompositorPlan,
    /// Compile-time `NodeId -> step index` map shared by runtime adapters.
    #[cfg(test)]
    pub node_indices: HashMap<NodeId, usize>,
    /// Precomputed GPU alias slots for luma-producing nodes.
    pub gpu_luma_slots: HashMap<NodeId, usize>,
    /// Precomputed GPU alias slots for mask-producing nodes.
    pub gpu_mask_slots: HashMap<NodeId, usize>,
    /// Persistent GPU feedback slot index for each stateful feedback node.
    pub feedback_slots: HashMap<NodeId, usize>,
    #[cfg(test)]
    pub has_non_layer_nodes: bool,
    #[cfg(test)]
    pub can_use_retained_layer_path: bool,
    pub resource_plan: CompiledResourcePlan,
}

impl CompiledGraph {
    /// Return dense runtime slot index for one compiled node id.
    #[cfg(test)]
    pub fn node_index(&self, node_id: NodeId) -> Option<usize> {
        self.node_indices.get(&node_id).copied()
    }
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
    let mut node_indices = HashMap::with_capacity(graph.nodes.len());
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
        let mut input_indices = Vec::with_capacity(inputs.len());
        for source in &inputs {
            let Some(index) = node_indices.get(source).copied() else {
                return Err(GraphBuildError::new(format!(
                    "compile topology missing step index for input node {:?}",
                    source
                )));
            };
            input_indices.push(index);
        }

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

        let node_index = steps.len();
        steps.push(CompiledNodeStep {
            node_id,
            node_index,
            op,
            inputs,
            input_indices,
        });
        node_indices.insert(node_id, node_index);
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
    validate_gpu_release_schedule(&resource_plan, &node_indices, steps.len())?;
    let (gpu_luma_slots, gpu_mask_slots) = build_gpu_slot_maps(&resource_plan);
    let final_compositor_plan = build_final_compositor_plan(&output_bindings, &gpu_luma_slots)?;

    Ok(CompiledGraph {
        width: graph.width,
        height: graph.height,
        seed: graph.seed,
        steps,
        primary_output_node,
        output_bindings,
        final_compositor_plan,
        #[cfg(test)]
        node_indices,
        gpu_luma_slots,
        gpu_mask_slots,
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

fn validate_gpu_release_schedule(
    resource_plan: &CompiledResourcePlan,
    node_indices: &HashMap<NodeId, usize>,
    step_count: usize,
) -> Result<(), GraphBuildError> {
    if resource_plan.gpu_releases_by_step.len() != step_count {
        return Err(GraphBuildError::new(format!(
            "gpu release schedule length {} does not match step count {}",
            resource_plan.gpu_releases_by_step.len(),
            step_count
        )));
    }

    for (step_index, releases) in resource_plan.gpu_releases_by_step.iter().enumerate() {
        for node_id in releases {
            let node_index = node_indices.get(node_id).copied().ok_or_else(|| {
                GraphBuildError::new(format!(
                    "missing compiled node index for release node {:?}",
                    node_id
                ))
            })?;
            if node_index >= step_count {
                return Err(GraphBuildError::new(format!(
                    "compiled node index {} out of bounds for release node {:?}",
                    node_index, node_id
                )));
            }
            let lifetime = resource_plan
                .gpu_lifetimes
                .get(node_id)
                .copied()
                .ok_or_else(|| {
                    GraphBuildError::new(format!(
                        "missing gpu lifetime for release node {:?}",
                        node_id
                    ))
                })?;
            if lifetime.last_step != step_index {
                return Err(GraphBuildError::new(format!(
                    "gpu release schedule mismatch for node {:?}: expected step {}, got {}",
                    node_id, lifetime.last_step, step_index
                )));
            }
        }
    }
    Ok(())
}

fn build_gpu_slot_maps(
    resource_plan: &CompiledResourcePlan,
) -> (HashMap<NodeId, usize>, HashMap<NodeId, usize>) {
    let mut gpu_luma_slots = HashMap::with_capacity(resource_plan.gpu_lifetimes.len());
    let mut gpu_mask_slots = HashMap::with_capacity(resource_plan.gpu_lifetimes.len());
    for (node_id, lifetime) in &resource_plan.gpu_lifetimes {
        match lifetime.kind {
            CompiledValueKind::Luma => {
                gpu_luma_slots.insert(*node_id, lifetime.alias_slot);
            }
            CompiledValueKind::Mask => {
                gpu_mask_slots.insert(*node_id, lifetime.alias_slot);
            }
        }
    }
    (gpu_luma_slots, gpu_mask_slots)
}

fn build_final_compositor_plan(
    output_bindings: &[CompiledOutputBinding],
    gpu_luma_slots: &HashMap<NodeId, usize>,
) -> Result<CompiledFinalCompositorPlan, GraphBuildError> {
    let mut primary_slot = None;
    let mut taps = Vec::new();
    for binding in output_bindings {
        let source_slot = gpu_luma_slots
            .get(&binding.source_node)
            .copied()
            .ok_or_else(|| {
                GraphBuildError::new(format!(
                    "missing precomputed luma slot for output source node {:?}",
                    binding.source_node
                ))
            })?;
        match binding.role {
            OutputRole::Primary => {
                primary_slot = Some(source_slot);
            }
            OutputRole::Tap => taps.push((binding.slot, source_slot)),
        }
    }
    let primary_slot =
        primary_slot.ok_or_else(|| GraphBuildError::new("compiled graph has no primary output"))?;
    taps.sort_by_key(|(slot, _)| *slot);
    Ok(CompiledFinalCompositorPlan { primary_slot, taps })
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

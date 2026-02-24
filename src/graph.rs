//! Typed graph model and validator for the V2 generated pipeline.

use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub use super::node::{
    BlendNode, GenerateLayerNode, MaskNode, NodeKind, OperatorFamily, OutputNode, OutputRole,
    PortType, SourceNoiseNode, ToneMapNode, WarpTransformNode,
};

/// Stable node identifier used across builder, compiler, and runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// Immutable node descriptor in a validated graph.
#[derive(Clone, Copy, Debug)]
pub struct NodeSpec {
    pub id: NodeId,
    pub kind: NodeKind,
}

/// Directed edge connecting typed ports between nodes.
#[derive(Clone, Copy, Debug)]
pub struct EdgeSpec {
    pub from: NodeId,
    pub to: NodeId,
    pub from_port: PortType,
    pub to_port: PortType,
    pub to_input: u8,
}

/// Fully built and validated graph for compiler input.
#[derive(Clone, Debug)]
pub struct GpuGraph {
    pub width: u32,
    pub height: u32,
    pub seed: u32,
    pub nodes: Vec<NodeSpec>,
    pub edges: Vec<EdgeSpec>,
}

/// Graph validation/build errors.
#[derive(Debug, Clone)]
pub struct GraphBuildError {
    message: String,
}

impl GraphBuildError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for GraphBuildError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for GraphBuildError {}

/// Programmatic graph builder used by generated presets.
#[derive(Debug)]
pub struct GraphBuilder {
    width: u32,
    height: u32,
    seed: u32,
    next_id: u32,
    nodes: Vec<NodeSpec>,
    edges: Vec<EdgeSpec>,
}

impl GraphBuilder {
    /// Create a new builder for a target output size and deterministic seed.
    pub fn new(width: u32, height: u32, seed: u32) -> Self {
        Self {
            width,
            height,
            seed,
            next_id: 0,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Add a layer-generation node.
    pub fn add_generate_layer(&mut self, params: GenerateLayerNode) -> NodeId {
        self.add_node(NodeKind::GenerateLayer(params))
    }

    /// Add a source-noise node.
    pub fn add_source_noise(&mut self, spec: SourceNoiseNode) -> NodeId {
        self.add_node(NodeKind::SourceNoise(spec))
    }

    /// Add a mask extraction node.
    pub fn add_mask(&mut self, spec: MaskNode) -> NodeId {
        self.add_node(NodeKind::Mask(spec))
    }

    /// Add an explicit blend node.
    pub fn add_blend(&mut self, spec: BlendNode) -> NodeId {
        self.add_node(NodeKind::Blend(spec))
    }

    /// Add a tone-map node.
    pub fn add_tonemap(&mut self, spec: ToneMapNode) -> NodeId {
        self.add_node(NodeKind::ToneMap(spec))
    }

    /// Add a warp/transform node.
    pub fn add_warp_transform(&mut self, spec: WarpTransformNode) -> NodeId {
        self.add_node(NodeKind::WarpTransform(spec))
    }

    /// Add one output node.
    pub fn add_output(&mut self) -> NodeId {
        self.add_output_with_contract(OutputNode::primary())
    }

    /// Add one non-primary tap output for parallel output products.
    pub fn add_output_tap(&mut self, slot: u8) -> NodeId {
        self.add_output_with_contract(OutputNode::tap(slot))
    }

    /// Add one output node with explicit output contract.
    pub fn add_output_with_contract(&mut self, output: OutputNode) -> NodeId {
        self.add_node(NodeKind::Output(output))
    }

    /// Connect luma output of `from` to the first luma input of `to`.
    pub fn connect_luma(&mut self, from: NodeId, to: NodeId) {
        self.connect_luma_input(from, to, 0);
    }

    /// Connect luma output of `from` to luma input `to_input` of `to`.
    pub fn connect_luma_input(&mut self, from: NodeId, to: NodeId, to_input: u8) {
        self.edges.push(EdgeSpec {
            from,
            to,
            from_port: PortType::LumaTexture,
            to_port: PortType::LumaTexture,
            to_input,
        });
    }

    /// Connect mask output of `from` to mask input `to_input` of `to`.
    pub fn connect_mask_input(&mut self, from: NodeId, to: NodeId, to_input: u8) {
        self.edges.push(EdgeSpec {
            from,
            to,
            from_port: PortType::MaskTexture,
            to_port: PortType::MaskTexture,
            to_input,
        });
    }

    /// Validate and produce an immutable graph.
    pub fn build(self) -> Result<GpuGraph, GraphBuildError> {
        if self.width == 0 || self.height == 0 {
            return Err(GraphBuildError::new(
                "graph dimensions must be greater than zero",
            ));
        }
        if self.nodes.is_empty() {
            return Err(GraphBuildError::new("graph must contain at least one node"));
        }

        let mut node_map = HashMap::with_capacity(self.nodes.len());
        for node in &self.nodes {
            if node_map.insert(node.id, node.kind).is_some() {
                return Err(GraphBuildError::new(format!(
                    "duplicate node id encountered: {:?}",
                    node.id
                )));
            }
        }

        let mut output_count = 0usize;
        let mut primary_output_count = 0usize;
        let mut output_slots = HashSet::new();
        let mut incoming: HashMap<NodeId, HashMap<u8, usize>> = HashMap::new();
        for node in &self.nodes {
            incoming.insert(node.id, HashMap::new());
            if let NodeKind::Output(output) = node.kind {
                output_count += 1;
                if !output_slots.insert(output.slot) {
                    return Err(GraphBuildError::new(format!(
                        "duplicate output slot {} on node {:?}",
                        output.slot, node.id
                    )));
                }
                if matches!(output.role, OutputRole::Primary) {
                    primary_output_count += 1;
                }
            }
        }

        if output_count == 0 {
            return Err(GraphBuildError::new(
                "graph must include at least one output node",
            ));
        }
        if primary_output_count != 1 {
            return Err(GraphBuildError::new(format!(
                "graph must include exactly one primary output node, got {}",
                primary_output_count
            )));
        }

        for edge in &self.edges {
            let from_kind = node_map.get(&edge.from).copied().ok_or_else(|| {
                GraphBuildError::new(format!("edge source node not found: {:?}", edge.from))
            })?;
            let to_kind = node_map.get(&edge.to).copied().ok_or_else(|| {
                GraphBuildError::new(format!("edge target node not found: {:?}", edge.to))
            })?;

            let expected_from = from_kind.output_port().ok_or_else(|| {
                GraphBuildError::new(format!(
                    "node {:?} does not expose an output port",
                    edge.from
                ))
            })?;
            if expected_from != edge.from_port {
                return Err(GraphBuildError::new(format!(
                    "edge from-port mismatch on {:?}: expected {:?}, got {:?}",
                    edge.from, expected_from, edge.from_port
                )));
            }

            let expected_to = to_kind.input_port(edge.to_input).ok_or_else(|| {
                GraphBuildError::new(format!(
                    "node {:?} has no input slot {}",
                    edge.to, edge.to_input
                ))
            })?;
            if expected_to != edge.to_port {
                return Err(GraphBuildError::new(format!(
                    "edge to-port mismatch on {:?} slot {}: expected {:?}, got {:?}",
                    edge.to, edge.to_input, expected_to, edge.to_port
                )));
            }

            let entry = incoming
                .get_mut(&edge.to)
                .ok_or_else(|| GraphBuildError::new("incoming edge table mismatch"))?;
            let slot_count = entry.entry(edge.to_input).or_insert(0);
            *slot_count += 1;
            if *slot_count > 1 {
                return Err(GraphBuildError::new(format!(
                    "node {:?} input slot {} has multiple incoming edges",
                    edge.to, edge.to_input
                )));
            }
        }

        for node in &self.nodes {
            let slot_counts = incoming.get(&node.id).cloned().unwrap_or_default();
            let total_inputs: usize = slot_counts.values().sum();
            let (min_inputs, max_inputs) = node.kind.input_range();
            if total_inputs < min_inputs || total_inputs > max_inputs {
                return Err(GraphBuildError::new(format!(
                    "node {:?} requires {}..={} inputs, got {}",
                    node.id, min_inputs, max_inputs, total_inputs
                )));
            }
        }

        validate_acyclic(&self.nodes, &self.edges)?;

        Ok(GpuGraph {
            width: self.width,
            height: self.height,
            seed: self.seed,
            nodes: self.nodes,
            edges: self.edges,
        })
    }

    fn add_node(&mut self, kind: NodeKind) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.nodes.push(NodeSpec { id, kind });
        id
    }
}

fn validate_acyclic(nodes: &[NodeSpec], edges: &[EdgeSpec]) -> Result<(), GraphBuildError> {
    let mut indegree: HashMap<NodeId, usize> = HashMap::with_capacity(nodes.len());
    let mut adjacency: HashMap<NodeId, Vec<NodeId>> = HashMap::with_capacity(nodes.len());

    for node in nodes {
        indegree.insert(node.id, 0);
        adjacency.insert(node.id, Vec::new());
    }

    for edge in edges {
        let degree = indegree
            .get_mut(&edge.to)
            .ok_or_else(|| GraphBuildError::new("edge references missing target node"))?;
        *degree += 1;
        adjacency
            .get_mut(&edge.from)
            .ok_or_else(|| GraphBuildError::new("edge references missing source node"))?
            .push(edge.to);
    }

    let mut queue = VecDeque::new();
    for node in nodes {
        if indegree.get(&node.id).copied().unwrap_or(0) == 0 {
            queue.push_back(node.id);
        }
    }

    let mut visited = 0usize;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        if let Some(next_nodes) = adjacency.get(&node) {
            for next in next_nodes {
                if let Some(value) = indegree.get_mut(next) {
                    *value -= 1;
                    if *value == 0 {
                        queue.push_back(*next);
                    }
                }
            }
        }
    }

    if visited != nodes.len() {
        return Err(GraphBuildError::new("graph contains a cycle"));
    }

    Ok(())
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;

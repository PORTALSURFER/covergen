//! Typed node-graph model for the V2 generated pipeline.
//!
//! The graph is authored programmatically and validated before compilation.
//! It intentionally models a no-GUI workflow where presets generate node
//! topology from deterministic seeds.

use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::model::{LayerBlendMode, Params};

/// Stable node identifier used across builder, compiler, and runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// Port categories supported by the V2 graph IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortType {
    /// Single-channel image data in normalized [0, 1] range.
    LumaTexture,
}

/// GPU layer generation node parameters.
///
/// This mirrors the shader's uniform-space controls while keeping blend and
/// per-layer post parameters colocated for graph execution.
#[derive(Clone, Copy, Debug)]
pub struct GenerateLayerNode {
    pub symmetry: u32,
    pub symmetry_style: u32,
    pub iterations: u32,
    pub seed: u32,
    pub fill_scale: f32,
    pub fractal_zoom: f32,
    pub art_style: u32,
    pub art_style_secondary: u32,
    pub art_style_mix: f32,
    pub bend_strength: f32,
    pub warp_strength: f32,
    pub warp_frequency: f32,
    pub tile_scale: f32,
    pub tile_phase: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub shader_layer_count: u32,
    pub blend_mode: LayerBlendMode,
    pub opacity: f32,
    pub contrast: f32,
}

impl GenerateLayerNode {
    /// Convert this node to shader uniform payload for a target render size.
    pub fn to_params(self, width: u32, height: u32, seed_offset: u32) -> Params {
        Params {
            width,
            height,
            symmetry: self.symmetry,
            symmetry_style: self.symmetry_style,
            iterations: self.iterations,
            seed: self.seed.wrapping_add(seed_offset),
            fill_scale: self.fill_scale,
            fractal_zoom: self.fractal_zoom,
            art_style: self.art_style,
            art_style_secondary: self.art_style_secondary,
            art_style_mix: self.art_style_mix,
            bend_strength: self.bend_strength,
            warp_strength: self.warp_strength,
            warp_frequency: self.warp_frequency,
            tile_scale: self.tile_scale,
            tile_phase: self.tile_phase,
            center_x: self.center_x,
            center_y: self.center_y,
            layer_count: self.shader_layer_count,
        }
    }
}

/// Graph node kinds currently supported by V2.
#[derive(Clone, Copy, Debug)]
pub enum NodeKind {
    /// Produce a luma layer using the fractal compute shader.
    GenerateLayer(GenerateLayerNode),
    /// Terminal node indicating which luma stream should be encoded.
    Output,
}

impl NodeKind {
    fn input_port(self) -> Option<PortType> {
        match self {
            Self::GenerateLayer(_) => Some(PortType::LumaTexture),
            Self::Output => Some(PortType::LumaTexture),
        }
    }

    fn output_port(self) -> Option<PortType> {
        match self {
            Self::GenerateLayer(_) => Some(PortType::LumaTexture),
            Self::Output => None,
        }
    }
}

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

    /// Add one output node.
    pub fn add_output(&mut self) -> NodeId {
        self.add_node(NodeKind::Output)
    }

    /// Connect luma output of `from` to luma input of `to`.
    pub fn connect_luma(&mut self, from: NodeId, to: NodeId) {
        self.edges.push(EdgeSpec {
            from,
            to,
            from_port: PortType::LumaTexture,
            to_port: PortType::LumaTexture,
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
            if node_map.insert(node.id, *node).is_some() {
                return Err(GraphBuildError::new(format!(
                    "duplicate node id encountered: {:?}",
                    node.id
                )));
            }
        }

        let mut incoming_counts: HashMap<NodeId, usize> = HashMap::new();
        let mut output_count = 0usize;

        for node in &self.nodes {
            incoming_counts.insert(node.id, 0);
            if matches!(node.kind, NodeKind::Output) {
                output_count += 1;
            }
        }

        if output_count == 0 {
            return Err(GraphBuildError::new(
                "graph must include at least one output node",
            ));
        }

        for edge in &self.edges {
            let from = node_map.get(&edge.from).ok_or_else(|| {
                GraphBuildError::new(format!("edge source node not found: {:?}", edge.from))
            })?;
            let to = node_map.get(&edge.to).ok_or_else(|| {
                GraphBuildError::new(format!("edge target node not found: {:?}", edge.to))
            })?;

            let expected_from = from.kind.output_port().ok_or_else(|| {
                GraphBuildError::new(format!("node {:?} does not expose output port", from.id))
            })?;
            if expected_from != edge.from_port {
                return Err(GraphBuildError::new(format!(
                    "edge from-port mismatch on {:?}: expected {:?}, got {:?}",
                    from.id, expected_from, edge.from_port
                )));
            }

            let expected_to = to.kind.input_port().ok_or_else(|| {
                GraphBuildError::new(format!("node {:?} does not accept input", to.id))
            })?;
            if expected_to != edge.to_port {
                return Err(GraphBuildError::new(format!(
                    "edge to-port mismatch on {:?}: expected {:?}, got {:?}",
                    to.id, expected_to, edge.to_port
                )));
            }

            let count = incoming_counts
                .get_mut(&edge.to)
                .ok_or_else(|| GraphBuildError::new("internal incoming edge table mismatch"))?;
            *count += 1;
        }

        for node in &self.nodes {
            let incoming = incoming_counts.get(&node.id).copied().unwrap_or(0);
            match node.kind {
                NodeKind::GenerateLayer(_) => {
                    if incoming > 1 {
                        return Err(GraphBuildError::new(format!(
                            "generate-layer node {:?} has {} inputs; at most one is supported",
                            node.id, incoming
                        )));
                    }
                }
                NodeKind::Output => {
                    if incoming != 1 {
                        return Err(GraphBuildError::new(format!(
                            "output node {:?} must have exactly one input (got {})",
                            node.id, incoming
                        )));
                    }
                }
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
        let next = adjacency
            .get_mut(&edge.from)
            .ok_or_else(|| GraphBuildError::new("edge references missing source node"))?;
        next.push(edge.to);
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

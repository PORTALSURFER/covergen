//! Graph compiler for the V2 GPU node runtime.

use std::collections::{HashMap, VecDeque};

use super::graph::{EdgeSpec, GpuGraph, GraphBuildError, NodeId, NodeSpec};
use super::node::{
    BlendNode, GenerateLayerNode, MaskNode, NodeKind, SourceNoiseNode, ToneMapNode,
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

/// Compiler output consumed by the V2 runtime.
#[derive(Clone, Debug)]
pub struct CompiledGraph {
    pub width: u32,
    pub height: u32,
    pub seed: u32,
    pub steps: Vec<CompiledNodeStep>,
    pub output_node: NodeId,
    pub has_non_layer_nodes: bool,
}

/// Compile and validate execution constraints for runtime evaluation.
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

    Ok(CompiledGraph {
        width: graph.width,
        height: graph.height,
        seed: graph.seed,
        steps,
        output_node,
        has_non_layer_nodes,
    })
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
mod tests {
    use super::*;
    use crate::model::LayerBlendMode;
    use crate::v2::graph::GraphBuilder;
    use crate::v2::node::{GenerateLayerNode, MaskNode};

    fn sample_layer() -> GenerateLayerNode {
        GenerateLayerNode {
            symmetry: 4,
            symmetry_style: 1,
            iterations: 200,
            seed: 1,
            fill_scale: 1.0,
            fractal_zoom: 0.8,
            art_style: 2,
            art_style_secondary: 3,
            art_style_mix: 0.5,
            bend_strength: 0.4,
            warp_strength: 0.3,
            warp_frequency: 2.5,
            tile_scale: 1.0,
            tile_phase: 0.2,
            center_x: 0.0,
            center_y: 0.0,
            shader_layer_count: 3,
            blend_mode: LayerBlendMode::Normal,
            opacity: 1.0,
            contrast: 1.1,
        }
    }

    #[test]
    fn compiles_linear_layer_graph() {
        let mut builder = GraphBuilder::new(512, 512, 123);
        let a = builder.add_generate_layer(sample_layer());
        let b = builder.add_generate_layer(sample_layer());
        let out = builder.add_output();
        builder.connect_luma(a, b);
        builder.connect_luma(b, out);
        let graph = builder.build().expect("graph should build");
        let compiled = compile_graph(&graph).expect("graph should compile");
        assert_eq!(compiled.steps.len(), 3);
        assert!(!compiled.has_non_layer_nodes);
        assert_eq!(compiled.output_node, out);
    }

    #[test]
    fn compiles_mask_node_graph() {
        let mut builder = GraphBuilder::new(256, 256, 9);
        let src = builder.add_generate_layer(sample_layer());
        let mask = builder.add_mask(MaskNode {
            threshold: 0.5,
            softness: 0.1,
            invert: false,
        });
        let out = builder.add_output();
        builder.connect_luma(src, mask);
        builder.connect_mask_input(mask, out, 0);
        let err = builder
            .build()
            .expect_err("output cannot accept mask input");
        assert!(err.to_string().contains("to-port mismatch"));
    }
}

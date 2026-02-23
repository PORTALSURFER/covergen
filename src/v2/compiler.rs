//! Graph compiler for the V2 GPU node runtime.
//!
//! The compiler lowers validated graph IR into an ordered execution plan that
//! the GPU runtime can execute without making additional structural decisions.

use std::collections::{HashMap, VecDeque};

use super::graph::{EdgeSpec, GenerateLayerNode, GpuGraph, GraphBuildError, NodeId, NodeKind};

/// One scheduled render-layer step in a compiled graph.
#[derive(Clone, Copy, Debug)]
pub struct CompiledLayerStep {
    pub layer: GenerateLayerNode,
}

/// Compiler output consumed by the V2 GPU runtime.
#[derive(Clone, Debug)]
pub struct CompiledGraph {
    pub width: u32,
    pub height: u32,
    pub seed: u32,
    pub steps: Vec<CompiledLayerStep>,
    pub output_node: NodeId,
}

/// Compile and validate execution constraints for the runtime.
pub fn compile_graph(graph: &GpuGraph) -> Result<CompiledGraph, GraphBuildError> {
    let topo_order = topological_order(&graph.nodes, &graph.edges)?;

    let mut node_map = HashMap::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        node_map.insert(node.id, node.kind);
    }

    let mut outgoing_counts: HashMap<NodeId, usize> = HashMap::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        outgoing_counts.insert(node.id, 0);
    }
    let mut output_node = None;

    for edge in &graph.edges {
        let count = outgoing_counts
            .get_mut(&edge.from)
            .ok_or_else(|| GraphBuildError::new("edge source missing during compilation"))?;
        *count += 1;
    }

    let mut steps = Vec::new();
    for node_id in &topo_order {
        let kind = node_map
            .get(node_id)
            .copied()
            .ok_or_else(|| GraphBuildError::new("topology references missing node"))?;

        match kind {
            NodeKind::GenerateLayer(layer) => {
                let out_degree = outgoing_counts.get(node_id).copied().unwrap_or(0);
                if out_degree > 1 {
                    return Err(GraphBuildError::new(format!(
                        "node {:?} has {} outgoing edges; V2 runtime supports one downstream edge per layer",
                        node_id, out_degree
                    )));
                }
                steps.push(CompiledLayerStep { layer });
            }
            NodeKind::Output => {
                if output_node.is_some() {
                    return Err(GraphBuildError::new(
                        "multiple output nodes are not supported by current V2 runtime",
                    ));
                }
                output_node = Some(*node_id);
            }
        }
    }

    if steps.is_empty() {
        return Err(GraphBuildError::new(
            "compiled graph contains no renderable layer nodes",
        ));
    }

    let output_node = output_node.ok_or_else(|| {
        GraphBuildError::new("compiled graph has no output node after topological pass")
    })?;

    Ok(CompiledGraph {
        width: graph.width,
        height: graph.height,
        seed: graph.seed,
        steps,
        output_node,
    })
}

fn topological_order(
    nodes: &[super::graph::NodeSpec],
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
    use crate::v2::graph::{GenerateLayerNode, GraphBuilder};

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
        assert_eq!(compiled.steps.len(), 2);
        assert_eq!(compiled.output_node, out);
    }
}

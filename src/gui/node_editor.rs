//! Left-panel node editor visualization for compiled graph topology.

use std::collections::HashMap;

use crate::compiler::{CompiledGraph, CompiledOp};
use crate::graph::NodeId;

use super::draw::{draw_line, draw_text, fill_rect, stroke_rect, Rect};

const LANE_COUNT: usize = 5;
const PANEL_BG: u32 = 0xFF111318;
const GRID_COLOR: u32 = 0xFF1B2028;
const BORDER_COLOR: u32 = 0xFF2A313A;
const EDGE_COLOR: u32 = 0xFF4A5564;
const TEXT_COLOR: u32 = 0xFFE5E7EB;

/// Precomputed node-editor draw data for one compiled graph.
pub(crate) struct NodeEditorLayout {
    panel_width: usize,
    headers: Vec<LaneHeader>,
    nodes: Vec<NodeVisual>,
    edges: Vec<EdgeVisual>,
}

impl NodeEditorLayout {
    /// Build one static layout from compiled graph topology.
    pub(crate) fn build(compiled: &CompiledGraph, panel_width: usize, panel_height: usize) -> Self {
        let lane_width = lane_width(panel_width);
        let lane_steps = bucket_steps_by_lane(compiled);
        let (nodes, anchors) = place_nodes(compiled, &lane_steps, lane_width, panel_height);
        let edges = build_edges(compiled, &anchors);
        let headers = lane_headers(lane_width);
        Self {
            panel_width,
            headers,
            nodes,
            edges,
        }
    }

    /// Draw the entire node-editor panel into the target frame.
    pub(crate) fn draw(&self, frame: &mut [u32], width: usize, height: usize) {
        fill_rect(
            frame,
            width,
            height,
            Rect::new(0, 0, self.panel_width as i32, height as i32),
            PANEL_BG,
        );
        draw_grid(
            frame,
            width,
            height,
            self.panel_width as i32,
            20,
            GRID_COLOR,
        );
        for header in &self.headers {
            draw_lane_header(frame, width, height, header);
        }
        for edge in &self.edges {
            draw_line(
                frame,
                width,
                height,
                edge.from.0,
                edge.from.1,
                edge.to.0,
                edge.to.1,
                edge.color,
            );
        }
        for node in &self.nodes {
            draw_node(frame, width, height, node);
        }
    }
}

#[derive(Clone, Copy)]
struct LaneHeader {
    title: &'static str,
    rect: Rect,
}

#[derive(Clone, Copy)]
struct NodeVisual {
    id: NodeId,
    label: &'static str,
    color: u32,
    rect: Rect,
}

#[derive(Clone, Copy)]
struct EdgeVisual {
    from: (i32, i32),
    to: (i32, i32),
    color: u32,
}

#[derive(Clone, Copy)]
struct NodeAnchor {
    input: (i32, i32),
    output: (i32, i32),
}

fn bucket_steps_by_lane(compiled: &CompiledGraph) -> Vec<Vec<usize>> {
    let mut lanes = vec![Vec::<usize>::new(); LANE_COUNT];
    for (index, step) in compiled.steps.iter().enumerate() {
        lanes[op_lane(step.op)].push(index);
    }
    lanes
}

fn place_nodes(
    compiled: &CompiledGraph,
    lane_steps: &[Vec<usize>],
    lane_width: i32,
    panel_height: usize,
) -> (Vec<NodeVisual>, HashMap<NodeId, NodeAnchor>) {
    let mut nodes = Vec::with_capacity(compiled.steps.len());
    let mut anchors = HashMap::with_capacity(compiled.steps.len());

    for (lane, steps) in lane_steps.iter().enumerate() {
        for (order, step_index) in steps.iter().enumerate() {
            let step = &compiled.steps[*step_index];
            let rect = layout_node_rect(lane, order, steps.len(), lane_width, panel_height);
            let (label, color) = op_style(step.op);
            nodes.push(NodeVisual {
                id: step.node_id,
                label,
                color,
                rect,
            });
            anchors.insert(step.node_id, node_anchor(rect));
        }
    }

    (nodes, anchors)
}

fn build_edges(compiled: &CompiledGraph, anchors: &HashMap<NodeId, NodeAnchor>) -> Vec<EdgeVisual> {
    let mut edges = Vec::new();
    for step in &compiled.steps {
        let Some(target) = anchors.get(&step.node_id).copied() else {
            continue;
        };
        for source_id in &step.inputs {
            let Some(source) = anchors.get(source_id).copied() else {
                continue;
            };
            edges.push(EdgeVisual {
                from: source.output,
                to: target.input,
                color: EDGE_COLOR,
            });
        }
    }
    edges
}

fn draw_grid(
    frame: &mut [u32],
    width: usize,
    height: usize,
    panel_width: i32,
    step: i32,
    color: u32,
) {
    let mut x = 0;
    while x < panel_width {
        draw_line(frame, width, height, x, 0, x, height as i32 - 1, color);
        x += step;
    }
    let mut y = 0;
    while y < height as i32 {
        draw_line(frame, width, height, 0, y, panel_width - 1, y, color);
        y += step;
    }
}

fn draw_lane_header(frame: &mut [u32], width: usize, height: usize, header: &LaneHeader) {
    fill_rect(frame, width, height, header.rect, 0xFF202631);
    stroke_rect(frame, width, height, header.rect, BORDER_COLOR);
    draw_text(
        frame,
        width,
        height,
        header.rect.x + 6,
        header.rect.y + 9,
        header.title,
        TEXT_COLOR,
    );
}

fn draw_node(frame: &mut [u32], width: usize, height: usize, node: &NodeVisual) {
    fill_rect(frame, width, height, node.rect, 0xFF151A22);
    fill_rect(
        frame,
        width,
        height,
        Rect::new(node.rect.x, node.rect.y, node.rect.w, 8),
        node.color,
    );
    stroke_rect(frame, width, height, node.rect, BORDER_COLOR);
    draw_text(
        frame,
        width,
        height,
        node.rect.x + 6,
        node.rect.y + 13,
        node.label,
        TEXT_COLOR,
    );
    let id_text = format!("#{}", node.id.0);
    draw_text(
        frame,
        width,
        height,
        node.rect.x + 6,
        node.rect.y + 24,
        &id_text,
        0xFFB8C0CC,
    );
}

fn lane_headers(lane_width: i32) -> Vec<LaneHeader> {
    let titles = ["GEN/FX", "CHOP", "SOP", "TOP", "OUT"];
    let mut headers = Vec::with_capacity(LANE_COUNT);
    for (lane, title) in titles.into_iter().enumerate() {
        let x = lane_x(lane, lane_width);
        headers.push(LaneHeader {
            title,
            rect: Rect::new(x, 8, lane_width, 28),
        });
    }
    headers
}

fn node_anchor(rect: Rect) -> NodeAnchor {
    NodeAnchor {
        input: (rect.x, rect.y + rect.h / 2),
        output: (rect.x + rect.w, rect.y + rect.h / 2),
    }
}

fn lane_width(panel_width: usize) -> i32 {
    let gap = 10i32;
    let total_gap = gap * (LANE_COUNT as i32 + 1);
    ((panel_width as i32 - total_gap) / LANE_COUNT as i32).max(68)
}

fn layout_node_rect(
    lane: usize,
    order: usize,
    total: usize,
    lane_width: i32,
    panel_height: usize,
) -> Rect {
    let node_h = 40i32;
    let top = 52i32;
    let bottom = 18i32;
    let x = lane_x(lane, lane_width);
    let available = (panel_height as i32 - top - bottom - node_h).max(1);
    let y = if total <= 1 {
        top + available / 2
    } else {
        let ratio = order as f32 / (total - 1) as f32;
        top + (ratio * available as f32).round() as i32
    };
    Rect::new(x, y, lane_width, node_h)
}

fn lane_x(lane: usize, lane_width: i32) -> i32 {
    let gap = 10i32;
    gap + lane as i32 * (lane_width + gap)
}

fn op_lane(op: CompiledOp) -> usize {
    match op {
        CompiledOp::GenerateLayer(_)
        | CompiledOp::SourceNoise(_)
        | CompiledOp::Mask(_)
        | CompiledOp::Blend(_)
        | CompiledOp::ToneMap(_)
        | CompiledOp::WarpTransform(_)
        | CompiledOp::StatefulFeedback(_) => 0,
        CompiledOp::ChopLfo(_) | CompiledOp::ChopMath(_) | CompiledOp::ChopRemap(_) => 1,
        CompiledOp::SopCircle(_) | CompiledOp::SopSphere(_) | CompiledOp::SopGeometry(_) => 2,
        CompiledOp::TopCameraRender(_) => 3,
        CompiledOp::Output(_) => 4,
    }
}

fn op_style(op: CompiledOp) -> (&'static str, u32) {
    match op {
        CompiledOp::GenerateLayer(_) => ("generate", 0xFF3B82F6),
        CompiledOp::SourceNoise(_) => ("noise", 0xFF2563EB),
        CompiledOp::Mask(_) => ("mask", 0xFF334155),
        CompiledOp::Blend(_) => ("blend", 0xFF0EA5E9),
        CompiledOp::ToneMap(_) => ("tone-map", 0xFF7C3AED),
        CompiledOp::WarpTransform(_) => ("warp", 0xFF8B5CF6),
        CompiledOp::StatefulFeedback(_) => ("feedback", 0xFF9333EA),
        CompiledOp::ChopLfo(_) => ("chop-lfo", 0xFFF97316),
        CompiledOp::ChopMath(_) => ("chop-math", 0xFFEA580C),
        CompiledOp::ChopRemap(_) => ("chop-remap", 0xFFFB923C),
        CompiledOp::SopCircle(_) => ("sop-circle", 0xFF10B981),
        CompiledOp::SopSphere(_) => ("sop-sphere", 0xFF059669),
        CompiledOp::SopGeometry(_) => ("sop-geo", 0xFF34D399),
        CompiledOp::TopCameraRender(_) => ("camera", 0xFFEF4444),
        CompiledOp::Output(_) => ("output", 0xFFE5E7EB),
    }
}

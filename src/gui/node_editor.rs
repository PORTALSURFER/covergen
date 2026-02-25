//! Left-panel node editor visualization for GUI project topology.

use std::collections::HashMap;

use super::draw::{draw_line, draw_text, fill_rect, stroke_rect, Rect};
use super::project::{GuiProject, ProjectNode, ProjectNodeFamily, ALL_NODE_FAMILIES};

const LANE_COUNT: usize = 5;
const PANEL_BG: u32 = 0xFF111318;
const GRID_COLOR: u32 = 0xFF1B2028;
const BORDER_COLOR: u32 = 0xFF2A313A;
const EDGE_COLOR: u32 = 0xFF4A5564;
const TEXT_COLOR: u32 = 0xFFE5E7EB;

/// Precomputed node-editor draw data for one GUI project.
pub(crate) struct NodeEditorLayout {
    panel_width: usize,
    headers: Vec<LaneHeader>,
    nodes: Vec<NodeVisual>,
    edges: Vec<EdgeVisual>,
}

impl NodeEditorLayout {
    /// Build one static node-editor layout from project topology.
    pub(crate) fn from_project(
        project: &GuiProject,
        panel_width: usize,
        panel_height: usize,
    ) -> Self {
        let lane_width = lane_width(panel_width);
        let lane_nodes = bucket_nodes_by_lane(project);
        let (nodes, anchors) = place_nodes(&lane_nodes, lane_width, panel_height);
        let edges = build_edges(project, &anchors);
        let headers = lane_headers(lane_width);
        Self {
            panel_width,
            headers,
            nodes,
            edges,
        }
    }

    /// Draw the node-editor panel into the target frame.
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

#[derive(Clone)]
struct NodeVisual {
    id: u32,
    label: String,
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

fn bucket_nodes_by_lane(project: &GuiProject) -> Vec<Vec<ProjectNode>> {
    let mut lanes = vec![Vec::<ProjectNode>::new(); LANE_COUNT];
    for node in &project.nodes {
        lanes[family_lane(node.family)].push(node.clone());
    }
    lanes
}

fn place_nodes(
    lane_nodes: &[Vec<ProjectNode>],
    lane_width: i32,
    panel_height: usize,
) -> (Vec<NodeVisual>, HashMap<u32, NodeAnchor>) {
    let total = lane_nodes.iter().map(|lane| lane.len()).sum();
    let mut nodes = Vec::with_capacity(total);
    let mut anchors = HashMap::with_capacity(total);

    for (lane, lane_nodes) in lane_nodes.iter().enumerate() {
        for (order, node) in lane_nodes.iter().enumerate() {
            let rect = layout_node_rect(lane, order, lane_nodes.len(), lane_width, panel_height);
            let color = family_color(node.family);
            nodes.push(NodeVisual {
                id: node.id,
                label: node.label.clone(),
                color,
                rect,
            });
            anchors.insert(node.id, node_anchor(rect));
        }
    }

    (nodes, anchors)
}

fn build_edges(project: &GuiProject, anchors: &HashMap<u32, NodeAnchor>) -> Vec<EdgeVisual> {
    let mut edges = Vec::new();
    for node in &project.nodes {
        let Some(target) = anchors.get(&node.id).copied() else {
            continue;
        };
        for source_id in &node.inputs {
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
        &node.label,
        TEXT_COLOR,
    );
    let id_text = format!("#{}", node.id);
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
    let mut headers = Vec::with_capacity(LANE_COUNT);
    for (lane, family) in ALL_NODE_FAMILIES.into_iter().enumerate() {
        let x = lane_x(lane, lane_width);
        headers.push(LaneHeader {
            title: family_title(family),
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

fn family_lane(family: ProjectNodeFamily) -> usize {
    match family {
        ProjectNodeFamily::GenFx => 0,
        ProjectNodeFamily::Chop => 1,
        ProjectNodeFamily::Sop => 2,
        ProjectNodeFamily::Top => 3,
        ProjectNodeFamily::Output => 4,
    }
}

fn family_color(family: ProjectNodeFamily) -> u32 {
    match family {
        ProjectNodeFamily::GenFx => 0xFF3B82F6,
        ProjectNodeFamily::Chop => 0xFFF97316,
        ProjectNodeFamily::Sop => 0xFF10B981,
        ProjectNodeFamily::Top => 0xFFEF4444,
        ProjectNodeFamily::Output => 0xFFE5E7EB,
    }
}

fn family_title(family: ProjectNodeFamily) -> &'static str {
    match family {
        ProjectNodeFamily::GenFx => "GEN/FX",
        ProjectNodeFamily::Chop => "CHOP",
        ProjectNodeFamily::Sop => "SOP",
        ProjectNodeFamily::Top => "TOP",
        ProjectNodeFamily::Output => "OUT",
    }
}

//! Left-panel graph editor visualization for GUI projects.

use std::collections::HashMap;

use super::draw::{draw_line, draw_text, fill_rect, stroke_rect, Rect};
use super::project::{GuiProject, ProjectNode, ProjectNodeKind, NODE_HEIGHT, NODE_WIDTH};

const PANEL_BG: u32 = 0xFF111318;
const GRID_COLOR: u32 = 0xFF1B2028;
const BORDER_COLOR: u32 = 0xFF2A313A;
const EDGE_COLOR: u32 = 0xFF4A5564;
const TEXT_COLOR: u32 = 0xFFE5E7EB;

/// Visual graph editor state for one left-side panel.
pub(crate) struct NodeEditorLayout {
    panel_width: usize,
}

impl NodeEditorLayout {
    /// Create a graph editor bound to one panel width.
    pub(crate) const fn new(panel_width: usize) -> Self {
        Self { panel_width }
    }

    /// Draw graph editor canvas, edges, and nodes for one project.
    pub(crate) fn draw(
        &self,
        frame: &mut [u32],
        width: usize,
        height: usize,
        project: &GuiProject,
        hover_node_id: Option<u32>,
        drag_node_id: Option<u32>,
    ) {
        draw_canvas(frame, width, height, self.panel_width);
        draw_header(frame, width, height, project);
        draw_edges(frame, width, height, project);
        draw_nodes(frame, width, height, project, hover_node_id, drag_node_id);
    }
}

fn draw_canvas(frame: &mut [u32], width: usize, height: usize, panel_width: usize) {
    fill_rect(
        frame,
        width,
        height,
        Rect::new(0, 0, panel_width as i32, height as i32),
        PANEL_BG,
    );
    draw_grid(frame, width, height, panel_width as i32, 20, GRID_COLOR);
}

fn draw_header(frame: &mut [u32], width: usize, height: usize, project: &GuiProject) {
    let header = Rect::new(8, 8, 360, 24);
    fill_rect(frame, width, height, header, 0xFF202631);
    stroke_rect(frame, width, height, header, BORDER_COLOR);
    let text = format!(
        "GRAPH EDITOR  {}  nodes={}",
        project.name,
        project.node_count()
    );
    draw_text(frame, width, height, 14, 16, &text, TEXT_COLOR);
}

fn draw_edges(frame: &mut [u32], width: usize, height: usize, project: &GuiProject) {
    let anchors: HashMap<u32, (i32, i32)> = project
        .nodes()
        .iter()
        .map(|node| (node.id(), center(node)))
        .collect();

    for node in project.nodes() {
        let Some((to_x, to_y)) = anchors.get(&node.id()).copied() else {
            continue;
        };
        for input_id in node.inputs() {
            let Some((from_x, from_y)) = anchors.get(input_id).copied() else {
                continue;
            };
            draw_line(frame, width, height, from_x, from_y, to_x, to_y, EDGE_COLOR);
        }
    }
}

fn draw_nodes(
    frame: &mut [u32],
    width: usize,
    height: usize,
    project: &GuiProject,
    hover_node_id: Option<u32>,
    drag_node_id: Option<u32>,
) {
    for node in project.nodes() {
        let rect = node_rect(node);
        let is_dragged = drag_node_id == Some(node.id());
        let is_hovered = hover_node_id == Some(node.id());
        fill_rect(frame, width, height, rect, 0xFF151A22);
        fill_rect(
            frame,
            width,
            height,
            Rect::new(rect.x, rect.y, rect.w, 8),
            node_color(node.kind()),
        );
        let border = if is_dragged {
            0xFFF59E0B
        } else if is_hovered {
            0xFF22D3EE
        } else {
            BORDER_COLOR
        };
        stroke_rect(frame, width, height, rect, border);
        draw_text(
            frame,
            width,
            height,
            rect.x + 6,
            rect.y + 14,
            node.kind().label(),
            TEXT_COLOR,
        );
        let id_text = format!("#{}", node.id());
        draw_text(
            frame,
            width,
            height,
            rect.x + 6,
            rect.y + 26,
            &id_text,
            0xFFB8C0CC,
        );
    }
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

fn node_rect(node: &ProjectNode) -> Rect {
    Rect::new(node.x(), node.y(), NODE_WIDTH, NODE_HEIGHT)
}

fn center(node: &ProjectNode) -> (i32, i32) {
    (node.x() + NODE_WIDTH / 2, node.y() + NODE_HEIGHT / 2)
}

fn node_color(kind: ProjectNodeKind) -> u32 {
    match kind {
        ProjectNodeKind::TopBasic => 0xFF3B82F6,
        ProjectNodeKind::Output => 0xFFEF4444,
    }
}

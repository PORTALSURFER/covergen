//! Link-cut interaction handling and wire-segment hit testing.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CutLink {
    source_id: u32,
    target_id: u32,
    param_index: Option<usize>,
}

pub(super) fn handle_link_cut(
    input: &InputSnapshot,
    project: &mut GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &mut PreviewState,
) -> bool {
    let mut changed = false;
    if input.alt_down
        && input.left_clicked
        && state.param_scrub.is_none()
        && !state.menu.open
        && !state.main_menu.open
        && !state.export_menu.open
    {
        if let Some((mx, my)) = input.mouse_pos {
            if super::inside_panel(mx, my, panel_width, panel_height) {
                state.link_cut = Some(LinkCutState {
                    start_x: mx,
                    start_y: my,
                    cursor_x: mx,
                    cursor_y: my,
                });
                state.drag = None;
                state.wire_drag = None;
                super::clear_param_hover_state(state);
                super::clear_param_edit_state(state);
                super::clear_timeline_edit_state(state);
                return true;
            }
        }
    }
    let Some(mut cut) = state.link_cut else {
        return false;
    };
    if let Some((mx, my)) = input.mouse_pos {
        if cut.cursor_x != mx || cut.cursor_y != my {
            cut.cursor_x = mx;
            cut.cursor_y = my;
            changed = true;
        }
    }
    if !input.left_down {
        let cut_links = collect_cut_links(project, panel_width, panel_height, state, cut);
        for link in cut_links {
            if let Some(param_index) = link.param_index {
                let _ = project.disconnect_param_link_from_param(link.target_id, param_index);
            } else {
                let _ = project.disconnect_link(link.source_id, link.target_id);
            }
        }
        state.link_cut = None;
        return true;
    }
    state.link_cut = Some(cut);
    changed
}

fn collect_cut_links(
    project: &GuiProject,
    panel_width: usize,
    panel_height: usize,
    state: &PreviewState,
    cut: LinkCutState,
) -> Vec<CutLink> {
    let mut links = Vec::new();
    let obstacle_signature = super::route_cache::obstacle_signature_for_project(project, None);
    let obstacles = super::collect_graph_node_obstacles(project);
    let route_map =
        crate::gui::scene::wire_route::RouteObstacleMap::from_obstacles(obstacles.as_slice());
    let (view_x0, view_y0, view_x1, view_y1) =
        super::panel_graph_rect(panel_width, panel_height, state);
    let target_ids = project.node_ids_overlapping_graph_rect(view_x0, view_y0, view_x1, view_y1);
    for target_id in target_ids.iter().copied() {
        collect_cut_links_for_target(
            project,
            state,
            cut,
            &route_map,
            obstacle_signature,
            target_id,
            &mut links,
        );
    }
    let cut_outside_panel =
        !super::inside_panel(cut.start_x, cut.start_y, panel_width, panel_height)
            || !super::inside_panel(cut.cursor_x, cut.cursor_y, panel_width, panel_height);
    if (links.is_empty() || cut_outside_panel) && target_ids.len() < project.node_count() {
        for target in project.nodes() {
            collect_cut_links_for_target(
                project,
                state,
                cut,
                &route_map,
                obstacle_signature,
                target.id(),
                &mut links,
            );
        }
    }
    links.sort_unstable();
    links.dedup();
    links
}

fn collect_cut_links_for_target(
    project: &GuiProject,
    state: &PreviewState,
    cut: LinkCutState,
    route_map: &crate::gui::scene::wire_route::RouteObstacleMap,
    obstacle_signature: u64,
    target_id: u32,
    links: &mut Vec<CutLink>,
) {
    let Some(target) = project.node(target_id) else {
        return;
    };
    if let Some(texture_source_id) = project.input_source_node_id(target_id) {
        let Some((to_x, to_y)) = input_pin_center(target) else {
            return;
        };
        let Some(source) = project.node(texture_source_id) else {
            return;
        };
        let Some((from_x, from_y)) = output_pin_center(source) else {
            return;
        };
        let route_graph = super::route_cache::route_with_tails_cached(
            crate::gui::scene::wire_route::RouteEndpoint {
                point: (from_x, from_y),
                corridor_dir: crate::gui::scene::wire_route::RouteDirection::East,
            },
            crate::gui::scene::wire_route::RouteEndpoint {
                point: (to_x, to_y),
                corridor_dir: crate::gui::scene::wire_route::RouteDirection::West,
            },
            route_map,
            obstacle_signature,
        );
        let route_panel = super::map_graph_path_to_panel(route_graph.as_ref(), state);
        if cut_intersects_path(cut, route_panel.as_slice()) {
            links.push(CutLink {
                source_id: texture_source_id,
                target_id,
                param_index: None,
            });
        }
    }
    for param_index in 0..target.param_count() {
        let Some((source_id, _resource_kind)) =
            project.param_link_source_for_param(target_id, param_index)
        else {
            continue;
        };
        let Some(source) = project.node(source_id) else {
            continue;
        };
        let Some((from_x, from_y)) = output_pin_center(source) else {
            continue;
        };
        let (to_x, to_y) = if let Some(row) = node_param_row_rect(target, param_index) {
            (row.x + row.w - 4, row.y + row.h / 2)
        } else if let Some((pin_x, pin_y)) = collapsed_param_entry_pin_center(target) {
            (pin_x, pin_y)
        } else {
            continue;
        };
        let route_graph = super::route_cache::route_with_tails_cached(
            crate::gui::scene::wire_route::RouteEndpoint {
                point: (from_x, from_y),
                corridor_dir: crate::gui::scene::wire_route::RouteDirection::East,
            },
            crate::gui::scene::wire_route::RouteEndpoint {
                point: (to_x, to_y),
                corridor_dir: crate::gui::scene::wire_route::RouteDirection::East,
            },
            route_map,
            obstacle_signature,
        );
        let route_panel = super::map_graph_path_to_panel(route_graph.as_ref(), state);
        if cut_intersects_path(cut, route_panel.as_slice()) {
            links.push(CutLink {
                source_id,
                target_id,
                param_index: Some(param_index),
            });
        }
    }
}

fn cut_intersects_path(cut: LinkCutState, path: &[(i32, i32)]) -> bool {
    if path.len() < 2 {
        return false;
    }
    for segment in path.windows(2) {
        if segments_intersect(
            (cut.start_x, cut.start_y),
            (cut.cursor_x, cut.cursor_y),
            segment[0],
            segment[1],
        ) {
            return true;
        }
    }
    false
}

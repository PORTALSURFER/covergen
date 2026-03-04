use super::*;

impl SceneBuilder {
    pub(super) fn push_param_links(&mut self, project: &GuiProject, state: &PreviewState) {
        if project.edge_count() == 0 {
            return;
        }
        let obstacle_epoch =
            param_route_obstacle_epoch(project, state, self.wire_routes.param_cache_epoch);
        if self.wire_routes.param_cache_epoch != Some(obstacle_epoch) {
            self.wire_routes.param_cache_epoch = Some(obstacle_epoch);
            self.wire_routes.param_cache.clear();
            let obstacles = collect_graph_node_obstacles(project);
            self.wire_routes.param_obstacle_map =
                wire_route::RouteObstacleMap::from_obstacles(&obstacles);
        }
        let active_epoch = self.wire_routes.param_cache_epoch.unwrap_or(obstacle_epoch);
        let mut live_route_keys =
            std::mem::take(&mut self.wire_routes.param_live_route_keys_scratch);
        let mut drawn_segments = std::mem::take(&mut self.param_drawn_segments_scratch);
        let mut drawn_segment_hash = std::mem::take(&mut self.param_drawn_segment_hash_scratch);
        live_route_keys.clear();
        drawn_segments.clear();
        drawn_segment_hash.clear();
        let mut param_occupied_edges = wire_route::RouteOccupiedEdges::default();
        let mut tail_slots = std::mem::take(&mut self.wire_routes.param_tail_slots_scratch);
        tail_slots.clear();
        let mut route_panel = std::mem::take(&mut self.wire_routes.param_route_panel_scratch);
        route_panel.clear();
        for target in project.nodes() {
            for param_index in 0..target.param_count() {
                let Some((source_id, _resource_kind)) =
                    project.param_link_source_for_param(target.id(), param_index)
                else {
                    continue;
                };
                let Some(source) = project.node(source_id) else {
                    continue;
                };
                let Some((from_x, from_y)) = output_pin_center(source) else {
                    continue;
                };
                let (to_x_graph, to_y_graph) =
                    if let Some(row) = node_param_row_rect(target, param_index) {
                        (row.x + row.w - 4, row.y + row.h / 2)
                    } else if let Some((pin_x, pin_y)) = collapsed_param_entry_pin_center(target) {
                        (pin_x, pin_y)
                    } else {
                        continue;
                    };
                let (to_x, to_y) = graph_point_to_panel(to_x_graph, to_y_graph, state);
                let start_endpoint = wire_route::RouteEndpoint {
                    point: (from_x, from_y),
                    corridor_dir: wire_route::RouteDirection::East,
                };
                let end_endpoint = wire_route::RouteEndpoint {
                    point: (to_x_graph, to_y_graph),
                    corridor_dir: wire_route::RouteDirection::East,
                };
                let start_tail_cells = next_staggered_tail_cells(&mut tail_slots, start_endpoint);
                let end_tail_cells = next_staggered_tail_cells(&mut tail_slots, end_endpoint);
                let route_key = ParamRouteCacheKey {
                    source_id,
                    target_id: target.id(),
                    param_index,
                    obstacle_epoch: active_epoch,
                    start_tail_cells,
                    end_tail_cells,
                };
                live_route_keys.insert(route_key);
                if !self.wire_routes.param_cache.contains_key(&route_key) {
                    let route =
                        wire_route::route_wire_path_with_tail_cells_avoiding_overlaps_with_dual_map(
                            start_endpoint,
                            end_endpoint,
                            &self.wire_routes.param_obstacle_map,
                            &self.wire_routes.edge_occupied,
                            &param_occupied_edges,
                            start_tail_cells,
                            end_tail_cells,
                        );
                    self.wire_routes
                        .param_cache
                        .insert(route_key, Arc::from(route));
                }
                let Some(route) = self.wire_routes.param_cache.get(&route_key).cloned() else {
                    continue;
                };
                map_graph_path_to_panel_into(route.as_ref(), state, &mut route_panel);
                let color = if path_intersects_cut_line(state, route_panel.as_slice()) {
                    CUT_EDGE_COLOR
                } else {
                    PARAM_EDGE_COLOR
                };
                self.push_path_lines_with_bridges(
                    route_panel.as_slice(),
                    color,
                    &mut drawn_segments,
                    &mut drawn_segment_hash,
                    state.zoom,
                );
                param_occupied_edges.record_path_non_tail(route.as_ref());
                self.push_param_target_marker(to_x, to_y, color);
            }
        }
        self.wire_routes
            .param_cache
            .retain(|key, _| key.obstacle_epoch == active_epoch && live_route_keys.contains(key));
        route_panel.clear();
        tail_slots.clear();
        drawn_segments.clear();
        live_route_keys.clear();
        self.wire_routes.param_route_panel_scratch = route_panel;
        self.wire_routes.param_tail_slots_scratch = tail_slots;
        self.param_drawn_segments_scratch = drawn_segments;
        self.param_drawn_segment_hash_scratch = drawn_segment_hash;
        self.wire_routes.param_live_route_keys_scratch = live_route_keys;
    }
}

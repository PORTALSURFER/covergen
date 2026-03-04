use super::*;

impl SceneBuilder {
    pub(super) fn push_edges(&mut self, project: &GuiProject, state: &PreviewState) {
        self.wire_routes.edge_occupied = wire_route::RouteOccupiedEdges::default();
        if project.edge_count() == 0 {
            return;
        }
        let obstacle_epoch = edge_route_obstacle_epoch(project);
        if self.wire_routes.edge_cache_epoch != Some(obstacle_epoch) {
            self.wire_routes.edge_cache_epoch = Some(obstacle_epoch);
            self.wire_routes.edge_cache.clear();
            let obstacles = collect_graph_node_obstacles(project);
            self.wire_routes.edge_obstacle_map =
                wire_route::RouteObstacleMap::from_obstacles(obstacles.as_slice());
        }
        let active_epoch = self.wire_routes.edge_cache_epoch.unwrap_or(obstacle_epoch);
        let mut live_route_keys =
            std::mem::take(&mut self.wire_routes.edge_live_route_keys_scratch);
        let mut drawn_segments = std::mem::take(&mut self.edge_drawn_segments_scratch);
        let mut drawn_segment_hash = std::mem::take(&mut self.edge_drawn_segment_hash_scratch);
        live_route_keys.clear();
        drawn_segments.clear();
        drawn_segment_hash.clear();
        let mut occupied_edges = wire_route::RouteOccupiedEdges::default();
        let mut tail_slots = std::mem::take(&mut self.wire_routes.edge_tail_slots_scratch);
        tail_slots.clear();
        let mut route_panel = std::mem::take(&mut self.wire_routes.edge_route_panel_scratch);
        route_panel.clear();
        for target in project.nodes() {
            let Some((default_to_x_graph, default_to_y_graph)) = input_pin_center(target) else {
                continue;
            };
            let (default_to_x, default_to_y) =
                graph_point_to_panel(default_to_x_graph, default_to_y_graph, state);
            for source_id in target.inputs() {
                let Some(source) = project.node(*source_id) else {
                    continue;
                };
                let Some((from_x_graph, from_y_graph)) = output_pin_center(source) else {
                    continue;
                };
                let (from_x, from_y) = graph_point_to_panel(from_x_graph, from_y_graph, state);
                let link_kind = project.link_resource_kind(*source_id, target.id());
                if link_kind == Some(ResourceKind::Signal) {
                    continue;
                }
                let (to_x, to_y) = (default_to_x, default_to_y);
                let insert_hover = state.drag.is_some()
                    && state
                        .hover_insert_link
                        .map(|link| link.source_id == *source_id && link.target_id == target.id())
                        .unwrap_or(false);
                let start_endpoint = wire_route::RouteEndpoint {
                    point: (from_x_graph, from_y_graph),
                    corridor_dir: wire_route::RouteDirection::East,
                };
                let end_endpoint = wire_route::RouteEndpoint {
                    point: (default_to_x_graph, default_to_y_graph),
                    corridor_dir: wire_route::RouteDirection::West,
                };
                let start_tail_cells = next_staggered_tail_cells(&mut tail_slots, start_endpoint);
                let end_tail_cells = next_staggered_tail_cells(&mut tail_slots, end_endpoint);
                let route_key = EdgeRouteCacheKey {
                    source_id: *source_id,
                    target_id: target.id(),
                    obstacle_epoch: active_epoch,
                    start_tail_cells,
                    end_tail_cells,
                };
                live_route_keys.insert(route_key);
                if !self.wire_routes.edge_cache.contains_key(&route_key) {
                    let route =
                        wire_route::route_wire_path_with_tail_cells_avoiding_overlaps_with_map(
                            start_endpoint,
                            end_endpoint,
                            &self.wire_routes.edge_obstacle_map,
                            &occupied_edges,
                            start_tail_cells,
                            end_tail_cells,
                        );
                    self.wire_routes
                        .edge_cache
                        .insert(route_key, Arc::from(route));
                }
                let Some(route_graph) = self.wire_routes.edge_cache.get(&route_key).cloned() else {
                    continue;
                };
                route_panel.clear();
                route_panel.extend(
                    route_graph
                        .iter()
                        .copied()
                        .map(|(x, y)| graph_point_to_panel(x, y, state)),
                );
                let color = if insert_hover {
                    EDGE_INSERT_HOVER
                } else if path_intersects_cut_line(state, route_panel.as_slice()) {
                    CUT_EDGE_COLOR
                } else {
                    EDGE_COLOR
                };
                self.push_path_lines_with_bridges(
                    route_panel.as_slice(),
                    color,
                    &mut drawn_segments,
                    &mut drawn_segment_hash,
                    state.zoom,
                );
                occupied_edges.record_path_non_tail(route_graph.as_ref());
                self.push_round_endpoint(from_x, from_y, color);
                self.push_round_endpoint(to_x, to_y, color);
            }
        }
        self.wire_routes.edge_occupied = occupied_edges;
        self.wire_routes
            .edge_cache
            .retain(|key, _| key.obstacle_epoch == active_epoch && live_route_keys.contains(key));
        route_panel.clear();
        tail_slots.clear();
        drawn_segments.clear();
        live_route_keys.clear();
        self.wire_routes.edge_route_panel_scratch = route_panel;
        self.wire_routes.edge_tail_slots_scratch = tail_slots;
        self.edge_drawn_segments_scratch = drawn_segments;
        self.edge_drawn_segment_hash_scratch = drawn_segment_hash;
        self.wire_routes.edge_live_route_keys_scratch = live_route_keys;
    }
}

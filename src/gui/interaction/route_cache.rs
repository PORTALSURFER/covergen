//! Shared route cache for interaction-side wire-path queries.
//!
//! Hover-insert and link-cut paths both query routed polylines. This module
//! retains recent route results keyed by obstacle epochs and endpoints so
//! repeated interaction checks can reuse the same routed path.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::gui::project::GuiProject;
use crate::gui::scene::wire_route::{self, RouteDirection, RouteEndpoint, RouteObstacleMap};

const ROUTE_CACHE_MAX_ENTRIES: usize = 4096;

thread_local! {
    static INTERACTION_ROUTE_CACHE: RefCell<InteractionRouteCache> =
        RefCell::new(InteractionRouteCache::default());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct InteractionObstacleKey {
    nodes_epoch: u64,
    excluded_node_id: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RouteCacheKey {
    obstacle_key: InteractionObstacleKey,
    start_point: (i32, i32),
    start_dir: RouteDirection,
    end_point: (i32, i32),
    end_dir: RouteDirection,
}

#[derive(Debug, Default)]
struct InteractionRouteCache {
    entries: HashMap<RouteCacheKey, Arc<[(i32, i32)]>>,
}

/// Return one epoch-keyed obstacle identity for current node geometry.
///
/// The optional `excluded_node_id` stays part of the cache key so callers can
/// cache routes against obstacle sets that intentionally ignore one node
/// (for example, drag-hover insertion checks).
pub(super) fn obstacle_key_for_project(
    project: &GuiProject,
    excluded_node_id: Option<u32>,
) -> InteractionObstacleKey {
    InteractionObstacleKey {
        nodes_epoch: project.invalidation().nodes,
        excluded_node_id,
    }
}

/// Route one path with endpoint tails, using interaction-local cache reuse.
pub(super) fn route_with_tails_cached(
    start: RouteEndpoint,
    end: RouteEndpoint,
    obstacle_map: &RouteObstacleMap,
    obstacle_key: InteractionObstacleKey,
) -> Arc<[(i32, i32)]> {
    let key = RouteCacheKey {
        obstacle_key,
        start_point: start.point,
        start_dir: start.corridor_dir,
        end_point: end.point,
        end_dir: end.corridor_dir,
    };
    INTERACTION_ROUTE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(route) = cache.entries.get(&key) {
            return route.clone();
        }
        let route = Arc::<[(i32, i32)]>::from(wire_route::route_wire_path_with_tails_with_map(
            start,
            end,
            obstacle_map,
        ));
        if cache.entries.len() >= ROUTE_CACHE_MAX_ENTRIES {
            cache.entries.clear();
        }
        cache.entries.insert(key, route.clone());
        route
    })
}

#[cfg(test)]
mod tests {
    use super::obstacle_key_for_project;
    use crate::gui::project::{GuiProject, ProjectNodeKind};

    #[test]
    fn obstacle_key_tracks_nodes_epoch_and_excluded_node() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 60, 640, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 80, 640, 480);

        let full = obstacle_key_for_project(&project, None);
        let excluded = obstacle_key_for_project(&project, Some(solid));
        assert_ne!(
            full, excluded,
            "excluded node id must stay in the cache key"
        );

        let after_repeat = obstacle_key_for_project(&project, None);
        assert_eq!(
            full, after_repeat,
            "repeated reads without project mutation should keep obstacle keys stable"
        );

        assert!(project.connect_image_link(solid, out));
        assert!(project.move_node(solid, 120, 60, 640, 480));
        let after_move = obstacle_key_for_project(&project, None);
        assert_ne!(
            full, after_move,
            "node geometry changes must invalidate obstacle geometry keys"
        );
    }
}

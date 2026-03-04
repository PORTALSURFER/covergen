//! Shared route cache for interaction-side wire-path queries.
//!
//! Hover-insert and link-cut paths both query routed polylines. This module
//! retains recent route results keyed by obstacle signature and endpoints so
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
struct RouteCacheKey {
    obstacle_signature: u64,
    start_point: (i32, i32),
    start_dir: RouteDirection,
    end_point: (i32, i32),
    end_dir: RouteDirection,
}

#[derive(Debug, Default)]
struct InteractionRouteCache {
    entries: HashMap<RouteCacheKey, Arc<[(i32, i32)]>>,
}

/// Return one stable signature for current node obstacles.
///
/// The optional `excluded_node_id` is omitted from the signature so callers
/// can cache routes against obstacle sets that intentionally ignore one node
/// (for example, drag-hover insertion checks).
pub(super) fn obstacle_signature_for_project(
    project: &GuiProject,
    excluded_node_id: Option<u32>,
) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for node in project.nodes() {
        if excluded_node_id == Some(node.id()) {
            continue;
        }
        hash = fnv1a(hash, node.id() as u64);
        hash = fnv1a(hash, node.x() as u32 as u64);
        hash = fnv1a(hash, node.y() as u32 as u64);
        hash = fnv1a(hash, node.card_height() as u32 as u64);
    }
    hash
}

/// Route one path with endpoint tails, using interaction-local cache reuse.
pub(super) fn route_with_tails_cached(
    start: RouteEndpoint,
    end: RouteEndpoint,
    obstacle_map: &RouteObstacleMap,
    obstacle_signature: u64,
) -> Arc<[(i32, i32)]> {
    let key = RouteCacheKey {
        obstacle_signature,
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

fn fnv1a(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(0x100000001b3)
}

use super::*;

use super::geometry::{
    collect_node_rect_bin_keys, hit_bin_key_for_point, input_pin_center, output_pin_center,
};
use super::params::{
    bind_texture_target_slot, default_params_for_kind, persisted_param_key_matches,
    rebuild_node_inputs, set_node_primary_input, set_slot_value, texture_source_display_label,
    texture_target_placeholder,
};
use super::signatures::{compose_graph_signature, signature_from_ui_epoch};

impl GuiProject {
    pub(crate) fn new_empty(preview_width: u32, preview_height: u32) -> Self {
        let render_epoch = 0;
        let ui_epoch = 0;
        let mut project = Self {
            name: next_project_name(),
            preview_width,
            preview_height,
            nodes: Vec::new(),
            node_index_lookup: HashMap::new(),
            next_node_id: 1,
            edge_count: 0,
            hit_test_cache: RefCell::new(HitTestCache::default()),
            hit_test_seen_scratch: RefCell::new(HashSet::new()),
            hit_test_candidates_scratch: RefCell::new(Vec::new()),
            hit_test_dirty: Cell::new(false),
            hit_test_scan_count: Cell::new(0),
            render_epoch,
            ui_epoch,
            render_signature_cache: Cell::new(0),
            render_signature_dirty: Cell::new(false),
            ui_signature_cache: 0,
            graph_signature_cache: Cell::new(0),
            graph_signature_dirty: Cell::new(false),
            nodes_epoch: 0,
            wires_epoch: 0,
            tex_eval_epoch: 0,
            lfo_sync_bpm: 120.0,
            has_signal_bindings_cached: Cell::new(false),
            has_temporal_nodes_cached: Cell::new(false),
            has_signal_preview_nodes_cached: Cell::new(false),
            runtime_flags_dirty: Cell::new(false),
        };
        project
            .render_signature_cache
            .set(project.compute_render_signature());
        project.ui_signature_cache = signature_from_ui_epoch(project.ui_epoch);
        project.graph_signature_cache.set(compose_graph_signature(
            project.render_signature_cache.get(),
            project.ui_signature_cache,
        ));
        project
    }

    /// Update timeline BPM used by beat-synced `ctl.lfo` nodes.
    ///
    /// Returns `true` when the effective BPM changed.
    pub(crate) fn set_lfo_sync_bpm(&mut self, bpm: f32) -> bool {
        let next = bpm.clamp(1.0, 400.0);
        if (self.lfo_sync_bpm - next).abs() < f32::EPSILON {
            return false;
        }
        self.lfo_sync_bpm = next;
        self.bump_tex_eval_epoch();
        true
    }

    /// Return timeline BPM currently used by beat-synced `ctl.lfo` nodes.
    pub(crate) fn lfo_sync_bpm(&self) -> f32 {
        self.lfo_sync_bpm
    }

    /// Export this in-memory graph to a persisted autosave payload.
    pub(crate) fn to_persisted(&self) -> PersistedGuiProject {
        let nodes = self
            .nodes
            .iter()
            .map(|node| PersistedGuiNode {
                id: node.id,
                kind: node.kind.stable_id().to_string(),
                x: node.x,
                y: node.y,
                texture_input: node.texture_input,
                selected_param: node.selected_param,
                expanded: node.expanded,
                params: node
                    .params
                    .iter()
                    .map(|slot| PersistedGuiParam {
                        key: slot.key.to_string(),
                        value: slot.value,
                        signal_source: slot.signal_source,
                        texture_source: slot.texture_source,
                    })
                    .collect(),
            })
            .collect();
        PersistedGuiProject {
            version: PERSISTED_GUI_PROJECT_VERSION,
            name: self.name.clone(),
            preview_width: self.preview_width,
            preview_height: self.preview_height,
            nodes,
        }
    }

    /// Reconstruct one GUI project and collect non-fatal load warnings.
    pub(crate) fn from_persisted_with_warnings(
        persisted: PersistedGuiProject,
        panel_width: usize,
        panel_height: usize,
    ) -> Result<PersistedProjectLoadOutcome, PersistedProjectLoadError> {
        if persisted.version != PERSISTED_GUI_PROJECT_VERSION {
            return Err(PersistedProjectLoadError::new(format!(
                "unsupported gui autosave version {}; expected {}",
                persisted.version, PERSISTED_GUI_PROJECT_VERSION
            )));
        }
        let mut project = GuiProject::new_empty(
            persisted.preview_width.max(1),
            persisted.preview_height.max(1),
        );
        project.name = persisted.name;
        let mut nodes = persisted.nodes;
        nodes.sort_by_key(|node| node.id);
        let mut id_map = HashMap::new();
        let mut dropped_param_keys = HashSet::new();
        let mut warnings = Vec::new();

        for persisted_node in &nodes {
            let kind =
                ProjectNodeKind::from_stable_id(persisted_node.kind.as_str()).ok_or_else(|| {
                    PersistedProjectLoadError::new(format!(
                        "unknown node kind '{}'",
                        persisted_node.kind
                    ))
                })?;
            if id_map.contains_key(&persisted_node.id) {
                return Err(PersistedProjectLoadError::new(format!(
                    "duplicate persisted node id {}",
                    persisted_node.id
                )));
            }
            let node_id = project.add_node(
                kind,
                persisted_node.x,
                persisted_node.y,
                panel_width,
                panel_height,
            );
            id_map.insert(persisted_node.id, node_id);
            let Some(node) = project.node_mut(node_id) else {
                continue;
            };
            for persisted_param in &persisted_node.params {
                let Some(slot) = node.params.iter_mut().find(|slot| {
                    persisted_param_key_matches(slot.key, persisted_param.key.as_str(), node.kind)
                }) else {
                    push_dropped_param_warning(
                        &mut dropped_param_keys,
                        &mut warnings,
                        persisted_node.id,
                        node.kind.stable_id(),
                        persisted_param.key.as_str(),
                    );
                    continue;
                };
                let _ = set_slot_value(slot, persisted_param.value);
            }
            node.selected_param = persisted_node
                .selected_param
                .min(node.params.len().saturating_sub(1));
            node.expanded = persisted_node.expanded && !node.params.is_empty();
        }

        for persisted_node in &nodes {
            let Some(target_id) = id_map.get(&persisted_node.id).copied() else {
                continue;
            };
            if let Some(source_old_id) = persisted_node.texture_input {
                if let Some(source_id) = id_map.get(&source_old_id).copied() {
                    let _ = project.connect_image_link(source_id, target_id);
                }
            }
            for persisted_param in &persisted_node.params {
                let Some(source_old_id) = persisted_param.signal_source else {
                    continue;
                };
                let Some(source_id) = id_map.get(&source_old_id).copied() else {
                    continue;
                };
                let Some(param_index) = project.node(target_id).and_then(|target| {
                    target.params.iter().position(|slot| {
                        persisted_param_key_matches(
                            slot.key,
                            persisted_param.key.as_str(),
                            target.kind,
                        )
                    })
                }) else {
                    let node_kind = project
                        .node(target_id)
                        .map(|node| node.kind.stable_id())
                        .unwrap_or(persisted_node.kind.as_str());
                    push_dropped_param_warning(
                        &mut dropped_param_keys,
                        &mut warnings,
                        persisted_node.id,
                        node_kind,
                        persisted_param.key.as_str(),
                    );
                    continue;
                };
                let _ = project.connect_signal_link_to_param(source_id, target_id, param_index);
            }
            for persisted_param in &persisted_node.params {
                let Some(source_old_id) = persisted_param.texture_source else {
                    continue;
                };
                let Some(source_id) = id_map.get(&source_old_id).copied() else {
                    continue;
                };
                let Some(param_index) = project.node(target_id).and_then(|target| {
                    target.params.iter().position(|slot| {
                        persisted_param_key_matches(
                            slot.key,
                            persisted_param.key.as_str(),
                            target.kind,
                        )
                    })
                }) else {
                    let node_kind = project
                        .node(target_id)
                        .map(|node| node.kind.stable_id())
                        .unwrap_or(persisted_node.kind.as_str());
                    push_dropped_param_warning(
                        &mut dropped_param_keys,
                        &mut warnings,
                        persisted_node.id,
                        node_kind,
                        persisted_param.key.as_str(),
                    );
                    continue;
                };
                let _ = project.connect_texture_link_to_param(source_id, target_id, param_index);
            }
        }

        project.recount_edges();
        project.invalidate_hit_test_cache();
        Ok(PersistedProjectLoadOutcome { project, warnings })
    }

    /// Return immutable node slice for rendering.
    pub(crate) fn nodes(&self) -> &[ProjectNode] {
        &self.nodes
    }

    /// Return current node count.
    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Return true when at least one node should render an animated signal scope.
    pub(crate) fn has_signal_preview_nodes(&self) -> bool {
        self.ensure_runtime_flags();
        self.has_signal_preview_nodes_cached.get()
    }

    /// Return and reset accumulated hit-test scan count since last call.
    pub(crate) fn take_hit_test_scan_count(&self) -> u64 {
        let count = self.hit_test_scan_count.get();
        self.hit_test_scan_count.set(0);
        count
    }

    /// Return total input-edge count currently stored in this project.
    pub(crate) fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Return immutable node by id.
    pub(crate) fn node(&self, node_id: u32) -> Option<&ProjectNode> {
        let index = self.node_index(node_id)?;
        self.nodes.get(index)
    }

    /// Return mutable node by id.
    pub(super) fn node_mut(&mut self, node_id: u32) -> Option<&mut ProjectNode> {
        let index = self.node_index(node_id)?;
        self.nodes.get_mut(index)
    }

    pub(super) fn node_index(&self, node_id: u32) -> Option<usize> {
        self.node_index_lookup.get(&node_id).copied()
    }

    pub(super) fn invalidate_hit_test_cache(&mut self) {
        self.hit_test_dirty.set(true);
        self.bump_hit_test_invalidation_epochs();
    }

    fn bump_hit_test_invalidation_epochs(&mut self) {
        self.bump_nodes_epoch();
        self.bump_wires_epoch();
        self.bump_ui_epoch();
    }

    pub(super) fn ensure_hit_test_cache(&self) {
        if !self.hit_test_dirty.get() {
            return;
        }
        let mut cache = HitTestCache::default();
        for node in &self.nodes {
            let mut node_bin_keys = Vec::new();
            collect_node_rect_bin_keys(node.x(), node.y(), node.card_height(), &mut node_bin_keys);
            for key in node_bin_keys.iter().copied() {
                cache.node_bins.entry(key).or_default().push(node.id());
            }
            cache.node_bin_keys_by_node.insert(node.id(), node_bin_keys);
            if let Some((x, y)) = output_pin_center(node) {
                let key = hit_bin_key_for_point(x, y);
                cache
                    .output_pin_bins
                    .entry(key)
                    .or_default()
                    .push(node.id());
                cache.output_pin_bin_key_by_node.insert(node.id(), key);
            }
            if let Some((x, y)) = input_pin_center(node) {
                let key = hit_bin_key_for_point(x, y);
                cache.input_pin_bins.entry(key).or_default().push(node.id());
                cache.input_pin_bin_key_by_node.insert(node.id(), key);
            }
        }
        *self.hit_test_cache.borrow_mut() = cache;
        self.hit_test_dirty.set(false);
    }

    fn apply_incremental_hit_test_cache_for_moved_nodes(&mut self, moved_node_ids: &[u32]) {
        self.bump_hit_test_invalidation_epochs();
        if moved_node_ids.is_empty() || self.hit_test_dirty.get() {
            return;
        }
        let mut cache = self.hit_test_cache.borrow_mut();
        for node_id in moved_node_ids.iter().copied() {
            self.refresh_cached_node_hit_test_entries(&mut cache, node_id);
        }
    }

    fn refresh_cached_node_hit_test_entries(&self, cache: &mut HitTestCache, node_id: u32) {
        if let Some(bin_keys) = cache.node_bin_keys_by_node.remove(&node_id) {
            for key in bin_keys {
                Self::remove_node_id_from_bin(&mut cache.node_bins, key, node_id);
            }
        }
        if let Some(key) = cache.output_pin_bin_key_by_node.remove(&node_id) {
            Self::remove_node_id_from_bin(&mut cache.output_pin_bins, key, node_id);
        }
        if let Some(key) = cache.input_pin_bin_key_by_node.remove(&node_id) {
            Self::remove_node_id_from_bin(&mut cache.input_pin_bins, key, node_id);
        }

        let Some(index) = self.node_index_lookup.get(&node_id).copied() else {
            return;
        };
        let Some(node) = self.nodes.get(index) else {
            return;
        };

        let mut node_bin_keys = Vec::new();
        collect_node_rect_bin_keys(node.x(), node.y(), node.card_height(), &mut node_bin_keys);
        for key in node_bin_keys.iter().copied() {
            Self::insert_node_id_sorted(
                &mut cache.node_bins,
                key,
                node_id,
                &self.node_index_lookup,
            );
        }
        cache.node_bin_keys_by_node.insert(node_id, node_bin_keys);

        if let Some((x, y)) = output_pin_center(node) {
            let key = hit_bin_key_for_point(x, y);
            Self::insert_node_id_sorted(
                &mut cache.output_pin_bins,
                key,
                node_id,
                &self.node_index_lookup,
            );
            cache.output_pin_bin_key_by_node.insert(node_id, key);
        }
        if let Some((x, y)) = input_pin_center(node) {
            let key = hit_bin_key_for_point(x, y);
            Self::insert_node_id_sorted(
                &mut cache.input_pin_bins,
                key,
                node_id,
                &self.node_index_lookup,
            );
            cache.input_pin_bin_key_by_node.insert(node_id, key);
        }
    }

    fn remove_node_id_from_bin(bins: &mut HashMap<i64, Vec<u32>>, key: i64, node_id: u32) {
        let mut remove_entry = false;
        if let Some(ids) = bins.get_mut(&key) {
            if let Some(index) = ids.iter().position(|existing| *existing == node_id) {
                ids.remove(index);
            }
            remove_entry = ids.is_empty();
        }
        if remove_entry {
            bins.remove(&key);
        }
    }

    fn insert_node_id_sorted(
        bins: &mut HashMap<i64, Vec<u32>>,
        key: i64,
        node_id: u32,
        node_index_lookup: &HashMap<u32, usize>,
    ) {
        let ids = bins.entry(key).or_default();
        if ids.contains(&node_id) {
            return;
        }
        ids.push(node_id);
        ids.sort_unstable_by_key(|id| node_index_lookup.get(id).copied().unwrap_or(usize::MAX));
    }

    pub(crate) fn add_node(
        &mut self,
        kind: ProjectNodeKind,
        x: i32,
        y: i32,
        panel_width: usize,
        panel_height: usize,
    ) -> u32 {
        let params = default_params_for_kind(kind);
        let card_h = node_card_height_for_param_count(false, params.len());
        let (x, y) = clamp_node_position(x, y, panel_width, panel_height, card_h);
        let node_id = self.next_node_id;
        self.next_node_id = self.next_node_id.saturating_add(1);
        self.nodes.push(ProjectNode {
            id: node_id,
            kind,
            x,
            y,
            texture_input: None,
            inputs: Vec::new(),
            params,
            selected_param: 0,
            expanded: false,
        });
        self.node_index_lookup
            .insert(node_id, self.nodes.len().saturating_sub(1));
        self.invalidate_hit_test_cache();
        self.bump_render_epoch();
        node_id
    }

    /// Move one node in graph space.
    ///
    /// Returns `true` when the node position changed.
    pub(crate) fn move_node(
        &mut self,
        node_id: u32,
        x: i32,
        y: i32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let Some(index) = self.node_index(node_id) else {
            return false;
        };
        let changed = {
            let node = &mut self.nodes[index];
            let (x, y) = clamp_node_position(x, y, panel_width, panel_height, node.card_height());
            if node.x == x && node.y == y {
                false
            } else {
                node.x = x;
                node.y = y;
                true
            }
        };
        if changed {
            self.apply_incremental_hit_test_cache_for_moved_nodes(&[node_id]);
        }
        changed
    }

    /// Move multiple nodes by one shared graph-space delta and invalidate once.
    ///
    /// This is used by group-drag interactions to avoid N repeated invalidation
    /// and hit-test rebuild signals when moving a selected set.
    pub(crate) fn move_nodes_by_delta_excluding(
        &mut self,
        node_ids: &[u32],
        exclude_node_id: Option<u32>,
        dx: i32,
        dy: i32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        if (dx == 0 && dy == 0) || node_ids.is_empty() {
            return false;
        }
        let mut changed = false;
        let mut moved_node_ids = Vec::with_capacity(node_ids.len());
        for node_id in node_ids.iter().copied() {
            if exclude_node_id == Some(node_id) {
                continue;
            }
            let Some(index) = self.node_index(node_id) else {
                continue;
            };
            let node = &mut self.nodes[index];
            let next_x = node.x.saturating_add(dx);
            let next_y = node.y.saturating_add(dy);
            let (clamped_x, clamped_y) = clamp_node_position(
                next_x,
                next_y,
                panel_width,
                panel_height,
                node.card_height(),
            );
            if node.x == clamped_x && node.y == clamped_y {
                continue;
            }
            node.x = clamped_x;
            node.y = clamped_y;
            changed = true;
            moved_node_ids.push(node_id);
        }
        if changed {
            self.apply_incremental_hit_test_cache_for_moved_nodes(moved_node_ids.as_slice());
        }
        changed
    }

    pub(crate) fn connect_image_link(&mut self, source_id: u32, target_id: u32) -> bool {
        if source_id == target_id {
            return false;
        }
        if self.depends_on(source_id, target_id) {
            // Reject links that would introduce a cycle.
            return false;
        }
        let Some(source) = self.node(source_id) else {
            return false;
        };
        if self.node(target_id).is_none() {
            return false;
        }
        let Some(source_kind) = source.kind().output_resource_kind() else {
            return false;
        };
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let changed = match source_kind {
            ResourceKind::Buffer
            | ResourceKind::Entity
            | ResourceKind::Scene
            | ResourceKind::Texture2D => {
                if target.kind.input_resource_kind() != Some(source_kind) {
                    return false;
                }
                if target.texture_input == Some(source_id) {
                    false
                } else {
                    target.texture_input = Some(source_id);
                    true
                }
            }
            ResourceKind::Signal => {
                if !target.kind.accepts_signal_bindings() || target.params.is_empty() {
                    return false;
                }
                let param_index = target
                    .selected_param
                    .min(target.params.len().saturating_sub(1));
                let slot = &mut target.params[param_index];
                if slot.widget.is_texture_target() {
                    return false;
                }
                if slot.signal_source == Some(source_id) {
                    false
                } else {
                    slot.signal_source = Some(source_id);
                    true
                }
            }
        };
        if !changed {
            return false;
        }
        rebuild_node_inputs(target);
        self.recount_edges();
        self.bump_render_epoch();
        true
    }

    pub(crate) fn insert_node_on_primary_link(
        &mut self,
        insert_node_id: u32,
        source_id: u32,
        target_id: u32,
    ) -> bool {
        if insert_node_id == source_id || insert_node_id == target_id || source_id == target_id {
            return false;
        }
        let Some(source) = self.node(source_id) else {
            return false;
        };
        let Some(insert) = self.node(insert_node_id) else {
            return false;
        };
        let Some(target) = self.node(target_id) else {
            return false;
        };
        if target.texture_input != Some(source_id) {
            return false;
        }
        let Some(source_out_kind) = source.kind.output_resource_kind() else {
            return false;
        };
        let Some(insert_in_kind) = insert.kind.input_resource_kind() else {
            return false;
        };
        let Some(insert_out_kind) = insert.kind.output_resource_kind() else {
            return false;
        };
        let Some(target_in_kind) = target.kind.input_resource_kind() else {
            return false;
        };
        if source_out_kind != insert_in_kind || insert_out_kind != target_in_kind {
            return false;
        }
        if self.depends_on(source_id, insert_node_id) || self.depends_on(insert_node_id, target_id)
        {
            return false;
        }
        let mut changed = false;
        let Some(insert) = self.node_mut(insert_node_id) else {
            return false;
        };
        changed |= set_node_primary_input(insert, Some(source_id));
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        changed |= set_node_primary_input(target, Some(insert_node_id));
        if !changed {
            return false;
        }
        self.recount_edges();
        self.bump_render_epoch();
        true
    }

    /// Disconnect one explicit source -> target link.
    ///
    /// Removes texture-input, texture-parameter, and signal-parameter bindings
    /// that match the source/target pair.
    pub(crate) fn disconnect_link(&mut self, source_id: u32, target_id: u32) -> bool {
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let mut changed = false;
        if target.texture_input == Some(source_id) {
            target.texture_input = None;
            changed = true;
        }
        for slot in &mut target.params {
            if slot.signal_source == Some(source_id) {
                slot.signal_source = None;
                changed = true;
            }
            if slot.texture_source == Some(source_id) {
                changed |= bind_texture_target_slot(slot, None);
            }
        }
        if !changed {
            return false;
        }
        rebuild_node_inputs(target);
        self.recount_edges();
        self.bump_render_epoch();
        true
    }

    /// Delete all nodes in `node_ids` and remove any links that referenced them.
    ///
    /// When possible, this also rewires surviving downstream links to the
    /// nearest surviving upstream source from the deleted chain so linear
    /// pipelines stay connected after node removal.
    ///
    /// Returns `true` when at least one node was removed.
    pub(crate) fn delete_nodes(&mut self, node_ids: &[u32]) -> bool {
        if node_ids.is_empty() {
            return false;
        }
        let mut removed_ids = node_ids.to_vec();
        removed_ids.sort_unstable();
        removed_ids.dedup();
        let removed_primary_inputs =
            collect_removed_primary_inputs(self.nodes.as_slice(), removed_ids.as_slice());
        let before_len = self.nodes.len();
        self.nodes
            .retain(|node| !contains_sorted_id(removed_ids.as_slice(), node.id()));
        let removed_any = self.nodes.len() != before_len;
        if removed_any {
            self.rebuild_node_index_lookup();
        }
        let output_kinds = collect_output_kinds(self.nodes.as_slice());
        let output_labels = collect_output_labels(self.nodes.as_slice());
        let mut links_changed = false;
        for node in &mut self.nodes {
            links_changed |= rewire_or_clear_deleted_links(
                node,
                removed_ids.as_slice(),
                &removed_primary_inputs,
                &output_kinds,
                &output_labels,
            );
        }
        if !removed_any && !links_changed {
            return false;
        }
        if removed_any {
            self.invalidate_hit_test_cache();
        }
        self.recount_edges();
        self.bump_render_epoch();
        true
    }

    /// Return source node id wired into the first `io.window_out` node, if any.
    pub(crate) fn window_out_input_node_id(&self) -> Option<u32> {
        let output = self
            .nodes
            .iter()
            .find(|node| matches!(node.kind, ProjectNodeKind::IoWindowOut))?;
        output.inputs.first().copied()
    }

    /// Return first input source node id for one node.
    pub(crate) fn input_source_node_id(&self, node_id: u32) -> Option<u32> {
        self.node(node_id)?.texture_input
    }

    /// Return resource kind for one explicit source -> target link.
    ///
    /// Returns `None` when no such link exists.
    pub(crate) fn link_resource_kind(
        &self,
        source_id: u32,
        target_id: u32,
    ) -> Option<ResourceKind> {
        let target = self.node(target_id)?;
        if target.texture_input == Some(source_id) {
            let source = self.node(source_id)?;
            return source.kind().output_resource_kind();
        }
        if target
            .params
            .iter()
            .any(|slot| slot.texture_source == Some(source_id))
        {
            return Some(ResourceKind::Texture2D);
        }
        if target
            .params
            .iter()
            .any(|slot| slot.signal_source == Some(source_id))
        {
            return Some(ResourceKind::Signal);
        }
        None
    }

    pub(super) fn depends_on(&self, start_node_id: u32, target_node_id: u32) -> bool {
        let mut stack = vec![start_node_id];
        let mut visited = HashSet::new();
        while let Some(node_id) = stack.pop() {
            if node_id == target_node_id {
                return true;
            }
            if !visited.insert(node_id) {
                continue;
            }
            if let Some(node) = self.node(node_id) {
                stack.extend(node.inputs.iter().copied());
            }
        }
        false
    }

    pub(super) fn recount_edges(&mut self) {
        self.edge_count = self.nodes.iter().map(|node| node.inputs.len()).sum();
        self.bump_wires_epoch();
        self.bump_render_epoch();
    }

    pub(super) fn bump_hit_test_scan_count(&self, delta: u64) {
        let next = self.hit_test_scan_count.get().saturating_add(delta);
        self.hit_test_scan_count.set(next);
    }

    fn rebuild_node_index_lookup(&mut self) {
        self.node_index_lookup.clear();
        for (index, node) in self.nodes.iter().enumerate() {
            self.node_index_lookup.insert(node.id(), index);
        }
    }
}

/// Push one deduplicated dropped-parameter warning.
fn push_dropped_param_warning(
    dropped_param_keys: &mut HashSet<(u32, String)>,
    warnings: &mut Vec<PersistedProjectLoadWarning>,
    persisted_node_id: u32,
    node_kind: &str,
    param_key: &str,
) {
    let key = (persisted_node_id, param_key.to_string());
    if dropped_param_keys.insert(key) {
        warnings.push(PersistedProjectLoadWarning::dropped_param(
            persisted_node_id,
            node_kind,
            param_key,
        ));
    }
}

pub(super) fn clamp_node_position(
    x: i32,
    y: i32,
    _panel_width: usize,
    _panel_height: usize,
    _node_height: i32,
) -> (i32, i32) {
    // Keep the graph canvas unbounded while enforcing deterministic grid snap.
    (snap_to_node_grid(x), snap_to_node_grid(y))
}

fn node_card_height_for_param_count(expanded: bool, param_count: usize) -> i32 {
    if !expanded || param_count == 0 {
        return NODE_HEIGHT;
    }
    NODE_HEIGHT + (param_count as i32 * NODE_PARAM_ROW_HEIGHT) + NODE_PARAM_FOOTER_PAD
}

fn collect_removed_primary_inputs(
    nodes: &[ProjectNode],
    removed_ids: &[u32],
) -> HashMap<u32, Option<u32>> {
    let mut out = HashMap::new();
    for node in nodes {
        if !contains_sorted_id(removed_ids, node.id()) {
            continue;
        }
        out.insert(node.id(), node.texture_input);
    }
    out
}

fn collect_output_kinds(nodes: &[ProjectNode]) -> HashMap<u32, ResourceKind> {
    let mut out = HashMap::new();
    for node in nodes {
        let Some(kind) = node.kind.output_resource_kind() else {
            continue;
        };
        out.insert(node.id(), kind);
    }
    out
}

fn collect_output_labels(nodes: &[ProjectNode]) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    for node in nodes {
        out.insert(node.id(), texture_source_display_label(node));
    }
    out
}

fn resolve_replacement_source(
    source_id: u32,
    removed_primary_inputs: &HashMap<u32, Option<u32>>,
) -> Option<u32> {
    let mut current = source_id;
    let mut hops = 0usize;
    loop {
        let Some(next) = removed_primary_inputs.get(&current) else {
            return Some(current);
        };
        let next = (*next)?;
        current = next;
        hops = hops.saturating_add(1);
        if hops > removed_primary_inputs.len() {
            return None;
        }
    }
}

fn rewire_or_clear_deleted_links(
    node: &mut ProjectNode,
    removed_ids: &[u32],
    removed_primary_inputs: &HashMap<u32, Option<u32>>,
    output_kinds: &HashMap<u32, ResourceKind>,
    output_labels: &HashMap<u32, String>,
) -> bool {
    let mut changed = false;
    if let Some(source) = node.texture_input {
        if contains_sorted_id(removed_ids, source) {
            let replacement =
                resolve_replacement_source(source, removed_primary_inputs).filter(|candidate| {
                    output_kinds.get(candidate).copied() == node.kind.input_resource_kind()
                });
            if node.texture_input != replacement {
                node.texture_input = replacement;
                changed = true;
            }
        }
    }
    for slot in &mut node.params {
        if let Some(source) = slot.signal_source {
            if contains_sorted_id(removed_ids, source) {
                slot.signal_source = None;
                changed = true;
            }
        }
        if let Some(source) = slot.texture_source {
            if contains_sorted_id(removed_ids, source) {
                let replacement = resolve_replacement_source(source, removed_primary_inputs)
                    .filter(|candidate| {
                        output_kinds.get(candidate).copied() == Some(ResourceKind::Texture2D)
                    });
                if let Some(source_id) = replacement {
                    let source_label = output_labels
                        .get(&source_id)
                        .cloned()
                        .unwrap_or_else(|| texture_target_placeholder().to_string());
                    changed |= bind_texture_target_slot(slot, Some((source_id, source_label)));
                } else {
                    changed |= bind_texture_target_slot(slot, None);
                }
            }
        }
    }
    if changed {
        rebuild_node_inputs(node);
    }
    changed
}

fn contains_sorted_id(ids: &[u32], id: u32) -> bool {
    ids.binary_search(&id).is_ok()
}

fn next_project_name() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("Untitled-{}", now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_node_position_snaps_to_node_grid() {
        let (x, y) = clamp_node_position(7, -7, 640, 480, NODE_HEIGHT);
        assert_eq!(x, snap_to_node_grid(7));
        assert_eq!(y, snap_to_node_grid(-7));
    }

    #[test]
    fn resolve_replacement_source_follows_primary_input_chain() {
        let removed = HashMap::from([(11u32, Some(7u32)), (7u32, Some(3u32))]);
        assert_eq!(resolve_replacement_source(11, &removed), Some(3));
    }

    #[test]
    fn resolve_replacement_source_breaks_cycles() {
        let removed = HashMap::from([(11u32, Some(7u32)), (7u32, Some(11u32))]);
        assert_eq!(resolve_replacement_source(11, &removed), None);
    }

    #[test]
    fn contains_sorted_id_uses_binary_search() {
        let ids = [1u32, 4, 9, 12];
        assert!(contains_sorted_id(&ids, 9));
        assert!(!contains_sorted_id(&ids, 8));
    }

    #[test]
    fn node_card_height_accounts_for_expanded_param_rows() {
        assert_eq!(node_card_height_for_param_count(false, 5), NODE_HEIGHT);
        assert_eq!(node_card_height_for_param_count(true, 0), NODE_HEIGHT);
        assert_eq!(
            node_card_height_for_param_count(true, 2),
            NODE_HEIGHT + 2 * NODE_PARAM_ROW_HEIGHT + NODE_PARAM_FOOTER_PAD
        );
    }
}

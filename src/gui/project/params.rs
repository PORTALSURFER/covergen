use super::state::clamp_node_position;
use super::*;

impl GuiProject {
    pub(crate) fn connect_signal_link_to_param(
        &mut self,
        source_id: u32,
        target_id: u32,
        param_index: usize,
    ) -> bool {
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
        if source.kind().output_resource_kind() != Some(ResourceKind::Signal) {
            return false;
        }
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        if !target.kind.accepts_signal_bindings() || target.params.is_empty() {
            return false;
        }
        let index = param_index.min(target.params.len().saturating_sub(1));
        let slot = &mut target.params[index];
        if slot.widget.is_texture_target() || slot.widget.is_action_button() {
            return false;
        }
        if slot.signal_source == Some(source_id) {
            return false;
        }
        slot.signal_source = Some(source_id);
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Connect one texture source node to one explicit texture-target parameter row.
    ///
    /// Returns `true` when the target parameter binding changed.
    pub(crate) fn connect_texture_link_to_param(
        &mut self,
        source_id: u32,
        target_id: u32,
        param_index: usize,
    ) -> bool {
        if source_id == target_id {
            return false;
        }
        let Some(source) = self.node(source_id) else {
            return false;
        };
        if source.kind().output_resource_kind() != Some(ResourceKind::Texture2D) {
            return false;
        }
        let source_label = texture_source_display_label(source);
        let Some(target_view) = self.node(target_id) else {
            return false;
        };
        let index = param_index.min(target_view.params.len().saturating_sub(1));
        let Some(target_slot) = target_view.params.get(index) else {
            return false;
        };
        let is_feedback_history_binding = target_view.kind == ProjectNodeKind::TexFeedback
            && target_slot.widget.is_texture_target()
            && is_feedback_history_param_key(target_slot.key);
        if !is_feedback_history_binding && self.depends_on(source_id, target_id) {
            return false;
        }
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let index = param_index.min(target.params.len().saturating_sub(1));
        let Some(slot) = target.params.get_mut(index) else {
            return false;
        };
        if !slot.widget.is_texture_target() {
            return false;
        }
        let changed = bind_texture_target_slot(slot, Some((source_id, source_label)));
        if !changed {
            return false;
        }
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn signal_param_index_for_source(
        &self,
        source_id: u32,
        target_id: u32,
    ) -> Option<usize> {
        let target = self.node(target_id)?;
        target
            .params
            .iter()
            .position(|slot| slot.signal_source == Some(source_id))
    }

    /// Return signal source node id bound to one target parameter row, if any.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn signal_source_for_param(
        &self,
        target_id: u32,
        param_index: usize,
    ) -> Option<u32> {
        let target = self.node(target_id)?;
        let slot = target.params.get(param_index)?;
        slot.signal_source
    }

    /// Return texture source node id bound to one target parameter row, if any.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn texture_source_for_param(
        &self,
        target_id: u32,
        param_index: usize,
    ) -> Option<u32> {
        let target = self.node(target_id)?;
        let slot = target.params.get(param_index)?;
        slot.texture_source
    }

    /// Return one bound source id/kind for a target parameter row, if any.
    pub(crate) fn param_link_source_for_param(
        &self,
        target_id: u32,
        param_index: usize,
    ) -> Option<(u32, ResourceKind)> {
        let target = self.node(target_id)?;
        let slot = target.params.get(param_index)?;
        if let Some(source) = slot.texture_source {
            return Some((source, ResourceKind::Texture2D));
        }
        slot.signal_source
            .map(|source| (source, ResourceKind::Signal))
    }

    /// Disconnect one explicit signal binding from a target parameter row.
    ///
    /// Returns `true` when the target parameter binding changed.
    pub(crate) fn disconnect_signal_link_from_param(
        &mut self,
        target_id: u32,
        param_index: usize,
    ) -> bool {
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let Some(slot) = target.params.get_mut(param_index) else {
            return false;
        };
        if slot.signal_source.is_none() {
            return false;
        }
        slot.signal_source = None;
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Disconnect one explicit texture binding from a target parameter row.
    ///
    /// Returns `true` when the target parameter binding changed.
    pub(crate) fn disconnect_texture_link_from_param(
        &mut self,
        target_id: u32,
        param_index: usize,
    ) -> bool {
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let Some(slot) = target.params.get_mut(param_index) else {
            return false;
        };
        if slot.texture_source.is_none() {
            return false;
        }
        if !bind_texture_target_slot(slot, None) {
            return false;
        }
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Disconnect any explicit parameter link (signal or texture) from one row.
    ///
    /// Returns `true` when the target parameter binding changed.
    pub(crate) fn disconnect_param_link_from_param(
        &mut self,
        target_id: u32,
        param_index: usize,
    ) -> bool {
        if self.disconnect_signal_link_from_param(target_id, param_index) {
            return true;
        }
        self.disconnect_texture_link_from_param(target_id, param_index)
    }

    /// Toggle one node expanded/collapsed state.
    ///
    /// Returns `true` when expanded state changed.
    pub(crate) fn toggle_node_expanded(
        &mut self,
        node_id: u32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let Some(index) = self.node_index(node_id) else {
            return false;
        };
        {
            let node = &mut self.nodes[index];
            if node.params.is_empty() {
                return false;
            }
            node.expanded = !node.expanded;
            let card_h = node.card_height();
            let (x, y) = clamp_node_position(node.x, node.y, panel_width, panel_height, card_h);
            node.x = x;
            node.y = y;
        }
        self.invalidate_hit_test_cache();
        true
    }

    /// Expand one node without toggling when it supports parameter rows.
    ///
    /// Returns `true` when expanded state changed.
    pub(crate) fn expand_node(
        &mut self,
        node_id: u32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let Some(index) = self.node_index(node_id) else {
            return false;
        };
        {
            let node = &mut self.nodes[index];
            if node.params.is_empty() || node.expanded {
                return false;
            }
            node.expanded = true;
            let card_h = node.card_height();
            let (x, y) = clamp_node_position(node.x, node.y, panel_width, panel_height, card_h);
            node.x = x;
            node.y = y;
        }
        self.invalidate_hit_test_cache();
        true
    }

    /// Collapse one node without toggling when it is currently expanded.
    ///
    /// Returns `true` when expanded state changed.
    pub(crate) fn collapse_node(
        &mut self,
        node_id: u32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let Some(index) = self.node_index(node_id) else {
            return false;
        };
        {
            let node = &mut self.nodes[index];
            if node.params.is_empty() || !node.expanded {
                return false;
            }
            node.expanded = false;
            let card_h = node.card_height();
            let (x, y) = clamp_node_position(node.x, node.y, panel_width, panel_height, card_h);
            node.x = x;
            node.y = y;
        }
        self.invalidate_hit_test_cache();
        true
    }

    /// Advance selected parameter row for one node.
    pub(crate) fn select_next_param(&mut self, node_id: u32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let max = node.params.len().saturating_sub(1);
        let next = (node.selected_param + 1).min(max);
        if next == node.selected_param {
            return false;
        }
        node.selected_param = next;
        self.bump_nodes_epoch();
        self.bump_ui_epoch();
        true
    }

    /// Move selected parameter row up for one node.
    pub(crate) fn select_prev_param(&mut self, node_id: u32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() || node.selected_param == 0 {
            return false;
        }
        node.selected_param -= 1;
        self.bump_nodes_epoch();
        self.bump_ui_epoch();
        true
    }

    /// Select one parameter row by index for one node.
    pub(crate) fn select_param(&mut self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let next = param_index.min(node.params.len().saturating_sub(1));
        if node.selected_param == next {
            return false;
        }
        node.selected_param = next;
        self.bump_nodes_epoch();
        self.bump_ui_epoch();
        true
    }

    /// Adjust selected parameter value by one step.
    pub(crate) fn adjust_selected_param(&mut self, node_id: u32, direction: f32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let index = node.selected_param.min(node.params.len().saturating_sub(1));
        let changed = adjust_slot_value(&mut node.params[index], direction);
        if changed {
            self.bump_render_epoch();
        }
        changed
    }

    /// Adjust one parameter value by `steps * slot.step` after clamping.
    ///
    /// Returns `true` when the parameter value changed.
    pub(crate) fn adjust_param(&mut self, node_id: u32, param_index: usize, steps: f32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() || !steps.is_finite() {
            return false;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        let changed = adjust_slot_value(&mut node.params[index], steps);
        if changed {
            self.bump_render_epoch();
        }
        changed
    }

    /// Return raw parameter value at one index for one node.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn node_param_raw_value(&self, node_id: u32, param_index: usize) -> Option<f32> {
        let node = self.node(node_id)?;
        if node.params.is_empty() {
            return None;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        node.params.get(index).map(|slot| slot.value)
    }

    /// Set one parameter value at one index after clamping to slot limits.
    pub(crate) fn set_param_value(&mut self, node_id: u32, param_index: usize, value: f32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        let changed = set_slot_value(&mut node.params[index], value);
        if changed {
            self.bump_render_epoch();
        }
        changed
    }

    /// Return true when a parameter row is rendered as dropdown.
    pub(crate) fn param_is_dropdown(&self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        let Some(slot) = node
            .params
            .get(param_index.min(node.params.len().saturating_sub(1)))
        else {
            return false;
        };
        slot.widget.is_dropdown()
    }

    /// Return true when a parameter row is rendered as action button.
    pub(crate) fn param_is_action_button(&self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        let Some(slot) = node
            .params
            .get(param_index.min(node.params.len().saturating_sub(1)))
        else {
            return false;
        };
        slot.widget.is_action_button()
    }

    /// Return true when a parameter row can be edited as free-form numeric text.
    pub(crate) fn param_supports_text_edit(&self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        let Some(slot) = node
            .params
            .get(param_index.min(node.params.len().saturating_sub(1)))
        else {
            return false;
        };
        !slot.widget.is_dropdown()
            && !slot.widget.is_texture_target()
            && !slot.widget.is_action_button()
    }

    /// Return true when one row accepts signal-source parameter bindings.
    pub(crate) fn param_accepts_signal_link(&self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        if !node.kind.accepts_signal_bindings() {
            return false;
        }
        let Some(slot) = node
            .params
            .get(param_index.min(node.params.len().saturating_sub(1)))
        else {
            return false;
        };
        !slot.widget.is_texture_target() && !slot.widget.is_action_button()
    }

    /// Return true when one row accepts texture-source parameter bindings.
    pub(crate) fn param_accepts_texture_link(&self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        let Some(slot) = node
            .params
            .get(param_index.min(node.params.len().saturating_sub(1)))
        else {
            return false;
        };
        slot.widget.is_texture_target()
    }

    /// Return dropdown options for one parameter row.
    pub(crate) fn node_param_dropdown_options(
        &self,
        node_id: u32,
        param_index: usize,
    ) -> Option<&'static [NodeParamOption]> {
        let node = self.node(node_id)?;
        let index = param_index.min(node.params.len().saturating_sub(1));
        node.params.get(index)?.widget.dropdown_options()
    }

    /// Return selected dropdown option index for one parameter row.
    pub(crate) fn node_param_dropdown_selected_index(
        &self,
        node_id: u32,
        param_index: usize,
    ) -> Option<usize> {
        let node = self.node(node_id)?;
        let index = param_index.min(node.params.len().saturating_sub(1));
        dropdown_selected_index(node.params.get(index)?)
    }

    /// Select one dropdown option by index for one parameter row.
    pub(crate) fn set_param_dropdown_index(
        &mut self,
        node_id: u32,
        param_index: usize,
        option_index: usize,
    ) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        let index = param_index.min(node.params.len().saturating_sub(1));
        let Some(slot) = node.params.get_mut(index) else {
            return false;
        };
        let Some(options) = slot.widget.dropdown_options() else {
            return false;
        };
        if options.is_empty() {
            return false;
        }
        let next_index = option_index.min(options.len().saturating_sub(1));
        let changed = apply_dropdown_value(slot, options, next_index);
        if changed {
            self.bump_render_epoch();
        }
        changed
    }

    /// Return expanded parameter row index hit by one graph-space point.
    pub(crate) fn param_row_at(&self, node_id: u32, x: i32, y: i32) -> Option<usize> {
        let node = self.node(node_id)?;
        if !node.expanded() {
            return None;
        }
        for index in 0..node.params.len() {
            let Some(rect) = node_param_row_rect(node, index) else {
                continue;
            };
            if rect.contains(x, y) {
                return Some(index);
            }
        }
        None
    }

    /// Return true when graph-space point falls inside one value input box.
    pub(crate) fn param_value_box_contains(
        &self,
        node_id: u32,
        param_index: usize,
        x: i32,
        y: i32,
    ) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        let Some(rect) = node_param_value_rect(node, param_index) else {
            return false;
        };
        rect.contains(x, y)
    }

    /// Return cached formatted parameter text at one index for one node.
    pub(crate) fn node_param_raw_text(&self, node_id: u32, param_index: usize) -> Option<&str> {
        let node = self.node(node_id)?;
        if node.params.is_empty() {
            return None;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        node.params.get(index).map(|slot| slot.value_text.as_str())
    }

    /// Return full descriptor details for one parameter row.
    pub(crate) fn node_param_descriptor(
        &self,
        node_id: u32,
        param_index: usize,
    ) -> Option<NodeParamDescriptor> {
        let node = self.node(node_id)?;
        if node.params.is_empty() {
            return None;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        let slot = node.params.get(index)?;
        Some(NodeParamDescriptor {
            key: slot.key,
            label: slot.label,
            value: slot.value,
            value_text: slot.value_text.clone(),
            min: slot.min,
            max: slot.max,
            step: slot.step,
            signal_source: slot.signal_source,
            texture_source: slot.texture_source,
            widget: slot.widget,
        })
    }

    /// Return true when a node is currently expanded.
    pub(crate) fn node_expanded(&self, node_id: u32) -> bool {
        self.node(node_id)
            .map(ProjectNode::expanded)
            .unwrap_or(false)
    }

    /// Return parameter slot index for one key on one node.
    pub(crate) fn node_param_slot_index(&self, node_id: u32, key: &'static str) -> Option<usize> {
        let node = self.node(node_id)?;
        node.params.iter().position(|slot| slot.key == key)
    }

    /// Return effective parameter value by pre-resolved slot index.
    ///
    /// This avoids per-sample key lookups and is intended for runtime hot-path
    /// evaluation after compile-time slot resolution.
    pub(crate) fn node_param_value_by_index(
        &self,
        node_id: u32,
        param_index: usize,
        time_secs: f32,
        eval_stack: &mut Vec<u32>,
    ) -> Option<f32> {
        let node = self.node(node_id)?;
        let slot = node.params.get(param_index)?;
        let mut value = slot.value;
        if let Some(source_id) = slot.signal_source {
            if let Some(signal) = self.sample_signal_node(source_id, time_secs, eval_stack) {
                value = signal;
            }
        }
        Some(value.clamp(slot.min, slot.max))
    }

    /// Return effective parameter value, resolving optional signal binding.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn node_param_value(
        &self,
        node_id: u32,
        key: &'static str,
        time_secs: f32,
        eval_stack: &mut Vec<u32>,
    ) -> Option<f32> {
        let index = self.node_param_slot_index(node_id, key)?;
        self.node_param_value_by_index(node_id, index, time_secs, eval_stack)
    }

    /// Evaluate one scalar signal node output.
    pub(crate) fn sample_signal_node(
        &self,
        node_id: u32,
        time_secs: f32,
        eval_stack: &mut Vec<u32>,
    ) -> Option<f32> {
        if eval_stack.contains(&node_id) {
            return None;
        }
        let node = self.node(node_id)?;
        if !node.kind.produces_signal_output() {
            return None;
        }
        eval_stack.push(node_id);
        const LFO_RATE_INDEX: usize = 0;
        const LFO_AMPLITUDE_INDEX: usize = 1;
        const LFO_PHASE_INDEX: usize = 2;
        const LFO_BIAS_INDEX: usize = 3;
        const LFO_SYNC_MODE_INDEX: usize = 4;
        const LFO_BEAT_MUL_INDEX: usize = 5;
        const LFO_TYPE_INDEX: usize = 6;
        const LFO_SHAPE_INDEX: usize = 7;
        let rate = self
            .node_param_value_by_index(node_id, LFO_RATE_INDEX, time_secs, eval_stack)
            .unwrap_or(0.4);
        let amplitude = self
            .node_param_value_by_index(node_id, LFO_AMPLITUDE_INDEX, time_secs, eval_stack)
            .unwrap_or(0.5);
        let phase = self
            .node_param_value_by_index(node_id, LFO_PHASE_INDEX, time_secs, eval_stack)
            .unwrap_or(0.0);
        let bias = self
            .node_param_value_by_index(node_id, LFO_BIAS_INDEX, time_secs, eval_stack)
            .unwrap_or(0.5);
        let sync_mode = self
            .node_param_value_by_index(node_id, LFO_SYNC_MODE_INDEX, time_secs, eval_stack)
            .unwrap_or(0.0)
            >= 0.5;
        let beat_mul = self
            .node_param_value_by_index(node_id, LFO_BEAT_MUL_INDEX, time_secs, eval_stack)
            .unwrap_or(1.0)
            .clamp(0.125, 32.0);
        let lfo_type = self
            .node_param_value_by_index(node_id, LFO_TYPE_INDEX, time_secs, eval_stack)
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, 4.0) as usize;
        let shape = self
            .node_param_value_by_index(node_id, LFO_SHAPE_INDEX, time_secs, eval_stack)
            .unwrap_or(0.0)
            .clamp(-1.0, 1.0);
        let rate_hz = if sync_mode {
            (self.lfo_sync_bpm / 60.0) * beat_mul
        } else {
            rate
        };
        let phase_time = time_secs * rate_hz + phase;
        let cycle = phase_time.rem_euclid(1.0);
        let v = (lfo_wave_sample(cycle, phase_time, lfo_type, shape) * amplitude) + bias;
        eval_stack.pop();
        Some(v)
    }
}

fn lfo_wave_sample(cycle: f32, phase_time: f32, lfo_type: usize, shape: f32) -> f32 {
    let cycle = cycle.rem_euclid(1.0);
    let shaped_cycle = apply_cycle_shape(cycle, shape);
    match lfo_type {
        1 => (2.0 * shaped_cycle) - 1.0,
        2 => 1.0 - (4.0 * (shaped_cycle - 0.5).abs()),
        3 => {
            let width = ((shape + 1.0) * 0.5).mul_add(0.8, 0.1);
            if cycle < width {
                1.0
            } else {
                -1.0
            }
        }
        4 => {
            // Drift is intentionally soft and slowly moving, using smooth 1D
            // value-noise layers over unwrapped phase time.
            let roughness = ((shape + 1.0) * 0.5).clamp(0.0, 1.0);
            let base = phase_time * (0.42 + roughness * 0.48);
            let low = smooth_value_noise(base * 0.65, 7.13);
            let mid = smooth_value_noise(base * 1.20, 19.71);
            let hi = smooth_value_noise(base * 2.30, 43.09);
            let blend = low * 0.72 + mid * 0.23 + hi * (0.05 + roughness * 0.12);
            let neighbor = smooth_value_noise((base - 0.35) * 0.65, 7.13);
            (blend * 0.78 + neighbor * 0.22).clamp(-1.0, 1.0)
        }
        _ => {
            let base = (shaped_cycle * std::f32::consts::TAU).sin();
            let harmonic = (shaped_cycle * std::f32::consts::TAU * 2.0).sin() * shape * 0.35;
            (base + harmonic).clamp(-1.0, 1.0)
        }
    }
}

fn smooth_value_noise(t: f32, offset: f32) -> f32 {
    let x = t + offset;
    let i0 = x.floor() as i32;
    let frac = x - i0 as f32;
    let v0 = hash01(i0);
    let v1 = hash01(i0 + 1);
    let smooth = frac * frac * (3.0 - 2.0 * frac);
    ((v0 + (v1 - v0) * smooth) * 2.0) - 1.0
}

fn hash01(index: i32) -> f32 {
    let value = ((index as f32 + 1.0) * 12.9898).sin() * 43_758.547;
    value - value.floor()
}

fn apply_cycle_shape(cycle: f32, shape: f32) -> f32 {
    if shape.abs() < f32::EPSILON {
        return cycle;
    }
    if shape > 0.0 {
        cycle.powf(1.0 + shape * 3.0)
    } else {
        1.0 - (1.0 - cycle).powf(1.0 + (-shape) * 3.0)
    }
}

pub(super) fn default_params_for_kind(kind: ProjectNodeKind) -> Vec<NodeParamSlot> {
    match kind {
        ProjectNodeKind::TexSolid => vec![
            param("color_r", "color_r", 0.9, 0.0, 1.0, 0.01),
            param("color_g", "color_g", 0.9, 0.0, 1.0, 0.01),
            param("color_b", "color_b", 0.9, 0.0, 1.0, 0.01),
            param("alpha", "alpha", 1.0, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::TexCircle => vec![
            param("center_x", "center_x", 0.5, 0.0, 1.0, 0.01),
            param("center_y", "center_y", 0.5, 0.0, 1.0, 0.01),
            param("radius", "radius", 0.24, 0.02, 0.5, 0.005),
            param("feather", "feather", 0.06, 0.0, 0.25, 0.005),
            param("color_r", "color_r", 0.9, 0.0, 1.0, 0.01),
            param("color_g", "color_g", 0.9, 0.0, 1.0, 0.01),
            param("color_b", "color_b", 0.9, 0.0, 1.0, 0.01),
            param("alpha", "alpha", 1.0, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::BufSphere => vec![
            param("radius", "radius", 0.28, 0.02, 0.5, 0.005),
            param("segments", "segments", 32.0, 3.0, 128.0, 1.0),
            param("rings", "rings", 16.0, 2.0, 64.0, 1.0),
        ],
        ProjectNodeKind::BufCircleNurbs => vec![
            param("radius", "radius", 0.28, 0.02, 0.95, 0.005),
            param("arc_start", "arc_start", 0.0, 0.0, 360.0, 1.0),
            param("arc_end", "arc_end", 360.0, 0.0, 360.0, 1.0),
            param_dropdown("arc_style", "arc_style", 0, &BUF_CIRCLE_ARC_STYLE_OPTIONS),
            param("line_width", "line_width", 0.01, 0.0005, 0.35, 0.001),
            param("order", "order", 3.0, 2.0, 5.0, 1.0),
            param("divisions", "divisions", 64.0, 8.0, 512.0, 1.0),
        ],
        ProjectNodeKind::BufNoise => vec![
            // Keep deformation disabled by default so inserting this node is
            // identity until users increase amplitude.
            param("amplitude", "amplitude", 0.0, 0.0, 1.0, 0.01),
            param("frequency", "frequency", 2.0, 0.05, 32.0, 0.05),
            param("speed_hz", "speed_hz", 0.35, 0.0, 16.0, 0.05),
            param("phase", "phase", 0.0, -8.0, 8.0, 0.05),
            param("seed", "seed", 1.0, 0.0, 1024.0, 1.0),
            param("twist", "twist", 0.0, -8.0, 8.0, 0.05),
            param("stretch", "stretch", 0.0, 0.0, 1.0, 0.01),
            // Loop mode quantizes time to timeline phase for clean first/last
            // frame matching and deterministic clip playback.
            param("loop_cyc", "loop_cyc", 12.0, 0.0, 256.0, 1.0),
            param_dropdown("loop_mode", "loop_mode", 0, &BUF_NOISE_LOOP_MODE_OPTIONS),
        ],
        ProjectNodeKind::TexTransform2D => vec![
            // Keep transform as identity by default so inserting this node
            // never changes output until the user edits parameters.
            param("brightness", "brightness", 1.0, 0.0, 64.0, 0.1),
            param("gain_r", "gain_r", 1.0, 0.0, 64.0, 0.1),
            param("gain_g", "gain_g", 1.0, 0.0, 64.0, 0.1),
            param("gain_b", "gain_b", 1.0, 0.0, 64.0, 0.1),
            param("alpha_mul", "alpha_mul", 1.0, 0.0, 64.0, 0.1),
        ],
        ProjectNodeKind::TexLevel => vec![
            // Keep level as identity by default so inserting this node
            // never changes output until the user edits parameters.
            param("in_low", "in_low", 0.0, 0.0, 1.0, 0.01),
            param("in_high", "in_high", 1.0, 0.0, 1.0, 0.01),
            param("gamma", "gamma", 1.0, 0.1, 8.0, 0.01),
            param("out_low", "out_low", 0.0, 0.0, 1.0, 0.01),
            param("out_high", "out_high", 1.0, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::TexFeedback => vec![
            // Optional external accumulation-history binding for feedback.
            param_texture_target(FEEDBACK_HISTORY_PARAM_KEY, FEEDBACK_HISTORY_PARAM_LABEL),
            // History output gain for delayed feedback (`history * feedback`).
            param("feedback", "feedback", 1.0, 0.0, 1.0, 0.01),
            // Clears this node's feedback history buffer.
            param_action_button(
                FEEDBACK_RESET_PARAM_KEY,
                FEEDBACK_RESET_PARAM_LABEL,
                "reset",
            ),
        ],
        ProjectNodeKind::TexReactionDiffusion => vec![
            // Gray-Scott diffusion coefficient for reagent A.
            param("diff_a", "diff_a", 1.0, 0.0, 2.0, 0.01),
            // Gray-Scott diffusion coefficient for reagent B.
            param("diff_b", "diff_b", 0.5, 0.0, 2.0, 0.01),
            // Feed rate that replenishes reagent A.
            param("feed", "feed", 0.055, 0.0, 0.12, 0.001),
            // Kill rate that removes reagent B.
            param("kill", "kill", 0.062, 0.0, 0.12, 0.001),
            // Integration step multiplier per frame.
            param("dt", "dt", 1.0, 0.0, 2.0, 0.01),
            // Blend amount for injecting source texture concentrations.
            param("seed_mix", "seed_mix", 0.04, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::TexPostColorTone => {
            post_process_params("effect", &POST_COLOR_TONE_EFFECT_OPTIONS)
        }
        ProjectNodeKind::TexPostEdgeStructure => {
            post_process_params("effect", &POST_EDGE_STRUCTURE_EFFECT_OPTIONS)
        }
        ProjectNodeKind::TexPostBlurDiffusion => {
            post_process_params("effect", &POST_BLUR_DIFFUSION_EFFECT_OPTIONS)
        }
        ProjectNodeKind::TexPostDistortion => {
            post_process_params("effect", &POST_DISTORTION_EFFECT_OPTIONS)
        }
        ProjectNodeKind::TexPostTemporal => {
            post_process_params("effect", &POST_TEMPORAL_EFFECT_OPTIONS)
        }
        ProjectNodeKind::TexPostNoiseTexture => {
            post_process_params("effect", &POST_NOISE_TEXTURE_EFFECT_OPTIONS)
        }
        ProjectNodeKind::TexPostLighting => {
            post_process_params("effect", &POST_LIGHTING_EFFECT_OPTIONS)
        }
        ProjectNodeKind::TexPostScreenSpace => {
            post_process_params("effect", &POST_SCREEN_SPACE_EFFECT_OPTIONS)
        }
        ProjectNodeKind::TexPostExperimental => {
            post_process_params("effect", &POST_EXPERIMENTAL_EFFECT_OPTIONS)
        }
        ProjectNodeKind::TexBlend => vec![
            // Optional secondary composite input for blend operations.
            param_texture_target(BLEND_LAYER_PARAM_KEY, BLEND_LAYER_PARAM_LABEL),
            param_dropdown("blend_mode", "blend_mode", 0, &TEX_BLEND_MODE_OPTIONS),
            // Keep blend as identity by default until users increase opacity.
            param("opacity", "opacity", 0.0, 0.0, 1.0, 0.01),
            // Optional post-composite background fill color.
            param("bg_r", "bg_r", 0.0, 0.0, 1.0, 0.01),
            param("bg_g", "bg_g", 0.0, 0.0, 1.0, 0.01),
            param("bg_b", "bg_b", 0.0, 0.0, 1.0, 0.01),
            // `0` keeps the output alpha unchanged; `1` fully fills background.
            param("bg_a", "bg_a", 0.0, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::SceneEntity => vec![
            param("pos_x", "pos_x", 0.5, 0.0, 1.0, 0.01),
            param("pos_y", "pos_y", 0.5, 0.0, 1.0, 0.01),
            param("scale", "scale", 1.0, 0.1, 2.0, 0.01),
            param("ambient", "ambient", 0.2, 0.0, 1.0, 0.01),
            param("color_r", "color_r", 0.9, 0.0, 1.0, 0.01),
            param("color_g", "color_g", 0.9, 0.0, 1.0, 0.01),
            param("color_b", "color_b", 0.9, 0.0, 1.0, 0.01),
            param("alpha", "alpha", 1.0, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::SceneBuild => Vec::new(),
        ProjectNodeKind::RenderCamera => {
            vec![param("zoom", "zoom", 1.0, 0.1, 8.0, 0.05)]
        }
        ProjectNodeKind::RenderScenePass => vec![
            // `0` keeps project preview resolution.
            param("res_width", "res_width", 0.0, 0.0, 8192.0, 1.0),
            // `0` keeps project preview resolution.
            param("res_height", "res_height", 0.0, 0.0, 8192.0, 1.0),
            // `with_bg` preserves the preview background clear; `alpha_clip`
            // clears transparent so only rendered scene objects remain.
            param_dropdown("bg_mode", "bg_mode", 0, &SCENE_PASS_BG_MODE_OPTIONS),
            param("edge_softness", "edge_soft", 0.01, 0.0, 0.25, 0.005),
            param("light_x", "light_x", 0.4, -1.0, 1.0, 0.02),
            param("light_y", "light_y", -0.5, -1.0, 1.0, 0.02),
            param("light_z", "light_z", 1.0, 0.0, 2.0, 0.02),
        ],
        ProjectNodeKind::CtlLfo => vec![
            param("rate_hz", "rate_hz", 0.4, 0.0, 8.0, 0.05),
            param("amplitude", "amplitude", 0.5, 0.0, 64.0, 0.1),
            param("phase", "phase", 0.0, -1.0, 1.0, 0.02),
            param("bias", "bias", 0.5, -1.0, 1.0, 0.02),
            param_dropdown("sync_mode", "sync_mode", 0, &LFO_SYNC_MODE_OPTIONS),
            param("beat_mul", "beat_mul", 1.0, 0.125, 32.0, 0.125),
            param_dropdown("lfo_type", "type", 0, &LFO_TYPE_OPTIONS),
            param("shape", "shape", 0.0, -1.0, 1.0, 0.02),
        ],
        ProjectNodeKind::IoWindowOut => Vec::new(),
    }
}

pub(super) fn param(
    key: &'static str,
    label: &'static str,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
) -> NodeParamSlot {
    assert_param_label_fits(label);
    NodeParamSlot {
        key,
        label,
        value,
        value_text: format_param_value_text(value),
        min,
        max,
        step,
        signal_source: None,
        texture_source: None,
        widget: NodeParamWidget::Number,
    }
}

/// Build one texture-target parameter slot.
fn param_texture_target(key: &'static str, label: &'static str) -> NodeParamSlot {
    assert_param_label_fits(label);
    NodeParamSlot {
        key,
        label,
        value: 0.0,
        value_text: texture_target_placeholder().to_string(),
        min: 0.0,
        max: 0.0,
        step: 0.0,
        signal_source: None,
        texture_source: None,
        widget: NodeParamWidget::TextureTarget,
    }
}

/// Build one action-button parameter slot.
fn param_action_button(
    key: &'static str,
    label: &'static str,
    button_text: &'static str,
) -> NodeParamSlot {
    assert_param_label_fits(label);
    NodeParamSlot {
        key,
        label,
        value: 0.0,
        value_text: button_text.to_string(),
        min: 0.0,
        max: 0.0,
        step: 0.0,
        signal_source: None,
        texture_source: None,
        widget: NodeParamWidget::ActionButton,
    }
}

/// Build one dropdown-parameter slot.
fn param_dropdown(
    key: &'static str,
    label: &'static str,
    default_index: usize,
    options: &'static [NodeParamOption],
) -> NodeParamSlot {
    assert_param_label_fits(label);
    let index = default_index.min(options.len().saturating_sub(1));
    let selected = options.get(index).copied().unwrap_or(NodeParamOption {
        label: "n/a",
        value: 0.0,
    });
    let mut min = selected.value;
    let mut max = selected.value;
    for option in options {
        min = min.min(option.value);
        max = max.max(option.value);
    }
    NodeParamSlot {
        key,
        label,
        value: selected.value,
        value_text: selected.label.to_string(),
        min,
        max,
        step: 1.0,
        signal_source: None,
        texture_source: None,
        widget: NodeParamWidget::Dropdown { options },
    }
}

fn post_process_params(
    effect_label: &'static str,
    options: &'static [NodeParamOption],
) -> Vec<NodeParamSlot> {
    vec![
        param_dropdown("effect", effect_label, 0, options),
        param("amount", "amount", 0.5, 0.0, 1.0, 0.01),
        param("scale", "scale", 1.0, 0.0, 8.0, 0.05),
        param("thresh", "thresh", 0.5, 0.0, 1.0, 0.01),
        param("speed", "speed", 1.0, 0.0, 8.0, 0.05),
    ]
}

fn format_param_value_text(value: f32) -> String {
    format!("{value:.3}")
}

pub(super) fn texture_target_placeholder() -> &'static str {
    TEXTURE_TARGET_PLACEHOLDER
}

pub(super) fn persisted_param_key_matches(
    slot_key: &'static str,
    persisted_key: &str,
    node_kind: ProjectNodeKind,
) -> bool {
    if slot_key == persisted_key {
        return true;
    }
    node_kind == ProjectNodeKind::TexFeedback
        && is_feedback_history_param_key(slot_key)
        && persisted_key == LEGACY_FEEDBACK_HISTORY_PARAM_KEY
}

pub(super) fn is_feedback_history_param_key(slot_key: &str) -> bool {
    slot_key == FEEDBACK_HISTORY_PARAM_KEY || slot_key == LEGACY_FEEDBACK_HISTORY_PARAM_KEY
}

fn assert_param_label_fits(label: &'static str) {
    assert!(
        label.len() <= PARAM_LABEL_MAX_LEN,
        "parameter label '{label}' exceeds {PARAM_LABEL_MAX_LEN} chars"
    );
}

pub(super) fn bind_texture_target_slot(
    slot: &mut NodeParamSlot,
    source: Option<(u32, String)>,
) -> bool {
    if !slot.widget.is_texture_target() {
        return false;
    }
    let next_source = source.as_ref().map(|(source_id, _)| *source_id);
    let next_label = source
        .as_ref()
        .map(|(_, label)| label.as_str())
        .unwrap_or(texture_target_placeholder());
    if slot.texture_source == next_source && slot.value_text == next_label {
        return false;
    }
    slot.texture_source = next_source;
    slot.value_text.clear();
    slot.value_text.push_str(next_label);
    true
}

pub(super) fn texture_source_display_label(source: &ProjectNode) -> String {
    format!("{}#{}", source.kind().label(), source.id())
}

/// Set one slot value while respecting widget semantics.
pub(super) fn set_slot_value(slot: &mut NodeParamSlot, value: f32) -> bool {
    if slot.widget.is_texture_target() || slot.widget.is_action_button() {
        return false;
    }
    if let Some(options) = slot.widget.dropdown_options() {
        if options.is_empty() {
            return false;
        }
        let next_index = nearest_dropdown_index(options, value);
        return apply_dropdown_value(slot, options, next_index);
    }
    let clamped = value.clamp(slot.min, slot.max);
    if (slot.value - clamped).abs() < 1e-6 {
        return false;
    }
    slot.value = clamped;
    slot.value_text = format_param_value_text(clamped);
    true
}

/// Adjust one slot by step count while respecting widget semantics.
fn adjust_slot_value(slot: &mut NodeParamSlot, steps: f32) -> bool {
    if !steps.is_finite() || steps.abs() <= f32::EPSILON {
        return false;
    }
    if slot.widget.is_texture_target() || slot.widget.is_action_button() {
        return false;
    }
    if let Some(options) = slot.widget.dropdown_options() {
        if options.is_empty() {
            return false;
        }
        let direction = if steps.is_sign_positive() { 1 } else { -1 };
        let current = dropdown_selected_index(slot)
            .unwrap_or_else(|| nearest_dropdown_index(options, slot.value));
        let next = if direction > 0 {
            (current + 1).min(options.len().saturating_sub(1))
        } else {
            current.saturating_sub(1)
        };
        return apply_dropdown_value(slot, options, next);
    }
    let next = (slot.value + slot.step * steps).clamp(slot.min, slot.max);
    if (next - slot.value).abs() < 1e-6 {
        return false;
    }
    slot.value = next;
    slot.value_text = format_param_value_text(next);
    true
}

/// Return selected dropdown index for one slot, if any.
fn dropdown_selected_index(slot: &NodeParamSlot) -> Option<usize> {
    let options = slot.widget.dropdown_options()?;
    if options.is_empty() {
        return None;
    }
    let by_value = options
        .iter()
        .position(|option| (option.value - slot.value).abs() < 1e-6);
    Some(by_value.unwrap_or_else(|| nearest_dropdown_index(options, slot.value)))
}

/// Return nearest option index for one dropdown value.
fn nearest_dropdown_index(options: &[NodeParamOption], value: f32) -> usize {
    let mut best_index = 0usize;
    let mut best_dist = f32::MAX;
    for (index, option) in options.iter().enumerate() {
        let dist = (option.value - value).abs();
        if dist < best_dist {
            best_dist = dist;
            best_index = index;
        }
    }
    best_index
}

/// Apply one dropdown option index to a slot value/text.
fn apply_dropdown_value(
    slot: &mut NodeParamSlot,
    options: &[NodeParamOption],
    option_index: usize,
) -> bool {
    let Some(option) = options.get(option_index).copied() else {
        return false;
    };
    if (slot.value - option.value).abs() < 1e-6 && slot.value_text == option.label {
        return false;
    }
    slot.value = option.value;
    slot.value_text.clear();
    slot.value_text.push_str(option.label);
    true
}

/// Set one node primary input source and rebuild cached input list.
pub(super) fn set_node_primary_input(node: &mut ProjectNode, source: Option<u32>) -> bool {
    if node.texture_input == source {
        return false;
    }
    node.texture_input = source;
    rebuild_node_inputs(node);
    true
}

pub(super) fn rebuild_node_inputs(node: &mut ProjectNode) {
    node.inputs.clear();
    if let Some(texture_source) = node.texture_input {
        node.inputs.push(texture_source);
    }
    for slot in &node.params {
        if node.kind == ProjectNodeKind::TexFeedback && is_feedback_history_param_key(slot.key) {
            // Feedback accumulation binding is persistent storage routing, not
            // dataflow dependency; exclude it from graph-cycle topology.
            continue;
        }
        let Some(texture_source) = slot.texture_source else {
            continue;
        };
        if !node.inputs.contains(&texture_source) {
            node.inputs.push(texture_source);
        }
    }
    for slot in &node.params {
        let Some(signal_source) = slot.signal_source else {
            continue;
        };
        if !node.inputs.contains(&signal_source) {
            node.inputs.push(signal_source);
        }
    }
}

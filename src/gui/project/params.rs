mod defaults;
mod mutation;
mod signal_eval;

use self::mutation::{adjust_slot_value, apply_dropdown_value, dropdown_selected_index};
use self::signal_eval::{lfo_wave_sample, sample_time_bucket};
use super::param_schema;
use super::state::clamp_node_position;
use super::*;

const SIGNAL_SAMPLE_TIME_BUCKETS_PER_SEC: f32 = 16_384.0;

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
        {
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
        }
        self.finalize_target_link_mutation(target_id);
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
        {
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
        }
        self.finalize_target_link_mutation(target_id);
        true
    }

    #[cfg(test)]
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
    #[cfg(test)]
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
        {
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
        }
        self.finalize_target_link_mutation(target_id);
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
        {
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
        }
        self.finalize_target_link_mutation(target_id);
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
    ///
    /// Manual edits detach an existing signal binding on the selected row.
    pub(crate) fn adjust_selected_param(&mut self, node_id: u32, direction: f32) -> bool {
        let Some(index) = self.node(node_id).and_then(|node| {
            if node.params.is_empty() {
                None
            } else {
                Some(node.selected_param.min(node.params.len().saturating_sub(1)))
            }
        }) else {
            return false;
        };
        self.mutate_param_slot_with_signal_detach(node_id, index, |slot| {
            adjust_slot_value(slot, direction)
        })
    }

    /// Adjust one parameter value by `steps * slot.step` after clamping.
    ///
    /// Manual edits detach an existing signal binding on the target row.
    ///
    /// Returns `true` when the parameter value changed.
    pub(crate) fn adjust_param(&mut self, node_id: u32, param_index: usize, steps: f32) -> bool {
        if !steps.is_finite() {
            return false;
        }
        self.mutate_param_slot_with_signal_detach(node_id, param_index, |slot| {
            adjust_slot_value(slot, steps)
        })
    }

    /// Return raw parameter value at one index for one node.
    #[cfg(test)]
    pub(crate) fn node_param_raw_value(&self, node_id: u32, param_index: usize) -> Option<f32> {
        let node = self.node(node_id)?;
        if node.params.is_empty() {
            return None;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        node.params.get(index).map(|slot| slot.value)
    }

    /// Set one parameter value at one index after clamping to slot limits.
    ///
    /// Manual edits detach an existing signal binding on the target row.
    pub(crate) fn set_param_value(&mut self, node_id: u32, param_index: usize, value: f32) -> bool {
        self.mutate_param_slot_with_signal_detach(node_id, param_index, |slot| {
            set_slot_value(slot, value)
        })
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
    ///
    /// Manual edits detach an existing signal binding on the target row.
    pub(crate) fn set_param_dropdown_index(
        &mut self,
        node_id: u32,
        param_index: usize,
        option_index: usize,
    ) -> bool {
        self.mutate_param_slot_with_signal_detach(node_id, param_index, |slot| {
            let Some(options) = slot.widget.dropdown_options() else {
                return false;
            };
            if options.is_empty() {
                return false;
            }
            let next_index = option_index.min(options.len().saturating_sub(1));
            apply_dropdown_value(slot, options, next_index)
        })
    }

    /// Mutate one parameter row, detaching signal bindings before manual edits.
    ///
    /// This centralizes the shared transaction: detach signal source, rebuild
    /// node inputs, recount edges when bindings changed, and bump render epoch
    /// when the slot value or binding changed.
    fn mutate_param_slot_with_signal_detach(
        &mut self,
        node_id: u32,
        param_index: usize,
        mutator: impl FnOnce(&mut NodeParamSlot) -> bool,
    ) -> bool {
        let mut changed = false;
        let mut binding_changed = false;
        {
            let Some(node) = self.node_mut(node_id) else {
                return false;
            };
            if node.params.is_empty() {
                return false;
            }
            let index = param_index.min(node.params.len().saturating_sub(1));
            let slot = &mut node.params[index];
            if slot.signal_source.take().is_some() {
                binding_changed = true;
                changed = true;
            }
            changed |= mutator(slot);
            if binding_changed {
                rebuild_node_inputs(node);
            }
        }
        if changed {
            if binding_changed {
                self.recount_edges();
            }
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
    #[cfg(test)]
    pub(crate) fn node_param_value_by_index<S: SignalEvalPath>(
        &self,
        node_id: u32,
        param_index: usize,
        time_secs: f32,
        eval_stack: &mut S,
    ) -> Option<f32> {
        let mut memo = None;
        self.node_param_value_by_index_impl(node_id, param_index, time_secs, eval_stack, &mut memo)
    }

    /// Return effective parameter value with a shared signal-sample memo cache.
    ///
    /// Runtime hot paths should prefer this variant while evaluating many
    /// parameters within one frame to avoid repeated recursive signal sampling.
    pub(crate) fn node_param_value_by_index_with_memo<S: SignalEvalPath>(
        &self,
        node_id: u32,
        param_index: usize,
        time_secs: f32,
        eval_stack: &mut S,
        memo: &mut SignalSampleMemo,
    ) -> Option<f32> {
        let mut memo = Some(memo);
        self.node_param_value_by_index_impl(node_id, param_index, time_secs, eval_stack, &mut memo)
    }

    fn node_param_value_by_index_impl<S: SignalEvalPath>(
        &self,
        node_id: u32,
        param_index: usize,
        time_secs: f32,
        eval_stack: &mut S,
        memo: &mut Option<&mut SignalSampleMemo>,
    ) -> Option<f32> {
        let node = self.node(node_id)?;
        let slot = node.params.get(param_index)?;
        let mut value = slot.value;
        if let Some(source_id) = slot.signal_source {
            if let Some(signal) =
                self.sample_signal_node_impl(source_id, time_secs, eval_stack, memo)
            {
                value = signal;
            }
        }
        Some(value.clamp(slot.min, slot.max))
    }

    /// Return effective parameter value, resolving optional signal binding.
    #[cfg(test)]
    pub(crate) fn node_param_value<S: SignalEvalPath>(
        &self,
        node_id: u32,
        key: &'static str,
        time_secs: f32,
        eval_stack: &mut S,
    ) -> Option<f32> {
        let index = self.node_param_slot_index(node_id, key)?;
        self.node_param_value_by_index(node_id, index, time_secs, eval_stack)
    }

    /// Evaluate one scalar signal node output.
    #[cfg(test)]
    pub(crate) fn sample_signal_node<S: SignalEvalPath>(
        &self,
        node_id: u32,
        time_secs: f32,
        eval_stack: &mut S,
    ) -> Option<f32> {
        let mut memo = None;
        self.sample_signal_node_impl(node_id, time_secs, eval_stack, &mut memo)
    }

    /// Evaluate one scalar signal node output with a shared per-frame memo map.
    ///
    /// The memo map is keyed by node id and a quantized time bucket so repeated
    /// graph evaluations within one frame can reuse prior signal samples.
    pub(crate) fn sample_signal_node_with_memo<S: SignalEvalPath>(
        &self,
        node_id: u32,
        time_secs: f32,
        eval_stack: &mut S,
        memo: &mut SignalSampleMemo,
    ) -> Option<f32> {
        let mut memo = Some(memo);
        self.sample_signal_node_impl(node_id, time_secs, eval_stack, &mut memo)
    }

    fn sample_signal_node_impl<S: SignalEvalPath>(
        &self,
        node_id: u32,
        time_secs: f32,
        eval_stack: &mut S,
        memo: &mut Option<&mut SignalSampleMemo>,
    ) -> Option<f32> {
        let bucket = sample_time_bucket(time_secs, SIGNAL_SAMPLE_TIME_BUCKETS_PER_SEC);
        if let Some(cached) = memo
            .as_deref()
            .and_then(|map| map.get(&(node_id, bucket)).copied())
        {
            return cached;
        }
        if eval_stack.contains_node(node_id) {
            if let Some(map) = memo.as_deref_mut() {
                map.insert((node_id, bucket), None);
            }
            return None;
        }
        let node = self.node(node_id)?;
        if !node.kind.produces_signal_output() {
            if let Some(map) = memo.as_deref_mut() {
                map.insert((node_id, bucket), None);
            }
            return None;
        }
        eval_stack.push_node(node_id);
        const LFO_RATE_INDEX: usize = param_schema::ctl_lfo::RATE_HZ_INDEX;
        const LFO_AMPLITUDE_INDEX: usize = param_schema::ctl_lfo::AMPLITUDE_INDEX;
        const LFO_PHASE_INDEX: usize = param_schema::ctl_lfo::PHASE_INDEX;
        const LFO_BIAS_INDEX: usize = param_schema::ctl_lfo::BIAS_INDEX;
        const LFO_SYNC_MODE_INDEX: usize = param_schema::ctl_lfo::SYNC_MODE_INDEX;
        const LFO_BEAT_MUL_INDEX: usize = param_schema::ctl_lfo::BEAT_MUL_INDEX;
        const LFO_TYPE_INDEX: usize = param_schema::ctl_lfo::LFO_TYPE_INDEX;
        const LFO_SHAPE_INDEX: usize = param_schema::ctl_lfo::SHAPE_INDEX;
        let rate = self
            .node_param_value_by_index_impl(node_id, LFO_RATE_INDEX, time_secs, eval_stack, memo)
            .unwrap_or(0.4);
        let amplitude = self
            .node_param_value_by_index_impl(
                node_id,
                LFO_AMPLITUDE_INDEX,
                time_secs,
                eval_stack,
                memo,
            )
            .unwrap_or(0.5);
        let phase = self
            .node_param_value_by_index_impl(node_id, LFO_PHASE_INDEX, time_secs, eval_stack, memo)
            .unwrap_or(0.0);
        let bias = self
            .node_param_value_by_index_impl(node_id, LFO_BIAS_INDEX, time_secs, eval_stack, memo)
            .unwrap_or(0.5);
        let sync_mode = self
            .node_param_value_by_index_impl(
                node_id,
                LFO_SYNC_MODE_INDEX,
                time_secs,
                eval_stack,
                memo,
            )
            .unwrap_or(0.0)
            >= 0.5;
        let beat_mul = self
            .node_param_value_by_index_impl(
                node_id,
                LFO_BEAT_MUL_INDEX,
                time_secs,
                eval_stack,
                memo,
            )
            .unwrap_or(1.0)
            .clamp(0.125, 32.0);
        let lfo_type = self
            .node_param_value_by_index_impl(node_id, LFO_TYPE_INDEX, time_secs, eval_stack, memo)
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, 4.0) as usize;
        let shape = self
            .node_param_value_by_index_impl(node_id, LFO_SHAPE_INDEX, time_secs, eval_stack, memo)
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
        eval_stack.pop_node();
        let sampled = Some(v);
        if let Some(map) = memo.as_deref_mut() {
            map.insert((node_id, bucket), sampled);
        }
        sampled
    }
}

pub(super) fn default_params_for_kind(kind: ProjectNodeKind) -> Vec<NodeParamSlot> {
    defaults::default_params_for_kind(kind)
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
    param_schema::feedback::is_history_key(slot_key)
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
    mutation::set_slot_value(slot, value)
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
    let mut seen_inputs =
        std::collections::HashSet::with_capacity(node.params.len().saturating_mul(2) + 1);
    if let Some(texture_source) = node.texture_input {
        if seen_inputs.insert(texture_source) {
            node.inputs.push(texture_source);
        }
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
        if seen_inputs.insert(texture_source) {
            node.inputs.push(texture_source);
        }
    }
    for slot in &node.params {
        let Some(signal_source) = slot.signal_source else {
            continue;
        };
        if seen_inputs.insert(signal_source) {
            node.inputs.push(signal_source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tex_feedback_inputs_exclude_history_binding_sources() {
        let mut project = GuiProject::new_empty(640, 480);
        let primary = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 640, 480);
        let history = project.add_node(ProjectNodeKind::TexCircle, 180, 40, 640, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 340, 40, 640, 480);

        assert!(project.connect_image_link(primary, feedback));
        assert!(project.connect_texture_link_to_param(history, feedback, 0));

        let feedback_node = project.node(feedback).expect("feedback node should exist");
        assert!(
            feedback_node.inputs().contains(&primary),
            "primary texture source should remain part of topology inputs"
        );
        assert!(
            !feedback_node.inputs().contains(&history),
            "history binding source should not be treated as graph topology input"
        );
    }

    #[test]
    fn dropdown_set_param_value_snaps_to_nearest_option_label() {
        let mut project = GuiProject::new_empty(640, 480);
        let blend = project.add_node(ProjectNodeKind::TexBlend, 20, 40, 640, 480);
        let mode_index = project
            .node_param_slot_index(blend, param_schema::blend::MODE)
            .expect("blend mode param index should exist");

        assert!(
            project.set_param_value(blend, mode_index, 2.4),
            "setting a dropdown param should update to the nearest option"
        );

        let blend_node = project.node(blend).expect("blend node should exist");
        let mode = blend_node
            .param_view(mode_index)
            .expect("blend mode param should exist");
        assert_eq!(mode.value_text, "subtract");
    }

    #[test]
    fn dropdown_adjust_selected_param_steps_directionally() {
        let mut project = GuiProject::new_empty(640, 480);
        let blend = project.add_node(ProjectNodeKind::TexBlend, 20, 40, 640, 480);
        let mode_index = project
            .node_param_slot_index(blend, param_schema::blend::MODE)
            .expect("blend mode param index should exist");
        assert!(
            project.select_param(blend, mode_index),
            "blend mode dropdown should become selected"
        );

        assert!(
            project.adjust_selected_param(blend, 1.0),
            "positive adjustment should advance dropdown to next option"
        );
        let blend_node = project.node(blend).expect("blend node should exist");
        let mode = blend_node
            .param_view(mode_index)
            .expect("blend mode param should exist");
        assert_eq!(mode.value_text, "add");

        assert!(
            project.adjust_selected_param(blend, -1.0),
            "negative adjustment should move dropdown back to previous option"
        );
        let blend_node = project.node(blend).expect("blend node should exist");
        let mode = blend_node
            .param_view(mode_index)
            .expect("blend mode param should exist");
        assert_eq!(mode.value_text, "normal");
    }
}

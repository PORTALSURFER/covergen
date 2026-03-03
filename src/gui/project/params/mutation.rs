//! Parameter slot mutation helpers shared by project param APIs.

use super::*;

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
pub(super) fn adjust_slot_value(slot: &mut NodeParamSlot, steps: f32) -> bool {
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
pub(super) fn dropdown_selected_index(slot: &NodeParamSlot) -> Option<usize> {
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
pub(super) fn nearest_dropdown_index(options: &[NodeParamOption], value: f32) -> usize {
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
pub(super) fn apply_dropdown_value(
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

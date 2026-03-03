//! Feedback/history routing helpers for GUI runtime execution.

use std::collections::HashSet;

use super::*;

/// Resolve explicit feedback-history source with canonical/legacy fallback.
pub(super) fn compiled_feedback_history_source(
    project: &GuiProject,
    step: &CompiledStep,
) -> Option<u32> {
    for slot_index in param_schema::feedback::RUNTIME_HISTORY_INDEX_FALLBACK {
        if let Some(texture_node_id) =
            super::eval_stage::compiled_texture_source_for_param(project, step, slot_index)
        {
            return Some(texture_node_id);
        }
    }
    None
}

pub(super) fn collect_external_feedback_history_sources(
    project: &GuiProject,
    steps: &[CompiledStep],
) -> HashSet<u32> {
    let mut sources = HashSet::new();
    for step in steps {
        if step.kind != CompiledStepKind::Feedback {
            continue;
        }
        if let Some(texture_node_id) = compiled_feedback_history_source(project, step) {
            sources.insert(texture_node_id);
        }
    }
    sources
}

pub(super) fn push_store_texture_op(out_ops: &mut Vec<TexRuntimeOp>, texture_node_id: u32) {
    if matches!(
        out_ops.last(),
        Some(TexRuntimeOp::StoreTexture {
            texture_node_id: last_id
        }) if *last_id == texture_node_id
    ) {
        return;
    }
    out_ops.push(TexRuntimeOp::StoreTexture { texture_node_id });
}

pub(super) fn post_process_uses_history(category: PostProcessCategory) -> bool {
    matches!(
        category,
        PostProcessCategory::Temporal
            | PostProcessCategory::NoiseTexture
            | PostProcessCategory::Experimental
    )
}

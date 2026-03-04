//! Helper routines shared by compile-stage graph traversal.

use super::*;

pub(super) fn compile_param_slots(
    project: &GuiProject,
    node_id: u32,
    keys: &[&'static str],
) -> Box<[Option<ParamSlotIndex>]> {
    keys.iter()
        .map(|key| {
            project
                .node_param_slot_index(node_id, key)
                .map(ParamSlotIndex)
        })
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

pub(super) fn compile_post_process_node(
    project: &GuiProject,
    node_id: u32,
    category: PostProcessCategory,
    traversal: &mut CompileTraversalState,
    out_steps: &mut Vec<CompiledStep>,
) -> bool {
    let Some(source_id) = project.input_source_node_id(node_id) else {
        return false;
    };
    if !compile_node(project, source_id, traversal, out_steps) {
        return false;
    }
    out_steps.push(compiled_step(
        project,
        node_id,
        CompiledStepKind::PostProcess { category },
        &param_schema::post_process::KEYS,
    ));
    true
}

//! Runtime graph compilation helpers for GUI tex preview execution.

use std::collections::HashSet;

use super::*;

fn compile_param_slots(
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

pub(super) fn compiled_step(
    project: &GuiProject,
    node_id: u32,
    kind: CompiledStepKind,
    param_keys: &[&'static str],
) -> CompiledStep {
    CompiledStep {
        node_id,
        kind,
        param_slots: compile_param_slots(project, node_id, param_keys),
    }
}

fn compile_post_process_node(
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
        &POST_PROCESS_PARAM_KEYS,
    ));
    true
}

pub(super) fn compile_node(
    project: &GuiProject,
    node_id: u32,
    traversal: &mut CompileTraversalState,
    out_steps: &mut Vec<CompiledStep>,
) -> bool {
    if traversal.visiting.contains(&node_id) {
        return false;
    }
    if traversal.visited.contains(&node_id) {
        return true;
    }
    let Some(node) = project.node(node_id) else {
        return false;
    };
    traversal.visiting.insert(node_id);
    let ok = match node.kind() {
        ProjectNodeKind::TexSolid => {
            out_steps.push(compiled_step(
                project,
                node_id,
                CompiledStepKind::Solid,
                &SOLID_PARAM_KEYS,
            ));
            true
        }
        ProjectNodeKind::TexCircle => {
            out_steps.push(compiled_step(
                project,
                node_id,
                CompiledStepKind::Circle,
                &CIRCLE_PARAM_KEYS,
            ));
            true
        }
        ProjectNodeKind::BufSphere => {
            out_steps.push(compiled_step(
                project,
                node_id,
                CompiledStepKind::SphereBuffer,
                &SPHERE_BUFFER_PARAM_KEYS,
            ));
            true
        }
        ProjectNodeKind::BufCircleNurbs => {
            out_steps.push(compiled_step(
                project,
                node_id,
                CompiledStepKind::CircleNurbsBuffer,
                &CIRCLE_NURBS_BUFFER_PARAM_KEYS,
            ));
            true
        }
        ProjectNodeKind::BufNoise => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, traversal, out_steps) {
                false
            } else {
                out_steps.push(compiled_step(
                    project,
                    node_id,
                    CompiledStepKind::BufferNoise,
                    &BUFFER_NOISE_PARAM_KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexTransform2D => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, traversal, out_steps) {
                false
            } else {
                out_steps.push(compiled_step(
                    project,
                    node_id,
                    CompiledStepKind::Transform,
                    &TRANSFORM_PARAM_KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexLevel => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, traversal, out_steps) {
                false
            } else {
                out_steps.push(compiled_step(
                    project,
                    node_id,
                    CompiledStepKind::Level,
                    &LEVEL_PARAM_KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexFeedback => {
            let source_id = project.input_source_node_id(node_id);
            let Some(source_id) = source_id else {
                return false;
            };
            if !compile_node(project, source_id, traversal, out_steps) {
                false
            } else {
                out_steps.push(compiled_step(
                    project,
                    node_id,
                    CompiledStepKind::Feedback,
                    &FEEDBACK_PARAM_KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexReactionDiffusion => {
            let source_id = project.input_source_node_id(node_id);
            let Some(source_id) = source_id else {
                return false;
            };
            if !compile_node(project, source_id, traversal, out_steps) {
                false
            } else {
                out_steps.push(compiled_step(
                    project,
                    node_id,
                    CompiledStepKind::ReactionDiffusion,
                    &REACTION_DIFFUSION_PARAM_KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexPostColorTone => compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::ColorTone,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostEdgeStructure => compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::EdgeStructure,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostBlurDiffusion => compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::BlurDiffusion,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostDistortion => compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::Distortion,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostTemporal => compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::Temporal,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostNoiseTexture => compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::NoiseTexture,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostLighting => compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::Lighting,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostScreenSpace => compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::ScreenSpace,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostExperimental => compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::Experimental,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexBlend => {
            let Some(base_source_id) = project.input_source_node_id(node_id) else {
                return false;
            };
            let layer_source_id = project
                .node_param_slot_index(node_id, BLEND_LAYER_PARAM_KEY)
                .and_then(|slot_index| project.texture_source_for_param(node_id, slot_index));
            let compile_layer_first = layer_source_id
                .map(|layer_id| node_depends_on(project, base_source_id, layer_id))
                .unwrap_or(false);
            if compile_layer_first {
                if let Some(layer_id) = layer_source_id {
                    if !compile_node(project, layer_id, traversal, out_steps) {
                        return false;
                    }
                    out_steps.push(compiled_step(
                        project,
                        layer_id,
                        CompiledStepKind::StoreTexture,
                        &[],
                    ));
                }
                if !compile_node(project, base_source_id, traversal, out_steps) {
                    return false;
                }
                out_steps.push(compiled_step(
                    project,
                    base_source_id,
                    CompiledStepKind::StoreTexture,
                    &[],
                ));
            } else {
                if !compile_node(project, base_source_id, traversal, out_steps) {
                    return false;
                }
                out_steps.push(compiled_step(
                    project,
                    base_source_id,
                    CompiledStepKind::StoreTexture,
                    &[],
                ));
                if let Some(layer_id) = layer_source_id {
                    if !compile_node(project, layer_id, traversal, out_steps) {
                        return false;
                    }
                    out_steps.push(compiled_step(
                        project,
                        layer_id,
                        CompiledStepKind::StoreTexture,
                        &[],
                    ));
                }
            }
            out_steps.push(compiled_step(
                project,
                node_id,
                CompiledStepKind::Blend {
                    base_source_id,
                    layer_source_id,
                },
                &BLEND_PARAM_KEYS,
            ));
            true
        }
        ProjectNodeKind::SceneEntity => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, traversal, out_steps) {
                false
            } else {
                out_steps.push(compiled_step(
                    project,
                    node_id,
                    CompiledStepKind::SceneEntity,
                    &SCENE_ENTITY_PARAM_KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::SceneBuild => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, traversal, out_steps) {
                false
            } else {
                out_steps.push(compiled_step(
                    project,
                    node_id,
                    CompiledStepKind::SceneBuild,
                    &[],
                ));
                true
            }
        }
        ProjectNodeKind::RenderCamera => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, traversal, out_steps) {
                false
            } else {
                out_steps.push(compiled_step(
                    project,
                    node_id,
                    CompiledStepKind::Camera,
                    &CAMERA_PARAM_KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::RenderScenePass => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, traversal, out_steps) {
                false
            } else {
                out_steps.push(compiled_step(
                    project,
                    node_id,
                    CompiledStepKind::ScenePass,
                    &SCENE_PASS_PARAM_KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::CtlLfo | ProjectNodeKind::IoWindowOut => false,
    };
    traversal.visiting.remove(&node_id);
    if ok {
        traversal.visited.insert(node_id);
    }
    ok
}

fn node_depends_on(project: &GuiProject, start_node_id: u32, target_node_id: u32) -> bool {
    if start_node_id == target_node_id {
        return true;
    }
    let mut stack = vec![start_node_id];
    let mut visited = HashSet::new();
    while let Some(node_id) = stack.pop() {
        if node_id == target_node_id {
            return true;
        }
        if !visited.insert(node_id) {
            continue;
        }
        let Some(node) = project.node(node_id) else {
            continue;
        };
        for input in node.inputs() {
            stack.push(*input);
        }
    }
    false
}

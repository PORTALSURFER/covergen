//! Runtime graph compilation helpers for GUI tex preview execution.

mod helpers;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use super::*;

pub(super) fn compiled_step(
    project: &GuiProject,
    node_id: u32,
    kind: CompiledStepKind,
    param_keys: &[&'static str],
) -> CompiledStep {
    CompiledStep {
        node_id,
        kind,
        param_slots: helpers::compile_param_slots(project, node_id, param_keys),
    }
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
                &param_schema::solid::KEYS,
            ));
            true
        }
        ProjectNodeKind::TexCircle => {
            out_steps.push(compiled_step(
                project,
                node_id,
                CompiledStepKind::Circle,
                &param_schema::circle::KEYS,
            ));
            true
        }
        ProjectNodeKind::TexSourceNoise => {
            out_steps.push(compiled_step(
                project,
                node_id,
                CompiledStepKind::SourceNoise,
                &param_schema::source_noise::KEYS,
            ));
            true
        }
        ProjectNodeKind::BufSphere => {
            out_steps.push(compiled_step(
                project,
                node_id,
                CompiledStepKind::SphereBuffer,
                &param_schema::sphere_buffer::KEYS,
            ));
            true
        }
        ProjectNodeKind::BufCircleNurbs => {
            out_steps.push(compiled_step(
                project,
                node_id,
                CompiledStepKind::CircleNurbsBuffer,
                &param_schema::circle_nurbs_buffer::KEYS,
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
                    &param_schema::buffer_noise::KEYS,
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
                    &param_schema::transform_2d::KEYS,
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
                    &param_schema::level::KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexMask => {
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
                    CompiledStepKind::Mask,
                    &param_schema::mask::KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexMorphology => {
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
                    CompiledStepKind::Morphology,
                    &param_schema::morphology::KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexToneMap => {
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
                    CompiledStepKind::ToneMap,
                    &param_schema::tone_map::KEYS,
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
                    &param_schema::feedback::RUNTIME_KEYS,
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
                    &param_schema::reaction_diffusion::KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexDomainWarp => {
            let Some(base_source_id) = project.input_source_node_id(node_id) else {
                return false;
            };
            let warp_source_id = project
                .node_param_slot_index(node_id, DOMAIN_WARP_TEXTURE_PARAM_KEY)
                .and_then(|slot_index| project.texture_source_for_param(node_id, slot_index));
            let compile_warp_first = warp_source_id
                .map(|warp_id| node_depends_on(project, base_source_id, warp_id))
                .unwrap_or(false);
            if compile_warp_first {
                if let Some(warp_id) = warp_source_id {
                    if !compile_node(project, warp_id, traversal, out_steps) {
                        return false;
                    }
                    out_steps.push(compiled_step(
                        project,
                        warp_id,
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
                if let Some(warp_id) = warp_source_id {
                    if !compile_node(project, warp_id, traversal, out_steps) {
                        return false;
                    }
                    out_steps.push(compiled_step(
                        project,
                        warp_id,
                        CompiledStepKind::StoreTexture,
                        &[],
                    ));
                }
            }
            out_steps.push(compiled_step(
                project,
                node_id,
                CompiledStepKind::DomainWarp {
                    base_source_id,
                    warp_source_id,
                },
                &param_schema::domain_warp::KEYS,
            ));
            true
        }
        ProjectNodeKind::TexWarpTransform => {
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
                    CompiledStepKind::WarpTransform,
                    &param_schema::warp_transform::KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexDirectionalSmear => {
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
                    CompiledStepKind::DirectionalSmear,
                    &param_schema::directional_smear::KEYS,
                ));
                true
            }
        }
        ProjectNodeKind::TexPostColorTone => helpers::compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::ColorTone,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostEdgeStructure => helpers::compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::EdgeStructure,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostBlurDiffusion => helpers::compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::BlurDiffusion,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostDistortion => helpers::compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::Distortion,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostTemporal => helpers::compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::Temporal,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostNoiseTexture => helpers::compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::NoiseTexture,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostLighting => helpers::compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::Lighting,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostScreenSpace => helpers::compile_post_process_node(
            project,
            node_id,
            PostProcessCategory::ScreenSpace,
            traversal,
            out_steps,
        ),
        ProjectNodeKind::TexPostExperimental => helpers::compile_post_process_node(
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
                &param_schema::blend::KEYS,
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
                    &param_schema::scene_entity::KEYS,
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
                    &param_schema::render_camera::KEYS,
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
                    &param_schema::render_scene_pass::KEYS,
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

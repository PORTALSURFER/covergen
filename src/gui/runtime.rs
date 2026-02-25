//! Compiled GPU runtime contract for GUI TOP preview graphs.
//!
//! This module normalizes GUI node graphs into a deterministic, executable
//! step list that can be evaluated directly into GPU preview operations.

use super::project::{GuiProject, ProjectNodeKind};

/// One GPU operation emitted by GUI runtime evaluation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TopRuntimeOp {
    /// `tex.solid` source operation.
    Solid {
        color_r: f32,
        color_g: f32,
        color_b: f32,
        alpha: f32,
    },
    /// `tex.circle` source operation.
    Circle {
        center_x: f32,
        center_y: f32,
        radius: f32,
        feather: f32,
        color_r: f32,
        color_g: f32,
        color_b: f32,
        alpha: f32,
    },
    /// `render.scene_pass` sphere shading operation.
    Sphere {
        center_x: f32,
        center_y: f32,
        radius: f32,
        edge_softness: f32,
        light_x: f32,
        light_y: f32,
        light_z: f32,
        ambient: f32,
        color_r: f32,
        color_g: f32,
        color_b: f32,
        alpha: f32,
    },
    /// `tex.transform_2d` operation.
    Transform {
        brightness: f32,
        gain_r: f32,
        gain_g: f32,
        gain_b: f32,
        alpha_mul: f32,
    },
}

/// One compiled step in GUI TOP runtime order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompiledStep {
    node_id: u32,
    kind: CompiledStepKind,
}

/// Executable operation kind for one compiled GUI runtime step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompiledStepKind {
    Solid,
    Circle,
    SphereBuffer,
    SceneEntity,
    SceneBuild,
    ScenePass,
    Transform,
}

#[derive(Clone, Copy, Debug)]
struct SceneEntityState {
    pos_x: f32,
    pos_y: f32,
    scale: f32,
    ambient: f32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
    alpha: f32,
}

/// Compiled GUI runtime graph rooted at `io.window_out`.
#[derive(Clone, Debug, Default)]
pub(crate) struct GuiCompiledRuntime {
    steps: Vec<CompiledStep>,
}

impl GuiCompiledRuntime {
    /// Compile one GUI project to an executable TOP runtime sequence.
    ///
    /// Returns `None` when no valid `io.window_out` chain can be compiled.
    pub(crate) fn compile(project: &GuiProject) -> Option<Self> {
        let output_source_id = project.window_out_input_node_id()?;
        let mut steps = Vec::new();
        let mut visiting = Vec::new();
        let mut visited = Vec::new();
        if !compile_node(
            project,
            output_source_id,
            &mut visiting,
            &mut visited,
            &mut steps,
        ) {
            return None;
        }
        if steps.is_empty() {
            return None;
        }
        Some(Self { steps })
    }

    /// Evaluate compiled steps into GPU runtime operations for one frame.
    pub(crate) fn evaluate_ops(
        &self,
        project: &GuiProject,
        time_secs: f32,
        eval_stack: &mut Vec<u32>,
        out_ops: &mut Vec<TopRuntimeOp>,
    ) {
        out_ops.clear();
        eval_stack.clear();
        let mut sphere_radius = None;
        let mut entity = None;
        let mut scene_ready = false;
        for step in &self.steps {
            match step.kind {
                CompiledStepKind::Solid => {
                    out_ops.push(TopRuntimeOp::Solid {
                        color_r: project
                            .node_param_value(step.node_id, "color_r", time_secs, eval_stack)
                            .unwrap_or(0.9),
                        color_g: project
                            .node_param_value(step.node_id, "color_g", time_secs, eval_stack)
                            .unwrap_or(0.9),
                        color_b: project
                            .node_param_value(step.node_id, "color_b", time_secs, eval_stack)
                            .unwrap_or(0.9),
                        alpha: project
                            .node_param_value(step.node_id, "alpha", time_secs, eval_stack)
                            .unwrap_or(1.0),
                    });
                }
                CompiledStepKind::Circle => {
                    out_ops.push(TopRuntimeOp::Circle {
                        center_x: project
                            .node_param_value(step.node_id, "center_x", time_secs, eval_stack)
                            .unwrap_or(0.5),
                        center_y: project
                            .node_param_value(step.node_id, "center_y", time_secs, eval_stack)
                            .unwrap_or(0.5),
                        radius: project
                            .node_param_value(step.node_id, "radius", time_secs, eval_stack)
                            .unwrap_or(0.24),
                        feather: project
                            .node_param_value(step.node_id, "feather", time_secs, eval_stack)
                            .unwrap_or(0.06),
                        color_r: project
                            .node_param_value(step.node_id, "color_r", time_secs, eval_stack)
                            .unwrap_or(0.9),
                        color_g: project
                            .node_param_value(step.node_id, "color_g", time_secs, eval_stack)
                            .unwrap_or(0.9),
                        color_b: project
                            .node_param_value(step.node_id, "color_b", time_secs, eval_stack)
                            .unwrap_or(0.9),
                        alpha: project
                            .node_param_value(step.node_id, "alpha", time_secs, eval_stack)
                            .unwrap_or(1.0),
                    });
                }
                CompiledStepKind::SphereBuffer => {
                    let radius = project
                        .node_param_value(step.node_id, "radius", time_secs, eval_stack)
                        .unwrap_or(0.28)
                        .max(0.01);
                    sphere_radius = Some(radius);
                    scene_ready = false;
                }
                CompiledStepKind::SceneEntity => {
                    entity = Some(SceneEntityState {
                        pos_x: project
                            .node_param_value(step.node_id, "pos_x", time_secs, eval_stack)
                            .unwrap_or(0.5),
                        pos_y: project
                            .node_param_value(step.node_id, "pos_y", time_secs, eval_stack)
                            .unwrap_or(0.5),
                        scale: project
                            .node_param_value(step.node_id, "scale", time_secs, eval_stack)
                            .unwrap_or(1.0)
                            .max(0.01),
                        ambient: project
                            .node_param_value(step.node_id, "ambient", time_secs, eval_stack)
                            .unwrap_or(0.2),
                        color_r: project
                            .node_param_value(step.node_id, "color_r", time_secs, eval_stack)
                            .unwrap_or(0.9),
                        color_g: project
                            .node_param_value(step.node_id, "color_g", time_secs, eval_stack)
                            .unwrap_or(0.9),
                        color_b: project
                            .node_param_value(step.node_id, "color_b", time_secs, eval_stack)
                            .unwrap_or(0.9),
                        alpha: project
                            .node_param_value(step.node_id, "alpha", time_secs, eval_stack)
                            .unwrap_or(1.0),
                    });
                    scene_ready = false;
                }
                CompiledStepKind::SceneBuild => {
                    scene_ready = sphere_radius.is_some() && entity.is_some();
                }
                CompiledStepKind::ScenePass => {
                    if !scene_ready {
                        continue;
                    }
                    let (Some(mesh_radius), Some(entity_state)) = (sphere_radius, entity) else {
                        continue;
                    };
                    out_ops.push(TopRuntimeOp::Sphere {
                        center_x: entity_state.pos_x,
                        center_y: entity_state.pos_y,
                        radius: (mesh_radius * entity_state.scale).max(0.01),
                        edge_softness: project
                            .node_param_value(step.node_id, "edge_softness", time_secs, eval_stack)
                            .unwrap_or(0.01),
                        light_x: project
                            .node_param_value(step.node_id, "light_x", time_secs, eval_stack)
                            .unwrap_or(0.4),
                        light_y: project
                            .node_param_value(step.node_id, "light_y", time_secs, eval_stack)
                            .unwrap_or(-0.5),
                        light_z: project
                            .node_param_value(step.node_id, "light_z", time_secs, eval_stack)
                            .unwrap_or(1.0),
                        ambient: entity_state.ambient,
                        color_r: entity_state.color_r,
                        color_g: entity_state.color_g,
                        color_b: entity_state.color_b,
                        alpha: entity_state.alpha,
                    });
                }
                CompiledStepKind::Transform => {
                    out_ops.push(TopRuntimeOp::Transform {
                        brightness: project
                            .node_param_value(step.node_id, "brightness", time_secs, eval_stack)
                            .unwrap_or(1.0),
                        gain_r: project
                            .node_param_value(step.node_id, "gain_r", time_secs, eval_stack)
                            .unwrap_or(1.0),
                        gain_g: project
                            .node_param_value(step.node_id, "gain_g", time_secs, eval_stack)
                            .unwrap_or(1.0),
                        gain_b: project
                            .node_param_value(step.node_id, "gain_b", time_secs, eval_stack)
                            .unwrap_or(1.0),
                        alpha_mul: project
                            .node_param_value(step.node_id, "alpha_mul", time_secs, eval_stack)
                            .unwrap_or(1.0),
                    });
                }
            }
        }
    }
}

fn compile_node(
    project: &GuiProject,
    node_id: u32,
    visiting: &mut Vec<u32>,
    visited: &mut Vec<u32>,
    out_steps: &mut Vec<CompiledStep>,
) -> bool {
    if visiting.contains(&node_id) {
        return false;
    }
    if visited.contains(&node_id) {
        return true;
    }
    let Some(node) = project.node(node_id) else {
        return false;
    };
    visiting.push(node_id);
    let ok = match node.kind() {
        ProjectNodeKind::TexSolid => {
            out_steps.push(CompiledStep {
                node_id,
                kind: CompiledStepKind::Solid,
            });
            true
        }
        ProjectNodeKind::TexCircle => {
            out_steps.push(CompiledStep {
                node_id,
                kind: CompiledStepKind::Circle,
            });
            true
        }
        ProjectNodeKind::BufSphere => {
            out_steps.push(CompiledStep {
                node_id,
                kind: CompiledStepKind::SphereBuffer,
            });
            true
        }
        ProjectNodeKind::TexTransform2D => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, visiting, visited, out_steps) {
                false
            } else {
                out_steps.push(CompiledStep {
                    node_id,
                    kind: CompiledStepKind::Transform,
                });
                true
            }
        }
        ProjectNodeKind::SceneEntity => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, visiting, visited, out_steps) {
                false
            } else {
                out_steps.push(CompiledStep {
                    node_id,
                    kind: CompiledStepKind::SceneEntity,
                });
                true
            }
        }
        ProjectNodeKind::SceneBuild => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, visiting, visited, out_steps) {
                false
            } else {
                out_steps.push(CompiledStep {
                    node_id,
                    kind: CompiledStepKind::SceneBuild,
                });
                true
            }
        }
        ProjectNodeKind::RenderScenePass => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, visiting, visited, out_steps) {
                false
            } else {
                out_steps.push(CompiledStep {
                    node_id,
                    kind: CompiledStepKind::ScenePass,
                });
                true
            }
        }
        ProjectNodeKind::CtlLfo | ProjectNodeKind::IoWindowOut => false,
    };
    let _ = visiting.pop();
    if ok {
        visited.push(node_id);
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::{GuiCompiledRuntime, TopRuntimeOp};
    use crate::gui::project::{GuiProject, ProjectNodeKind};

    #[test]
    fn transform_defaults_are_identity() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let transform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
        assert!(project.connect_image_link(solid, transform));
        assert!(project.connect_image_link(transform, out));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        assert_eq!(ops.len(), 2);
        assert!(matches!(
            ops[1],
            TopRuntimeOp::Transform {
                brightness,
                gain_r,
                gain_g,
                gain_b,
                alpha_mul
            } if brightness == 1.0
                && gain_r == 1.0
                && gain_g == 1.0
                && gain_b == 1.0
                && alpha_mul == 1.0
        ));
    }

    #[test]
    fn sphere_buffer_pipeline_compiles_to_sphere_op() {
        let mut project = GuiProject::new_empty(640, 480);
        let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
        assert!(project.connect_image_link(sphere, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TopRuntimeOp::Sphere { .. }));
    }
}

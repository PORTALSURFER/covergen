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
        center_x: f32,
        center_y: f32,
        radius: f32,
        feather: f32,
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
    Transform,
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
        for step in &self.steps {
            match step.kind {
                CompiledStepKind::Solid => {
                    out_ops.push(TopRuntimeOp::Solid {
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
                CompiledStepKind::Transform => {
                    out_ops.push(TopRuntimeOp::Transform {
                        brightness: project
                            .node_param_value(step.node_id, "brightness", time_secs, eval_stack)
                            .unwrap_or(1.08),
                        gain_r: project
                            .node_param_value(step.node_id, "gain_r", time_secs, eval_stack)
                            .unwrap_or(0.45),
                        gain_g: project
                            .node_param_value(step.node_id, "gain_g", time_secs, eval_stack)
                            .unwrap_or(0.8),
                        gain_b: project
                            .node_param_value(step.node_id, "gain_b", time_secs, eval_stack)
                            .unwrap_or(1.0),
                        alpha_mul: project
                            .node_param_value(step.node_id, "alpha_mul", time_secs, eval_stack)
                            .unwrap_or(0.8),
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
        ProjectNodeKind::CtlLfo | ProjectNodeKind::IoWindowOut => false,
    };
    let _ = visiting.pop();
    if ok {
        visited.push(node_id);
    }
    ok
}

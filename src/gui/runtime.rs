//! Compiled GPU runtime contract for GUI TOP preview graphs.
//!
//! This module normalizes GUI node graphs into a deterministic, executable
//! step list that can be evaluated directly into GPU preview operations.

use super::project::{GuiProject, ProjectNodeKind};

const FEEDBACK_HISTORY_PARAM_KEY: &str = "accumulation_tex";
const LEGACY_FEEDBACK_HISTORY_PARAM_KEY: &str = "target_tex";

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
        line_width: f32,
        noise_amount: f32,
        noise_freq: f32,
        noise_phase: f32,
        noise_twist: f32,
        noise_stretch: f32,
        arc_start_deg: f32,
        arc_end_deg: f32,
        segment_count: f32,
        arc_open: f32,
        color_r: f32,
        color_g: f32,
        color_b: f32,
        alpha: f32,
        alpha_clip: bool,
    },
    /// `render.scene_pass` sphere shading operation.
    Sphere {
        center_x: f32,
        center_y: f32,
        radius: f32,
        edge_softness: f32,
        noise_amount: f32,
        noise_freq: f32,
        noise_phase: f32,
        noise_twist: f32,
        noise_stretch: f32,
        light_x: f32,
        light_y: f32,
        light_z: f32,
        ambient: f32,
        color_r: f32,
        color_g: f32,
        color_b: f32,
        alpha: f32,
        alpha_clip: bool,
    },
    /// `tex.transform_2d` operation.
    Transform {
        brightness: f32,
        gain_r: f32,
        gain_g: f32,
        gain_b: f32,
        alpha_mul: f32,
    },
    /// `tex.feedback` one-frame delayed feedback operation.
    Feedback {
        mix: f32,
        history: TopRuntimeFeedbackHistoryBinding,
    },
}

/// History storage binding for one feedback operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TopRuntimeFeedbackHistoryBinding {
    /// Internal history slot owned by this feedback node.
    Internal { feedback_node_id: u32 },
    /// External history slot keyed by a texture-node id.
    External { texture_node_id: u32 },
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
    CircleNurbsBuffer,
    BufferNoise,
    SceneEntity,
    SceneBuild,
    Camera,
    ScenePass,
    Transform,
    Feedback,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SceneMeshProfile {
    Sphere,
    CircleNurbs,
}

#[derive(Clone, Copy, Debug)]
struct SceneMeshState {
    profile: SceneMeshProfile,
    radius: f32,
    arc_start_deg: f32,
    arc_end_deg: f32,
    line_width: f32,
    noise_amount: f32,
    noise_freq: f32,
    noise_phase: f32,
    noise_twist: f32,
    noise_stretch: f32,
    order: f32,
    segment_count: f32,
    arc_open: bool,
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
        let mut mesh = None;
        let mut entity = None;
        let mut scene_ready = false;
        let mut camera_zoom = 1.0_f32;
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
                        line_width: 0.0,
                        noise_amount: 0.0,
                        noise_freq: 1.0,
                        noise_phase: 0.0,
                        noise_twist: 0.0,
                        noise_stretch: 0.0,
                        arc_start_deg: 0.0,
                        arc_end_deg: 360.0,
                        segment_count: 0.0,
                        arc_open: 0.0,
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
                        alpha_clip: false,
                    });
                }
                CompiledStepKind::SphereBuffer => {
                    let radius = project
                        .node_param_value(step.node_id, "radius", time_secs, eval_stack)
                        .unwrap_or(0.28)
                        .max(0.01);
                    mesh = Some(SceneMeshState {
                        profile: SceneMeshProfile::Sphere,
                        radius,
                        arc_start_deg: 0.0,
                        arc_end_deg: 360.0,
                        line_width: 0.0,
                        noise_amount: 0.0,
                        noise_freq: 1.0,
                        noise_phase: 0.0,
                        noise_twist: 0.0,
                        noise_stretch: 0.0,
                        order: 3.0,
                        segment_count: 0.0,
                        arc_open: false,
                    });
                    scene_ready = false;
                }
                CompiledStepKind::CircleNurbsBuffer => {
                    let radius = project
                        .node_param_value(step.node_id, "radius", time_secs, eval_stack)
                        .unwrap_or(0.28)
                        .max(0.01);
                    let arc_start_deg = project
                        .node_param_value(step.node_id, "arc_start", time_secs, eval_stack)
                        .unwrap_or(0.0)
                        .clamp(0.0, 360.0);
                    let arc_end_deg = project
                        .node_param_value(step.node_id, "arc_end", time_secs, eval_stack)
                        .unwrap_or(360.0)
                        .clamp(0.0, 360.0);
                    let line_width = project
                        .node_param_value(step.node_id, "line_width", time_secs, eval_stack)
                        .unwrap_or(0.01)
                        .clamp(0.0005, 0.35);
                    let order = project
                        .node_param_value(step.node_id, "order", time_secs, eval_stack)
                        .unwrap_or(3.0)
                        .clamp(2.0, 5.0);
                    let segment_count = project
                        .node_param_value(step.node_id, "divisions", time_secs, eval_stack)
                        .unwrap_or(64.0)
                        .clamp(3.0, 512.0);
                    let arc_open = project
                        .node_param_value(step.node_id, "arc_style", time_secs, eval_stack)
                        .unwrap_or(0.0)
                        >= 0.5;
                    mesh = Some(SceneMeshState {
                        profile: SceneMeshProfile::CircleNurbs,
                        radius,
                        arc_start_deg,
                        arc_end_deg,
                        line_width,
                        noise_amount: 0.0,
                        noise_freq: 1.0,
                        noise_phase: 0.0,
                        noise_twist: 0.0,
                        noise_stretch: 0.0,
                        order,
                        segment_count,
                        arc_open,
                    });
                    scene_ready = false;
                }
                CompiledStepKind::BufferNoise => {
                    let Some(mut mesh_state) = mesh else {
                        continue;
                    };
                    let amplitude = project
                        .node_param_value(step.node_id, "amplitude", time_secs, eval_stack)
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    let frequency = project
                        .node_param_value(step.node_id, "frequency", time_secs, eval_stack)
                        .unwrap_or(2.0)
                        .max(0.01);
                    let speed_hz = project
                        .node_param_value(step.node_id, "speed_hz", time_secs, eval_stack)
                        .unwrap_or(0.35)
                        .max(0.0);
                    let phase = project
                        .node_param_value(step.node_id, "phase", time_secs, eval_stack)
                        .unwrap_or(0.0);
                    let seed = project
                        .node_param_value(step.node_id, "seed", time_secs, eval_stack)
                        .unwrap_or(1.0);
                    let twist = project
                        .node_param_value(step.node_id, "twist", time_secs, eval_stack)
                        .unwrap_or(0.0)
                        .clamp(-8.0, 8.0);
                    let stretch = project
                        .node_param_value(step.node_id, "stretch", time_secs, eval_stack)
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    let t = time_secs * speed_hz * std::f32::consts::TAU;
                    let phase_warp = layered_sine_noise(t * 0.37, frequency, phase, seed);
                    mesh_state.noise_amount = amplitude;
                    mesh_state.noise_freq = frequency;
                    mesh_state.noise_phase =
                        t + phase * std::f32::consts::TAU + seed * 0.173 + phase_warp * 0.65;
                    mesh_state.noise_twist = twist;
                    mesh_state.noise_stretch = stretch;
                    mesh = Some(mesh_state);
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
                    scene_ready = mesh.is_some() && entity.is_some();
                }
                CompiledStepKind::Camera => {
                    camera_zoom = project
                        .node_param_value(step.node_id, "zoom", time_secs, eval_stack)
                        .unwrap_or(1.0)
                        .clamp(0.1, 8.0);
                }
                CompiledStepKind::ScenePass => {
                    if !scene_ready {
                        continue;
                    }
                    let (Some(mesh_state), Some(entity_state)) = (mesh, entity) else {
                        continue;
                    };
                    let alpha_clip = project
                        .node_param_value(step.node_id, "bg_mode", time_secs, eval_stack)
                        .unwrap_or(0.0)
                        >= 0.5;
                    let zoom = camera_zoom.max(0.1);
                    let center_x = (entity_state.pos_x - 0.5) * zoom + 0.5;
                    let center_y = (entity_state.pos_y - 0.5) * zoom + 0.5;
                    match mesh_state.profile {
                        SceneMeshProfile::Sphere => out_ops.push(TopRuntimeOp::Sphere {
                            center_x,
                            center_y,
                            radius: (mesh_state.radius * entity_state.scale * zoom).max(0.01),
                            edge_softness: project
                                .node_param_value(
                                    step.node_id,
                                    "edge_softness",
                                    time_secs,
                                    eval_stack,
                                )
                                .unwrap_or(0.01)
                                .max(0.0)
                                * zoom,
                            noise_amount: mesh_state.noise_amount,
                            noise_freq: mesh_state.noise_freq,
                            noise_phase: mesh_state.noise_phase,
                            noise_twist: mesh_state.noise_twist,
                            noise_stretch: mesh_state.noise_stretch,
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
                            alpha_clip,
                        }),
                        SceneMeshProfile::CircleNurbs => out_ops.push(TopRuntimeOp::Circle {
                            center_x,
                            center_y,
                            radius: (mesh_state.radius * entity_state.scale * zoom).max(0.01),
                            feather: project
                                .node_param_value(
                                    step.node_id,
                                    "edge_softness",
                                    time_secs,
                                    eval_stack,
                                )
                                .unwrap_or(0.01)
                                * (1.0 + (5.0 - mesh_state.order).max(0.0) * 0.35)
                                * zoom,
                            line_width: (mesh_state.line_width * entity_state.scale * zoom)
                                .max(0.0005),
                            noise_amount: mesh_state.noise_amount,
                            noise_freq: mesh_state.noise_freq,
                            noise_phase: mesh_state.noise_phase,
                            noise_twist: mesh_state.noise_twist,
                            noise_stretch: mesh_state.noise_stretch,
                            arc_start_deg: mesh_state.arc_start_deg,
                            arc_end_deg: mesh_state.arc_end_deg,
                            segment_count: mesh_state.segment_count,
                            arc_open: mesh_state.arc_open as u32 as f32,
                            color_r: entity_state.color_r,
                            color_g: entity_state.color_g,
                            color_b: entity_state.color_b,
                            alpha: entity_state.alpha,
                            alpha_clip,
                        }),
                    }
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
                CompiledStepKind::Feedback => {
                    let history = project
                        .texture_source_for_param_key(step.node_id, FEEDBACK_HISTORY_PARAM_KEY)
                        .or_else(|| {
                            project.texture_source_for_param_key(
                                step.node_id,
                                LEGACY_FEEDBACK_HISTORY_PARAM_KEY,
                            )
                        })
                        .map_or(
                            TopRuntimeFeedbackHistoryBinding::Internal {
                                feedback_node_id: step.node_id,
                            },
                            |texture_node_id| TopRuntimeFeedbackHistoryBinding::External {
                                texture_node_id,
                            },
                        );
                    out_ops.push(TopRuntimeOp::Feedback {
                        mix: project
                            .node_param_value(step.node_id, "feedback", time_secs, eval_stack)
                            .unwrap_or(0.95)
                            .clamp(0.0, 1.0),
                        history,
                    });
                }
            }
        }
    }

    /// Return resolved render-texture size for this compiled output chain.
    ///
    /// The current implementation uses `render.scene_pass` `res_width`/`res_height`
    /// when present, with `0` meaning "use project preview size".
    pub(crate) fn output_texture_size(
        &self,
        project: &GuiProject,
        time_secs: f32,
        eval_stack: &mut Vec<u32>,
    ) -> (u32, u32) {
        eval_stack.clear();
        let default_w = project.preview_width.max(1);
        let default_h = project.preview_height.max(1);
        for step in self.steps.iter().rev() {
            if step.kind != CompiledStepKind::ScenePass {
                continue;
            }
            let raw_w = project
                .node_param_value(step.node_id, "res_width", time_secs, eval_stack)
                .unwrap_or(0.0);
            let raw_h = project
                .node_param_value(step.node_id, "res_height", time_secs, eval_stack)
                .unwrap_or(0.0);
            let width = if raw_w >= 1.0 {
                raw_w.round().clamp(1.0, 8192.0) as u32
            } else {
                default_w
            };
            let height = if raw_h >= 1.0 {
                raw_h.round().clamp(1.0, 8192.0) as u32
            } else {
                default_h
            };
            return (width.max(1), height.max(1));
        }
        (default_w, default_h)
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
        ProjectNodeKind::BufCircleNurbs => {
            out_steps.push(CompiledStep {
                node_id,
                kind: CompiledStepKind::CircleNurbsBuffer,
            });
            true
        }
        ProjectNodeKind::BufNoise => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, visiting, visited, out_steps) {
                false
            } else {
                out_steps.push(CompiledStep {
                    node_id,
                    kind: CompiledStepKind::BufferNoise,
                });
                true
            }
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
        ProjectNodeKind::TexFeedback => {
            let source_id = project.input_source_node_id(node_id);
            let Some(source_id) = source_id else {
                return false;
            };
            if !compile_node(project, source_id, visiting, visited, out_steps) {
                false
            } else {
                out_steps.push(CompiledStep {
                    node_id,
                    kind: CompiledStepKind::Feedback,
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
        ProjectNodeKind::RenderCamera => {
            let source_id = match project.input_source_node_id(node_id) {
                Some(id) => id,
                None => return false,
            };
            if !compile_node(project, source_id, visiting, visited, out_steps) {
                false
            } else {
                out_steps.push(CompiledStep {
                    node_id,
                    kind: CompiledStepKind::Camera,
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

/// Deterministic, lightweight pseudo-noise for buffer deformation previews.
fn layered_sine_noise(t: f32, frequency: f32, phase: f32, seed: f32) -> f32 {
    let s0 = seed * 0.13 + phase;
    let s1 = seed * 0.73 + phase * 1.9;
    let s2 = seed * 1.37 + phase * 0.47;
    let n0 = (t * frequency + s0).sin();
    let n1 = (t * frequency * 2.11 + s1).sin();
    let n2 = (t * frequency * 4.37 + s2).sin();
    (n0 * 0.62 + n1 * 0.28 + n2 * 0.10).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{GuiCompiledRuntime, TopRuntimeFeedbackHistoryBinding, TopRuntimeOp};
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
    fn feedback_pipeline_compiles_to_feedback_op() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
        assert!(project.connect_image_link(solid, feedback));
        assert!(project.connect_image_link(feedback, out));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], TopRuntimeOp::Solid { .. }));
        assert!(matches!(
            ops[1],
            TopRuntimeOp::Feedback {
                history: TopRuntimeFeedbackHistoryBinding::Internal { feedback_node_id },
                ..
            } if feedback_node_id == feedback
        ));
    }

    #[test]
    fn feedback_pipeline_requires_primary_input_even_with_target_tex_binding() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
        assert!(project.connect_texture_link_to_param(solid, feedback, 0));
        assert!(project.connect_image_link(feedback, out));

        assert!(GuiCompiledRuntime::compile(&project).is_none());
    }

    #[test]
    fn feedback_target_tex_binding_does_not_override_primary_input_source() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let circle = project.add_node(ProjectNodeKind::TexCircle, 120, 40, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 280, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 440, 40, 420, 480);
        assert!(project.connect_image_link(solid, feedback));
        assert!(project.connect_texture_link_to_param(circle, feedback, 0));
        assert!(project.connect_image_link(feedback, out));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], TopRuntimeOp::Solid { .. }));
        assert!(matches!(
            ops[1],
            TopRuntimeOp::Feedback {
                history: TopRuntimeFeedbackHistoryBinding::External { texture_node_id },
                ..
            } if texture_node_id == circle
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

    #[test]
    fn scene_pass_bg_mode_controls_alpha_clip_flag() {
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
        match ops[0] {
            TopRuntimeOp::Sphere { alpha_clip, .. } => assert!(!alpha_clip),
            _ => panic!("expected sphere op"),
        }

        assert!(project.set_param_dropdown_index(pass, 2, 1));
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        match ops[0] {
            TopRuntimeOp::Sphere { alpha_clip, .. } => assert!(alpha_clip),
            _ => panic!("expected sphere op"),
        }
    }

    #[test]
    fn tex_circle_op_keeps_alpha_clip_disabled() {
        let mut project = GuiProject::new_empty(640, 480);
        let circle = project.add_node(ProjectNodeKind::TexCircle, 20, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 180, 40, 420, 480);
        assert!(project.connect_image_link(circle, out));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        match ops[0] {
            TopRuntimeOp::Circle { alpha_clip, .. } => assert!(!alpha_clip),
            _ => panic!("expected circle op"),
        }
    }

    #[test]
    fn camera_zoom_scales_scene_pass_radius() {
        let mut project = GuiProject::new_empty(640, 480);
        let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
        let camera = project.add_node(ProjectNodeKind::RenderCamera, 500, 40, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 660, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 820, 40, 420, 480);
        assert!(project.connect_image_link(sphere, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, camera));
        assert!(project.connect_image_link(camera, pass));
        assert!(project.connect_image_link(pass, out));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        let radius_default = match ops[0] {
            TopRuntimeOp::Sphere { radius, .. } => radius,
            _ => panic!("expected sphere op"),
        };

        assert!(project.set_param_value(camera, 0, 2.0));
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        let radius_zoomed = match ops[0] {
            TopRuntimeOp::Sphere { radius, .. } => radius,
            _ => panic!("expected sphere op"),
        };
        assert!(radius_zoomed > radius_default * 1.9);
    }

    #[test]
    fn scene_pass_resolution_defaults_to_project_size_when_zero() {
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
        let (w, h) = runtime.output_texture_size(&project, 0.0, &mut eval_stack);
        assert_eq!((w, h), (640, 480));
        assert!(project.set_param_value(pass, 0, 320.0));
        assert!(project.set_param_value(pass, 1, 200.0));
        let (w2, h2) = runtime.output_texture_size(&project, 0.0, &mut eval_stack);
        assert_eq!((w2, h2), (320, 200));
    }

    #[test]
    fn circle_nurbs_buffer_pipeline_compiles_to_circle_op() {
        let mut project = GuiProject::new_empty(640, 480);
        let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 20, 40, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
        assert!(project.connect_image_link(circle, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], TopRuntimeOp::Circle { .. }));
    }

    #[test]
    fn circle_nurbs_params_propagate_to_circle_op() {
        let mut project = GuiProject::new_empty(640, 480);
        let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 20, 40, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
        assert!(project.connect_image_link(circle, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));

        assert!(project.set_param_value(circle, 1, 30.0));
        assert!(project.set_param_value(circle, 2, 150.0));
        assert!(project.set_param_value(circle, 3, 1.0));
        assert!(project.set_param_value(circle, 4, 0.006));
        assert!(project.set_param_value(circle, 5, 2.0));
        assert!(project.set_param_value(circle, 6, 12.0));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        match ops[0] {
            TopRuntimeOp::Circle {
                arc_start_deg,
                arc_end_deg,
                segment_count,
                arc_open,
                line_width,
                feather,
                ..
            } => {
                assert_eq!(arc_start_deg, 30.0);
                assert_eq!(arc_end_deg, 150.0);
                assert_eq!(segment_count, 12.0);
                assert_eq!(arc_open, 1.0);
                assert!(line_width <= 0.007);
                assert!(feather > 0.01);
            }
            _ => panic!("expected circle op"),
        }
    }

    #[test]
    fn buffer_noise_deforms_mesh_shape_parameters() {
        let mut project = GuiProject::new_empty(640, 480);
        let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
        let noise = project.add_node(ProjectNodeKind::BufNoise, 180, 40, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 340, 40, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 500, 40, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 660, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 820, 40, 420, 480);
        assert!(project.connect_image_link(sphere, noise));
        assert!(project.connect_image_link(noise, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));

        assert!(project.set_param_value(noise, 0, 0.5));
        assert!(project.set_param_value(noise, 1, 3.0));
        assert!(project.set_param_value(noise, 2, 1.0));
        assert!(project.set_param_value(noise, 4, 17.0));
        assert!(project.set_param_value(noise, 5, 2.5));
        assert!(project.set_param_value(noise, 6, 0.4));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops_t0 = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops_t0);
        let mut ops_t1 = Vec::new();
        runtime.evaluate_ops(&project, 0.5, &mut eval_stack, &mut ops_t1);
        let (r0, phase0, twist0, stretch0) = match ops_t0[0] {
            TopRuntimeOp::Sphere {
                radius,
                noise_phase,
                noise_twist,
                noise_stretch,
                ..
            } => (radius, noise_phase, noise_twist, noise_stretch),
            _ => panic!("expected sphere op"),
        };
        let (r1, phase1, twist1, stretch1) = match ops_t1[0] {
            TopRuntimeOp::Sphere {
                radius,
                noise_phase,
                noise_twist,
                noise_stretch,
                ..
            } => (radius, noise_phase, noise_twist, noise_stretch),
            _ => panic!("expected sphere op"),
        };
        assert_eq!(r0, r1);
        assert_ne!(phase0, phase1);
        assert!(twist0 > 2.4 && twist1 > 2.4);
        assert!(stretch0 > 0.39 && stretch1 > 0.39);
    }
}

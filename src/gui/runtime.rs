//! Compiled GPU runtime contract for GUI tex preview graphs.
//!
//! This module normalizes GUI node graphs into a deterministic, executable
//! step list that can be evaluated directly into GPU preview operations.

use super::project::{GuiProject, ProjectNodeKind};

const FEEDBACK_HISTORY_PARAM_KEY: &str = "accumulation_tex";
const LEGACY_FEEDBACK_HISTORY_PARAM_KEY: &str = "target_tex";
const BLEND_LAYER_PARAM_KEY: &str = "blend_tex";
const DEFAULT_LOOP_FPS: u32 = 60;
const SOLID_PARAM_KEYS: [&str; 4] = ["color_r", "color_g", "color_b", "alpha"];
const SOLID_COLOR_R_SLOT: usize = 0;
const SOLID_COLOR_G_SLOT: usize = 1;
const SOLID_COLOR_B_SLOT: usize = 2;
const SOLID_ALPHA_SLOT: usize = 3;
const CIRCLE_PARAM_KEYS: [&str; 8] = [
    "center_x", "center_y", "radius", "feather", "color_r", "color_g", "color_b", "alpha",
];
const CIRCLE_CENTER_X_SLOT: usize = 0;
const CIRCLE_CENTER_Y_SLOT: usize = 1;
const CIRCLE_RADIUS_SLOT: usize = 2;
const CIRCLE_FEATHER_SLOT: usize = 3;
const CIRCLE_COLOR_R_SLOT: usize = 4;
const CIRCLE_COLOR_G_SLOT: usize = 5;
const CIRCLE_COLOR_B_SLOT: usize = 6;
const CIRCLE_ALPHA_SLOT: usize = 7;
const SPHERE_BUFFER_PARAM_KEYS: [&str; 1] = ["radius"];
const SPHERE_BUFFER_RADIUS_SLOT: usize = 0;
const CIRCLE_NURBS_BUFFER_PARAM_KEYS: [&str; 7] = [
    "radius",
    "arc_start",
    "arc_end",
    "line_width",
    "order",
    "divisions",
    "arc_style",
];
const CIRCLE_NURBS_BUFFER_RADIUS_SLOT: usize = 0;
const CIRCLE_NURBS_BUFFER_ARC_START_SLOT: usize = 1;
const CIRCLE_NURBS_BUFFER_ARC_END_SLOT: usize = 2;
const CIRCLE_NURBS_BUFFER_LINE_WIDTH_SLOT: usize = 3;
const CIRCLE_NURBS_BUFFER_ORDER_SLOT: usize = 4;
const CIRCLE_NURBS_BUFFER_DIVISIONS_SLOT: usize = 5;
const CIRCLE_NURBS_BUFFER_ARC_STYLE_SLOT: usize = 6;
const BUFFER_NOISE_PARAM_KEYS: [&str; 9] = [
    "amplitude",
    "frequency",
    "speed_hz",
    "phase",
    "seed",
    "twist",
    "stretch",
    "loop_cyc",
    "loop_mode",
];
const BUFFER_NOISE_AMPLITUDE_SLOT: usize = 0;
const BUFFER_NOISE_FREQUENCY_SLOT: usize = 1;
const BUFFER_NOISE_SPEED_HZ_SLOT: usize = 2;
const BUFFER_NOISE_PHASE_SLOT: usize = 3;
const BUFFER_NOISE_SEED_SLOT: usize = 4;
const BUFFER_NOISE_TWIST_SLOT: usize = 5;
const BUFFER_NOISE_STRETCH_SLOT: usize = 6;
const BUFFER_NOISE_LOOP_CYC_SLOT: usize = 7;
const BUFFER_NOISE_LOOP_MODE_SLOT: usize = 8;
const SCENE_ENTITY_PARAM_KEYS: [&str; 8] = [
    "pos_x", "pos_y", "scale", "ambient", "color_r", "color_g", "color_b", "alpha",
];
const SCENE_ENTITY_POS_X_SLOT: usize = 0;
const SCENE_ENTITY_POS_Y_SLOT: usize = 1;
const SCENE_ENTITY_SCALE_SLOT: usize = 2;
const SCENE_ENTITY_AMBIENT_SLOT: usize = 3;
const SCENE_ENTITY_COLOR_R_SLOT: usize = 4;
const SCENE_ENTITY_COLOR_G_SLOT: usize = 5;
const SCENE_ENTITY_COLOR_B_SLOT: usize = 6;
const SCENE_ENTITY_ALPHA_SLOT: usize = 7;
const CAMERA_PARAM_KEYS: [&str; 1] = ["zoom"];
const CAMERA_ZOOM_SLOT: usize = 0;
const SCENE_PASS_PARAM_KEYS: [&str; 7] = [
    "res_width",
    "res_height",
    "bg_mode",
    "edge_softness",
    "light_x",
    "light_y",
    "light_z",
];
const SCENE_PASS_RES_WIDTH_SLOT: usize = 0;
const SCENE_PASS_RES_HEIGHT_SLOT: usize = 1;
const SCENE_PASS_BG_MODE_SLOT: usize = 2;
const SCENE_PASS_EDGE_SOFTNESS_SLOT: usize = 3;
const SCENE_PASS_LIGHT_X_SLOT: usize = 4;
const SCENE_PASS_LIGHT_Y_SLOT: usize = 5;
const SCENE_PASS_LIGHT_Z_SLOT: usize = 6;
const TRANSFORM_PARAM_KEYS: [&str; 5] = ["brightness", "gain_r", "gain_g", "gain_b", "alpha_mul"];
const TRANSFORM_BRIGHTNESS_SLOT: usize = 0;
const TRANSFORM_GAIN_R_SLOT: usize = 1;
const TRANSFORM_GAIN_G_SLOT: usize = 2;
const TRANSFORM_GAIN_B_SLOT: usize = 3;
const TRANSFORM_ALPHA_MUL_SLOT: usize = 4;
const LEVEL_PARAM_KEYS: [&str; 5] = ["in_low", "in_high", "gamma", "out_low", "out_high"];
const LEVEL_IN_LOW_SLOT: usize = 0;
const LEVEL_IN_HIGH_SLOT: usize = 1;
const LEVEL_GAMMA_SLOT: usize = 2;
const LEVEL_OUT_LOW_SLOT: usize = 3;
const LEVEL_OUT_HIGH_SLOT: usize = 4;
const FEEDBACK_PARAM_KEYS: [&str; 3] = [
    "feedback",
    FEEDBACK_HISTORY_PARAM_KEY,
    LEGACY_FEEDBACK_HISTORY_PARAM_KEY,
];
const FEEDBACK_MIX_SLOT: usize = 0;
const FEEDBACK_HISTORY_SLOT: usize = 1;
const FEEDBACK_LEGACY_HISTORY_SLOT: usize = 2;
const REACTION_DIFFUSION_PARAM_KEYS: [&str; 6] =
    ["diff_a", "diff_b", "feed", "kill", "dt", "seed_mix"];
const REACTION_DIFFUSION_DIFF_A_SLOT: usize = 0;
const REACTION_DIFFUSION_DIFF_B_SLOT: usize = 1;
const REACTION_DIFFUSION_FEED_SLOT: usize = 2;
const REACTION_DIFFUSION_KILL_SLOT: usize = 3;
const REACTION_DIFFUSION_DT_SLOT: usize = 4;
const REACTION_DIFFUSION_SEED_MIX_SLOT: usize = 5;
const BLEND_PARAM_KEYS: [&str; 6] = ["blend_mode", "opacity", "bg_r", "bg_g", "bg_b", "bg_a"];
const BLEND_MODE_SLOT: usize = 0;
const BLEND_OPACITY_SLOT: usize = 1;
const BLEND_BG_R_SLOT: usize = 2;
const BLEND_BG_G_SLOT: usize = 3;
const BLEND_BG_B_SLOT: usize = 4;
const BLEND_BG_A_SLOT: usize = 5;

/// Compile-time resolved parameter slot index for one node parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ParamSlotIndex(usize);

/// One GPU operation emitted by GUI runtime evaluation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TexRuntimeOp {
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
    /// `tex.level` operation.
    Level {
        in_low: f32,
        in_high: f32,
        gamma: f32,
        out_low: f32,
        out_high: f32,
    },
    /// `tex.feedback` one-frame delayed feedback operation.
    Feedback {
        mix: f32,
        history: TexRuntimeFeedbackHistoryBinding,
    },
    /// `tex.reaction_diffusion` temporal Gray-Scott simulation operation.
    ReactionDiffusion {
        diffusion_a: f32,
        diffusion_b: f32,
        feed: f32,
        kill: f32,
        dt: f32,
        seed_mix: f32,
        history: TexRuntimeFeedbackHistoryBinding,
    },
    /// Cache the current operation output under one texture-node id.
    StoreTexture { texture_node_id: u32 },
    /// `tex.blend` two-texture compositing operation.
    Blend {
        mode: f32,
        opacity: f32,
        bg_r: f32,
        bg_g: f32,
        bg_b: f32,
        bg_a: f32,
        base_texture_node_id: u32,
        layer_texture_node_id: Option<u32>,
    },
}

/// History storage binding for one feedback operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TexRuntimeFeedbackHistoryBinding {
    /// Internal history slot owned by this feedback node.
    Internal { feedback_node_id: u32 },
    /// External history slot keyed by a texture-node id.
    External { texture_node_id: u32 },
}

/// Frame-clock context used for timeline-locked operation evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TexRuntimeFrameContext {
    /// Current timeline frame index.
    pub(crate) frame_index: u32,
    /// Total timeline frame count.
    pub(crate) frame_total: u32,
}

/// One compiled step in GUI tex runtime order.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CompiledStep {
    node_id: u32,
    kind: CompiledStepKind,
    param_slots: Box<[Option<ParamSlotIndex>]>,
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
    Level,
    Feedback,
    ReactionDiffusion,
    StoreTexture,
    Blend {
        base_source_id: u32,
        layer_source_id: Option<u32>,
    },
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
    /// Compile one GUI project to an executable tex runtime sequence.
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
    #[cfg(test)]
    pub(crate) fn evaluate_ops(
        &self,
        project: &GuiProject,
        time_secs: f32,
        eval_stack: &mut Vec<u32>,
        out_ops: &mut Vec<TexRuntimeOp>,
    ) {
        self.evaluate_ops_with_frame(project, time_secs, None, eval_stack, out_ops);
    }

    /// Evaluate compiled steps into GPU runtime operations for one frame.
    ///
    /// When `frame` is provided, loop-mode temporal nodes can lock animation to
    /// timeline phase and guarantee seam-free first/last frame matching.
    pub(crate) fn evaluate_ops_with_frame(
        &self,
        project: &GuiProject,
        time_secs: f32,
        frame: Option<TexRuntimeFrameContext>,
        eval_stack: &mut Vec<u32>,
        out_ops: &mut Vec<TexRuntimeOp>,
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
                    out_ops.push(TexRuntimeOp::Solid {
                        color_r: compiled_param_value_opt(
                            project,
                            step,
                            SOLID_COLOR_R_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.9),
                        color_g: compiled_param_value_opt(
                            project,
                            step,
                            SOLID_COLOR_G_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.9),
                        color_b: compiled_param_value_opt(
                            project,
                            step,
                            SOLID_COLOR_B_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.9),
                        alpha: compiled_param_value_opt(
                            project,
                            step,
                            SOLID_ALPHA_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0),
                    });
                }
                CompiledStepKind::Circle => {
                    out_ops.push(TexRuntimeOp::Circle {
                        center_x: compiled_param_value_opt(
                            project,
                            step,
                            CIRCLE_CENTER_X_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.5),
                        center_y: compiled_param_value_opt(
                            project,
                            step,
                            CIRCLE_CENTER_Y_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.5),
                        radius: compiled_param_value_opt(
                            project,
                            step,
                            CIRCLE_RADIUS_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.24),
                        feather: compiled_param_value_opt(
                            project,
                            step,
                            CIRCLE_FEATHER_SLOT,
                            time_secs,
                            eval_stack,
                        )
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
                        color_r: compiled_param_value_opt(
                            project,
                            step,
                            CIRCLE_COLOR_R_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.9),
                        color_g: compiled_param_value_opt(
                            project,
                            step,
                            CIRCLE_COLOR_G_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.9),
                        color_b: compiled_param_value_opt(
                            project,
                            step,
                            CIRCLE_COLOR_B_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.9),
                        alpha: compiled_param_value_opt(
                            project,
                            step,
                            CIRCLE_ALPHA_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0),
                        alpha_clip: false,
                    });
                }
                CompiledStepKind::SphereBuffer => {
                    let radius = compiled_param_value_opt(
                        project,
                        step,
                        SPHERE_BUFFER_RADIUS_SLOT,
                        time_secs,
                        eval_stack,
                    )
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
                    let radius = compiled_param_value_opt(
                        project,
                        step,
                        CIRCLE_NURBS_BUFFER_RADIUS_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.28)
                    .max(0.01);
                    let arc_start_deg = compiled_param_value_opt(
                        project,
                        step,
                        CIRCLE_NURBS_BUFFER_ARC_START_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.0)
                    .clamp(0.0, 360.0);
                    let arc_end_deg = compiled_param_value_opt(
                        project,
                        step,
                        CIRCLE_NURBS_BUFFER_ARC_END_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(360.0)
                    .clamp(0.0, 360.0);
                    let line_width = compiled_param_value_opt(
                        project,
                        step,
                        CIRCLE_NURBS_BUFFER_LINE_WIDTH_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.01)
                    .clamp(0.0005, 0.35);
                    let order = compiled_param_value_opt(
                        project,
                        step,
                        CIRCLE_NURBS_BUFFER_ORDER_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(3.0)
                    .clamp(2.0, 5.0);
                    let segment_count = compiled_param_value_opt(
                        project,
                        step,
                        CIRCLE_NURBS_BUFFER_DIVISIONS_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(64.0)
                    .clamp(3.0, 512.0);
                    let arc_open = compiled_param_value_opt(
                        project,
                        step,
                        CIRCLE_NURBS_BUFFER_ARC_STYLE_SLOT,
                        time_secs,
                        eval_stack,
                    )
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
                    let amplitude = compiled_param_value_opt(
                        project,
                        step,
                        BUFFER_NOISE_AMPLITUDE_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                    let frequency = compiled_param_value_opt(
                        project,
                        step,
                        BUFFER_NOISE_FREQUENCY_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(2.0)
                    .max(0.01);
                    let speed_hz = compiled_param_value_opt(
                        project,
                        step,
                        BUFFER_NOISE_SPEED_HZ_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.35)
                    .max(0.0);
                    let phase = compiled_param_value_opt(
                        project,
                        step,
                        BUFFER_NOISE_PHASE_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.0);
                    let seed = compiled_param_value_opt(
                        project,
                        step,
                        BUFFER_NOISE_SEED_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(1.0);
                    let twist = compiled_param_value_opt(
                        project,
                        step,
                        BUFFER_NOISE_TWIST_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.0)
                    .clamp(-8.0, 8.0);
                    let stretch = compiled_param_value_opt(
                        project,
                        step,
                        BUFFER_NOISE_STRETCH_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                    let loop_cycles = compiled_param_value_opt(
                        project,
                        step,
                        BUFFER_NOISE_LOOP_CYC_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(12.0)
                    .clamp(0.0, 256.0);
                    let loop_mode = compiled_param_value_opt(
                        project,
                        step,
                        BUFFER_NOISE_LOOP_MODE_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.0)
                        >= 0.5;
                    let (base_phase, warp_freq, warp_input) = if loop_mode {
                        let loop_phase = timeline_loop_phase(frame, time_secs);
                        (
                            loop_phase * loop_cycles.round(),
                            frequency.round().clamp(1.0, 64.0),
                            loop_phase,
                        )
                    } else {
                        (
                            time_secs * speed_hz * std::f32::consts::TAU,
                            frequency,
                            time_secs * speed_hz * std::f32::consts::TAU * 0.37,
                        )
                    };
                    let phase_warp = if loop_mode {
                        layered_loop_sine_noise(warp_input, warp_freq, phase, seed)
                    } else {
                        layered_sine_noise(warp_input, warp_freq, phase, seed)
                    };
                    let mut noise_phase = base_phase
                        + phase * std::f32::consts::TAU
                        + seed * 0.173
                        + phase_warp * 0.65;
                    if loop_mode {
                        noise_phase = noise_phase.rem_euclid(std::f32::consts::TAU);
                    }
                    mesh_state.noise_amount = amplitude;
                    mesh_state.noise_freq = frequency;
                    mesh_state.noise_phase = noise_phase;
                    mesh_state.noise_twist = twist;
                    mesh_state.noise_stretch = stretch;
                    mesh = Some(mesh_state);
                    scene_ready = false;
                }
                CompiledStepKind::SceneEntity => {
                    entity = Some(SceneEntityState {
                        pos_x: compiled_param_value_opt(
                            project,
                            step,
                            SCENE_ENTITY_POS_X_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.5),
                        pos_y: compiled_param_value_opt(
                            project,
                            step,
                            SCENE_ENTITY_POS_Y_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.5),
                        scale: compiled_param_value_opt(
                            project,
                            step,
                            SCENE_ENTITY_SCALE_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0)
                        .max(0.01),
                        ambient: compiled_param_value_opt(
                            project,
                            step,
                            SCENE_ENTITY_AMBIENT_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.2),
                        color_r: compiled_param_value_opt(
                            project,
                            step,
                            SCENE_ENTITY_COLOR_R_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.9),
                        color_g: compiled_param_value_opt(
                            project,
                            step,
                            SCENE_ENTITY_COLOR_G_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.9),
                        color_b: compiled_param_value_opt(
                            project,
                            step,
                            SCENE_ENTITY_COLOR_B_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.9),
                        alpha: compiled_param_value_opt(
                            project,
                            step,
                            SCENE_ENTITY_ALPHA_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0),
                    });
                    scene_ready = false;
                }
                CompiledStepKind::SceneBuild => {
                    scene_ready = mesh.is_some() && entity.is_some();
                }
                CompiledStepKind::Camera => {
                    camera_zoom = compiled_param_value_opt(
                        project,
                        step,
                        CAMERA_ZOOM_SLOT,
                        time_secs,
                        eval_stack,
                    )
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
                    let alpha_clip = compiled_param_value_opt(
                        project,
                        step,
                        SCENE_PASS_BG_MODE_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.0)
                        >= 0.5;
                    let zoom = camera_zoom.max(0.1);
                    let center_x = (entity_state.pos_x - 0.5) * zoom + 0.5;
                    let center_y = (entity_state.pos_y - 0.5) * zoom + 0.5;
                    let edge_softness = compiled_param_value_opt(
                        project,
                        step,
                        SCENE_PASS_EDGE_SOFTNESS_SLOT,
                        time_secs,
                        eval_stack,
                    )
                    .unwrap_or(0.01)
                    .max(0.0);
                    match mesh_state.profile {
                        SceneMeshProfile::Sphere => out_ops.push(TexRuntimeOp::Sphere {
                            center_x,
                            center_y,
                            radius: (mesh_state.radius * entity_state.scale * zoom).max(0.01),
                            edge_softness: edge_softness * zoom,
                            noise_amount: mesh_state.noise_amount,
                            noise_freq: mesh_state.noise_freq,
                            noise_phase: mesh_state.noise_phase,
                            noise_twist: mesh_state.noise_twist,
                            noise_stretch: mesh_state.noise_stretch,
                            light_x: compiled_param_value_opt(
                                project,
                                step,
                                SCENE_PASS_LIGHT_X_SLOT,
                                time_secs,
                                eval_stack,
                            )
                            .unwrap_or(0.4),
                            light_y: compiled_param_value_opt(
                                project,
                                step,
                                SCENE_PASS_LIGHT_Y_SLOT,
                                time_secs,
                                eval_stack,
                            )
                            .unwrap_or(-0.5),
                            light_z: compiled_param_value_opt(
                                project,
                                step,
                                SCENE_PASS_LIGHT_Z_SLOT,
                                time_secs,
                                eval_stack,
                            )
                            .unwrap_or(1.0),
                            ambient: entity_state.ambient,
                            color_r: entity_state.color_r,
                            color_g: entity_state.color_g,
                            color_b: entity_state.color_b,
                            alpha: entity_state.alpha,
                            alpha_clip,
                        }),
                        SceneMeshProfile::CircleNurbs => out_ops.push(TexRuntimeOp::Circle {
                            center_x,
                            center_y,
                            radius: (mesh_state.radius * entity_state.scale * zoom).max(0.01),
                            feather: edge_softness
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
                    out_ops.push(TexRuntimeOp::Transform {
                        brightness: compiled_param_value_opt(
                            project,
                            step,
                            TRANSFORM_BRIGHTNESS_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0),
                        gain_r: compiled_param_value_opt(
                            project,
                            step,
                            TRANSFORM_GAIN_R_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0),
                        gain_g: compiled_param_value_opt(
                            project,
                            step,
                            TRANSFORM_GAIN_G_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0),
                        gain_b: compiled_param_value_opt(
                            project,
                            step,
                            TRANSFORM_GAIN_B_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0),
                        alpha_mul: compiled_param_value_opt(
                            project,
                            step,
                            TRANSFORM_ALPHA_MUL_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0),
                    });
                }
                CompiledStepKind::Level => {
                    out_ops.push(TexRuntimeOp::Level {
                        in_low: compiled_param_value_opt(
                            project,
                            step,
                            LEVEL_IN_LOW_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0),
                        in_high: compiled_param_value_opt(
                            project,
                            step,
                            LEVEL_IN_HIGH_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0)
                        .clamp(0.0, 1.0),
                        gamma: compiled_param_value_opt(
                            project,
                            step,
                            LEVEL_GAMMA_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0)
                        .clamp(0.1, 8.0),
                        out_low: compiled_param_value_opt(
                            project,
                            step,
                            LEVEL_OUT_LOW_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0),
                        out_high: compiled_param_value_opt(
                            project,
                            step,
                            LEVEL_OUT_HIGH_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0)
                        .clamp(0.0, 1.0),
                    });
                }
                CompiledStepKind::Feedback => {
                    let history =
                        compiled_texture_source_for_param(project, step, FEEDBACK_HISTORY_SLOT)
                            .or_else(|| {
                                compiled_texture_source_for_param(
                                    project,
                                    step,
                                    FEEDBACK_LEGACY_HISTORY_SLOT,
                                )
                            })
                            .map_or(
                                TexRuntimeFeedbackHistoryBinding::Internal {
                                    feedback_node_id: step.node_id,
                                },
                                |texture_node_id| TexRuntimeFeedbackHistoryBinding::External {
                                    texture_node_id,
                                },
                            );
                    out_ops.push(TexRuntimeOp::Feedback {
                        mix: compiled_param_value_opt(
                            project,
                            step,
                            FEEDBACK_MIX_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.95)
                        .clamp(0.0, 1.0),
                        history,
                    });
                }
                CompiledStepKind::ReactionDiffusion => {
                    out_ops.push(TexRuntimeOp::ReactionDiffusion {
                        diffusion_a: compiled_param_value_opt(
                            project,
                            step,
                            REACTION_DIFFUSION_DIFF_A_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0)
                        .clamp(0.0, 2.0),
                        diffusion_b: compiled_param_value_opt(
                            project,
                            step,
                            REACTION_DIFFUSION_DIFF_B_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.5)
                        .clamp(0.0, 2.0),
                        feed: compiled_param_value_opt(
                            project,
                            step,
                            REACTION_DIFFUSION_FEED_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.055)
                        .clamp(0.0, 0.12),
                        kill: compiled_param_value_opt(
                            project,
                            step,
                            REACTION_DIFFUSION_KILL_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.062)
                        .clamp(0.0, 0.12),
                        dt: compiled_param_value_opt(
                            project,
                            step,
                            REACTION_DIFFUSION_DT_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(1.0)
                        .clamp(0.0, 2.0),
                        seed_mix: compiled_param_value_opt(
                            project,
                            step,
                            REACTION_DIFFUSION_SEED_MIX_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.04)
                        .clamp(0.0, 1.0),
                        history: TexRuntimeFeedbackHistoryBinding::Internal {
                            feedback_node_id: step.node_id,
                        },
                    });
                }
                CompiledStepKind::StoreTexture => {
                    out_ops.push(TexRuntimeOp::StoreTexture {
                        texture_node_id: step.node_id,
                    });
                }
                CompiledStepKind::Blend {
                    base_source_id,
                    layer_source_id,
                } => {
                    out_ops.push(TexRuntimeOp::Blend {
                        mode: compiled_param_value_opt(
                            project,
                            step,
                            BLEND_MODE_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.0)
                        .round()
                        .clamp(0.0, 8.0),
                        opacity: compiled_param_value_opt(
                            project,
                            step,
                            BLEND_OPACITY_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0),
                        bg_r: compiled_param_value_opt(
                            project,
                            step,
                            BLEND_BG_R_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0),
                        bg_g: compiled_param_value_opt(
                            project,
                            step,
                            BLEND_BG_G_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0),
                        bg_b: compiled_param_value_opt(
                            project,
                            step,
                            BLEND_BG_B_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0),
                        bg_a: compiled_param_value_opt(
                            project,
                            step,
                            BLEND_BG_A_SLOT,
                            time_secs,
                            eval_stack,
                        )
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0),
                        base_texture_node_id: base_source_id,
                        layer_texture_node_id: layer_source_id,
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
            let raw_w = compiled_param_value_opt(
                project,
                step,
                SCENE_PASS_RES_WIDTH_SLOT,
                time_secs,
                eval_stack,
            )
            .unwrap_or(0.0);
            let raw_h = compiled_param_value_opt(
                project,
                step,
                SCENE_PASS_RES_HEIGHT_SLOT,
                time_secs,
                eval_stack,
            )
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

fn compiled_step(
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

fn compiled_param_value_opt(
    project: &GuiProject,
    step: &CompiledStep,
    param_slot: usize,
    time_secs: f32,
    eval_stack: &mut Vec<u32>,
) -> Option<f32> {
    let index = step.param_slots.get(param_slot).copied().flatten()?.0;
    project.node_param_value_by_index(step.node_id, index, time_secs, eval_stack)
}

fn compiled_texture_source_for_param(
    project: &GuiProject,
    step: &CompiledStep,
    param_slot: usize,
) -> Option<u32> {
    let index = step.param_slots.get(param_slot).copied().flatten()?.0;
    project.texture_source_for_param(step.node_id, index)
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
            if !compile_node(project, source_id, visiting, visited, out_steps) {
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
            if !compile_node(project, source_id, visiting, visited, out_steps) {
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
            if !compile_node(project, source_id, visiting, visited, out_steps) {
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
            if !compile_node(project, source_id, visiting, visited, out_steps) {
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
            if !compile_node(project, source_id, visiting, visited, out_steps) {
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
                    if !compile_node(project, layer_id, visiting, visited, out_steps) {
                        return false;
                    }
                    out_steps.push(compiled_step(
                        project,
                        layer_id,
                        CompiledStepKind::StoreTexture,
                        &[],
                    ));
                }
                if !compile_node(project, base_source_id, visiting, visited, out_steps) {
                    return false;
                }
                out_steps.push(compiled_step(
                    project,
                    base_source_id,
                    CompiledStepKind::StoreTexture,
                    &[],
                ));
            } else {
                if !compile_node(project, base_source_id, visiting, visited, out_steps) {
                    return false;
                }
                out_steps.push(compiled_step(
                    project,
                    base_source_id,
                    CompiledStepKind::StoreTexture,
                    &[],
                ));
                if let Some(layer_id) = layer_source_id {
                    if !compile_node(project, layer_id, visiting, visited, out_steps) {
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
            if !compile_node(project, source_id, visiting, visited, out_steps) {
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
            if !compile_node(project, source_id, visiting, visited, out_steps) {
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
            if !compile_node(project, source_id, visiting, visited, out_steps) {
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
            if !compile_node(project, source_id, visiting, visited, out_steps) {
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
    let _ = visiting.pop();
    if ok {
        visited.push(node_id);
    }
    ok
}

fn node_depends_on(project: &GuiProject, start_node_id: u32, target_node_id: u32) -> bool {
    if start_node_id == target_node_id {
        return true;
    }
    let mut stack = vec![start_node_id];
    let mut visited = Vec::new();
    while let Some(node_id) = stack.pop() {
        if node_id == target_node_id {
            return true;
        }
        if visited.contains(&node_id) {
            continue;
        }
        visited.push(node_id);
        let Some(node) = project.node(node_id) else {
            continue;
        };
        for input in node.inputs() {
            stack.push(*input);
        }
    }
    false
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

/// Timeline-safe pseudo-noise that stays periodic across a full clip loop.
fn layered_loop_sine_noise(loop_phase: f32, frequency: f32, phase: f32, seed: f32) -> f32 {
    let freq_cycles = frequency.round().clamp(1.0, 64.0);
    let s0 = seed * 0.13 + phase;
    let s1 = seed * 0.73 + phase * 1.9;
    let s2 = seed * 1.37 + phase * 0.47;
    let n0 = (loop_phase * freq_cycles + s0).sin();
    let n1 = (loop_phase * freq_cycles * 2.0 + s1).sin();
    let n2 = (loop_phase * freq_cycles * 4.0 + s2).sin();
    (n0 * 0.62 + n1 * 0.28 + n2 * 0.10).clamp(-1.0, 1.0)
}

/// Convert frame-clock context to a deterministic `[0, TAU]` loop phase.
///
/// The phase is end-inclusive so the first and last timeline frames resolve to
/// identical loop positions for seam-free clip wraps.
fn timeline_loop_phase(frame: Option<TexRuntimeFrameContext>, time_secs: f32) -> f32 {
    let progress = match frame {
        Some(ctx) => normalized_loop_progress(ctx.frame_index, ctx.frame_total),
        None => {
            let frame_total = 30 * DEFAULT_LOOP_FPS;
            let loop_secs = frame_total as f32 / DEFAULT_LOOP_FPS as f32;
            let wrapped_secs = time_secs.max(0.0).rem_euclid(loop_secs);
            let frame_index = (wrapped_secs * DEFAULT_LOOP_FPS as f32).floor() as u32;
            normalized_loop_progress(frame_index, frame_total)
        }
    };
    progress * std::f32::consts::TAU
}

/// Return normalized loop progress in `[0, 1]` for a frame counter.
fn normalized_loop_progress(frame_index: u32, frame_total: u32) -> f32 {
    if frame_total <= 1 {
        return 0.0;
    }
    let max_index = frame_total - 1;
    let wrapped = frame_index % frame_total;
    wrapped as f32 / max_index as f32
}

#[cfg(test)]
mod tests {
    use super::{
        compiled_param_value_opt, compiled_step, compiled_texture_source_for_param,
        CompiledStepKind, GuiCompiledRuntime, TexRuntimeFeedbackHistoryBinding,
        TexRuntimeFrameContext, TexRuntimeOp, FEEDBACK_HISTORY_SLOT, FEEDBACK_PARAM_KEYS,
        SOLID_PARAM_KEYS,
    };
    use crate::gui::project::{GuiProject, ProjectNodeKind};

    #[test]
    fn compiled_param_slots_match_keyed_param_values() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        assert!(project.set_param_value(solid, 0, 0.15));
        assert!(project.set_param_value(solid, 1, 0.35));
        assert!(project.set_param_value(solid, 2, 0.55));
        assert!(project.set_param_value(solid, 3, 0.75));
        let step = compiled_step(&project, solid, CompiledStepKind::Solid, &SOLID_PARAM_KEYS);
        let mut eval_stack = Vec::new();
        for (slot_index, key) in SOLID_PARAM_KEYS.iter().enumerate() {
            let keyed = project.node_param_value(solid, key, 0.0, &mut eval_stack);
            eval_stack.clear();
            let indexed =
                compiled_param_value_opt(&project, &step, slot_index, 0.0, &mut eval_stack);
            assert_eq!(
                indexed, keyed,
                "compiled slot {slot_index} should match keyed read for {key}"
            );
            eval_stack.clear();
        }
    }

    #[test]
    fn compiled_feedback_history_slot_matches_texture_binding() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 40, 420, 480);
        assert!(project.connect_texture_link_to_param(solid, feedback, 0));
        let step = compiled_step(
            &project,
            feedback,
            CompiledStepKind::Feedback,
            &FEEDBACK_PARAM_KEYS,
        );
        assert_eq!(
            compiled_texture_source_for_param(&project, &step, FEEDBACK_HISTORY_SLOT),
            Some(solid)
        );
    }

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
            TexRuntimeOp::Transform {
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
    fn level_defaults_are_identity() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let level = project.add_node(ProjectNodeKind::TexLevel, 180, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
        assert!(project.connect_image_link(solid, level));
        assert!(project.connect_image_link(level, out));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        assert_eq!(ops.len(), 2);
        assert!(matches!(
            ops[1],
            TexRuntimeOp::Level {
                in_low,
                in_high,
                gamma,
                out_low,
                out_high
            } if in_low == 0.0
                && in_high == 1.0
                && gamma == 1.0
                && out_low == 0.0
                && out_high == 1.0
        ));
    }

    #[test]
    fn reaction_diffusion_emits_temporal_op_with_defaults() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let reaction = project.add_node(ProjectNodeKind::TexReactionDiffusion, 180, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
        assert!(project.connect_image_link(solid, reaction));
        assert!(project.connect_image_link(reaction, out));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        assert_eq!(ops.len(), 2);
        assert!(matches!(
            ops[1],
            TexRuntimeOp::ReactionDiffusion {
                diffusion_a,
                diffusion_b,
                feed,
                kill,
                dt,
                seed_mix,
                history: TexRuntimeFeedbackHistoryBinding::Internal { feedback_node_id },
            } if diffusion_a == 1.0
                && diffusion_b == 0.5
                && (feed - 0.055).abs() < 1e-6
                && (kill - 0.062).abs() < 1e-6
                && dt == 1.0
                && (seed_mix - 0.04).abs() < 1e-6
                && feedback_node_id == reaction
        ));
    }

    #[test]
    fn blend_pipeline_compiles_to_store_and_blend_ops() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let circle = project.add_node(ProjectNodeKind::TexCircle, 120, 40, 420, 480);
        let blend = project.add_node(ProjectNodeKind::TexBlend, 280, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 440, 40, 420, 480);
        assert!(project.connect_image_link(solid, blend));
        assert!(project.connect_texture_link_to_param(circle, blend, 0));
        assert!(project.connect_image_link(blend, out));
        assert!(project.set_param_dropdown_index(blend, 1, 1));
        assert!(project.set_param_value(blend, 2, 0.75));
        assert!(project.set_param_value(blend, 3, 0.2));
        assert!(project.set_param_value(blend, 4, 0.3));
        assert!(project.set_param_value(blend, 5, 0.4));
        assert!(project.set_param_value(blend, 6, 0.5));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        assert!(matches!(ops[0], TexRuntimeOp::Solid { .. }));
        assert!(matches!(
            ops[1],
            TexRuntimeOp::StoreTexture { texture_node_id } if texture_node_id == solid
        ));
        assert!(matches!(ops[2], TexRuntimeOp::Circle { .. }));
        assert!(matches!(
            ops[3],
            TexRuntimeOp::StoreTexture { texture_node_id } if texture_node_id == circle
        ));
        assert!(matches!(
            ops[4],
            TexRuntimeOp::Blend {
                mode,
                opacity,
                bg_r,
                bg_g,
                bg_b,
                bg_a,
                base_texture_node_id,
                layer_texture_node_id: Some(layer_id),
            } if mode == 1.0
                && (opacity - 0.75).abs() < 1e-6
                && (bg_r - 0.2).abs() < 1e-6
                && (bg_g - 0.3).abs() < 1e-6
                && (bg_b - 0.4).abs() < 1e-6
                && (bg_a - 0.5).abs() < 1e-6
                && base_texture_node_id == solid
                && layer_id == circle
        ));
    }

    #[test]
    fn blend_pipeline_orders_branch_compilation_for_dependent_inputs() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
        let blend = project.add_node(ProjectNodeKind::TexBlend, 340, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 500, 40, 420, 480);
        assert!(project.connect_image_link(solid, xform));
        assert!(project.connect_image_link(xform, blend));
        assert!(project.connect_texture_link_to_param(solid, blend, 0));
        assert!(project.connect_image_link(blend, out));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        assert!(matches!(ops[0], TexRuntimeOp::Solid { .. }));
        assert!(matches!(
            ops[1],
            TexRuntimeOp::StoreTexture { texture_node_id } if texture_node_id == solid
        ));
        assert!(matches!(ops[2], TexRuntimeOp::Transform { .. }));
        assert!(matches!(
            ops[3],
            TexRuntimeOp::StoreTexture { texture_node_id } if texture_node_id == xform
        ));
        assert!(matches!(
            ops[4],
            TexRuntimeOp::Blend {
                base_texture_node_id,
                layer_texture_node_id: Some(layer_id),
                ..
            } if base_texture_node_id == xform && layer_id == solid
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
        assert!(matches!(ops[0], TexRuntimeOp::Solid { .. }));
        assert!(matches!(
            ops[1],
            TexRuntimeOp::Feedback {
                history: TexRuntimeFeedbackHistoryBinding::Internal { feedback_node_id },
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
        assert!(matches!(ops[0], TexRuntimeOp::Solid { .. }));
        assert!(matches!(
            ops[1],
            TexRuntimeOp::Feedback {
                history: TexRuntimeFeedbackHistoryBinding::External { texture_node_id },
                ..
            } if texture_node_id == circle
        ));
    }

    #[test]
    fn feedback_accumulation_binding_allows_downstream_transform_source() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 180, 40, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 340, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 500, 40, 420, 480);
        assert!(project.connect_image_link(solid, feedback));
        assert!(project.connect_image_link(feedback, xform));
        assert!(project.connect_image_link(xform, out));
        assert!(project.connect_texture_link_to_param(xform, feedback, 0));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops = Vec::new();
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        assert_eq!(ops.len(), 3);
        assert!(matches!(ops[0], TexRuntimeOp::Solid { .. }));
        assert!(matches!(
            ops[1],
            TexRuntimeOp::Feedback {
                history: TexRuntimeFeedbackHistoryBinding::External { texture_node_id },
                ..
            } if texture_node_id == xform
        ));
        assert!(matches!(ops[2], TexRuntimeOp::Transform { .. }));
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
        assert!(matches!(ops[0], TexRuntimeOp::Sphere { .. }));
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
            TexRuntimeOp::Sphere { alpha_clip, .. } => assert!(!alpha_clip),
            _ => panic!("expected sphere op"),
        }

        assert!(project.set_param_dropdown_index(pass, 2, 1));
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        match ops[0] {
            TexRuntimeOp::Sphere { alpha_clip, .. } => assert!(alpha_clip),
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
            TexRuntimeOp::Circle { alpha_clip, .. } => assert!(!alpha_clip),
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
            TexRuntimeOp::Sphere { radius, .. } => radius,
            _ => panic!("expected sphere op"),
        };

        assert!(project.set_param_value(camera, 0, 2.0));
        runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
        let radius_zoomed = match ops[0] {
            TexRuntimeOp::Sphere { radius, .. } => radius,
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
        assert!(matches!(ops[0], TexRuntimeOp::Circle { .. }));
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
            TexRuntimeOp::Circle {
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
            TexRuntimeOp::Sphere {
                radius,
                noise_phase,
                noise_twist,
                noise_stretch,
                ..
            } => (radius, noise_phase, noise_twist, noise_stretch),
            _ => panic!("expected sphere op"),
        };
        let (r1, phase1, twist1, stretch1) = match ops_t1[0] {
            TexRuntimeOp::Sphere {
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

    #[test]
    fn buffer_noise_loop_mode_matches_first_and_last_timeline_frame() {
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
        assert!(project.set_param_value(noise, 1, 3.4));
        assert!(project.set_param_value(noise, 3, 0.2));
        assert!(project.set_param_value(noise, 4, 11.0));
        assert!(project.set_param_value(noise, 7, 9.0));
        assert!(project.set_param_dropdown_index(noise, 8, 1));

        let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
        let mut eval_stack = Vec::new();
        let mut ops_first = Vec::new();
        runtime.evaluate_ops_with_frame(
            &project,
            0.0,
            Some(TexRuntimeFrameContext {
                frame_index: 0,
                frame_total: 1_800,
            }),
            &mut eval_stack,
            &mut ops_first,
        );
        let mut ops_last = Vec::new();
        runtime.evaluate_ops_with_frame(
            &project,
            1_799.0 / 60.0,
            Some(TexRuntimeFrameContext {
                frame_index: 1_799,
                frame_total: 1_800,
            }),
            &mut eval_stack,
            &mut ops_last,
        );
        let mut ops_wrapped = Vec::new();
        runtime.evaluate_ops_with_frame(
            &project,
            1_800.0 / 60.0,
            Some(TexRuntimeFrameContext {
                frame_index: 1_800,
                frame_total: 1_800,
            }),
            &mut eval_stack,
            &mut ops_wrapped,
        );
        let phase_first = match ops_first[0] {
            TexRuntimeOp::Sphere { noise_phase, .. } => noise_phase,
            _ => panic!("expected sphere op"),
        };
        let phase_last = match ops_last[0] {
            TexRuntimeOp::Sphere { noise_phase, .. } => noise_phase,
            _ => panic!("expected sphere op"),
        };
        let phase_wrapped = match ops_wrapped[0] {
            TexRuntimeOp::Sphere { noise_phase, .. } => noise_phase,
            _ => panic!("expected sphere op"),
        };
        assert!(
            (phase_first - phase_last).abs() < 1e-4,
            "loop mode should match first/last frame phase: first={phase_first}, last={phase_last}"
        );
        assert!(
            (phase_first - phase_wrapped).abs() < 1e-4,
            "loop mode should wrap back to frame 0 phase: first={phase_first}, wrapped={phase_wrapped}"
        );
    }
}

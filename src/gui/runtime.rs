//! Compiled GPU runtime contract for GUI tex preview graphs.
//!
//! This module normalizes GUI node graphs into a deterministic, executable
//! step list that can be evaluated directly into GPU preview operations.

mod compile_stage;
mod emit_stage;
mod eval_stage;
mod feedback_stage;
#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::HashSet;
use std::time::Instant;

use super::project::{
    param_schema, GuiProject, ProjectNodeKind, SignalEvalPath, SignalEvalStack,
    BLEND_LAYER_PARAM_KEY, DOMAIN_WARP_TEXTURE_PARAM_KEY,
};
use crate::telemetry;
use compile_stage::compile_node;
use eval_stage::{
    compiled_param_value_opt, layered_loop_sine_noise, layered_sine_noise, timeline_loop_phase,
};
use feedback_stage::{
    collect_external_feedback_history_sources, compiled_feedback_history_source,
    post_process_uses_history, push_store_texture_op,
};

const DEFAULT_LOOP_FPS: u32 = 60;

/// Compile-time resolved parameter slot index for one node parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ParamSlotIndex(usize);

/// Category discriminator for generalized post-process runtime operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PostProcessCategory {
    ColorTone,
    EdgeStructure,
    BlurDiffusion,
    Distortion,
    Temporal,
    NoiseTexture,
    Lighting,
    ScreenSpace,
    Experimental,
}

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
    /// `tex.source_noise` procedural noise source operation.
    SourceNoise {
        seed: f32,
        scale: f32,
        octaves: f32,
        amplitude: f32,
        mode: f32,
    },
    /// `render.scene_pass` box shading operation.
    Box {
        center_x: f32,
        center_y: f32,
        size_x: f32,
        size_y: f32,
        corner_radius: f32,
        edge_softness: f32,
        noise_amount: f32,
        noise_freq: f32,
        noise_phase: f32,
        noise_twist: f32,
        noise_stretch: f32,
        color_r: f32,
        color_g: f32,
        color_b: f32,
        alpha: f32,
        alpha_clip: bool,
    },
    /// `render.scene_pass` grid shading operation.
    Grid {
        center_x: f32,
        center_y: f32,
        size_x: f32,
        size_y: f32,
        cells_x: f32,
        cells_y: f32,
        line_width: f32,
        edge_softness: f32,
        noise_amount: f32,
        noise_freq: f32,
        noise_phase: f32,
        noise_twist: f32,
        noise_stretch: f32,
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
    Transform2D {
        offset_x: f32,
        offset_y: f32,
        scale_x: f32,
        scale_y: f32,
        rotate_deg: f32,
        pivot_x: f32,
        pivot_y: f32,
    },
    /// `tex.color_adjust` operation.
    ColorAdjust {
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
    /// `tex.mask` operation.
    Mask {
        threshold: f32,
        softness: f32,
        invert: f32,
    },
    /// `tex.morphology` operation.
    Morphology { mode: f32, radius: f32, amount: f32 },
    /// `tex.tone_map` operation.
    ToneMap {
        contrast: f32,
        low_pct: f32,
        high_pct: f32,
    },
    /// `tex.feedback` delayed history output operation with optional frame-gap decimation.
    Feedback {
        mix: f32,
        frame_gap: u32,
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
    /// `tex.domain_warp` warp-field driven UV distortion operation.
    DomainWarp {
        strength: f32,
        frequency: f32,
        rotation: f32,
        octaves: f32,
        base_texture_node_id: u32,
        warp_texture_node_id: Option<u32>,
    },
    /// `tex.directional_smear` operation.
    DirectionalSmear {
        angle: f32,
        length: f32,
        jitter: f32,
        amount: f32,
    },
    /// `tex.warp_transform` operation.
    WarpTransform {
        strength: f32,
        frequency: f32,
        phase: f32,
    },
    /// Category post-process operation with shared controls.
    PostProcess {
        category: PostProcessCategory,
        effect: f32,
        amount: f32,
        scale: f32,
        threshold: f32,
        speed: f32,
        time: f32,
        history: Option<TexRuntimeFeedbackHistoryBinding>,
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
    SourceNoise,
    SphereBuffer,
    BoxBuffer,
    GridBuffer,
    CircleNurbsBuffer,
    BufferNoise,
    SceneEntity,
    SceneBuild,
    Camera,
    ScenePass,
    Transform2D,
    ColorAdjust,
    Level,
    Mask,
    Morphology,
    ToneMap,
    Feedback,
    ReactionDiffusion,
    DomainWarp {
        base_source_id: u32,
        warp_source_id: Option<u32>,
    },
    DirectionalSmear,
    WarpTransform,
    PostProcess {
        category: PostProcessCategory,
    },
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
    Box,
    Grid,
    CircleNurbs,
}

#[derive(Clone, Copy, Debug)]
struct SceneMeshState {
    profile: SceneMeshProfile,
    radius: f32,
    size_x: f32,
    size_y: f32,
    corner_radius: f32,
    arc_start_deg: f32,
    arc_end_deg: f32,
    line_width: f32,
    cells_x: f32,
    cells_y: f32,
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
    external_feedback_history_sources: HashSet<u32>,
}

thread_local! {
    /// Per-thread signal sample memo reused across one runtime evaluation pass.
    static RUNTIME_SIGNAL_SAMPLE_MEMO: RefCell<crate::gui::project::SignalSampleMemo> =
        RefCell::new(crate::gui::project::SignalSampleMemo::default());
}

#[derive(Debug, Default)]
struct CompileTraversalState {
    visiting: HashSet<u32>,
    visited: HashSet<u32>,
}

impl GuiCompiledRuntime {
    /// Compile one GUI project to an executable tex runtime sequence.
    ///
    /// Returns `None` when no valid `io.window_out` chain can be compiled.
    pub(crate) fn compile(project: &GuiProject) -> Option<Self> {
        let compile_begin = Instant::now();
        let compiled = (|| {
            let output_source_id = project.window_out_input_node_id()?;
            let mut steps = Vec::new();
            let mut traversal = CompileTraversalState::default();
            if !compile_node(project, output_source_id, &mut traversal, &mut steps) {
                return None;
            }
            if steps.is_empty() {
                return None;
            }
            let external_feedback_history_sources =
                collect_external_feedback_history_sources(project, &steps);
            Some(Self {
                steps,
                external_feedback_history_sources,
            })
        })();
        telemetry::record_timing("gui.runtime.compile", compile_begin.elapsed());
        telemetry::record_counter_u64(
            "gui.runtime.compile.step_count",
            compiled
                .as_ref()
                .map(|runtime| runtime.steps.len() as u64)
                .unwrap_or(0),
        );
        compiled
    }

    /// Evaluate compiled steps into GPU runtime operations for one frame.
    #[cfg(test)]
    pub(crate) fn evaluate_ops(
        &self,
        project: &GuiProject,
        time_secs: f32,
        eval_stack: &mut SignalEvalStack,
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
        eval_stack: &mut SignalEvalStack,
        out_ops: &mut Vec<TexRuntimeOp>,
    ) {
        out_ops.clear();
        begin_runtime_eval_pass(eval_stack);
        let mut step_state = RuntimeEvalState::new();
        let mut ctx = RuntimeEvalContext::new(project, time_secs, frame, eval_stack, out_ops);
        for step in &self.steps {
            let out_len_before = ctx.out_ops.len();
            Self::emit_step(step, &mut step_state, &mut ctx);
            if step.kind != CompiledStepKind::StoreTexture
                && self
                    .external_feedback_history_sources
                    .contains(&step.node_id)
                && ctx.out_ops.len() > out_len_before
            {
                push_store_texture_op(ctx.out_ops, step.node_id);
            }
        }
    }

    fn emit_step(
        step: &CompiledStep,
        step_state: &mut RuntimeEvalState,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        match step.kind {
            CompiledStepKind::Solid => Self::emit_solid(step, ctx),
            CompiledStepKind::Circle => Self::emit_circle(step, ctx),
            CompiledStepKind::SourceNoise => Self::emit_source_noise(step, ctx),
            CompiledStepKind::SphereBuffer => Self::update_sphere_mesh(step, step_state, ctx),
            CompiledStepKind::BoxBuffer => Self::update_box_mesh(step, step_state, ctx),
            CompiledStepKind::GridBuffer => Self::update_grid_mesh(step, step_state, ctx),
            CompiledStepKind::CircleNurbsBuffer => {
                Self::update_circle_nurbs_mesh(step, step_state, ctx)
            }
            CompiledStepKind::BufferNoise => Self::update_noise_mesh(step, step_state, ctx),
            CompiledStepKind::SceneEntity => Self::update_scene_entity(step, step_state, ctx),
            CompiledStepKind::SceneBuild => Self::mark_scene_ready(step_state),
            CompiledStepKind::Camera => Self::update_camera(step, step_state, ctx),
            CompiledStepKind::ScenePass => Self::emit_scene_pass(step, step_state, ctx),
            CompiledStepKind::Transform2D => Self::emit_transform_2d(step, ctx),
            CompiledStepKind::ColorAdjust => Self::emit_color_adjust(step, ctx),
            CompiledStepKind::Level => Self::emit_level(step, ctx),
            CompiledStepKind::Mask => Self::emit_mask(step, ctx),
            CompiledStepKind::Morphology => Self::emit_morphology(step, ctx),
            CompiledStepKind::ToneMap => Self::emit_tone_map(step, ctx),
            CompiledStepKind::Feedback => Self::emit_feedback(step, ctx),
            CompiledStepKind::ReactionDiffusion => Self::emit_reaction_diffusion(step, ctx),
            CompiledStepKind::DomainWarp {
                base_source_id,
                warp_source_id,
            } => Self::emit_domain_warp(step, base_source_id, warp_source_id, ctx),
            CompiledStepKind::DirectionalSmear => Self::emit_directional_smear(step, ctx),
            CompiledStepKind::WarpTransform => Self::emit_warp_transform(step, ctx),
            CompiledStepKind::PostProcess { category } => {
                Self::emit_post_process(step, category, ctx)
            }
            CompiledStepKind::StoreTexture => push_store_texture_op(ctx.out_ops, step.node_id),
            CompiledStepKind::Blend {
                base_source_id,
                layer_source_id,
            } => Self::emit_blend(step, base_source_id, layer_source_id, ctx),
        }
    }

    fn emit_solid(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let color_r = ctx
            .param(step, param_schema::solid::COLOR_R_INDEX)
            .unwrap_or(0.9);
        let color_g = ctx
            .param(step, param_schema::solid::COLOR_G_INDEX)
            .unwrap_or(0.9);
        let color_b = ctx
            .param(step, param_schema::solid::COLOR_B_INDEX)
            .unwrap_or(0.9);
        let alpha = ctx
            .param(step, param_schema::solid::ALPHA_INDEX)
            .unwrap_or(1.0);
        ctx.out_ops.push(TexRuntimeOp::Solid {
            color_r,
            color_g,
            color_b,
            alpha,
        });
    }

    fn emit_circle(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let center_x = ctx
            .param(step, param_schema::circle::CENTER_X_INDEX)
            .unwrap_or(0.5);
        let center_y = ctx
            .param(step, param_schema::circle::CENTER_Y_INDEX)
            .unwrap_or(0.5);
        let radius = ctx
            .param(step, param_schema::circle::RADIUS_INDEX)
            .unwrap_or(0.24);
        let feather = ctx
            .param(step, param_schema::circle::FEATHER_INDEX)
            .unwrap_or(0.06);
        let color_r = ctx
            .param(step, param_schema::circle::COLOR_R_INDEX)
            .unwrap_or(0.9);
        let color_g = ctx
            .param(step, param_schema::circle::COLOR_G_INDEX)
            .unwrap_or(0.9);
        let color_b = ctx
            .param(step, param_schema::circle::COLOR_B_INDEX)
            .unwrap_or(0.9);
        let alpha = ctx
            .param(step, param_schema::circle::ALPHA_INDEX)
            .unwrap_or(1.0);
        ctx.out_ops.push(TexRuntimeOp::Circle {
            center_x,
            center_y,
            radius,
            feather,
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
            color_r,
            color_g,
            color_b,
            alpha,
            alpha_clip: false,
        });
    }

    fn emit_source_noise(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let seed = ctx
            .param(step, param_schema::source_noise::SEED_INDEX)
            .unwrap_or(1.0)
            .round()
            .clamp(0.0, 65535.0);
        let scale = ctx
            .param(step, param_schema::source_noise::SCALE_INDEX)
            .unwrap_or(4.0)
            .clamp(0.05, 32.0);
        let octaves = ctx
            .param(step, param_schema::source_noise::OCTAVES_INDEX)
            .unwrap_or(4.0)
            .round()
            .clamp(1.0, 8.0);
        let amplitude = ctx
            .param(step, param_schema::source_noise::AMPLITUDE_INDEX)
            .unwrap_or(1.0)
            .clamp(0.0, 2.0);
        let mode = ctx
            .param(step, param_schema::source_noise::MODE_INDEX)
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, 3.0);
        ctx.out_ops.push(TexRuntimeOp::SourceNoise {
            seed,
            scale,
            octaves,
            amplitude,
            mode,
        });
    }

    fn update_sphere_mesh(
        step: &CompiledStep,
        step_state: &mut RuntimeEvalState,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        let radius = ctx
            .param(step, param_schema::sphere_buffer::RADIUS_INDEX)
            .unwrap_or(0.28)
            .max(0.01);
        step_state.mesh = Some(SceneMeshState {
            profile: SceneMeshProfile::Sphere,
            radius,
            size_x: radius * 2.0,
            size_y: radius * 2.0,
            corner_radius: 0.0,
            arc_start_deg: 0.0,
            arc_end_deg: 360.0,
            line_width: 0.0,
            cells_x: 1.0,
            cells_y: 1.0,
            noise_amount: 0.0,
            noise_freq: 1.0,
            noise_phase: 0.0,
            noise_twist: 0.0,
            noise_stretch: 0.0,
            order: 3.0,
            segment_count: 0.0,
            arc_open: false,
        });
        step_state.scene_ready = false;
    }

    fn update_box_mesh(
        step: &CompiledStep,
        step_state: &mut RuntimeEvalState,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        let size_x = ctx
            .param(step, param_schema::box_buffer::SIZE_X_INDEX)
            .unwrap_or(0.52)
            .max(0.01);
        let size_y = ctx
            .param(step, param_schema::box_buffer::SIZE_Y_INDEX)
            .unwrap_or(0.52)
            .max(0.01);
        let corner_radius = ctx
            .param(step, param_schema::box_buffer::CORNER_INDEX)
            .unwrap_or(0.02)
            .clamp(0.0, size_x.min(size_y) * 0.5);
        step_state.mesh = Some(SceneMeshState {
            profile: SceneMeshProfile::Box,
            radius: size_x.max(size_y) * 0.5,
            size_x,
            size_y,
            corner_radius,
            arc_start_deg: 0.0,
            arc_end_deg: 360.0,
            line_width: 0.0,
            cells_x: 1.0,
            cells_y: 1.0,
            noise_amount: 0.0,
            noise_freq: 1.0,
            noise_phase: 0.0,
            noise_twist: 0.0,
            noise_stretch: 0.0,
            order: 0.0,
            segment_count: 0.0,
            arc_open: false,
        });
        step_state.scene_ready = false;
    }

    fn update_grid_mesh(
        step: &CompiledStep,
        step_state: &mut RuntimeEvalState,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        let size_x = ctx
            .param(step, param_schema::grid_buffer::SIZE_X_INDEX)
            .unwrap_or(0.84)
            .max(0.01);
        let size_y = ctx
            .param(step, param_schema::grid_buffer::SIZE_Y_INDEX)
            .unwrap_or(0.84)
            .max(0.01);
        let cells_x = ctx
            .param(step, param_schema::grid_buffer::CELLS_X_INDEX)
            .unwrap_or(8.0)
            .round()
            .clamp(1.0, 64.0);
        let cells_y = ctx
            .param(step, param_schema::grid_buffer::CELLS_Y_INDEX)
            .unwrap_or(8.0)
            .round()
            .clamp(1.0, 64.0);
        let line_width = ctx
            .param(step, param_schema::grid_buffer::LINE_WIDTH_INDEX)
            .unwrap_or(0.01)
            .clamp(0.0005, 0.2);
        step_state.mesh = Some(SceneMeshState {
            profile: SceneMeshProfile::Grid,
            radius: size_x.max(size_y) * 0.5,
            size_x,
            size_y,
            corner_radius: 0.0,
            arc_start_deg: 0.0,
            arc_end_deg: 360.0,
            line_width,
            cells_x,
            cells_y,
            noise_amount: 0.0,
            noise_freq: 1.0,
            noise_phase: 0.0,
            noise_twist: 0.0,
            noise_stretch: 0.0,
            order: 0.0,
            segment_count: 0.0,
            arc_open: false,
        });
        step_state.scene_ready = false;
    }

    fn update_circle_nurbs_mesh(
        step: &CompiledStep,
        step_state: &mut RuntimeEvalState,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        let radius = ctx
            .param(step, param_schema::circle_nurbs_buffer::RADIUS_INDEX)
            .unwrap_or(0.28)
            .max(0.01);
        let arc_start_deg = ctx
            .param(step, param_schema::circle_nurbs_buffer::ARC_START_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 360.0);
        let arc_end_deg = ctx
            .param(step, param_schema::circle_nurbs_buffer::ARC_END_INDEX)
            .unwrap_or(360.0)
            .clamp(0.0, 360.0);
        let line_width = ctx
            .param(step, param_schema::circle_nurbs_buffer::LINE_WIDTH_INDEX)
            .unwrap_or(0.01)
            .clamp(0.0005, 0.35);
        let order = ctx
            .param(step, param_schema::circle_nurbs_buffer::ORDER_INDEX)
            .unwrap_or(3.0)
            .clamp(2.0, 5.0);
        let segment_count = ctx
            .param(step, param_schema::circle_nurbs_buffer::DIVISIONS_INDEX)
            .unwrap_or(64.0)
            .clamp(3.0, 512.0);
        let arc_open = ctx
            .param(step, param_schema::circle_nurbs_buffer::ARC_STYLE_INDEX)
            .unwrap_or(0.0)
            >= 0.5;
        step_state.mesh = Some(SceneMeshState {
            profile: SceneMeshProfile::CircleNurbs,
            radius,
            size_x: radius * 2.0,
            size_y: radius * 2.0,
            corner_radius: 0.0,
            arc_start_deg,
            arc_end_deg,
            line_width,
            cells_x: 1.0,
            cells_y: 1.0,
            noise_amount: 0.0,
            noise_freq: 1.0,
            noise_phase: 0.0,
            noise_twist: 0.0,
            noise_stretch: 0.0,
            order,
            segment_count,
            arc_open,
        });
        step_state.scene_ready = false;
    }

    fn update_scene_entity(
        step: &CompiledStep,
        step_state: &mut RuntimeEvalState,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        step_state.entity = Some(SceneEntityState {
            pos_x: ctx
                .param(step, param_schema::scene_entity::POS_X_INDEX)
                .unwrap_or(0.5),
            pos_y: ctx
                .param(step, param_schema::scene_entity::POS_Y_INDEX)
                .unwrap_or(0.5),
            scale: ctx
                .param(step, param_schema::scene_entity::SCALE_INDEX)
                .unwrap_or(1.0)
                .max(0.01),
            ambient: ctx
                .param(step, param_schema::scene_entity::AMBIENT_INDEX)
                .unwrap_or(0.2),
            color_r: ctx
                .param(step, param_schema::scene_entity::COLOR_R_INDEX)
                .unwrap_or(0.9),
            color_g: ctx
                .param(step, param_schema::scene_entity::COLOR_G_INDEX)
                .unwrap_or(0.9),
            color_b: ctx
                .param(step, param_schema::scene_entity::COLOR_B_INDEX)
                .unwrap_or(0.9),
            alpha: ctx
                .param(step, param_schema::scene_entity::ALPHA_INDEX)
                .unwrap_or(1.0),
        });
        step_state.scene_ready = false;
    }

    fn mark_scene_ready(step_state: &mut RuntimeEvalState) {
        step_state.scene_ready = step_state.mesh.is_some() && step_state.entity.is_some();
    }

    fn update_camera(
        step: &CompiledStep,
        step_state: &mut RuntimeEvalState,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        step_state.camera_zoom = ctx
            .param(step, param_schema::render_camera::ZOOM_INDEX)
            .unwrap_or(1.0)
            .clamp(0.1, 8.0);
    }

    fn emit_transform_2d(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let offset_x = ctx
            .param(step, param_schema::transform_2d::OFFSET_X_INDEX)
            .unwrap_or(0.0);
        let offset_y = ctx
            .param(step, param_schema::transform_2d::OFFSET_Y_INDEX)
            .unwrap_or(0.0);
        let scale_x = ctx
            .param(step, param_schema::transform_2d::SCALE_X_INDEX)
            .unwrap_or(1.0)
            .max(0.05);
        let scale_y = ctx
            .param(step, param_schema::transform_2d::SCALE_Y_INDEX)
            .unwrap_or(1.0)
            .max(0.05);
        let rotate_deg = ctx
            .param(step, param_schema::transform_2d::ROTATE_DEG_INDEX)
            .unwrap_or(0.0);
        let pivot_x = ctx
            .param(step, param_schema::transform_2d::PIVOT_X_INDEX)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        let pivot_y = ctx
            .param(step, param_schema::transform_2d::PIVOT_Y_INDEX)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        ctx.out_ops.push(TexRuntimeOp::Transform2D {
            offset_x,
            offset_y,
            scale_x,
            scale_y,
            rotate_deg,
            pivot_x,
            pivot_y,
        });
    }

    fn emit_color_adjust(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let brightness = ctx
            .param(step, param_schema::color_adjust::BRIGHTNESS_INDEX)
            .unwrap_or(1.0);
        let gain_r = ctx
            .param(step, param_schema::color_adjust::GAIN_R_INDEX)
            .unwrap_or(1.0);
        let gain_g = ctx
            .param(step, param_schema::color_adjust::GAIN_G_INDEX)
            .unwrap_or(1.0);
        let gain_b = ctx
            .param(step, param_schema::color_adjust::GAIN_B_INDEX)
            .unwrap_or(1.0);
        let alpha_mul = ctx
            .param(step, param_schema::color_adjust::ALPHA_MUL_INDEX)
            .unwrap_or(1.0);
        ctx.out_ops.push(TexRuntimeOp::ColorAdjust {
            brightness,
            gain_r,
            gain_g,
            gain_b,
            alpha_mul,
        });
    }

    fn emit_level(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let in_low = ctx
            .param(step, param_schema::level::IN_LOW_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let in_high = ctx
            .param(step, param_schema::level::IN_HIGH_INDEX)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let gamma = ctx
            .param(step, param_schema::level::GAMMA_INDEX)
            .unwrap_or(1.0)
            .clamp(0.1, 8.0);
        let out_low = ctx
            .param(step, param_schema::level::OUT_LOW_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let out_high = ctx
            .param(step, param_schema::level::OUT_HIGH_INDEX)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        ctx.out_ops.push(TexRuntimeOp::Level {
            in_low,
            in_high,
            gamma,
            out_low,
            out_high,
        });
    }

    fn emit_mask(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let threshold = ctx
            .param(step, param_schema::mask::THRESHOLD_INDEX)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        let softness = ctx
            .param(step, param_schema::mask::SOFTNESS_INDEX)
            .unwrap_or(0.1)
            .clamp(0.0, 1.0);
        let invert = ctx
            .param(step, param_schema::mask::INVERT_INDEX)
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, 1.0);
        ctx.out_ops.push(TexRuntimeOp::Mask {
            threshold,
            softness,
            invert,
        });
    }

    fn emit_morphology(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let mode = ctx
            .param(step, param_schema::morphology::MODE_INDEX)
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, 3.0);
        let radius = ctx
            .param(step, param_schema::morphology::RADIUS_INDEX)
            .unwrap_or(1.0)
            .clamp(0.0, 8.0);
        let amount = ctx
            .param(step, param_schema::morphology::AMOUNT_INDEX)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        ctx.out_ops.push(TexRuntimeOp::Morphology {
            mode,
            radius,
            amount,
        });
    }

    fn emit_tone_map(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let low_pct = ctx
            .param(step, param_schema::tone_map::LOW_PCT_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 0.9);
        let high_pct = ctx
            .param(step, param_schema::tone_map::HIGH_PCT_INDEX)
            .unwrap_or(1.0)
            .clamp((low_pct + 0.01).min(1.0), 1.0);
        let contrast = ctx
            .param(step, param_schema::tone_map::CONTRAST_INDEX)
            .unwrap_or(1.0)
            .clamp(1.0, 3.0);
        ctx.out_ops.push(TexRuntimeOp::ToneMap {
            contrast,
            low_pct,
            high_pct,
        });
    }

    fn emit_blend(
        step: &CompiledStep,
        base_source_id: u32,
        layer_source_id: Option<u32>,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        let mode = ctx
            .param(step, param_schema::blend::MODE_INDEX)
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, 8.0);
        let opacity = ctx
            .param(step, param_schema::blend::OPACITY_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let bg_r = ctx
            .param(step, param_schema::blend::BG_R_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let bg_g = ctx
            .param(step, param_schema::blend::BG_G_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let bg_b = ctx
            .param(step, param_schema::blend::BG_B_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let bg_a = ctx
            .param(step, param_schema::blend::BG_A_INDEX)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        ctx.out_ops.push(TexRuntimeOp::Blend {
            mode,
            opacity,
            bg_r,
            bg_g,
            bg_b,
            bg_a,
            base_texture_node_id: base_source_id,
            layer_texture_node_id: layer_source_id,
        });
    }

    fn emit_warp_transform(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let strength = ctx
            .param(step, param_schema::warp_transform::STRENGTH_INDEX)
            .unwrap_or(0.5)
            .clamp(0.0, 2.4);
        let frequency = ctx
            .param(step, param_schema::warp_transform::FREQUENCY_INDEX)
            .unwrap_or(2.0)
            .clamp(0.05, 12.0);
        let phase = ctx
            .param(step, param_schema::warp_transform::PHASE_INDEX)
            .unwrap_or(0.0);
        ctx.out_ops.push(TexRuntimeOp::WarpTransform {
            strength,
            frequency,
            phase,
        });
    }

    fn emit_domain_warp(
        step: &CompiledStep,
        base_source_id: u32,
        warp_source_id: Option<u32>,
        ctx: &mut RuntimeEvalContext<'_>,
    ) {
        let strength = ctx
            .param(step, param_schema::domain_warp::STRENGTH_INDEX)
            .unwrap_or(0.28)
            .clamp(0.0, 2.0);
        let frequency = ctx
            .param(step, param_schema::domain_warp::FREQUENCY_INDEX)
            .unwrap_or(2.5)
            .clamp(0.05, 16.0);
        let rotation = ctx
            .param(step, param_schema::domain_warp::ROTATION_INDEX)
            .unwrap_or(0.0)
            .clamp(-180.0, 180.0);
        let octaves = ctx
            .param(step, param_schema::domain_warp::OCTAVES_INDEX)
            .unwrap_or(3.0)
            .round()
            .clamp(1.0, 6.0);
        ctx.out_ops.push(TexRuntimeOp::DomainWarp {
            strength,
            frequency,
            rotation,
            octaves,
            base_texture_node_id: base_source_id,
            warp_texture_node_id: warp_source_id,
        });
    }

    fn emit_directional_smear(step: &CompiledStep, ctx: &mut RuntimeEvalContext<'_>) {
        let angle = ctx
            .param(step, param_schema::directional_smear::ANGLE_INDEX)
            .unwrap_or(90.0)
            .clamp(-180.0, 180.0);
        let length = ctx
            .param(step, param_schema::directional_smear::LENGTH_INDEX)
            .unwrap_or(18.0)
            .clamp(0.0, 96.0);
        let jitter = ctx
            .param(step, param_schema::directional_smear::JITTER_INDEX)
            .unwrap_or(0.2)
            .clamp(0.0, 1.0);
        let amount = ctx
            .param(step, param_schema::directional_smear::AMOUNT_INDEX)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        ctx.out_ops.push(TexRuntimeOp::DirectionalSmear {
            angle,
            length,
            jitter,
            amount,
        });
    }

    /// Return resolved render-texture size for this compiled output chain.
    ///
    /// The current implementation uses `render.scene_pass` `res_width`/`res_height`
    /// when present, with `0` meaning "use project preview size".
    pub(crate) fn output_texture_size(
        &self,
        project: &GuiProject,
        time_secs: f32,
        eval_stack: &mut SignalEvalStack,
    ) -> (u32, u32) {
        begin_runtime_eval_pass(eval_stack);
        let default_w = project.preview_width.max(1);
        let default_h = project.preview_height.max(1);
        for step in self.steps.iter().rev() {
            if step.kind != CompiledStepKind::ScenePass {
                continue;
            }
            let raw_w = compiled_param_value_opt(
                project,
                step,
                param_schema::render_scene_pass::RES_WIDTH_INDEX,
                time_secs,
                eval_stack,
            )
            .unwrap_or(0.0);
            let raw_h = compiled_param_value_opt(
                project,
                step,
                param_schema::render_scene_pass::RES_HEIGHT_INDEX,
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

#[derive(Clone, Copy, Debug)]
struct RuntimeEvalState {
    mesh: Option<SceneMeshState>,
    entity: Option<SceneEntityState>,
    scene_ready: bool,
    camera_zoom: f32,
}

impl RuntimeEvalState {
    const fn new() -> Self {
        Self {
            mesh: None,
            entity: None,
            scene_ready: false,
            camera_zoom: 1.0,
        }
    }
}

struct RuntimeEvalContext<'a> {
    project: &'a GuiProject,
    time_secs: f32,
    frame: Option<TexRuntimeFrameContext>,
    eval_stack: &'a mut SignalEvalStack,
    out_ops: &'a mut Vec<TexRuntimeOp>,
}

impl<'a> RuntimeEvalContext<'a> {
    fn new(
        project: &'a GuiProject,
        time_secs: f32,
        frame: Option<TexRuntimeFrameContext>,
        eval_stack: &'a mut SignalEvalStack,
        out_ops: &'a mut Vec<TexRuntimeOp>,
    ) -> Self {
        Self {
            project,
            time_secs,
            frame,
            eval_stack,
            out_ops,
        }
    }

    fn param(&mut self, step: &CompiledStep, param_index: usize) -> Option<f32> {
        compiled_param_value_opt(
            self.project,
            step,
            param_index,
            self.time_secs,
            self.eval_stack,
        )
    }
}

fn begin_runtime_eval_pass(eval_stack: &mut SignalEvalStack) {
    eval_stack.clear_nodes();
    RUNTIME_SIGNAL_SAMPLE_MEMO.with(|memo| memo.borrow_mut().clear());
}

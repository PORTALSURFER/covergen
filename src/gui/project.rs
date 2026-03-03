//! GUI project scaffolding and editable node model.
//!
//! The GUI currently starts with an empty project and supports adding and
//! moving nodes directly on the graph canvas.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Width of one graph node card in the editor canvas.
pub(crate) const NODE_WIDTH: i32 = 208;
/// Height of one graph node card in the editor canvas.
pub(crate) const NODE_HEIGHT: i32 = 44;
/// Shared graph grid pitch used for node placement and wire routing.
pub(crate) const NODE_GRID_PITCH: i32 = 4;
/// Maximum allowed length for one parameter label.
///
/// New parameter labels should fit in this budget to keep row naming
/// consistent and avoid text-overflow behavior in node cards.
pub(crate) const PARAM_LABEL_MAX_LEN: usize = 10;
/// Width/height of node header expand/collapse toggle in graph-space pixels.
pub(crate) const NODE_TOGGLE_SIZE: i32 = 8;
/// Top-left inset from node origin to toggle origin in graph-space pixels.
pub(crate) const NODE_TOGGLE_MARGIN: i32 = 3;
/// Diameter of one node pin in editor pixels.
pub(crate) const NODE_PIN_SIZE: i32 = 8;
/// Height of one expanded parameter row in node cards.
pub(crate) const NODE_PARAM_ROW_HEIGHT: i32 = 16;
/// Horizontal padding for expanded parameter row content.
pub(crate) const NODE_PARAM_ROW_PAD_X: i32 = 4;
/// Horizontal padding from parameter row right edge to value input box.
pub(crate) const NODE_PARAM_VALUE_BOX_RIGHT_PAD: i32 = 6;
/// Width of one parameter value input box in graph-space pixels.
pub(crate) const NODE_PARAM_VALUE_BOX_WIDTH: i32 = 52;
/// Height of one dropdown option row in graph-space pixels.
pub(crate) const NODE_PARAM_DROPDOWN_ROW_HEIGHT: i32 = NODE_PARAM_ROW_HEIGHT;
const NODE_PIN_HALF: i32 = NODE_PIN_SIZE / 2;
const NODE_PARAM_FOOTER_PAD: i32 = 8;
const NODE_SIGNAL_SCOPE_EXTRA_HEIGHT: i32 = 20;
const HIT_BIN_SIZE: i32 = 128;
const PERSISTED_GUI_PROJECT_VERSION: u32 = 1;
const TEXTURE_TARGET_PLACEHOLDER: &str = "none";
/// Stable parameter key for feedback accumulation history texture binding.
pub(crate) const FEEDBACK_HISTORY_PARAM_KEY: &str = "accumulation_tex";
/// Legacy persisted feedback-history key kept for backward-compatible loads.
pub(crate) const LEGACY_FEEDBACK_HISTORY_PARAM_KEY: &str = "target_tex";
const FEEDBACK_HISTORY_PARAM_LABEL: &str = "accum_tex";
/// Stable parameter key for feedback history write decimation interval.
///
/// `0` updates history every frame (classic one-frame delay). Larger values
/// insert additional frame holds between history writes for stepped trails.
pub(crate) const FEEDBACK_FRAME_GAP_PARAM_KEY: &str = "frame_gap";
const FEEDBACK_FRAME_GAP_PARAM_LABEL: &str = "frame_gap";
/// Stable parameter key for the feedback history reset action button.
pub(crate) const FEEDBACK_RESET_PARAM_KEY: &str = "reset";
const FEEDBACK_RESET_PARAM_LABEL: &str = "reset";
/// Stable parameter key for `tex.blend` secondary texture input.
pub(crate) const BLEND_LAYER_PARAM_KEY: &str = "blend_tex";
const BLEND_LAYER_PARAM_LABEL: &str = "blend_tex";
const SIGNATURE_DOMAIN_UI: u64 = 0x5549_5f53_4947_4e5f;

/// Per-frame signal sampling memo keyed by `(node_id, quantized_time_bucket)`.
pub(crate) type SignalSampleMemo = HashMap<(u32, i32), Option<f32>>;

/// Stack abstraction for recursive signal evaluation cycle checks.
pub(crate) trait SignalEvalPath {
    /// Return `true` when the node is currently active on the recursion path.
    fn contains_node(&self, node_id: u32) -> bool;
    /// Push one active node id onto the recursion path.
    fn push_node(&mut self, node_id: u32);
    /// Pop one active node id from the recursion path.
    fn pop_node(&mut self);
    /// Reset recursion path state.
    fn clear_nodes(&mut self);
}

impl SignalEvalPath for Vec<u32> {
    fn contains_node(&self, node_id: u32) -> bool {
        self.contains(&node_id)
    }

    fn push_node(&mut self, node_id: u32) {
        self.push(node_id);
    }

    fn pop_node(&mut self) {
        let _ = self.pop();
    }

    fn clear_nodes(&mut self) {
        self.clear();
    }
}

/// Hash-backed signal recursion stack used by runtime hot paths.
#[derive(Clone, Debug, Default)]
pub(crate) struct SignalEvalStack {
    order: Vec<u32>,
    active: HashSet<u32>,
}

impl SignalEvalPath for SignalEvalStack {
    fn contains_node(&self, node_id: u32) -> bool {
        self.active.contains(&node_id)
    }

    fn push_node(&mut self, node_id: u32) {
        self.order.push(node_id);
        self.active.insert(node_id);
    }

    fn pop_node(&mut self) {
        let Some(node_id) = self.order.pop() else {
            return;
        };
        self.active.remove(&node_id);
    }

    fn clear_nodes(&mut self) {
        self.order.clear();
        self.active.clear();
    }
}

/// Snap one graph-space scalar position to the shared node grid.
pub(crate) fn snap_to_node_grid(value: i32) -> i32 {
    let base = value.div_euclid(NODE_GRID_PITCH) * NODE_GRID_PITCH;
    let next = base + NODE_GRID_PITCH;
    if (value - base).abs() <= (next - value).abs() {
        base
    } else {
        next
    }
}

/// Arc style options exposed by the `buf.circle_nurbs` node.
const BUF_CIRCLE_ARC_STYLE_OPTIONS: [NodeParamOption; 2] = [
    NodeParamOption {
        label: "closed",
        value: 0.0,
    },
    NodeParamOption {
        label: "open_arc",
        value: 1.0,
    },
];
/// Temporal animation modes exposed by the `buf.noise` node.
const BUF_NOISE_LOOP_MODE_OPTIONS: [NodeParamOption; 2] = [
    NodeParamOption {
        label: "free",
        value: 0.0,
    },
    NodeParamOption {
        label: "loop",
        value: 1.0,
    },
];
/// Background compositing modes exposed by the `render.scene_pass` node.
const SCENE_PASS_BG_MODE_OPTIONS: [NodeParamOption; 2] = [
    NodeParamOption {
        label: "with_bg",
        value: 0.0,
    },
    NodeParamOption {
        label: "alpha_clip",
        value: 1.0,
    },
];
/// Timing modes exposed by the `ctl.lfo` node.
const LFO_SYNC_MODE_OPTIONS: [NodeParamOption; 2] = [
    NodeParamOption {
        label: "free",
        value: 0.0,
    },
    NodeParamOption {
        label: "beat",
        value: 1.0,
    },
];
/// Waveform types exposed by the `ctl.lfo` node.
const LFO_TYPE_OPTIONS: [NodeParamOption; 5] = [
    NodeParamOption {
        label: "sine",
        value: 0.0,
    },
    NodeParamOption {
        label: "saw",
        value: 1.0,
    },
    NodeParamOption {
        label: "triangle",
        value: 2.0,
    },
    NodeParamOption {
        label: "pulse",
        value: 3.0,
    },
    NodeParamOption {
        label: "drift",
        value: 4.0,
    },
];
/// Blend/composite modes exposed by the `tex.blend` node.
const TEX_BLEND_MODE_OPTIONS: [NodeParamOption; 9] = [
    NodeParamOption {
        label: "normal",
        value: 0.0,
    },
    NodeParamOption {
        label: "add",
        value: 1.0,
    },
    NodeParamOption {
        label: "subtract",
        value: 2.0,
    },
    NodeParamOption {
        label: "multiply",
        value: 3.0,
    },
    NodeParamOption {
        label: "screen",
        value: 4.0,
    },
    NodeParamOption {
        label: "overlay",
        value: 5.0,
    },
    NodeParamOption {
        label: "darken",
        value: 6.0,
    },
    NodeParamOption {
        label: "lighten",
        value: 7.0,
    },
    NodeParamOption {
        label: "difference",
        value: 8.0,
    },
];
/// Effect options exposed by the `tex.post_color_tone` node.
const POST_COLOR_TONE_EFFECT_OPTIONS: [NodeParamOption; 10] = [
    NodeParamOption {
        label: "bloom",
        value: 0.0,
    },
    NodeParamOption {
        label: "tone_map",
        value: 1.0,
    },
    NodeParamOption {
        label: "grading",
        value: 2.0,
    },
    NodeParamOption {
        label: "wb_shift",
        value: 3.0,
    },
    NodeParamOption {
        label: "exposure",
        value: 4.0,
    },
    NodeParamOption {
        label: "contrast",
        value: 5.0,
    },
    NodeParamOption {
        label: "gamma",
        value: 6.0,
    },
    NodeParamOption {
        label: "vibrance",
        value: 7.0,
    },
    NodeParamOption {
        label: "posterize",
        value: 8.0,
    },
    NodeParamOption {
        label: "duotone",
        value: 9.0,
    },
];
/// Effect options exposed by the `tex.post_edge_structure` node.
const POST_EDGE_STRUCTURE_EFFECT_OPTIONS: [NodeParamOption; 6] = [
    NodeParamOption {
        label: "edge_detect",
        value: 0.0,
    },
    NodeParamOption {
        label: "toon_edge",
        value: 1.0,
    },
    NodeParamOption {
        label: "emboss",
        value: 2.0,
    },
    NodeParamOption {
        label: "sharpen",
        value: 3.0,
    },
    NodeParamOption {
        label: "kuwahara",
        value: 4.0,
    },
    NodeParamOption {
        label: "depth_edge",
        value: 5.0,
    },
];
/// Effect options exposed by the `tex.post_blur_diffusion` node.
const POST_BLUR_DIFFUSION_EFFECT_OPTIONS: [NodeParamOption; 7] = [
    NodeParamOption {
        label: "gaussian",
        value: 0.0,
    },
    NodeParamOption {
        label: "box",
        value: 1.0,
    },
    NodeParamOption {
        label: "kawase",
        value: 2.0,
    },
    NodeParamOption {
        label: "radial",
        value: 3.0,
    },
    NodeParamOption {
        label: "motion",
        value: 4.0,
    },
    NodeParamOption {
        label: "dof",
        value: 5.0,
    },
    NodeParamOption {
        label: "tilt_shift",
        value: 6.0,
    },
];
/// Effect options exposed by the `tex.post_distortion` node.
const POST_DISTORTION_EFFECT_OPTIONS: [NodeParamOption; 6] = [
    NodeParamOption {
        label: "chrom_ab",
        value: 0.0,
    },
    NodeParamOption {
        label: "lens_warp",
        value: 1.0,
    },
    NodeParamOption {
        label: "heat",
        value: 2.0,
    },
    NodeParamOption {
        label: "shockwave",
        value: 3.0,
    },
    NodeParamOption {
        label: "refract",
        value: 4.0,
    },
    NodeParamOption {
        label: "glitch",
        value: 5.0,
    },
];
/// Effect options exposed by the `tex.post_temporal` node.
const POST_TEMPORAL_EFFECT_OPTIONS: [NodeParamOption; 5] = [
    NodeParamOption {
        label: "trails",
        value: 0.0,
    },
    NodeParamOption {
        label: "feedback",
        value: 1.0,
    },
    NodeParamOption {
        label: "datamosh",
        value: 2.0,
    },
    NodeParamOption {
        label: "afterimg",
        value: 3.0,
    },
    NodeParamOption {
        label: "echo",
        value: 4.0,
    },
];
/// Effect options exposed by the `tex.post_noise_texture` node.
const POST_NOISE_TEXTURE_EFFECT_OPTIONS: [NodeParamOption; 6] = [
    NodeParamOption {
        label: "grain",
        value: 0.0,
    },
    NodeParamOption {
        label: "dither",
        value: 1.0,
    },
    NodeParamOption {
        label: "scanline",
        value: 2.0,
    },
    NodeParamOption {
        label: "vhs",
        value: 3.0,
    },
    NodeParamOption {
        label: "pixelate",
        value: 4.0,
    },
    NodeParamOption {
        label: "mosaic",
        value: 5.0,
    },
];
/// Effect options exposed by the `tex.post_lighting` node.
const POST_LIGHTING_EFFECT_OPTIONS: [NodeParamOption; 6] = [
    NodeParamOption {
        label: "god_rays",
        value: 0.0,
    },
    NodeParamOption {
        label: "lens_flare",
        value: 1.0,
    },
    NodeParamOption {
        label: "vignette",
        value: 2.0,
    },
    NodeParamOption {
        label: "leaks",
        value: 3.0,
    },
    NodeParamOption {
        label: "anamorph",
        value: 4.0,
    },
    NodeParamOption {
        label: "halation",
        value: 5.0,
    },
];
/// Effect options exposed by the `tex.post_screen_space` node.
const POST_SCREEN_SPACE_EFFECT_OPTIONS: [NodeParamOption; 6] = [
    NodeParamOption {
        label: "ssao",
        value: 0.0,
    },
    NodeParamOption {
        label: "ssr",
        value: 1.0,
    },
    NodeParamOption {
        label: "ss_refract",
        value: 2.0,
    },
    NodeParamOption {
        label: "depth_fog",
        value: 3.0,
    },
    NodeParamOption {
        label: "height_fade",
        value: 4.0,
    },
    NodeParamOption {
        label: "curvature",
        value: 5.0,
    },
];
/// Effect options exposed by the `tex.post_experimental` node.
const POST_EXPERIMENTAL_EFFECT_OPTIONS: [NodeParamOption; 9] = [
    NodeParamOption {
        label: "rd_filter",
        value: 0.0,
    },
    NodeParamOption {
        label: "cell_auto",
        value: 1.0,
    },
    NodeParamOption {
        label: "zoom_fb",
        value: 2.0,
    },
    NodeParamOption {
        label: "kaleido",
        value: 3.0,
    },
    NodeParamOption {
        label: "polar",
        value: 4.0,
    },
    NodeParamOption {
        label: "sdf_remap",
        value: 5.0,
    },
    NodeParamOption {
        label: "flow_adv",
        value: 6.0,
    },
    NodeParamOption {
        label: "fourier",
        value: 7.0,
    },
    NodeParamOption {
        label: "grad_style",
        value: 8.0,
    },
];

/// Resource kinds currently carried by GUI graph ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResourceKind {
    /// GPU mesh buffer resource.
    Buffer,
    /// Scene entity resource (mesh + transform + material binding).
    Entity,
    /// Built scene resource ready for rendering.
    Scene,
    /// GPU 2D texture resource.
    Texture2D,
    /// CPU-side scalar signal resource.
    Signal,
}

/// Execution kinds currently represented by GUI nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ExecutionKind {
    /// Node executes in CPU/data-prep domain.
    Cpu,
    /// Node executes through a render pass.
    Render,
    /// Node executes in control domain.
    Control,
    /// Node is a runtime IO boundary.
    Io,
}

/// Minimal set of node kinds exposed by the Add Node menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ProjectNodeKind {
    /// `tex.solid` source node (full-frame solid color).
    TexSolid,
    /// `tex.circle` source node.
    TexCircle,
    /// `buf.sphere` mesh buffer source node.
    BufSphere,
    /// `buf.circle_nurbs` curve buffer source node.
    BufCircleNurbs,
    /// `buf.noise` mesh deformation node.
    BufNoise,
    /// `tex.transform_2d` render node for texture-space color/alpha mutation.
    TexTransform2D,
    /// `tex.level` render node for input/output remapping and gamma shaping.
    TexLevel,
    /// `tex.feedback` delayed texture feedback node with optional frame-gap stepping.
    TexFeedback,
    /// `tex.reaction_diffusion` temporal Gray-Scott simulation node.
    TexReactionDiffusion,
    /// `tex.post_color_tone` category post-process node.
    TexPostColorTone,
    /// `tex.post_edge_structure` category post-process node.
    TexPostEdgeStructure,
    /// `tex.post_blur_diffusion` category post-process node.
    TexPostBlurDiffusion,
    /// `tex.post_distortion` category post-process node.
    TexPostDistortion,
    /// `tex.post_temporal` category post-process node.
    TexPostTemporal,
    /// `tex.post_noise_texture` category post-process node.
    TexPostNoiseTexture,
    /// `tex.post_lighting` category post-process node.
    TexPostLighting,
    /// `tex.post_screen_space` category post-process node.
    TexPostScreenSpace,
    /// `tex.post_experimental` category post-process node.
    TexPostExperimental,
    /// `tex.blend` two-texture composite node.
    TexBlend,
    /// `scene.entity` mesh + transform + material binding node.
    SceneEntity,
    /// `scene.build` scene aggregation node.
    SceneBuild,
    /// `render.camera` scene-view camera node.
    RenderCamera,
    /// `render.scene_pass` scene-to-texture render node.
    RenderScenePass,
    /// `ctl.lfo` signal generator node.
    CtlLfo,
    /// `io.window_out` sink node.
    IoWindowOut,
}

impl ProjectNodeKind {
    /// Return stable registry id used by UI labels and serialization.
    pub(crate) const fn stable_id(self) -> &'static str {
        match self {
            Self::TexSolid => "tex.solid",
            Self::TexCircle => "tex.circle",
            Self::BufSphere => "buf.sphere",
            Self::BufCircleNurbs => "buf.circle_nurbs",
            Self::BufNoise => "buf.noise",
            Self::TexTransform2D => "tex.transform_2d",
            Self::TexLevel => "tex.level",
            Self::TexFeedback => "tex.feedback",
            Self::TexReactionDiffusion => "tex.reaction_diffusion",
            Self::TexPostColorTone => "tex.post_color_tone",
            Self::TexPostEdgeStructure => "tex.post_edge_structure",
            Self::TexPostBlurDiffusion => "tex.post_blur_diffusion",
            Self::TexPostDistortion => "tex.post_distortion",
            Self::TexPostTemporal => "tex.post_temporal",
            Self::TexPostNoiseTexture => "tex.post_noise_texture",
            Self::TexPostLighting => "tex.post_lighting",
            Self::TexPostScreenSpace => "tex.post_screen_space",
            Self::TexPostExperimental => "tex.post_experimental",
            Self::TexBlend => "tex.blend",
            Self::SceneEntity => "scene.entity",
            Self::SceneBuild => "scene.build",
            Self::RenderCamera => "render.camera",
            Self::RenderScenePass => "render.scene_pass",
            Self::CtlLfo => "ctl.lfo",
            Self::IoWindowOut => "io.window_out",
        }
    }

    /// Parse node kind from a stable node id.
    pub(crate) fn from_stable_id(id: &str) -> Option<Self> {
        match id {
            "tex.solid" => Some(Self::TexSolid),
            "tex.circle" => Some(Self::TexCircle),
            "buf.sphere" => Some(Self::BufSphere),
            "buf.circle_nurbs" => Some(Self::BufCircleNurbs),
            "buf.noise" => Some(Self::BufNoise),
            "tex.transform_2d" => Some(Self::TexTransform2D),
            "tex.level" => Some(Self::TexLevel),
            "tex.feedback" => Some(Self::TexFeedback),
            "tex.reaction_diffusion" => Some(Self::TexReactionDiffusion),
            "tex.post_color_tone" => Some(Self::TexPostColorTone),
            "tex.post_edge_structure" => Some(Self::TexPostEdgeStructure),
            "tex.post_blur_diffusion" => Some(Self::TexPostBlurDiffusion),
            "tex.post_distortion" => Some(Self::TexPostDistortion),
            "tex.post_temporal" => Some(Self::TexPostTemporal),
            "tex.post_noise_texture" => Some(Self::TexPostNoiseTexture),
            "tex.post_lighting" => Some(Self::TexPostLighting),
            "tex.post_screen_space" => Some(Self::TexPostScreenSpace),
            "tex.post_experimental" => Some(Self::TexPostExperimental),
            "tex.blend" => Some(Self::TexBlend),
            "scene.entity" => Some(Self::SceneEntity),
            "scene.build" => Some(Self::SceneBuild),
            "render.camera" => Some(Self::RenderCamera),
            "render.scene_pass" => Some(Self::RenderScenePass),
            "ctl.lfo" => Some(Self::CtlLfo),
            "io.window_out" => Some(Self::IoWindowOut),
            _ => None,
        }
    }

    /// Return execution kind for this node.
    #[allow(dead_code)]
    pub(crate) const fn execution_kind(self) -> ExecutionKind {
        match self {
            Self::TexSolid => ExecutionKind::Render,
            Self::TexCircle => ExecutionKind::Render,
            Self::BufSphere => ExecutionKind::Cpu,
            Self::BufCircleNurbs => ExecutionKind::Cpu,
            Self::BufNoise => ExecutionKind::Cpu,
            Self::TexTransform2D => ExecutionKind::Render,
            Self::TexLevel => ExecutionKind::Render,
            Self::TexFeedback => ExecutionKind::Render,
            Self::TexReactionDiffusion => ExecutionKind::Render,
            Self::TexPostColorTone => ExecutionKind::Render,
            Self::TexPostEdgeStructure => ExecutionKind::Render,
            Self::TexPostBlurDiffusion => ExecutionKind::Render,
            Self::TexPostDistortion => ExecutionKind::Render,
            Self::TexPostTemporal => ExecutionKind::Render,
            Self::TexPostNoiseTexture => ExecutionKind::Render,
            Self::TexPostLighting => ExecutionKind::Render,
            Self::TexPostScreenSpace => ExecutionKind::Render,
            Self::TexPostExperimental => ExecutionKind::Render,
            Self::TexBlend => ExecutionKind::Render,
            Self::SceneEntity => ExecutionKind::Control,
            Self::SceneBuild => ExecutionKind::Control,
            Self::RenderCamera => ExecutionKind::Control,
            Self::RenderScenePass => ExecutionKind::Render,
            Self::CtlLfo => ExecutionKind::Control,
            Self::IoWindowOut => ExecutionKind::Io,
        }
    }

    /// Return short display label used by node and menu UI.
    pub(crate) const fn label(self) -> &'static str {
        self.stable_id()
    }

    /// Return required primary input resource kind for this node, if any.
    pub(crate) const fn input_resource_kind(self) -> Option<ResourceKind> {
        match self {
            Self::TexTransform2D
            | Self::TexLevel
            | Self::TexFeedback
            | Self::TexReactionDiffusion
            | Self::TexPostColorTone
            | Self::TexPostEdgeStructure
            | Self::TexPostBlurDiffusion
            | Self::TexPostDistortion
            | Self::TexPostTemporal
            | Self::TexPostNoiseTexture
            | Self::TexPostLighting
            | Self::TexPostScreenSpace
            | Self::TexPostExperimental
            | Self::TexBlend
            | Self::IoWindowOut => Some(ResourceKind::Texture2D),
            Self::BufNoise => Some(ResourceKind::Buffer),
            Self::SceneEntity => Some(ResourceKind::Buffer),
            Self::SceneBuild => Some(ResourceKind::Entity),
            Self::RenderCamera => Some(ResourceKind::Scene),
            Self::RenderScenePass => Some(ResourceKind::Scene),
            _ => None,
        }
    }

    /// Return true when this node kind can bind scalar signal parameters.
    pub(crate) const fn accepts_signal_bindings(self) -> bool {
        matches!(
            self,
            Self::TexSolid
                | Self::TexCircle
                | Self::BufSphere
                | Self::BufCircleNurbs
                | Self::BufNoise
                | Self::TexTransform2D
                | Self::TexLevel
                | Self::TexFeedback
                | Self::TexReactionDiffusion
                | Self::TexPostColorTone
                | Self::TexPostEdgeStructure
                | Self::TexPostBlurDiffusion
                | Self::TexPostDistortion
                | Self::TexPostTemporal
                | Self::TexPostNoiseTexture
                | Self::TexPostLighting
                | Self::TexPostScreenSpace
                | Self::TexPostExperimental
                | Self::TexBlend
                | Self::SceneEntity
                | Self::RenderCamera
                | Self::RenderScenePass
                | Self::CtlLfo
        )
    }

    /// Return true when this node kind has a scalar signal output pin.
    pub(crate) const fn produces_signal_output(self) -> bool {
        matches!(self, Self::CtlLfo)
    }

    /// Return whether this node should render the inline data-signal preview field.
    ///
    /// The preview field is reserved for data-signal producers (for example
    /// `ctl.lfo`) and should stay hidden on texture/buffer/scene/output nodes.
    pub(crate) const fn shows_signal_preview(self) -> bool {
        self.produces_signal_output()
    }

    /// Return true when this node kind has a typed graph input pin.
    pub(crate) const fn has_input_pin(self) -> bool {
        self.input_resource_kind().is_some()
    }

    /// Return true when this node kind has any output pin.
    pub(crate) const fn has_output_pin(self) -> bool {
        self.output_resource_kind().is_some()
    }

    /// Return output resource kind when this node publishes one.
    pub(crate) const fn output_resource_kind(self) -> Option<ResourceKind> {
        match self {
            Self::BufSphere | Self::BufCircleNurbs | Self::BufNoise => Some(ResourceKind::Buffer),
            Self::SceneEntity => Some(ResourceKind::Entity),
            Self::SceneBuild | Self::RenderCamera => Some(ResourceKind::Scene),
            Self::TexSolid
            | Self::TexCircle
            | Self::TexTransform2D
            | Self::TexLevel
            | Self::TexFeedback
            | Self::TexReactionDiffusion
            | Self::TexPostColorTone
            | Self::TexPostEdgeStructure
            | Self::TexPostBlurDiffusion
            | Self::TexPostDistortion
            | Self::TexPostTemporal
            | Self::TexPostNoiseTexture
            | Self::TexPostLighting
            | Self::TexPostScreenSpace
            | Self::TexPostExperimental
            | Self::TexBlend
            | Self::RenderScenePass => Some(ResourceKind::Texture2D),
            Self::CtlLfo => Some(ResourceKind::Signal),
            Self::IoWindowOut => None,
        }
    }
}

/// Persisted GUI project payload used for autosave/reload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PersistedGuiProject {
    /// Schema version for migration compatibility.
    pub(crate) version: u32,
    /// Project display name.
    pub(crate) name: String,
    /// Preview texture width.
    pub(crate) preview_width: u32,
    /// Preview texture height.
    pub(crate) preview_height: u32,
    /// Persisted node records.
    pub(crate) nodes: Vec<PersistedGuiNode>,
}

/// Persisted node record for GUI autosave.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PersistedGuiNode {
    /// Stable node id from saved graph.
    pub(crate) id: u32,
    /// Stable node kind id.
    pub(crate) kind: String,
    /// Node x-position in graph space.
    pub(crate) x: i32,
    /// Node y-position in graph space.
    pub(crate) y: i32,
    /// Optional source node id for the typed input pin.
    pub(crate) texture_input: Option<u32>,
    /// Saved selected parameter index.
    pub(crate) selected_param: usize,
    /// Saved expanded state.
    pub(crate) expanded: bool,
    /// Persisted parameter state.
    pub(crate) params: Vec<PersistedGuiParam>,
}

/// Persisted parameter value and optional signal binding source.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PersistedGuiParam {
    /// Stable parameter key.
    pub(crate) key: String,
    /// Parameter scalar value.
    pub(crate) value: f32,
    /// Optional signal source node id.
    pub(crate) signal_source: Option<u32>,
    /// Optional texture source node id for texture-target parameter rows.
    #[serde(default)]
    pub(crate) texture_source: Option<u32>,
}

/// Error returned when persisted GUI project payload cannot be loaded.
#[derive(Clone, Debug)]
pub(crate) struct PersistedProjectLoadError {
    message: String,
}

impl PersistedProjectLoadError {
    /// Build one load error with an actionable message.
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PersistedProjectLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message.as_str())
    }
}

impl std::error::Error for PersistedProjectLoadError {}

/// One non-fatal warning emitted while loading a persisted GUI project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PersistedProjectLoadWarning {
    /// Persisted source node identifier where the warning originated.
    pub(crate) persisted_node_id: u32,
    /// Stable node kind identifier of the warning source node.
    pub(crate) node_kind: String,
    /// Persisted parameter key that could not be mapped during load.
    pub(crate) param_key: String,
}

impl PersistedProjectLoadWarning {
    /// Build one warning for a dropped persisted parameter key.
    pub(crate) fn dropped_param(
        persisted_node_id: u32,
        node_kind: impl Into<String>,
        param_key: impl Into<String>,
    ) -> Self {
        Self {
            persisted_node_id,
            node_kind: node_kind.into(),
            param_key: param_key.into(),
        }
    }
}

impl fmt::Display for PersistedProjectLoadWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dropped unknown persisted param '{}' on node {}#{}",
            self.param_key, self.node_kind, self.persisted_node_id
        )
    }
}

/// Structured persisted-project load result with non-fatal warnings.
#[derive(Debug)]
pub(crate) struct PersistedProjectLoadOutcome {
    /// Loaded in-memory GUI project.
    pub(crate) project: GuiProject,
    /// Non-fatal load warnings collected during migration/mapping.
    pub(crate) warnings: Vec<PersistedProjectLoadWarning>,
}

/// One selectable dropdown option for a node parameter.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeParamOption {
    /// User-facing option label rendered in the dropdown.
    pub(crate) label: &'static str,
    /// Scalar value stored for this option.
    pub(crate) value: f32,
}

/// Widget style used by one node parameter row.
#[derive(Clone, Copy, Debug)]
pub(crate) enum NodeParamWidget {
    /// Free-form numeric input field.
    Number,
    /// Fixed-option dropdown selector.
    Dropdown {
        /// Static option list used for this parameter.
        options: &'static [NodeParamOption],
    },
    /// Texture-node binding target used by feedback routing parameters.
    TextureTarget,
    /// Stateless action button rendered in the value field.
    ActionButton,
}

impl NodeParamWidget {
    /// Return true when this parameter uses a dropdown widget.
    pub(crate) const fn is_dropdown(self) -> bool {
        matches!(self, Self::Dropdown { .. })
    }

    /// Return dropdown options when this widget is dropdown-based.
    pub(crate) const fn dropdown_options(self) -> Option<&'static [NodeParamOption]> {
        match self {
            Self::Number => None,
            Self::Dropdown { options } => Some(options),
            Self::TextureTarget => None,
            Self::ActionButton => None,
        }
    }

    /// Return true when this parameter binds one texture node source id.
    pub(crate) const fn is_texture_target(self) -> bool {
        matches!(self, Self::TextureTarget)
    }

    /// Return true when this parameter is an action button.
    pub(crate) const fn is_action_button(self) -> bool {
        matches!(self, Self::ActionButton)
    }
}

/// Editable node-parameter state with optional signal binding.
#[derive(Clone, Debug)]
pub(crate) struct NodeParamSlot {
    key: &'static str,
    label: &'static str,
    value: f32,
    value_text: String,
    min: f32,
    max: f32,
    step: f32,
    signal_source: Option<u32>,
    texture_source: Option<u32>,
    widget: NodeParamWidget,
}

/// Immutable descriptor for one node-parameter row.
#[derive(Clone, Debug)]
pub(crate) struct NodeParamDescriptor {
    /// Stable parameter key.
    pub(crate) key: &'static str,
    /// User-facing row label.
    pub(crate) label: &'static str,
    /// Current scalar value after edits/bind resolution.
    pub(crate) value: f32,
    /// Cached formatted UI text for the current value.
    pub(crate) value_text: String,
    /// Lower clamp bound.
    pub(crate) min: f32,
    /// Upper clamp bound.
    pub(crate) max: f32,
    /// Increment/decrement step size.
    pub(crate) step: f32,
    /// Source node id for signal binds when present.
    pub(crate) signal_source: Option<u32>,
    /// Source node id for texture binds when present.
    pub(crate) texture_source: Option<u32>,
    /// Parameter widget flavor.
    pub(crate) widget: NodeParamWidget,
}

/// Read-only parameter view for rendering node UI.
#[derive(Clone, Debug)]
pub(crate) struct NodeParamView<'a> {
    pub(crate) label: &'a str,
    pub(crate) value_text: &'a str,
    pub(crate) bound: bool,
    pub(crate) selected: bool,
    pub(crate) dropdown: bool,
    pub(crate) action_button: bool,
}

/// Zero-allocation iterator over one node's parameter rows.
///
/// This keeps UI traversal allocation-free by borrowing slots directly instead
/// of materializing an intermediate vector every frame.
pub(crate) struct NodeParamIter<'a> {
    params: std::slice::Iter<'a, NodeParamSlot>,
    selected_index: usize,
    index: usize,
}

impl<'a> Iterator for NodeParamIter<'a> {
    type Item = NodeParamView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let slot = self.params.next()?;
        let selected = self.index == self.selected_index;
        self.index += 1;
        Some(NodeParamView {
            label: slot.label,
            value_text: slot.value_text.as_str(),
            bound: slot.signal_source.is_some() || slot.texture_source.is_some(),
            selected,
            dropdown: slot.widget.is_dropdown(),
            action_button: slot.widget.is_action_button(),
        })
    }
}

/// One user-editable graph node instance in a GUI project.
#[derive(Clone, Debug)]
pub(crate) struct ProjectNode {
    id: u32,
    kind: ProjectNodeKind,
    x: i32,
    y: i32,
    texture_input: Option<u32>,
    inputs: Vec<u32>,
    params: Vec<NodeParamSlot>,
    selected_param: usize,
    expanded: bool,
}

/// Axis-aligned bounds of all graph nodes in world-space coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GraphBounds {
    pub(crate) min_x: i32,
    pub(crate) min_y: i32,
    pub(crate) max_x: i32,
    pub(crate) max_y: i32,
}

impl ProjectNode {
    /// Return stable node id.
    pub(crate) const fn id(&self) -> u32 {
        self.id
    }

    /// Return node kind.
    pub(crate) const fn kind(&self) -> ProjectNodeKind {
        self.kind
    }

    /// Return top-left x-position in panel space.
    pub(crate) const fn x(&self) -> i32 {
        self.x
    }

    /// Return top-left y-position in panel space.
    pub(crate) const fn y(&self) -> i32 {
        self.y
    }

    /// Return input node ids.
    pub(crate) fn inputs(&self) -> &[u32] {
        &self.inputs
    }

    /// Return true when node card is expanded.
    pub(crate) const fn expanded(&self) -> bool {
        self.expanded
    }

    /// Return true when this node supports expand/collapse parameter UI.
    pub(crate) fn supports_expand_toggle(&self) -> bool {
        !self.params.is_empty()
    }

    /// Return node card height in world-space canvas pixels.
    pub(crate) fn card_height(&self) -> i32 {
        if !self.expanded || self.params.is_empty() {
            return NODE_HEIGHT;
        }
        let mut height = NODE_HEIGHT
            + (self.params.len() as i32 * NODE_PARAM_ROW_HEIGHT)
            + NODE_PARAM_FOOTER_PAD;
        if self.kind.shows_signal_preview() {
            height += NODE_SIGNAL_SCOPE_EXTRA_HEIGHT;
        }
        height
    }

    /// Return number of editable parameters for this node.
    pub(crate) fn param_count(&self) -> usize {
        self.params.len()
    }

    /// Return currently selected parameter row index, clamped to valid bounds.
    pub(crate) fn selected_param_index(&self) -> usize {
        self.selected_param.min(self.params.len().saturating_sub(1))
    }

    /// Return allocation-free iterator of parameter rows for rendering.
    pub(crate) fn param_views(&self) -> NodeParamIter<'_> {
        NodeParamIter {
            params: self.params.iter(),
            selected_index: self.selected_param.min(self.params.len().saturating_sub(1)),
            index: 0,
        }
    }

    /// Return read-only parameter row data for one index.
    #[allow(dead_code)]
    pub(crate) fn param_view(&self, param_index: usize) -> Option<NodeParamView<'_>> {
        let slot = self.params.get(param_index)?;
        let selected = param_index == self.selected_param.min(self.params.len().saturating_sub(1));
        Some(NodeParamView {
            label: slot.label,
            value_text: slot.value_text.as_str(),
            bound: slot.signal_source.is_some() || slot.texture_source.is_some(),
            selected,
            dropdown: slot.widget.is_dropdown(),
            action_button: slot.widget.is_action_button(),
        })
    }
}

/// In-memory GUI project model.
#[derive(Clone, Debug)]
pub(crate) struct GuiProject {
    /// Project display name.
    pub(crate) name: String,
    /// Preview canvas width.
    pub(crate) preview_width: u32,
    /// Preview canvas height.
    pub(crate) preview_height: u32,
    nodes: Vec<ProjectNode>,
    node_index_lookup: HashMap<u32, usize>,
    next_node_id: u32,
    edge_count: usize,
    hit_test_cache: RefCell<HitTestCache>,
    hit_test_seen_scratch: RefCell<HashSet<u32>>,
    hit_test_candidates_scratch: RefCell<Vec<u32>>,
    hit_test_dirty: Cell<bool>,
    hit_test_scan_count: Cell<u64>,
    render_epoch: u64,
    ui_epoch: u64,
    render_signature_cache: Cell<u64>,
    render_signature_dirty: Cell<bool>,
    ui_signature_cache: u64,
    graph_signature_cache: Cell<u64>,
    graph_signature_dirty: Cell<bool>,
    nodes_epoch: u64,
    wires_epoch: u64,
    tex_eval_epoch: u64,
    lfo_sync_bpm: f32,
    has_signal_bindings_cached: Cell<bool>,
    has_temporal_nodes_cached: Cell<bool>,
    has_signal_preview_nodes_cached: Cell<bool>,
    runtime_flags_dirty: Cell<bool>,
}

/// Project-scoped invalidation epochs consumed by GUI retained layers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GuiProjectInvalidation {
    pub(crate) nodes: u64,
    pub(crate) wires: u64,
    pub(crate) tex_eval: u64,
}

/// Cached spatial/index structures for fast graph hit-testing.
#[derive(Clone, Debug, Default)]
struct HitTestCache {
    node_bins: HashMap<i64, Vec<u32>>,
    output_pin_bins: HashMap<i64, Vec<u32>>,
    input_pin_bins: HashMap<i64, Vec<u32>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinHitKind {
    Output,
    Input,
}

mod geometry;
mod params;
pub(crate) mod param_schema;
mod signatures;
mod state;
#[cfg(test)]
mod tests;

pub(crate) use geometry::{
    collapsed_param_entry_pin_center, input_pin_center, node_expand_toggle_rect,
    node_param_dropdown_rect, node_param_row_rect, node_param_value_rect, output_pin_center,
    pin_rect,
};

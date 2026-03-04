//! GUI project scaffolding and editable node model.
//!
//! The GUI currently starts with an empty project and supports adding and
//! moving nodes directly on the graph canvas.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use options::*;
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

/// Metadata descriptor for one `ProjectNodeKind`.
#[derive(Clone, Copy, Debug)]
struct ProjectNodeKindDescriptor {
    kind: ProjectNodeKind,
    stable_id: &'static str,
    #[allow(dead_code)]
    execution_kind: ExecutionKind,
    input_resource_kind: Option<ResourceKind>,
    output_resource_kind: Option<ResourceKind>,
    accepts_signal_bindings: bool,
    shows_signal_preview: bool,
}

const PROJECT_NODE_KIND_DESCRIPTORS: [ProjectNodeKindDescriptor; 25] = [
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexSolid,
        stable_id: "tex.solid",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: None,
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexCircle,
        stable_id: "tex.circle",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: None,
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::BufSphere,
        stable_id: "buf.sphere",
        execution_kind: ExecutionKind::Cpu,
        input_resource_kind: None,
        output_resource_kind: Some(ResourceKind::Buffer),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::BufCircleNurbs,
        stable_id: "buf.circle_nurbs",
        execution_kind: ExecutionKind::Cpu,
        input_resource_kind: None,
        output_resource_kind: Some(ResourceKind::Buffer),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::BufNoise,
        stable_id: "buf.noise",
        execution_kind: ExecutionKind::Cpu,
        input_resource_kind: Some(ResourceKind::Buffer),
        output_resource_kind: Some(ResourceKind::Buffer),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexTransform2D,
        stable_id: "tex.transform_2d",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexLevel,
        stable_id: "tex.level",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexFeedback,
        stable_id: "tex.feedback",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexReactionDiffusion,
        stable_id: "tex.reaction_diffusion",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexPostColorTone,
        stable_id: "tex.post_color_tone",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexPostEdgeStructure,
        stable_id: "tex.post_edge_structure",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexPostBlurDiffusion,
        stable_id: "tex.post_blur_diffusion",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexPostDistortion,
        stable_id: "tex.post_distortion",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexPostTemporal,
        stable_id: "tex.post_temporal",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexPostNoiseTexture,
        stable_id: "tex.post_noise_texture",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexPostLighting,
        stable_id: "tex.post_lighting",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexPostScreenSpace,
        stable_id: "tex.post_screen_space",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexPostExperimental,
        stable_id: "tex.post_experimental",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::TexBlend,
        stable_id: "tex.blend",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::SceneEntity,
        stable_id: "scene.entity",
        execution_kind: ExecutionKind::Control,
        input_resource_kind: Some(ResourceKind::Buffer),
        output_resource_kind: Some(ResourceKind::Entity),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::SceneBuild,
        stable_id: "scene.build",
        execution_kind: ExecutionKind::Control,
        input_resource_kind: Some(ResourceKind::Entity),
        output_resource_kind: Some(ResourceKind::Scene),
        accepts_signal_bindings: false,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::RenderCamera,
        stable_id: "render.camera",
        execution_kind: ExecutionKind::Control,
        input_resource_kind: Some(ResourceKind::Scene),
        output_resource_kind: Some(ResourceKind::Scene),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::RenderScenePass,
        stable_id: "render.scene_pass",
        execution_kind: ExecutionKind::Render,
        input_resource_kind: Some(ResourceKind::Scene),
        output_resource_kind: Some(ResourceKind::Texture2D),
        accepts_signal_bindings: true,
        shows_signal_preview: false,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::CtlLfo,
        stable_id: "ctl.lfo",
        execution_kind: ExecutionKind::Control,
        input_resource_kind: None,
        output_resource_kind: Some(ResourceKind::Signal),
        accepts_signal_bindings: true,
        shows_signal_preview: true,
    },
    ProjectNodeKindDescriptor {
        kind: ProjectNodeKind::IoWindowOut,
        stable_id: "io.window_out",
        execution_kind: ExecutionKind::Io,
        input_resource_kind: Some(ResourceKind::Texture2D),
        output_resource_kind: None,
        accepts_signal_bindings: false,
        shows_signal_preview: false,
    },
];

impl ProjectNodeKind {
    fn descriptor(self) -> &'static ProjectNodeKindDescriptor {
        PROJECT_NODE_KIND_DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.kind == self)
            .expect("project node kind descriptor missing")
    }

    /// Return stable registry id used by UI labels and serialization.
    pub(crate) fn stable_id(self) -> &'static str {
        self.descriptor().stable_id
    }

    /// Parse node kind from a stable node id.
    pub(crate) fn from_stable_id(id: &str) -> Option<Self> {
        PROJECT_NODE_KIND_DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.stable_id == id)
            .map(|descriptor| descriptor.kind)
    }

    /// Return execution kind for this node.
    #[allow(dead_code)]
    pub(crate) fn execution_kind(self) -> ExecutionKind {
        self.descriptor().execution_kind
    }

    /// Return short display label used by node and menu UI.
    pub(crate) fn label(self) -> &'static str {
        self.stable_id()
    }

    /// Return required primary input resource kind for this node, if any.
    pub(crate) fn input_resource_kind(self) -> Option<ResourceKind> {
        self.descriptor().input_resource_kind
    }

    /// Return true when this node kind can bind scalar signal parameters.
    pub(crate) fn accepts_signal_bindings(self) -> bool {
        self.descriptor().accepts_signal_bindings
    }

    /// Return true when this node kind has a scalar signal output pin.
    pub(crate) fn produces_signal_output(self) -> bool {
        self.output_resource_kind() == Some(ResourceKind::Signal)
    }

    /// Return whether this node should render the inline data-signal preview field.
    ///
    /// The preview field is reserved for data-signal producers (for example
    /// `ctl.lfo`) and should stay hidden on texture/buffer/scene/output nodes.
    pub(crate) fn shows_signal_preview(self) -> bool {
        self.descriptor().shows_signal_preview
    }

    /// Return true when this node kind has a typed graph input pin.
    pub(crate) fn has_input_pin(self) -> bool {
        self.input_resource_kind().is_some()
    }

    /// Return true when this node kind has any output pin.
    pub(crate) fn has_output_pin(self) -> bool {
        self.output_resource_kind().is_some()
    }

    /// Return output resource kind when this node publishes one.
    pub(crate) fn output_resource_kind(self) -> Option<ResourceKind> {
        self.descriptor().output_resource_kind
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
    #[cfg(test)]
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
    node_bin_keys_by_node: HashMap<u32, Vec<i64>>,
    output_pin_bin_key_by_node: HashMap<u32, i64>,
    input_pin_bin_key_by_node: HashMap<u32, i64>,
}

/// Combined hover hit-test result for one cursor position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HoverHitResult {
    /// Topmost node card under the cursor, if any.
    pub(crate) node_id: Option<u32>,
    /// Topmost node output pin within hover radius, if any.
    pub(crate) output_pin_node_id: Option<u32>,
    /// Topmost node input pin within hover radius, if any.
    pub(crate) input_pin_node_id: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinHitKind {
    Output,
    Input,
}

mod geometry;
mod options;
pub(crate) mod param_schema;
mod params;
mod signatures;
mod state;
#[cfg(test)]
mod tests;

pub(crate) use geometry::{
    collapsed_param_entry_pin_center, input_pin_center, node_expand_toggle_rect,
    node_param_dropdown_rect, node_param_row_rect, node_param_value_rect, output_pin_center,
    pin_rect,
};

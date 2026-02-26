//! GUI project scaffolding and editable node model.
//!
//! The GUI currently starts with an empty project and supports adding and
//! moving nodes directly on the graph canvas.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Width of one graph node card in the editor canvas.
pub(crate) const NODE_WIDTH: i32 = 208;
/// Height of one graph node card in the editor canvas.
pub(crate) const NODE_HEIGHT: i32 = 44;
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
const HIT_BIN_SIZE: i32 = 128;
const PERSISTED_GUI_PROJECT_VERSION: u32 = 1;
const TEXTURE_TARGET_PLACEHOLDER: &str = "none";

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
    /// `tex.feedback` one-frame delayed texture feedback node.
    TexFeedback,
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
            Self::TexFeedback => "tex.feedback",
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
            "tex.feedback" => Some(Self::TexFeedback),
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
            Self::TexFeedback => ExecutionKind::Render,
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
            Self::TexTransform2D | Self::TexFeedback | Self::IoWindowOut => {
                Some(ResourceKind::Texture2D)
            }
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
                | Self::TexFeedback
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
            | Self::TexFeedback
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
        }
    }

    /// Return true when this parameter binds one texture node source id.
    pub(crate) const fn is_texture_target(self) -> bool {
        matches!(self, Self::TextureTarget)
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

/// Read-only parameter view for rendering node UI.
#[derive(Clone, Debug)]
pub(crate) struct NodeParamView<'a> {
    pub(crate) label: &'a str,
    pub(crate) value_text: &'a str,
    pub(crate) bound: bool,
    pub(crate) selected: bool,
    pub(crate) dropdown: bool,
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
        NODE_HEIGHT + (self.params.len() as i32 * NODE_PARAM_ROW_HEIGHT) + NODE_PARAM_FOOTER_PAD
    }

    /// Return number of editable parameters for this node.
    pub(crate) fn param_count(&self) -> usize {
        self.params.len()
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
    next_node_id: u32,
    edge_count: usize,
    hit_test_cache: RefCell<HitTestCache>,
    hit_test_dirty: Cell<bool>,
    hit_test_scan_count: Cell<u64>,
}

/// Cached spatial/index structures for fast graph hit-testing.
#[derive(Clone, Debug, Default)]
struct HitTestCache {
    node_index_by_id: HashMap<u32, usize>,
    node_bins: HashMap<i64, Vec<u32>>,
    output_pin_bins: HashMap<i64, Vec<u32>>,
    input_pin_bins: HashMap<i64, Vec<u32>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinHitKind {
    Output,
    Input,
}

impl GuiProject {
    /// Create a fresh empty project sized for the active preview canvas.
    pub(crate) fn new_empty(preview_width: u32, preview_height: u32) -> Self {
        Self {
            name: next_project_name(),
            preview_width,
            preview_height,
            nodes: Vec::new(),
            next_node_id: 1,
            edge_count: 0,
            hit_test_cache: RefCell::new(HitTestCache::default()),
            hit_test_dirty: Cell::new(false),
            hit_test_scan_count: Cell::new(0),
        }
    }

    /// Export this in-memory graph to a persisted autosave payload.
    pub(crate) fn to_persisted(&self) -> PersistedGuiProject {
        let nodes = self
            .nodes
            .iter()
            .map(|node| PersistedGuiNode {
                id: node.id,
                kind: node.kind.stable_id().to_string(),
                x: node.x,
                y: node.y,
                texture_input: node.texture_input,
                selected_param: node.selected_param,
                expanded: node.expanded,
                params: node
                    .params
                    .iter()
                    .map(|slot| PersistedGuiParam {
                        key: slot.key.to_string(),
                        value: slot.value,
                        signal_source: slot.signal_source,
                        texture_source: slot.texture_source,
                    })
                    .collect(),
            })
            .collect();
        PersistedGuiProject {
            version: PERSISTED_GUI_PROJECT_VERSION,
            name: self.name.clone(),
            preview_width: self.preview_width,
            preview_height: self.preview_height,
            nodes,
        }
    }

    /// Reconstruct one GUI project from a persisted autosave payload.
    pub(crate) fn from_persisted(
        persisted: PersistedGuiProject,
        panel_width: usize,
        panel_height: usize,
    ) -> Result<Self, PersistedProjectLoadError> {
        if persisted.version != PERSISTED_GUI_PROJECT_VERSION {
            return Err(PersistedProjectLoadError::new(format!(
                "unsupported gui autosave version {}; expected {}",
                persisted.version, PERSISTED_GUI_PROJECT_VERSION
            )));
        }
        let mut project = GuiProject::new_empty(
            persisted.preview_width.max(1),
            persisted.preview_height.max(1),
        );
        project.name = persisted.name;
        let mut nodes = persisted.nodes;
        nodes.sort_by_key(|node| node.id);
        let mut id_map = HashMap::new();

        for persisted_node in &nodes {
            let kind =
                ProjectNodeKind::from_stable_id(persisted_node.kind.as_str()).ok_or_else(|| {
                    PersistedProjectLoadError::new(format!(
                        "unknown node kind '{}'",
                        persisted_node.kind
                    ))
                })?;
            if id_map.contains_key(&persisted_node.id) {
                return Err(PersistedProjectLoadError::new(format!(
                    "duplicate persisted node id {}",
                    persisted_node.id
                )));
            }
            let node_id = project.add_node(
                kind,
                persisted_node.x,
                persisted_node.y,
                panel_width,
                panel_height,
            );
            id_map.insert(persisted_node.id, node_id);
            let Some(node) = project.node_mut(node_id) else {
                continue;
            };
            for persisted_param in &persisted_node.params {
                let Some(slot) = node
                    .params
                    .iter_mut()
                    .find(|slot| slot.key == persisted_param.key.as_str())
                else {
                    continue;
                };
                let _ = set_slot_value(slot, persisted_param.value);
            }
            node.selected_param = persisted_node
                .selected_param
                .min(node.params.len().saturating_sub(1));
            node.expanded = persisted_node.expanded && !node.params.is_empty();
        }

        for persisted_node in &nodes {
            let Some(target_id) = id_map.get(&persisted_node.id).copied() else {
                continue;
            };
            if let Some(source_old_id) = persisted_node.texture_input {
                if let Some(source_id) = id_map.get(&source_old_id).copied() {
                    let _ = project.connect_image_link(source_id, target_id);
                }
            }
            for persisted_param in &persisted_node.params {
                let Some(source_old_id) = persisted_param.signal_source else {
                    continue;
                };
                let Some(source_id) = id_map.get(&source_old_id).copied() else {
                    continue;
                };
                let Some(param_index) = project.node(target_id).and_then(|target| {
                    target
                        .params
                        .iter()
                        .position(|slot| slot.key == persisted_param.key.as_str())
                }) else {
                    continue;
                };
                let _ = project.connect_signal_link_to_param(source_id, target_id, param_index);
            }
            for persisted_param in &persisted_node.params {
                let Some(source_old_id) = persisted_param.texture_source else {
                    continue;
                };
                let Some(source_id) = id_map.get(&source_old_id).copied() else {
                    continue;
                };
                let Some(param_index) = project.node(target_id).and_then(|target| {
                    target
                        .params
                        .iter()
                        .position(|slot| slot.key == persisted_param.key.as_str())
                }) else {
                    continue;
                };
                let _ = project.connect_texture_link_to_param(source_id, target_id, param_index);
            }
        }

        project.recount_edges();
        project.invalidate_hit_test_cache();
        Ok(project)
    }

    /// Return immutable node slice for rendering.
    pub(crate) fn nodes(&self) -> &[ProjectNode] {
        &self.nodes
    }

    /// Return current node count.
    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Return and reset accumulated hit-test scan count since last call.
    pub(crate) fn take_hit_test_scan_count(&self) -> u64 {
        let count = self.hit_test_scan_count.get();
        self.hit_test_scan_count.set(0);
        count
    }

    /// Return total input-edge count currently stored in this project.
    pub(crate) fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Return immutable node by id.
    pub(crate) fn node(&self, node_id: u32) -> Option<&ProjectNode> {
        let index = self.node_index(node_id)?;
        self.nodes.get(index)
    }

    /// Return mutable node by id.
    fn node_mut(&mut self, node_id: u32) -> Option<&mut ProjectNode> {
        let index = self.node_index(node_id)?;
        self.nodes.get_mut(index)
    }

    fn node_index(&self, node_id: u32) -> Option<usize> {
        self.ensure_hit_test_cache();
        self.hit_test_cache
            .borrow()
            .node_index_by_id
            .get(&node_id)
            .copied()
    }

    fn invalidate_hit_test_cache(&self) {
        self.hit_test_dirty.set(true);
    }

    fn ensure_hit_test_cache(&self) {
        if !self.hit_test_dirty.get() {
            return;
        }
        let mut cache = HitTestCache::default();
        for (index, node) in self.nodes.iter().enumerate() {
            cache.node_index_by_id.insert(node.id(), index);
            cache_node_rect_bins(
                &mut cache.node_bins,
                node.id(),
                node.x(),
                node.y(),
                node.card_height(),
            );
            if let Some((x, y)) = output_pin_center(node) {
                cache_pin_bin(&mut cache.output_pin_bins, node.id(), x, y);
            }
            if let Some((x, y)) = input_pin_center(node) {
                cache_pin_bin(&mut cache.input_pin_bins, node.id(), x, y);
            }
        }
        *self.hit_test_cache.borrow_mut() = cache;
        self.hit_test_dirty.set(false);
    }

    /// Add one node at canvas position and return created id.
    pub(crate) fn add_node(
        &mut self,
        kind: ProjectNodeKind,
        x: i32,
        y: i32,
        panel_width: usize,
        panel_height: usize,
    ) -> u32 {
        let params = default_params_for_kind(kind);
        let card_h = node_card_height_for_param_count(false, params.len());
        let (x, y) = clamp_node_position(x, y, panel_width, panel_height, card_h);
        let node_id = self.next_node_id;
        self.next_node_id = self.next_node_id.saturating_add(1);
        self.nodes.push(ProjectNode {
            id: node_id,
            kind,
            x,
            y,
            texture_input: None,
            inputs: Vec::new(),
            params,
            selected_param: 0,
            expanded: false,
        });
        self.invalidate_hit_test_cache();
        node_id
    }

    /// Move one node in graph space.
    ///
    /// Returns `true` when the node position changed.
    pub(crate) fn move_node(
        &mut self,
        node_id: u32,
        x: i32,
        y: i32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let Some(index) = self.node_index(node_id) else {
            return false;
        };
        let changed = {
            let node = &mut self.nodes[index];
            let (x, y) = clamp_node_position(x, y, panel_width, panel_height, node.card_height());
            if node.x == x && node.y == y {
                false
            } else {
                node.x = x;
                node.y = y;
                true
            }
        };
        if changed {
            self.invalidate_hit_test_cache();
        }
        changed
    }

    /// Return top-most node id at the given panel-space position.
    pub(crate) fn node_at(&self, x: i32, y: i32) -> Option<u32> {
        self.ensure_hit_test_cache();
        let key = hit_bin_key_for_point(x, y);
        let cache = self.hit_test_cache.borrow();
        let candidates = cache.node_bins.get(&key)?;
        for node_id in candidates.iter().rev() {
            self.bump_hit_test_scan_count(1);
            let Some(index) = cache.node_index_by_id.get(node_id).copied() else {
                continue;
            };
            let Some(node) = self.nodes.get(index) else {
                continue;
            };
            if x >= node.x()
                && x < node.x() + NODE_WIDTH
                && y >= node.y()
                && y < node.y() + node.card_height()
            {
                return Some(*node_id);
            }
        }
        None
    }

    /// Return world-space graph bounds for all current nodes.
    pub(crate) fn graph_bounds(&self) -> Option<GraphBounds> {
        let first = self.nodes.first()?;
        let mut min_x = first.x();
        let mut min_y = first.y();
        let mut max_x = first.x() + NODE_WIDTH;
        let mut max_y = first.y() + first.card_height();
        for node in self.nodes.iter().skip(1) {
            min_x = min_x.min(node.x());
            min_y = min_y.min(node.y());
            max_x = max_x.max(node.x() + NODE_WIDTH);
            max_y = max_y.max(node.y() + node.card_height());
        }
        Some(GraphBounds {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    /// Return the node id whose output pin is hit by the cursor.
    pub(crate) fn output_pin_at(&self, x: i32, y: i32, radius_px: i32) -> Option<u32> {
        self.pin_at(x, y, radius_px, None, output_pin_center, PinHitKind::Output)
    }

    /// Return the node id whose input pin is hit by the cursor.
    pub(crate) fn input_pin_at(
        &self,
        x: i32,
        y: i32,
        radius_px: i32,
        disallow_source: Option<u32>,
    ) -> Option<u32> {
        self.pin_at(
            x,
            y,
            radius_px,
            disallow_source,
            input_pin_center,
            PinHitKind::Input,
        )
    }

    fn pin_at(
        &self,
        x: i32,
        y: i32,
        radius_px: i32,
        disallow_source: Option<u32>,
        center_for_node: fn(&ProjectNode) -> Option<(i32, i32)>,
        pin_kind: PinHitKind,
    ) -> Option<u32> {
        self.ensure_hit_test_cache();
        let radius_sq = radius_px.saturating_mul(radius_px);
        let min_x = x.saturating_sub(radius_px);
        let max_x = x.saturating_add(radius_px);
        let min_y = y.saturating_sub(radius_px);
        let max_y = y.saturating_add(radius_px);
        let mut seen = Vec::new();
        let mut hit = None;
        let mut hit_z = 0_usize;

        let cache = self.hit_test_cache.borrow();
        let bins = match pin_kind {
            PinHitKind::Output => &cache.output_pin_bins,
            PinHitKind::Input => &cache.input_pin_bins,
        };
        for by in hit_bin_coord(min_y)..=hit_bin_coord(max_y) {
            for bx in hit_bin_coord(min_x)..=hit_bin_coord(max_x) {
                let key = hit_bin_key(bx, by);
                let Some(candidates) = bins.get(&key) else {
                    continue;
                };
                for node_id in candidates.iter().rev() {
                    self.bump_hit_test_scan_count(1);
                    if Some(*node_id) == disallow_source || seen.contains(node_id) {
                        continue;
                    }
                    seen.push(*node_id);
                    let Some(index) = cache.node_index_by_id.get(node_id).copied() else {
                        continue;
                    };
                    let Some(node) = self.nodes.get(index) else {
                        continue;
                    };
                    let Some((px, py)) = center_for_node(node) else {
                        continue;
                    };
                    if distance_sq(x, y, px, py) <= radius_sq && (hit.is_none() || index >= hit_z) {
                        hit = Some(*node_id);
                        hit_z = index;
                    }
                }
            }
        }
        hit
    }

    /// Connect one source node output pin to one target node input pin.
    ///
    /// Data-plane links (`Buffer`, `Entity`, `Scene`, `Texture2D`) replace the
    /// target primary input slot. Signal links bind to the target's currently
    /// selected parameter slot.
    ///
    /// Returns `true` when graph wiring changed.
    pub(crate) fn connect_image_link(&mut self, source_id: u32, target_id: u32) -> bool {
        if source_id == target_id {
            return false;
        }
        if self.depends_on(source_id, target_id) {
            // Reject links that would introduce a cycle.
            return false;
        }
        let Some(source) = self.node(source_id) else {
            return false;
        };
        if self.node(target_id).is_none() {
            return false;
        }
        let Some(source_kind) = source.kind().output_resource_kind() else {
            return false;
        };
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let changed = match source_kind {
            ResourceKind::Buffer
            | ResourceKind::Entity
            | ResourceKind::Scene
            | ResourceKind::Texture2D => {
                if target.kind.input_resource_kind() != Some(source_kind) {
                    return false;
                }
                if target.texture_input == Some(source_id) {
                    false
                } else {
                    target.texture_input = Some(source_id);
                    true
                }
            }
            ResourceKind::Signal => {
                if !target.kind.accepts_signal_bindings() || target.params.is_empty() {
                    return false;
                }
                let param_index = target
                    .selected_param
                    .min(target.params.len().saturating_sub(1));
                let slot = &mut target.params[param_index];
                if slot.widget.is_texture_target() {
                    return false;
                }
                if slot.signal_source == Some(source_id) {
                    false
                } else {
                    slot.signal_source = Some(source_id);
                    true
                }
            }
        };
        if !changed {
            return false;
        }
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Connect one signal source node to one explicit target parameter row.
    ///
    /// Returns `true` when the target parameter binding changed.
    pub(crate) fn connect_signal_link_to_param(
        &mut self,
        source_id: u32,
        target_id: u32,
        param_index: usize,
    ) -> bool {
        if source_id == target_id {
            return false;
        }
        if self.depends_on(source_id, target_id) {
            // Reject links that would introduce a cycle.
            return false;
        }
        let Some(source) = self.node(source_id) else {
            return false;
        };
        if source.kind().output_resource_kind() != Some(ResourceKind::Signal) {
            return false;
        }
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        if !target.kind.accepts_signal_bindings() || target.params.is_empty() {
            return false;
        }
        let index = param_index.min(target.params.len().saturating_sub(1));
        let slot = &mut target.params[index];
        if slot.widget.is_texture_target() {
            return false;
        }
        if slot.signal_source == Some(source_id) {
            return false;
        }
        slot.signal_source = Some(source_id);
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Connect one texture source node to one explicit texture-target parameter row.
    ///
    /// Returns `true` when the target parameter binding changed.
    pub(crate) fn connect_texture_link_to_param(
        &mut self,
        source_id: u32,
        target_id: u32,
        param_index: usize,
    ) -> bool {
        if source_id == target_id {
            return false;
        }
        if self.depends_on(source_id, target_id) {
            return false;
        }
        let Some(source) = self.node(source_id) else {
            return false;
        };
        if source.kind().output_resource_kind() != Some(ResourceKind::Texture2D) {
            return false;
        }
        let source_label = texture_source_display_label(source);
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let index = param_index.min(target.params.len().saturating_sub(1));
        let Some(slot) = target.params.get_mut(index) else {
            return false;
        };
        if !slot.widget.is_texture_target() {
            return false;
        }
        let changed = bind_texture_target_slot(slot, Some((source_id, source_label)));
        if !changed {
            return false;
        }
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Insert one node on an existing primary input link.
    ///
    /// Replaces `source_id -> target_id` with
    /// `source_id -> insert_node_id -> target_id` when all resource kinds are
    /// compatible and the original primary link exists.
    pub(crate) fn insert_node_on_primary_link(
        &mut self,
        insert_node_id: u32,
        source_id: u32,
        target_id: u32,
    ) -> bool {
        if insert_node_id == source_id || insert_node_id == target_id || source_id == target_id {
            return false;
        }
        let Some(source) = self.node(source_id) else {
            return false;
        };
        let Some(insert) = self.node(insert_node_id) else {
            return false;
        };
        let Some(target) = self.node(target_id) else {
            return false;
        };
        if target.texture_input != Some(source_id) {
            return false;
        }
        let Some(source_out_kind) = source.kind.output_resource_kind() else {
            return false;
        };
        let Some(insert_in_kind) = insert.kind.input_resource_kind() else {
            return false;
        };
        let Some(insert_out_kind) = insert.kind.output_resource_kind() else {
            return false;
        };
        let Some(target_in_kind) = target.kind.input_resource_kind() else {
            return false;
        };
        if source_out_kind != insert_in_kind || insert_out_kind != target_in_kind {
            return false;
        }
        if self.depends_on(source_id, insert_node_id) || self.depends_on(insert_node_id, target_id)
        {
            return false;
        }
        let mut changed = false;
        let Some(insert) = self.node_mut(insert_node_id) else {
            return false;
        };
        changed |= set_node_primary_input(insert, Some(source_id));
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        changed |= set_node_primary_input(target, Some(insert_node_id));
        if !changed {
            return false;
        }
        self.recount_edges();
        true
    }

    /// Disconnect one explicit source -> target link.
    ///
    /// Removes texture-input, texture-parameter, and signal-parameter bindings
    /// that match the source/target pair.
    pub(crate) fn disconnect_link(&mut self, source_id: u32, target_id: u32) -> bool {
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let mut changed = false;
        if target.texture_input == Some(source_id) {
            target.texture_input = None;
            changed = true;
        }
        for slot in &mut target.params {
            if slot.signal_source == Some(source_id) {
                slot.signal_source = None;
                changed = true;
            }
            if slot.texture_source == Some(source_id) {
                changed |= bind_texture_target_slot(slot, None);
            }
        }
        if !changed {
            return false;
        }
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Delete all nodes in `node_ids` and remove any links that referenced them.
    ///
    /// When possible, this also rewires surviving downstream links to the
    /// nearest surviving upstream source from the deleted chain so linear
    /// pipelines stay connected after node removal.
    ///
    /// Returns `true` when at least one node was removed.
    pub(crate) fn delete_nodes(&mut self, node_ids: &[u32]) -> bool {
        if node_ids.is_empty() {
            return false;
        }
        let mut removed_ids = node_ids.to_vec();
        removed_ids.sort_unstable();
        removed_ids.dedup();
        let removed_primary_inputs =
            collect_removed_primary_inputs(self.nodes.as_slice(), removed_ids.as_slice());
        let before_len = self.nodes.len();
        self.nodes
            .retain(|node| !contains_sorted_id(removed_ids.as_slice(), node.id()));
        let removed_any = self.nodes.len() != before_len;
        let output_kinds = collect_output_kinds(self.nodes.as_slice());
        let output_labels = collect_output_labels(self.nodes.as_slice());
        let mut links_changed = false;
        for node in &mut self.nodes {
            links_changed |= rewire_or_clear_deleted_links(
                node,
                removed_ids.as_slice(),
                &removed_primary_inputs,
                &output_kinds,
                &output_labels,
            );
        }
        if !removed_any && !links_changed {
            return false;
        }
        if removed_any {
            self.invalidate_hit_test_cache();
        }
        self.recount_edges();
        true
    }

    /// Return source node id wired into the first `io.window_out` node, if any.
    pub(crate) fn window_out_input_node_id(&self) -> Option<u32> {
        let output = self
            .nodes
            .iter()
            .find(|node| matches!(node.kind, ProjectNodeKind::IoWindowOut))?;
        output.inputs.first().copied()
    }

    /// Return first input source node id for one node.
    pub(crate) fn input_source_node_id(&self, node_id: u32) -> Option<u32> {
        self.node(node_id)?.texture_input
    }

    /// Return resource kind for one explicit source -> target link.
    ///
    /// Returns `None` when no such link exists.
    pub(crate) fn link_resource_kind(
        &self,
        source_id: u32,
        target_id: u32,
    ) -> Option<ResourceKind> {
        let target = self.node(target_id)?;
        if target.texture_input == Some(source_id) {
            let source = self.node(source_id)?;
            return source.kind().output_resource_kind();
        }
        if target
            .params
            .iter()
            .any(|slot| slot.texture_source == Some(source_id))
        {
            return Some(ResourceKind::Texture2D);
        }
        if target
            .params
            .iter()
            .any(|slot| slot.signal_source == Some(source_id))
        {
            return Some(ResourceKind::Signal);
        }
        None
    }

    /// Return parameter index bound from `source_id` into `target_id`, if any.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn signal_param_index_for_source(
        &self,
        source_id: u32,
        target_id: u32,
    ) -> Option<usize> {
        let target = self.node(target_id)?;
        target
            .params
            .iter()
            .position(|slot| slot.signal_source == Some(source_id))
    }

    /// Return signal source node id bound to one target parameter row, if any.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn signal_source_for_param(
        &self,
        target_id: u32,
        param_index: usize,
    ) -> Option<u32> {
        let target = self.node(target_id)?;
        let slot = target.params.get(param_index)?;
        slot.signal_source
    }

    /// Return texture source node id bound to one target parameter row, if any.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn texture_source_for_param(
        &self,
        target_id: u32,
        param_index: usize,
    ) -> Option<u32> {
        let target = self.node(target_id)?;
        let slot = target.params.get(param_index)?;
        slot.texture_source
    }

    /// Return one bound source id/kind for a target parameter row, if any.
    pub(crate) fn param_link_source_for_param(
        &self,
        target_id: u32,
        param_index: usize,
    ) -> Option<(u32, ResourceKind)> {
        let target = self.node(target_id)?;
        let slot = target.params.get(param_index)?;
        if let Some(source) = slot.texture_source {
            return Some((source, ResourceKind::Texture2D));
        }
        slot.signal_source
            .map(|source| (source, ResourceKind::Signal))
    }

    /// Return texture source node id bound to one parameter key, if any.
    pub(crate) fn texture_source_for_param_key(
        &self,
        target_id: u32,
        key: &'static str,
    ) -> Option<u32> {
        let target = self.node(target_id)?;
        let slot = target.params.iter().find(|slot| slot.key == key)?;
        slot.texture_source
    }

    /// Disconnect one explicit signal binding from a target parameter row.
    ///
    /// Returns `true` when the target parameter binding changed.
    pub(crate) fn disconnect_signal_link_from_param(
        &mut self,
        target_id: u32,
        param_index: usize,
    ) -> bool {
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let Some(slot) = target.params.get_mut(param_index) else {
            return false;
        };
        if slot.signal_source.is_none() {
            return false;
        }
        slot.signal_source = None;
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Disconnect one explicit texture binding from a target parameter row.
    ///
    /// Returns `true` when the target parameter binding changed.
    pub(crate) fn disconnect_texture_link_from_param(
        &mut self,
        target_id: u32,
        param_index: usize,
    ) -> bool {
        let Some(target) = self.node_mut(target_id) else {
            return false;
        };
        let Some(slot) = target.params.get_mut(param_index) else {
            return false;
        };
        if slot.texture_source.is_none() {
            return false;
        }
        if !bind_texture_target_slot(slot, None) {
            return false;
        }
        rebuild_node_inputs(target);
        self.recount_edges();
        true
    }

    /// Disconnect any explicit parameter link (signal or texture) from one row.
    ///
    /// Returns `true` when the target parameter binding changed.
    pub(crate) fn disconnect_param_link_from_param(
        &mut self,
        target_id: u32,
        param_index: usize,
    ) -> bool {
        if self.disconnect_signal_link_from_param(target_id, param_index) {
            return true;
        }
        self.disconnect_texture_link_from_param(target_id, param_index)
    }

    /// Toggle one node expanded/collapsed state.
    ///
    /// Returns `true` when expanded state changed.
    pub(crate) fn toggle_node_expanded(
        &mut self,
        node_id: u32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let Some(index) = self.node_index(node_id) else {
            return false;
        };
        {
            let node = &mut self.nodes[index];
            if node.params.is_empty() {
                return false;
            }
            node.expanded = !node.expanded;
            let card_h = node.card_height();
            let (x, y) = clamp_node_position(node.x, node.y, panel_width, panel_height, card_h);
            node.x = x;
            node.y = y;
        }
        self.invalidate_hit_test_cache();
        true
    }

    /// Expand one node without toggling when it supports parameter rows.
    ///
    /// Returns `true` when expanded state changed.
    pub(crate) fn expand_node(
        &mut self,
        node_id: u32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let Some(index) = self.node_index(node_id) else {
            return false;
        };
        {
            let node = &mut self.nodes[index];
            if node.params.is_empty() || node.expanded {
                return false;
            }
            node.expanded = true;
            let card_h = node.card_height();
            let (x, y) = clamp_node_position(node.x, node.y, panel_width, panel_height, card_h);
            node.x = x;
            node.y = y;
        }
        self.invalidate_hit_test_cache();
        true
    }

    /// Collapse one node without toggling when it is currently expanded.
    ///
    /// Returns `true` when expanded state changed.
    pub(crate) fn collapse_node(
        &mut self,
        node_id: u32,
        panel_width: usize,
        panel_height: usize,
    ) -> bool {
        let Some(index) = self.node_index(node_id) else {
            return false;
        };
        {
            let node = &mut self.nodes[index];
            if node.params.is_empty() || !node.expanded {
                return false;
            }
            node.expanded = false;
            let card_h = node.card_height();
            let (x, y) = clamp_node_position(node.x, node.y, panel_width, panel_height, card_h);
            node.x = x;
            node.y = y;
        }
        self.invalidate_hit_test_cache();
        true
    }

    /// Advance selected parameter row for one node.
    pub(crate) fn select_next_param(&mut self, node_id: u32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let max = node.params.len().saturating_sub(1);
        let next = (node.selected_param + 1).min(max);
        if next == node.selected_param {
            return false;
        }
        node.selected_param = next;
        true
    }

    /// Move selected parameter row up for one node.
    pub(crate) fn select_prev_param(&mut self, node_id: u32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() || node.selected_param == 0 {
            return false;
        }
        node.selected_param -= 1;
        true
    }

    /// Select one parameter row by index for one node.
    pub(crate) fn select_param(&mut self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let next = param_index.min(node.params.len().saturating_sub(1));
        if node.selected_param == next {
            return false;
        }
        node.selected_param = next;
        true
    }

    /// Adjust selected parameter value by one step.
    pub(crate) fn adjust_selected_param(&mut self, node_id: u32, direction: f32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let index = node.selected_param.min(node.params.len().saturating_sub(1));
        adjust_slot_value(&mut node.params[index], direction)
    }

    /// Adjust one parameter value by `steps * slot.step` after clamping.
    ///
    /// Returns `true` when the parameter value changed.
    pub(crate) fn adjust_param(&mut self, node_id: u32, param_index: usize, steps: f32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() || !steps.is_finite() {
            return false;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        adjust_slot_value(&mut node.params[index], steps)
    }

    /// Return raw parameter value at one index for one node.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn node_param_raw_value(&self, node_id: u32, param_index: usize) -> Option<f32> {
        let node = self.node(node_id)?;
        if node.params.is_empty() {
            return None;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        node.params.get(index).map(|slot| slot.value)
    }

    /// Set one parameter value at one index after clamping to slot limits.
    pub(crate) fn set_param_value(&mut self, node_id: u32, param_index: usize, value: f32) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        if node.params.is_empty() {
            return false;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        set_slot_value(&mut node.params[index], value)
    }

    /// Return true when a parameter row is rendered as dropdown.
    pub(crate) fn param_is_dropdown(&self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        let Some(slot) = node
            .params
            .get(param_index.min(node.params.len().saturating_sub(1)))
        else {
            return false;
        };
        slot.widget.is_dropdown()
    }

    /// Return true when a parameter row can be edited as free-form numeric text.
    pub(crate) fn param_supports_text_edit(&self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        let Some(slot) = node
            .params
            .get(param_index.min(node.params.len().saturating_sub(1)))
        else {
            return false;
        };
        !slot.widget.is_dropdown() && !slot.widget.is_texture_target()
    }

    /// Return true when one row accepts signal-source parameter bindings.
    pub(crate) fn param_accepts_signal_link(&self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        if !node.kind.accepts_signal_bindings() {
            return false;
        }
        let Some(slot) = node
            .params
            .get(param_index.min(node.params.len().saturating_sub(1)))
        else {
            return false;
        };
        !slot.widget.is_texture_target()
    }

    /// Return true when one row accepts texture-source parameter bindings.
    pub(crate) fn param_accepts_texture_link(&self, node_id: u32, param_index: usize) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        let Some(slot) = node
            .params
            .get(param_index.min(node.params.len().saturating_sub(1)))
        else {
            return false;
        };
        slot.widget.is_texture_target()
    }

    /// Return dropdown options for one parameter row.
    pub(crate) fn node_param_dropdown_options(
        &self,
        node_id: u32,
        param_index: usize,
    ) -> Option<&'static [NodeParamOption]> {
        let node = self.node(node_id)?;
        let index = param_index.min(node.params.len().saturating_sub(1));
        node.params.get(index)?.widget.dropdown_options()
    }

    /// Return selected dropdown option index for one parameter row.
    pub(crate) fn node_param_dropdown_selected_index(
        &self,
        node_id: u32,
        param_index: usize,
    ) -> Option<usize> {
        let node = self.node(node_id)?;
        let index = param_index.min(node.params.len().saturating_sub(1));
        dropdown_selected_index(node.params.get(index)?)
    }

    /// Select one dropdown option by index for one parameter row.
    pub(crate) fn set_param_dropdown_index(
        &mut self,
        node_id: u32,
        param_index: usize,
        option_index: usize,
    ) -> bool {
        let Some(node) = self.node_mut(node_id) else {
            return false;
        };
        let index = param_index.min(node.params.len().saturating_sub(1));
        let Some(slot) = node.params.get_mut(index) else {
            return false;
        };
        let Some(options) = slot.widget.dropdown_options() else {
            return false;
        };
        if options.is_empty() {
            return false;
        }
        let next_index = option_index.min(options.len().saturating_sub(1));
        apply_dropdown_value(slot, options, next_index)
    }

    /// Return expanded parameter row index hit by one graph-space point.
    pub(crate) fn param_row_at(&self, node_id: u32, x: i32, y: i32) -> Option<usize> {
        let node = self.node(node_id)?;
        if !node.expanded() {
            return None;
        }
        for index in 0..node.params.len() {
            let Some(rect) = node_param_row_rect(node, index) else {
                continue;
            };
            if rect.contains(x, y) {
                return Some(index);
            }
        }
        None
    }

    /// Return true when graph-space point falls inside one value input box.
    pub(crate) fn param_value_box_contains(
        &self,
        node_id: u32,
        param_index: usize,
        x: i32,
        y: i32,
    ) -> bool {
        let Some(node) = self.node(node_id) else {
            return false;
        };
        let Some(rect) = node_param_value_rect(node, param_index) else {
            return false;
        };
        rect.contains(x, y)
    }

    /// Return cached formatted parameter text at one index for one node.
    pub(crate) fn node_param_raw_text(&self, node_id: u32, param_index: usize) -> Option<&str> {
        let node = self.node(node_id)?;
        if node.params.is_empty() {
            return None;
        }
        let index = param_index.min(node.params.len().saturating_sub(1));
        node.params.get(index).map(|slot| slot.value_text.as_str())
    }

    /// Return true when a node is currently expanded.
    pub(crate) fn node_expanded(&self, node_id: u32) -> bool {
        self.node(node_id)
            .map(ProjectNode::expanded)
            .unwrap_or(false)
    }

    /// Return effective parameter value, resolving optional signal binding.
    pub(crate) fn node_param_value(
        &self,
        node_id: u32,
        key: &'static str,
        time_secs: f32,
        eval_stack: &mut Vec<u32>,
    ) -> Option<f32> {
        let node = self.node(node_id)?;
        let slot = node.params.iter().find(|slot| slot.key == key)?;
        let mut value = slot.value;
        if let Some(source_id) = slot.signal_source {
            if let Some(signal) = self.sample_signal_node(source_id, time_secs, eval_stack) {
                value = signal;
            }
        }
        Some(value.clamp(slot.min, slot.max))
    }

    /// Evaluate one scalar signal node output.
    pub(crate) fn sample_signal_node(
        &self,
        node_id: u32,
        time_secs: f32,
        eval_stack: &mut Vec<u32>,
    ) -> Option<f32> {
        if eval_stack.contains(&node_id) {
            return None;
        }
        let node = self.node(node_id)?;
        if !node.kind.produces_signal_output() {
            return None;
        }
        eval_stack.push(node_id);
        let rate = self
            .node_param_value(node_id, "rate_hz", time_secs, eval_stack)
            .unwrap_or(0.4);
        let amplitude = self
            .node_param_value(node_id, "amplitude", time_secs, eval_stack)
            .unwrap_or(0.5);
        let phase = self
            .node_param_value(node_id, "phase", time_secs, eval_stack)
            .unwrap_or(0.0);
        let bias = self
            .node_param_value(node_id, "bias", time_secs, eval_stack)
            .unwrap_or(0.5);
        let v = (time_secs * rate * std::f32::consts::TAU + phase * std::f32::consts::TAU).sin()
            * amplitude
            + bias;
        eval_stack.pop();
        Some(v)
    }

    /// Return stable signature for render-affecting graph state.
    ///
    /// This signature intentionally excludes UI-only fields such as expanded
    /// state and selected parameter row so preview caches only invalidate when
    /// output content can change.
    pub(crate) fn render_signature(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for node in &self.nodes {
            hash = fnv1a_u64(hash, node.id as u64);
            for byte in node.kind.stable_id().as_bytes() {
                hash = fnv1a_u64(hash, *byte as u64);
            }
            if let Some(texture_input) = node.texture_input {
                hash = fnv1a_u64(hash, texture_input as u64);
            }
            hash = fnv1a_u64(hash, 0xff);
            for slot in &node.params {
                for byte in slot.key.as_bytes() {
                    hash = fnv1a_u64(hash, *byte as u64);
                }
                hash = fnv1a_u64(hash, slot.value.to_bits() as u64);
                if let Some(source) = slot.signal_source {
                    hash = fnv1a_u64(hash, source as u64);
                }
                if let Some(source) = slot.texture_source {
                    hash = fnv1a_u64(hash, source as u64);
                }
            }
            hash = fnv1a_u64(hash, 0xfe);
        }
        hash
    }

    /// Return stable signature for UI-only node-editor state.
    ///
    /// This can be used by UI caches that should react to node-card expansion,
    /// row selection, or node position updates without affecting render caches.
    pub(crate) fn ui_signature(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for node in &self.nodes {
            hash = fnv1a_u64(hash, node.id as u64);
            hash = fnv1a_u64(hash, node.x as i64 as u64);
            hash = fnv1a_u64(hash, node.y as i64 as u64);
            hash = fnv1a_u64(hash, node.expanded as u64);
            hash = fnv1a_u64(hash, node.selected_param as u64);
            hash = fnv1a_u64(hash, 0xfd);
        }
        hash
    }

    /// Return stable signature for both render and UI graph state.
    ///
    /// Prefer [`Self::render_signature`] for TOP/render invalidation.
    pub(crate) fn graph_signature(&self) -> u64 {
        fnv1a_u64(self.render_signature(), self.ui_signature())
    }

    /// Return true when at least one parameter has a live signal binding.
    pub(crate) fn has_signal_bindings(&self) -> bool {
        self.nodes
            .iter()
            .any(|node| node.params.iter().any(|slot| slot.signal_source.is_some()))
    }

    /// Return true when the graph contains time-driven nodes.
    ///
    /// This includes nodes that change output over time without explicit
    /// signal bindings, such as feedback and buffer noise deformation.
    pub(crate) fn has_temporal_nodes(&self) -> bool {
        self.nodes.iter().any(|node| {
            matches!(
                node.kind,
                ProjectNodeKind::TexFeedback | ProjectNodeKind::BufNoise
            )
        })
    }

    fn depends_on(&self, start_node_id: u32, target_node_id: u32) -> bool {
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
            if let Some(node) = self.node(node_id) {
                stack.extend(node.inputs.iter().copied());
            }
        }
        false
    }

    fn recount_edges(&mut self) {
        self.edge_count = self.nodes.iter().map(|node| node.inputs.len()).sum();
    }

    fn bump_hit_test_scan_count(&self, delta: u64) {
        let next = self.hit_test_scan_count.get().saturating_add(delta);
        self.hit_test_scan_count.set(next);
    }
}

/// Return panel-space center of a node output pin.
pub(crate) fn output_pin_center(node: &ProjectNode) -> Option<(i32, i32)> {
    if !node.kind().has_output_pin() {
        return None;
    }
    let x = node.x() + NODE_WIDTH - 1;
    let y = node.y() + (node.card_height() / 2);
    Some((x, y))
}

/// Return panel-space center of a node input pin.
pub(crate) fn input_pin_center(node: &ProjectNode) -> Option<(i32, i32)> {
    if !node.kind().has_input_pin() {
        return None;
    }
    let x = node.x();
    let y = node.y() + (node.card_height() / 2);
    Some((x, y))
}

/// Return one pin rectangle centered around a pin position.
pub(crate) fn pin_rect(cx: i32, cy: i32) -> super::geometry::Rect {
    super::geometry::Rect::new(
        cx - NODE_PIN_HALF,
        cy - NODE_PIN_HALF,
        NODE_PIN_SIZE,
        NODE_PIN_SIZE,
    )
}

/// Return node header expand/collapse toggle rectangle in graph-space coordinates.
pub(crate) fn node_expand_toggle_rect(node: &ProjectNode) -> Option<super::geometry::Rect> {
    if !node.supports_expand_toggle() {
        return None;
    }
    Some(super::geometry::Rect::new(
        node.x() + NODE_TOGGLE_MARGIN,
        node.y() + NODE_TOGGLE_MARGIN,
        NODE_TOGGLE_SIZE,
        NODE_TOGGLE_SIZE,
    ))
}

/// Return one parameter row rectangle in graph-space coordinates.
pub(crate) fn node_param_row_rect(
    node: &ProjectNode,
    param_index: usize,
) -> Option<super::geometry::Rect> {
    if !node.expanded() || param_index >= node.params.len() {
        return None;
    }
    let row_y = node.y() + NODE_HEIGHT + param_index as i32 * NODE_PARAM_ROW_HEIGHT;
    Some(super::geometry::Rect::new(
        node.x() + NODE_PARAM_ROW_PAD_X,
        row_y,
        NODE_WIDTH - NODE_PARAM_ROW_PAD_X * 2,
        NODE_PARAM_ROW_HEIGHT,
    ))
}

/// Return one parameter value input box rectangle in graph-space coordinates.
pub(crate) fn node_param_value_rect(
    node: &ProjectNode,
    param_index: usize,
) -> Option<super::geometry::Rect> {
    let row = node_param_row_rect(node, param_index)?;
    let width = NODE_PARAM_VALUE_BOX_WIDTH
        .min(row.w.saturating_sub(8))
        .max(20);
    let x = row.x + row.w - width - NODE_PARAM_VALUE_BOX_RIGHT_PAD;
    Some(super::geometry::Rect::new(
        x,
        row.y + 1,
        width,
        row.h.saturating_sub(2),
    ))
}

/// Return one parameter dropdown popup rectangle in graph-space coordinates.
pub(crate) fn node_param_dropdown_rect(
    node: &ProjectNode,
    param_index: usize,
    option_count: usize,
) -> Option<super::geometry::Rect> {
    if option_count == 0 {
        return None;
    }
    let value_rect = node_param_value_rect(node, param_index)?;
    Some(super::geometry::Rect::new(
        value_rect.x,
        value_rect.y + value_rect.h + 1,
        value_rect.w,
        option_count as i32 * NODE_PARAM_DROPDOWN_ROW_HEIGHT,
    ))
}

fn clamp_node_position(
    x: i32,
    y: i32,
    _panel_width: usize,
    _panel_height: usize,
    _node_height: i32,
) -> (i32, i32) {
    // Keep the call sites stable for now, but stop clamping node coordinates:
    // the graph canvas is intentionally unbounded.
    (x, y)
}

fn node_card_height_for_param_count(expanded: bool, param_count: usize) -> i32 {
    if !expanded || param_count == 0 {
        return NODE_HEIGHT;
    }
    NODE_HEIGHT + (param_count as i32 * NODE_PARAM_ROW_HEIGHT) + NODE_PARAM_FOOTER_PAD
}

fn default_params_for_kind(kind: ProjectNodeKind) -> Vec<NodeParamSlot> {
    match kind {
        ProjectNodeKind::TexSolid => vec![
            param("color_r", "color_r", 0.9, 0.0, 1.0, 0.01),
            param("color_g", "color_g", 0.9, 0.0, 1.0, 0.01),
            param("color_b", "color_b", 0.9, 0.0, 1.0, 0.01),
            param("alpha", "alpha", 1.0, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::TexCircle => vec![
            param("center_x", "center_x", 0.5, 0.0, 1.0, 0.01),
            param("center_y", "center_y", 0.5, 0.0, 1.0, 0.01),
            param("radius", "radius", 0.24, 0.02, 0.5, 0.005),
            param("feather", "feather", 0.06, 0.0, 0.25, 0.005),
            param("color_r", "color_r", 0.9, 0.0, 1.0, 0.01),
            param("color_g", "color_g", 0.9, 0.0, 1.0, 0.01),
            param("color_b", "color_b", 0.9, 0.0, 1.0, 0.01),
            param("alpha", "alpha", 1.0, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::BufSphere => vec![
            param("radius", "radius", 0.28, 0.02, 0.5, 0.005),
            param("segments", "segments", 32.0, 3.0, 128.0, 1.0),
            param("rings", "rings", 16.0, 2.0, 64.0, 1.0),
        ],
        ProjectNodeKind::BufCircleNurbs => vec![
            param("radius", "radius", 0.28, 0.02, 0.95, 0.005),
            param("arc_start", "arc_start", 0.0, 0.0, 360.0, 1.0),
            param("arc_end", "arc_end", 360.0, 0.0, 360.0, 1.0),
            param_dropdown("arc_style", "arc_style", 0, &BUF_CIRCLE_ARC_STYLE_OPTIONS),
            param("line_width", "line_width", 0.01, 0.0005, 0.35, 0.001),
            param("order", "order", 3.0, 2.0, 5.0, 1.0),
            param("divisions", "divisions", 64.0, 8.0, 512.0, 1.0),
        ],
        ProjectNodeKind::BufNoise => vec![
            // Keep deformation disabled by default so inserting this node is
            // identity until users increase amplitude.
            param("amplitude", "amplitude", 0.0, 0.0, 1.0, 0.01),
            param("frequency", "frequency", 2.0, 0.05, 32.0, 0.05),
            param("speed_hz", "speed_hz", 0.35, 0.0, 16.0, 0.05),
            param("phase", "phase", 0.0, -8.0, 8.0, 0.05),
            param("seed", "seed", 1.0, 0.0, 1024.0, 1.0),
            param("twist", "twist", 0.0, -8.0, 8.0, 0.05),
            param("stretch", "stretch", 0.0, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::TexTransform2D => vec![
            // Keep transform as identity by default so inserting this node
            // never changes output until the user edits parameters.
            param("brightness", "brightness", 1.0, 0.0, 64.0, 0.1),
            param("gain_r", "gain_r", 1.0, 0.0, 64.0, 0.1),
            param("gain_g", "gain_g", 1.0, 0.0, 64.0, 0.1),
            param("gain_b", "gain_b", 1.0, 0.0, 64.0, 0.1),
            param("alpha_mul", "alpha_mul", 1.0, 0.0, 64.0, 0.1),
        ],
        ProjectNodeKind::TexFeedback => vec![
            param_texture_target("target_tex", "target_tex"),
            param("feedback", "feedback", 0.95, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::SceneEntity => vec![
            param("pos_x", "pos_x", 0.5, 0.0, 1.0, 0.01),
            param("pos_y", "pos_y", 0.5, 0.0, 1.0, 0.01),
            param("scale", "scale", 1.0, 0.1, 2.0, 0.01),
            param("ambient", "ambient", 0.2, 0.0, 1.0, 0.01),
            param("color_r", "color_r", 0.9, 0.0, 1.0, 0.01),
            param("color_g", "color_g", 0.9, 0.0, 1.0, 0.01),
            param("color_b", "color_b", 0.9, 0.0, 1.0, 0.01),
            param("alpha", "alpha", 1.0, 0.0, 1.0, 0.01),
        ],
        ProjectNodeKind::SceneBuild => Vec::new(),
        ProjectNodeKind::RenderCamera => {
            vec![param("zoom", "zoom", 1.0, 0.1, 8.0, 0.05)]
        }
        ProjectNodeKind::RenderScenePass => vec![
            // `0` keeps project preview resolution.
            param("res_width", "res_width", 0.0, 0.0, 8192.0, 1.0),
            // `0` keeps project preview resolution.
            param("res_height", "res_height", 0.0, 0.0, 8192.0, 1.0),
            // `with_bg` preserves the preview background clear; `alpha_clip`
            // clears transparent so only rendered scene objects remain.
            param_dropdown("bg_mode", "bg_mode", 0, &SCENE_PASS_BG_MODE_OPTIONS),
            param("edge_softness", "edge_soft", 0.01, 0.0, 0.25, 0.005),
            param("light_x", "light_x", 0.4, -1.0, 1.0, 0.02),
            param("light_y", "light_y", -0.5, -1.0, 1.0, 0.02),
            param("light_z", "light_z", 1.0, 0.0, 2.0, 0.02),
        ],
        ProjectNodeKind::CtlLfo => vec![
            param("rate_hz", "rate_hz", 0.4, 0.0, 8.0, 0.05),
            param("amplitude", "amplitude", 0.5, 0.0, 64.0, 0.1),
            param("phase", "phase", 0.0, -1.0, 1.0, 0.02),
            param("bias", "bias", 0.5, -1.0, 1.0, 0.02),
        ],
        ProjectNodeKind::IoWindowOut => Vec::new(),
    }
}

fn param(
    key: &'static str,
    label: &'static str,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
) -> NodeParamSlot {
    assert_param_label_fits(label);
    NodeParamSlot {
        key,
        label,
        value,
        value_text: format_param_value_text(value),
        min,
        max,
        step,
        signal_source: None,
        texture_source: None,
        widget: NodeParamWidget::Number,
    }
}

/// Build one texture-target parameter slot.
fn param_texture_target(key: &'static str, label: &'static str) -> NodeParamSlot {
    assert_param_label_fits(label);
    NodeParamSlot {
        key,
        label,
        value: 0.0,
        value_text: texture_target_placeholder().to_string(),
        min: 0.0,
        max: 0.0,
        step: 0.0,
        signal_source: None,
        texture_source: None,
        widget: NodeParamWidget::TextureTarget,
    }
}

/// Build one dropdown-parameter slot.
fn param_dropdown(
    key: &'static str,
    label: &'static str,
    default_index: usize,
    options: &'static [NodeParamOption],
) -> NodeParamSlot {
    assert_param_label_fits(label);
    let index = default_index.min(options.len().saturating_sub(1));
    let selected = options.get(index).copied().unwrap_or(NodeParamOption {
        label: "n/a",
        value: 0.0,
    });
    let mut min = selected.value;
    let mut max = selected.value;
    for option in options {
        min = min.min(option.value);
        max = max.max(option.value);
    }
    NodeParamSlot {
        key,
        label,
        value: selected.value,
        value_text: selected.label.to_string(),
        min,
        max,
        step: 1.0,
        signal_source: None,
        texture_source: None,
        widget: NodeParamWidget::Dropdown { options },
    }
}

fn format_param_value_text(value: f32) -> String {
    format!("{value:.3}")
}

fn texture_target_placeholder() -> &'static str {
    TEXTURE_TARGET_PLACEHOLDER
}

fn assert_param_label_fits(label: &'static str) {
    assert!(
        label.len() <= PARAM_LABEL_MAX_LEN,
        "parameter label '{label}' exceeds {PARAM_LABEL_MAX_LEN} chars"
    );
}

fn bind_texture_target_slot(slot: &mut NodeParamSlot, source: Option<(u32, String)>) -> bool {
    if !slot.widget.is_texture_target() {
        return false;
    }
    let next_source = source.as_ref().map(|(source_id, _)| *source_id);
    let next_label = source
        .as_ref()
        .map(|(_, label)| label.as_str())
        .unwrap_or(texture_target_placeholder());
    if slot.texture_source == next_source && slot.value_text == next_label {
        return false;
    }
    slot.texture_source = next_source;
    slot.value_text.clear();
    slot.value_text.push_str(next_label);
    true
}

fn texture_source_display_label(source: &ProjectNode) -> String {
    format!("{}#{}", source.kind().label(), source.id())
}

/// Set one slot value while respecting widget semantics.
fn set_slot_value(slot: &mut NodeParamSlot, value: f32) -> bool {
    if slot.widget.is_texture_target() {
        return false;
    }
    if let Some(options) = slot.widget.dropdown_options() {
        if options.is_empty() {
            return false;
        }
        let next_index = nearest_dropdown_index(options, value);
        return apply_dropdown_value(slot, options, next_index);
    }
    let clamped = value.clamp(slot.min, slot.max);
    if (slot.value - clamped).abs() < 1e-6 {
        return false;
    }
    slot.value = clamped;
    slot.value_text = format_param_value_text(clamped);
    true
}

/// Adjust one slot by step count while respecting widget semantics.
fn adjust_slot_value(slot: &mut NodeParamSlot, steps: f32) -> bool {
    if !steps.is_finite() || steps.abs() <= f32::EPSILON {
        return false;
    }
    if slot.widget.is_texture_target() {
        return false;
    }
    if let Some(options) = slot.widget.dropdown_options() {
        if options.is_empty() {
            return false;
        }
        let direction = if steps.is_sign_positive() { 1 } else { -1 };
        let current = dropdown_selected_index(slot)
            .unwrap_or_else(|| nearest_dropdown_index(options, slot.value));
        let next = if direction > 0 {
            (current + 1).min(options.len().saturating_sub(1))
        } else {
            current.saturating_sub(1)
        };
        return apply_dropdown_value(slot, options, next);
    }
    let next = (slot.value + slot.step * steps).clamp(slot.min, slot.max);
    if (next - slot.value).abs() < 1e-6 {
        return false;
    }
    slot.value = next;
    slot.value_text = format_param_value_text(next);
    true
}

/// Return selected dropdown index for one slot, if any.
fn dropdown_selected_index(slot: &NodeParamSlot) -> Option<usize> {
    let options = slot.widget.dropdown_options()?;
    if options.is_empty() {
        return None;
    }
    let by_value = options
        .iter()
        .position(|option| (option.value - slot.value).abs() < 1e-6);
    Some(by_value.unwrap_or_else(|| nearest_dropdown_index(options, slot.value)))
}

/// Return nearest option index for one dropdown value.
fn nearest_dropdown_index(options: &[NodeParamOption], value: f32) -> usize {
    let mut best_index = 0usize;
    let mut best_dist = f32::MAX;
    for (index, option) in options.iter().enumerate() {
        let dist = (option.value - value).abs();
        if dist < best_dist {
            best_dist = dist;
            best_index = index;
        }
    }
    best_index
}

/// Apply one dropdown option index to a slot value/text.
fn apply_dropdown_value(
    slot: &mut NodeParamSlot,
    options: &[NodeParamOption],
    option_index: usize,
) -> bool {
    let Some(option) = options.get(option_index).copied() else {
        return false;
    };
    if (slot.value - option.value).abs() < 1e-6 && slot.value_text == option.label {
        return false;
    }
    slot.value = option.value;
    slot.value_text.clear();
    slot.value_text.push_str(option.label);
    true
}

/// Set one node primary input source and rebuild cached input list.
fn set_node_primary_input(node: &mut ProjectNode, source: Option<u32>) -> bool {
    if node.texture_input == source {
        return false;
    }
    node.texture_input = source;
    rebuild_node_inputs(node);
    true
}

fn rebuild_node_inputs(node: &mut ProjectNode) {
    node.inputs.clear();
    if let Some(texture_source) = node.texture_input {
        node.inputs.push(texture_source);
    }
    for slot in &node.params {
        let Some(texture_source) = slot.texture_source else {
            continue;
        };
        if !node.inputs.contains(&texture_source) {
            node.inputs.push(texture_source);
        }
    }
    for slot in &node.params {
        let Some(signal_source) = slot.signal_source else {
            continue;
        };
        if !node.inputs.contains(&signal_source) {
            node.inputs.push(signal_source);
        }
    }
}

fn collect_removed_primary_inputs(
    nodes: &[ProjectNode],
    removed_ids: &[u32],
) -> HashMap<u32, Option<u32>> {
    let mut out = HashMap::new();
    for node in nodes {
        if !contains_sorted_id(removed_ids, node.id()) {
            continue;
        }
        out.insert(node.id(), node.texture_input);
    }
    out
}

fn collect_output_kinds(nodes: &[ProjectNode]) -> HashMap<u32, ResourceKind> {
    let mut out = HashMap::new();
    for node in nodes {
        let Some(kind) = node.kind.output_resource_kind() else {
            continue;
        };
        out.insert(node.id(), kind);
    }
    out
}

fn collect_output_labels(nodes: &[ProjectNode]) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    for node in nodes {
        out.insert(node.id(), texture_source_display_label(node));
    }
    out
}

fn resolve_replacement_source(
    source_id: u32,
    removed_primary_inputs: &HashMap<u32, Option<u32>>,
) -> Option<u32> {
    let mut current = source_id;
    let mut hops = 0usize;
    loop {
        let Some(next) = removed_primary_inputs.get(&current) else {
            return Some(current);
        };
        let next = (*next)?;
        current = next;
        hops = hops.saturating_add(1);
        if hops > removed_primary_inputs.len() {
            return None;
        }
    }
}

fn rewire_or_clear_deleted_links(
    node: &mut ProjectNode,
    removed_ids: &[u32],
    removed_primary_inputs: &HashMap<u32, Option<u32>>,
    output_kinds: &HashMap<u32, ResourceKind>,
    output_labels: &HashMap<u32, String>,
) -> bool {
    let mut changed = false;
    if let Some(source) = node.texture_input {
        if contains_sorted_id(removed_ids, source) {
            let replacement =
                resolve_replacement_source(source, removed_primary_inputs).filter(|candidate| {
                    output_kinds.get(candidate).copied() == node.kind.input_resource_kind()
                });
            if node.texture_input != replacement {
                node.texture_input = replacement;
                changed = true;
            }
        }
    }
    for slot in &mut node.params {
        if let Some(source) = slot.signal_source {
            if contains_sorted_id(removed_ids, source) {
                slot.signal_source = None;
                changed = true;
            }
        }
        if let Some(source) = slot.texture_source {
            if contains_sorted_id(removed_ids, source) {
                let replacement = resolve_replacement_source(source, removed_primary_inputs)
                    .filter(|candidate| {
                        output_kinds.get(candidate).copied() == Some(ResourceKind::Texture2D)
                    });
                if let Some(source_id) = replacement {
                    let source_label = output_labels
                        .get(&source_id)
                        .cloned()
                        .unwrap_or_else(|| texture_target_placeholder().to_string());
                    changed |= bind_texture_target_slot(slot, Some((source_id, source_label)));
                } else {
                    changed |= bind_texture_target_slot(slot, None);
                }
            }
        }
    }
    if changed {
        rebuild_node_inputs(node);
    }
    changed
}

fn contains_sorted_id(ids: &[u32], id: u32) -> bool {
    ids.binary_search(&id).is_ok()
}

fn distance_sq(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    let dx = ax - bx;
    let dy = ay - by;
    dx.saturating_mul(dx) + dy.saturating_mul(dy)
}

fn cache_node_rect_bins(
    bins: &mut HashMap<i64, Vec<u32>>,
    node_id: u32,
    x: i32,
    y: i32,
    card_height: i32,
) {
    if card_height <= 0 {
        return;
    }
    let max_x = x.saturating_add(NODE_WIDTH.saturating_sub(1));
    let max_y = y.saturating_add(card_height.saturating_sub(1));
    for by in hit_bin_coord(y)..=hit_bin_coord(max_y) {
        for bx in hit_bin_coord(x)..=hit_bin_coord(max_x) {
            bins.entry(hit_bin_key(bx, by)).or_default().push(node_id);
        }
    }
}

fn cache_pin_bin(bins: &mut HashMap<i64, Vec<u32>>, node_id: u32, x: i32, y: i32) {
    bins.entry(hit_bin_key_for_point(x, y))
        .or_default()
        .push(node_id);
}

fn hit_bin_coord(value: i32) -> i32 {
    value.div_euclid(HIT_BIN_SIZE)
}

fn hit_bin_key_for_point(x: i32, y: i32) -> i64 {
    hit_bin_key(hit_bin_coord(x), hit_bin_coord(y))
}

fn hit_bin_key(x: i32, y: i32) -> i64 {
    ((x as i64) << 32) | ((y as u32) as i64)
}

fn fnv1a_u64(hash: u64, data: u64) -> u64 {
    (hash ^ data).wrapping_mul(0x100000001b3)
}

fn next_project_name() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("Untitled-{}", now)
}

#[cfg(test)]
mod tests {
    use super::{
        input_pin_center, node_expand_toggle_rect, node_param_value_rect, output_pin_center,
        GraphBounds, GuiProject, PersistedGuiProject, ProjectNodeKind, ResourceKind, NODE_HEIGHT,
        PARAM_LABEL_MAX_LEN,
    };

    #[test]
    fn empty_project_has_no_nodes() {
        let project = GuiProject::new_empty(640, 480);
        assert_eq!(project.node_count(), 0);
    }

    #[test]
    fn add_node_assigns_incrementing_ids() {
        let mut project = GuiProject::new_empty(640, 480);
        let a = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let b = project.add_node(ProjectNodeKind::IoWindowOut, 120, 120, 420, 480);
        assert_eq!(a, 1);
        assert_eq!(b, 2);
    }

    #[test]
    fn node_hit_test_uses_topmost_order() {
        let mut project = GuiProject::new_empty(640, 480);
        let a = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let b = project.add_node(ProjectNodeKind::IoWindowOut, 80, 80, 420, 480);
        assert_eq!(project.node_at(90, 90), Some(b));
        assert_ne!(project.node_at(90, 90), Some(a));
    }

    #[test]
    fn node_hit_test_updates_after_move_without_full_scan_state_drift() {
        let mut project = GuiProject::new_empty(640, 480);
        let node = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        assert_eq!(project.node_at(90, 90), Some(node));
        assert!(project.move_node(node, 260, 220, 420, 480));
        assert_eq!(project.node_at(90, 90), None);
        assert_eq!(project.node_at(270, 230), Some(node));
    }

    #[test]
    fn expanded_node_hit_bounds_update_after_toggle() {
        let mut project = GuiProject::new_empty(640, 480);
        let node = project.add_node(ProjectNodeKind::TexSolid, 60, 60, 420, 480);
        let base_miss_y = 60 + NODE_HEIGHT + 4;
        assert_eq!(project.node_at(72, base_miss_y), None);
        assert!(project.toggle_node_expanded(node, 420, 480));
        assert_eq!(project.node_at(72, base_miss_y), Some(node));
    }

    #[test]
    fn pin_hit_tests_work_through_spatial_bins() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 240, 80, 420, 480);
        let solid_node = project.node(solid).expect("solid node");
        let out_node = project.node(out).expect("output node");
        let (ox, oy) = output_pin_center(solid_node).expect("solid output");
        let (ix, iy) = input_pin_center(out_node).expect("output input");
        assert_eq!(project.output_pin_at(ox, oy, 10), Some(solid));
        assert_eq!(project.input_pin_at(ix, iy, 10, None), Some(out));
        assert_eq!(project.input_pin_at(ix, iy, 10, Some(out)), None);
    }

    #[test]
    fn connect_image_link_wires_solid_to_window_out() {
        let mut project = GuiProject::new_empty(640, 480);
        let top = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(top, out));
        assert_eq!(project.edge_count(), 1);
        let source_id = project
            .window_out_input_node_id()
            .expect("window-out input must exist");
        let source = project.node(source_id).expect("source node must exist");
        assert_eq!(source.kind(), ProjectNodeKind::TexSolid);
        assert!(!project.connect_image_link(top, out));
    }

    #[test]
    fn transform_node_supports_in_and_out_links() {
        let mut project = GuiProject::new_empty(640, 480);
        let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 160, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
        assert!(project.connect_image_link(source, xform));
        assert!(project.connect_image_link(xform, out));
        assert_eq!(project.edge_count(), 2);
        let source_id = project
            .window_out_input_node_id()
            .expect("window-out input must exist");
        let source = project.node(source_id).expect("source node must exist");
        assert_eq!(source.kind(), ProjectNodeKind::TexTransform2D);
    }

    #[test]
    fn feedback_node_supports_in_and_out_links() {
        let mut project = GuiProject::new_empty(640, 480);
        let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 160, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
        assert!(project.connect_image_link(source, feedback));
        assert!(project.connect_image_link(feedback, out));
        assert_eq!(project.edge_count(), 2);
        let source_id = project
            .window_out_input_node_id()
            .expect("window-out input must exist");
        let source = project.node(source_id).expect("source node must exist");
        assert_eq!(source.kind(), ProjectNodeKind::TexFeedback);
    }

    #[test]
    fn sphere_buffer_scene_chain_requires_typed_intermediate_nodes() {
        let mut project = GuiProject::new_empty(640, 480);
        let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
        assert!(!project.connect_image_link(sphere, out));
        assert!(project.connect_image_link(sphere, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));
        assert_eq!(
            project.link_resource_kind(sphere, entity),
            Some(ResourceKind::Buffer)
        );
        assert_eq!(
            project.link_resource_kind(entity, scene),
            Some(ResourceKind::Entity)
        );
        assert_eq!(
            project.link_resource_kind(scene, pass),
            Some(ResourceKind::Scene)
        );
        assert_eq!(
            project.link_resource_kind(pass, out),
            Some(ResourceKind::Texture2D)
        );
    }

    #[test]
    fn camera_node_accepts_scene_and_outputs_scene() {
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
        assert_eq!(
            project.link_resource_kind(scene, camera),
            Some(ResourceKind::Scene)
        );
        assert_eq!(
            project.link_resource_kind(camera, pass),
            Some(ResourceKind::Scene)
        );
    }

    #[test]
    fn circle_nurbs_buffer_scene_chain_requires_typed_intermediate_nodes() {
        let mut project = GuiProject::new_empty(640, 480);
        let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 20, 40, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
        assert!(!project.connect_image_link(circle, out));
        assert!(project.connect_image_link(circle, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));
        assert_eq!(
            project.link_resource_kind(circle, entity),
            Some(ResourceKind::Buffer)
        );
        assert_eq!(
            project.link_resource_kind(entity, scene),
            Some(ResourceKind::Entity)
        );
        assert_eq!(
            project.link_resource_kind(scene, pass),
            Some(ResourceKind::Scene)
        );
        assert_eq!(
            project.link_resource_kind(pass, out),
            Some(ResourceKind::Texture2D)
        );
    }

    #[test]
    fn buffer_noise_chain_requires_buffer_input_and_outputs_buffer() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 120, 420, 480);
        let noise = project.add_node(ProjectNodeKind::BufNoise, 180, 120, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 340, 120, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 500, 120, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 660, 120, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 820, 120, 420, 480);

        assert!(!project.connect_image_link(solid, noise));
        assert!(project.connect_image_link(sphere, noise));
        assert!(project.connect_image_link(noise, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));
        assert_eq!(
            project.link_resource_kind(sphere, noise),
            Some(ResourceKind::Buffer)
        );
        assert_eq!(
            project.link_resource_kind(noise, entity),
            Some(ResourceKind::Buffer)
        );
    }

    #[test]
    fn connect_image_link_rejects_cycle() {
        let mut project = GuiProject::new_empty(640, 480);
        let a = project.add_node(ProjectNodeKind::TexTransform2D, 20, 40, 420, 480);
        let b = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
        assert!(project.connect_image_link(a, b));
        assert!(!project.connect_image_link(b, a));
    }

    #[test]
    fn insert_node_on_primary_link_rewires_texture_chain() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
        assert!(project.connect_image_link(solid, out));
        assert!(project.insert_node_on_primary_link(xform, solid, out));
        assert_eq!(project.input_source_node_id(xform), Some(solid));
        assert_eq!(project.input_source_node_id(out), Some(xform));
    }

    #[test]
    fn insert_node_on_primary_link_rejects_incompatible_node() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 180, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
        assert!(project.connect_image_link(solid, out));
        assert!(!project.insert_node_on_primary_link(lfo, solid, out));
        assert_eq!(project.input_source_node_id(out), Some(solid));
        assert_eq!(project.input_source_node_id(lfo), None);
    }

    #[test]
    fn disconnect_link_removes_texture_and_signal_bindings() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 160, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 40, 420, 480);
        assert!(project.connect_image_link(solid, out));
        assert!(project.toggle_node_expanded(solid, 420, 480));
        assert!(project.select_next_param(solid));
        assert!(project.connect_image_link(lfo, solid));
        assert!(project.edge_count() >= 2);
        assert!(project.disconnect_link(lfo, solid));
        assert!(project.disconnect_link(solid, out));
        assert_eq!(project.edge_count(), 0);
    }

    #[test]
    fn connect_signal_link_to_specific_param_row() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
        let circle = project.add_node(ProjectNodeKind::TexCircle, 160, 40, 420, 480);
        assert!(project.connect_signal_link_to_param(lfo, circle, 5));
        let source = project.sample_signal_node(lfo, 0.25, &mut Vec::new());
        let value = project.node_param_value(circle, "color_g", 0.25, &mut Vec::new());
        assert_eq!(source, value);
        assert!(!project.connect_signal_link_to_param(lfo, circle, 5));
    }

    #[test]
    fn disconnect_signal_link_from_param_only_unbinds_target_row() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
        let circle = project.add_node(ProjectNodeKind::TexCircle, 160, 40, 420, 480);
        assert!(project.connect_signal_link_to_param(lfo, circle, 0));
        assert!(project.connect_signal_link_to_param(lfo, circle, 1));
        assert_eq!(project.signal_source_for_param(circle, 0), Some(lfo));
        assert_eq!(project.signal_source_for_param(circle, 1), Some(lfo));

        assert!(project.disconnect_signal_link_from_param(circle, 0));
        assert_eq!(project.signal_source_for_param(circle, 0), None);
        assert_eq!(project.signal_source_for_param(circle, 1), Some(lfo));
        assert!(!project.disconnect_signal_link_from_param(circle, 0));
    }

    #[test]
    fn connect_texture_link_to_feedback_target_param_row() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 220, 40, 420, 480);
        assert!(project.connect_texture_link_to_param(solid, feedback, 0));
        assert_eq!(project.texture_source_for_param(feedback, 0), Some(solid));
        assert_eq!(
            project.node_param_raw_text(feedback, 0),
            Some("tex.solid#1")
        );
        assert_eq!(
            project.param_link_source_for_param(feedback, 0),
            Some((solid, ResourceKind::Texture2D))
        );
        assert!(!project.connect_texture_link_to_param(solid, feedback, 0));
    }

    #[test]
    fn link_resource_kind_reports_texture_and_signal_links() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 120, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 200, 40, 420, 480);
        assert!(project.connect_image_link(solid, xform));
        assert!(project.connect_signal_link_to_param(lfo, xform, 3));
        assert_eq!(
            project.link_resource_kind(solid, xform),
            Some(ResourceKind::Texture2D)
        );
        assert_eq!(
            project.link_resource_kind(lfo, xform),
            Some(ResourceKind::Signal)
        );
        assert_eq!(project.signal_param_index_for_source(lfo, xform), Some(3));
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 360, 40, 420, 480);
        assert!(project.connect_texture_link_to_param(solid, feedback, 0));
        assert_eq!(
            project.link_resource_kind(solid, feedback),
            Some(ResourceKind::Texture2D)
        );
    }

    #[test]
    fn delete_nodes_removes_nodes_and_clears_referenced_links() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 160, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 320, 40, 420, 480);
        assert!(project.connect_image_link(solid, out));
        assert!(project.toggle_node_expanded(solid, 420, 480));
        assert!(project.select_next_param(solid));
        assert!(project.connect_image_link(lfo, solid));
        assert!(project.edge_count() >= 2);
        assert!(project.delete_nodes(&[solid]));
        assert!(project.node(solid).is_none());
        assert_eq!(project.edge_count(), 0);
        assert!(project.window_out_input_node_id().is_none());
    }

    #[test]
    fn delete_nodes_rewires_single_texture_gap() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
        assert!(project.connect_image_link(solid, xform));
        assert!(project.connect_image_link(xform, out));
        assert_eq!(project.window_out_input_node_id(), Some(xform));

        assert!(project.delete_nodes(&[xform]));
        assert!(project.node(xform).is_none());
        assert_eq!(project.window_out_input_node_id(), Some(solid));
        assert_eq!(project.edge_count(), 1);
    }

    #[test]
    fn delete_nodes_rewires_multiple_deleted_texture_nodes() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let xform_a = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
        let xform_b = project.add_node(ProjectNodeKind::TexTransform2D, 340, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 500, 40, 420, 480);
        assert!(project.connect_image_link(solid, xform_a));
        assert!(project.connect_image_link(xform_a, xform_b));
        assert!(project.connect_image_link(xform_b, out));
        assert_eq!(project.window_out_input_node_id(), Some(xform_b));

        assert!(project.delete_nodes(&[xform_a, xform_b]));
        assert!(project.node(xform_a).is_none());
        assert!(project.node(xform_b).is_none());
        assert_eq!(project.window_out_input_node_id(), Some(solid));
        assert_eq!(project.edge_count(), 1);
    }

    #[test]
    fn delete_nodes_rewires_texture_target_param_binding_gap() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let xform = project.add_node(ProjectNodeKind::TexTransform2D, 180, 40, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 340, 40, 420, 480);
        assert!(project.connect_image_link(solid, xform));
        assert!(project.connect_texture_link_to_param(xform, feedback, 0));
        assert_eq!(project.texture_source_for_param(feedback, 0), Some(xform));

        assert!(project.delete_nodes(&[xform]));
        assert_eq!(project.texture_source_for_param(feedback, 0), Some(solid));
    }

    #[test]
    fn set_param_value_clamps_to_slot_range() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        assert!(project.set_param_value(solid, 0, 10.0));
        let value = project
            .node_param_raw_value(solid, 0)
            .expect("param value should exist");
        assert_eq!(value, 1.0);
        let value_text = project
            .node_param_raw_text(solid, 0)
            .expect("param text should exist");
        assert_eq!(value_text, "1.000");
    }

    #[test]
    fn lfo_amplitude_accepts_higher_values() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
        assert!(project.set_param_value(lfo, 1, 12.5));
        let value = project
            .node_param_raw_value(lfo, 1)
            .expect("param value should exist");
        assert_eq!(value, 12.5);
    }

    #[test]
    fn circle_nurbs_arc_style_uses_dropdown_options() {
        let mut project = GuiProject::new_empty(640, 480);
        let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 40, 40, 420, 480);
        assert!(project.param_is_dropdown(circle, 3));
        assert!(!project.param_supports_text_edit(circle, 3));
        let options = project
            .node_param_dropdown_options(circle, 3)
            .expect("dropdown options should exist");
        assert_eq!(options.len(), 2);
        assert_eq!(project.node_param_raw_text(circle, 3), Some("closed"));
        assert!(project.set_param_dropdown_index(circle, 3, 1));
        assert_eq!(project.node_param_raw_text(circle, 3), Some("open_arc"));
        assert!(project.adjust_param(circle, 3, -1.0));
        assert_eq!(project.node_param_raw_text(circle, 3), Some("closed"));
    }

    #[test]
    fn render_scene_pass_bg_mode_uses_dropdown_options() {
        let mut project = GuiProject::new_empty(640, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 40, 40, 420, 480);
        assert!(project.param_is_dropdown(pass, 2));
        assert!(!project.param_supports_text_edit(pass, 2));
        let options = project
            .node_param_dropdown_options(pass, 2)
            .expect("dropdown options should exist");
        assert_eq!(options.len(), 2);
        assert_eq!(project.node_param_raw_text(pass, 2), Some("with_bg"));
        assert!(project.set_param_dropdown_index(pass, 2, 1));
        assert_eq!(project.node_param_raw_text(pass, 2), Some("alpha_clip"));
        assert!(project.adjust_param(pass, 2, -1.0));
        assert_eq!(project.node_param_raw_text(pass, 2), Some("with_bg"));
    }

    #[test]
    fn all_default_parameter_labels_fit_length_budget() {
        let mut project = GuiProject::new_empty(640, 480);
        let kinds = [
            ProjectNodeKind::TexSolid,
            ProjectNodeKind::TexCircle,
            ProjectNodeKind::BufSphere,
            ProjectNodeKind::BufCircleNurbs,
            ProjectNodeKind::BufNoise,
            ProjectNodeKind::TexTransform2D,
            ProjectNodeKind::TexFeedback,
            ProjectNodeKind::SceneEntity,
            ProjectNodeKind::SceneBuild,
            ProjectNodeKind::RenderCamera,
            ProjectNodeKind::RenderScenePass,
            ProjectNodeKind::CtlLfo,
            ProjectNodeKind::IoWindowOut,
        ];
        let mut x = 20;
        for kind in kinds {
            let node_id = project.add_node(kind, x, 40, 420, 480);
            x += 20;
            let node = project.node(node_id).expect("node should exist");
            for row in node.param_views() {
                assert!(
                    row.label.len() <= PARAM_LABEL_MAX_LEN,
                    "label '{}' exceeds {} chars",
                    row.label,
                    PARAM_LABEL_MAX_LEN
                );
            }
        }
    }

    #[test]
    #[should_panic(expected = "parameter label")]
    fn parameter_constructor_rejects_labels_longer_than_budget() {
        let _ = super::param("key", "label_too_long", 0.0, 0.0, 1.0, 0.1);
    }

    #[test]
    fn cached_param_text_updates_when_value_changes() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let initial = project
            .node_param_raw_text(solid, 0)
            .expect("param text should exist")
            .to_string();
        assert_eq!(initial, "0.900");

        assert!(project.set_param_value(solid, 0, 0.25));
        let after_set = project
            .node_param_raw_text(solid, 0)
            .expect("param text should exist");
        assert_eq!(after_set, "0.250");

        assert!(project.adjust_selected_param(solid, 1.0));
        let after_adjust = project
            .node_param_raw_text(solid, 0)
            .expect("param text should exist");
        assert_eq!(after_adjust, "0.260");
    }

    #[test]
    fn adjust_param_changes_specific_row_value() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        assert!(project.adjust_param(solid, 1, 3.0));
        let value = project
            .node_param_raw_value(solid, 1)
            .expect("param value should exist");
        assert!((value - 0.93).abs() < 1e-5);
    }

    #[test]
    fn render_signature_ignores_expand_and_param_selection_state() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 180, 40, 420, 480);
        assert!(project.connect_image_link(solid, out));
        let base = project.render_signature();

        assert!(project.toggle_node_expanded(solid, 420, 480));
        assert_eq!(project.render_signature(), base);

        assert!(project.select_next_param(solid));
        assert_eq!(project.render_signature(), base);
    }

    #[test]
    fn ui_signature_changes_for_expand_or_param_selection() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let base = project.ui_signature();

        assert!(project.toggle_node_expanded(solid, 420, 480));
        let after_expand = project.ui_signature();
        assert_ne!(after_expand, base);

        assert!(project.select_next_param(solid));
        assert_ne!(project.ui_signature(), after_expand);
    }

    #[test]
    fn render_signature_changes_when_render_param_changes() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
        let base = project.render_signature();

        assert!(project.set_param_value(solid, 0, 0.2));
        assert_ne!(project.render_signature(), base);
    }

    #[test]
    fn param_row_hit_returns_index_for_expanded_node() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
        assert!(project.toggle_node_expanded(solid, 420, 480));
        let node = project.node(solid).expect("node should exist");
        let row = super::node_param_row_rect(node, 2).expect("row rect");
        let hit = project.param_row_at(solid, row.x + 2, row.y + 2);
        assert_eq!(hit, Some(2));
        let value_rect = node_param_value_rect(node, 2).expect("value rect");
        let value_hit =
            project.param_value_box_contains(solid, 2, value_rect.x + 2, value_rect.y + 2);
        assert!(value_hit);
    }

    #[test]
    fn expand_toggle_rect_exists_for_param_nodes_only() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 60, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        let solid_node = project.node(solid).expect("solid node");
        let out_node = project.node(out).expect("out node");
        let solid_rect = node_expand_toggle_rect(solid_node).expect("solid toggle");
        assert_eq!(solid_rect.x, solid_node.x() + super::NODE_TOGGLE_MARGIN);
        assert_eq!(solid_rect.y, solid_node.y() + super::NODE_TOGGLE_MARGIN);
        assert!(node_expand_toggle_rect(out_node).is_none());
    }

    #[test]
    fn pin_centers_follow_node_kind_capabilities() {
        let mut project = GuiProject::new_empty(640, 480);
        let top = project.add_node(ProjectNodeKind::TexSolid, 60, 70, 420, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 60, 140, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 70, 420, 480);
        let top_node = project.node(top).expect("top node must exist");
        let lfo_node = project.node(lfo).expect("lfo node must exist");
        let out_node = project.node(out).expect("output node must exist");
        assert!(output_pin_center(top_node).is_some());
        assert!(input_pin_center(top_node).is_none());
        assert!(output_pin_center(lfo_node).is_some());
        assert!(input_pin_center(lfo_node).is_none());
        assert!(output_pin_center(out_node).is_none());
        assert!(input_pin_center(out_node).is_some());
    }

    #[test]
    fn graph_bounds_span_all_nodes() {
        let mut project = GuiProject::new_empty(640, 480);
        project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
        project.add_node(ProjectNodeKind::IoWindowOut, 200, 160, 420, 480);
        assert_eq!(
            project.graph_bounds(),
            Some(GraphBounds {
                min_x: 40,
                min_y: 80,
                max_x: 200 + super::NODE_WIDTH,
                max_y: 204,
            })
        );
    }

    #[test]
    fn persisted_roundtrip_restores_nodes_links_and_bindings() {
        let mut project = GuiProject::new_empty(640, 480);
        let lfo = project.add_node(ProjectNodeKind::CtlLfo, 40, 40, 420, 480);
        let circle = project.add_node(ProjectNodeKind::BufCircleNurbs, 200, 40, 420, 480);
        let noise = project.add_node(ProjectNodeKind::BufNoise, 360, 40, 420, 480);
        let entity = project.add_node(ProjectNodeKind::SceneEntity, 520, 40, 420, 480);
        let scene = project.add_node(ProjectNodeKind::SceneBuild, 680, 40, 420, 480);
        let pass = project.add_node(ProjectNodeKind::RenderScenePass, 840, 40, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 1000, 40, 420, 480);
        let feedback = project.add_node(ProjectNodeKind::TexFeedback, 1160, 40, 420, 480);
        assert!(project.connect_image_link(circle, noise));
        assert!(project.connect_image_link(noise, entity));
        assert!(project.connect_image_link(entity, scene));
        assert!(project.connect_image_link(scene, pass));
        assert!(project.connect_image_link(pass, out));
        assert!(project.connect_texture_link_to_param(pass, feedback, 0));
        assert!(project.connect_signal_link_to_param(lfo, noise, 0));
        assert!(project.toggle_node_expanded(noise, 420, 480));
        assert!(project.set_param_value(noise, 1, 4.5));

        let persisted = project.to_persisted();
        let restored =
            GuiProject::from_persisted(persisted, 420, 480).expect("restore should work");
        assert_eq!(restored.node_count(), project.node_count());
        assert_eq!(restored.edge_count(), project.edge_count());
        assert_eq!(restored.render_signature(), project.render_signature());
        assert!(restored.has_signal_bindings());
    }

    #[test]
    fn from_persisted_rejects_unsupported_version() {
        let persisted = PersistedGuiProject {
            version: 999,
            name: "broken".to_string(),
            preview_width: 640,
            preview_height: 480,
            nodes: Vec::new(),
        };
        assert!(GuiProject::from_persisted(persisted, 420, 480).is_err());
    }
}

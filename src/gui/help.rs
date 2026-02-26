//! Contextual help-model builders for the graph editor.
//!
//! `F1` opens a modal driven by hovered graph context. The modal content is
//! assembled from live node/parameter metadata so docs stay aligned with
//! runtime behavior and parameter contracts.

use super::project::{
    GuiProject, NodeParamDescriptor, NodeParamWidget, ProjectNodeKind, ResourceKind,
};

/// Modal payload rendered by the scene overlay layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HelpModalContent {
    /// Modal heading.
    pub(crate) title: String,
    /// Body lines rendered top-to-bottom.
    pub(crate) lines: Vec<String>,
}

/// Build a fallback modal when no hover target is available.
pub(crate) fn build_global_help_modal() -> HelpModalContent {
    HelpModalContent {
        title: "Help".to_string(),
        lines: vec![
            "Hover a node or parameter, then press F1.".to_string(),
            "Press F1 again, or click, to close this help popup.".to_string(),
            "Node help includes parameter ranges, widgets, and bindings.".to_string(),
        ],
    }
}

/// Build node-level help content for one node id.
pub(crate) fn build_node_help_modal(
    project: &GuiProject,
    node_id: u32,
) -> Option<HelpModalContent> {
    let node = project.node(node_id)?;
    let kind = node.kind();
    let mut lines = Vec::new();
    lines.push(format!("Node Id: {}#{}", kind.label(), node.id()));
    lines.push(node_summary(kind).to_string());
    lines.push(format!(
        "Primary Input: {}",
        resource_kind_label(kind.input_resource_kind())
    ));
    lines.push(format!(
        "Primary Output: {}",
        resource_kind_label(kind.output_resource_kind())
    ));
    lines.push(format!("Parameter Rows: {}", node.param_count()));
    if node.param_count() == 0 {
        lines.push("This node has no editable parameters.".to_string());
    } else {
        lines.push("Parameters:".to_string());
        for index in 0..node.param_count() {
            let Some(param) = project.node_param_descriptor(node_id, index) else {
                continue;
            };
            lines.push(format!("{}. {} ({})", index + 1, param.label, param.key));
            lines.push(format!("   {}", param_summary(kind, param.key)));
            lines.push(format!("   {}", param_value_line(&param)));
        }
    }
    Some(HelpModalContent {
        title: format!("Node Help: {}", kind.label()),
        lines,
    })
}

/// Build parameter-level help content for one node parameter row.
pub(crate) fn build_param_help_modal(
    project: &GuiProject,
    node_id: u32,
    param_index: usize,
) -> Option<HelpModalContent> {
    let node = project.node(node_id)?;
    let kind = node.kind();
    let param = project.node_param_descriptor(node_id, param_index)?;
    let mut lines = Vec::new();
    lines.push(format!("Node: {}#{}", kind.label(), node_id));
    lines.push(format!("Parameter: {} ({})", param.label, param.key));
    lines.push(param_summary(kind, param.key).to_string());
    lines.push(param_value_line(&param));
    lines.push(param_widget_line(&param));
    if let Some(source) = param.signal_source {
        lines.push(format!("Signal Binding: node #{source}"));
    }
    if let Some(source) = param.texture_source {
        lines.push(format!("Texture Binding: node #{source}"));
    }
    if param.signal_source.is_none() && param.texture_source.is_none() {
        lines.push("Binding: none".to_string());
    }
    Some(HelpModalContent {
        title: format!("Parameter Help: {}", param.label),
        lines,
    })
}

fn resource_kind_label(kind: Option<ResourceKind>) -> &'static str {
    match kind {
        Some(ResourceKind::Buffer) => "buffer",
        Some(ResourceKind::Entity) => "entity",
        Some(ResourceKind::Scene) => "scene",
        Some(ResourceKind::Texture2D) => "texture_2d",
        Some(ResourceKind::Signal) => "signal",
        None => "none",
    }
}

fn node_summary(kind: ProjectNodeKind) -> &'static str {
    match kind {
        ProjectNodeKind::TexSolid => "Generates a full-frame constant color texture.",
        ProjectNodeKind::TexCircle => "Renders a soft-edged circle with RGBA controls.",
        ProjectNodeKind::BufSphere => "Creates sphere mesh geometry in buffer space.",
        ProjectNodeKind::BufCircleNurbs => {
            "Creates curve geometry with arc and tessellation controls."
        }
        ProjectNodeKind::BufNoise => "Applies procedural deformation to incoming buffer geometry.",
        ProjectNodeKind::TexTransform2D => {
            "Applies color gain and alpha scaling to incoming texture."
        }
        ProjectNodeKind::TexLevel => "Remaps input range and gamma to reshape texture levels.",
        ProjectNodeKind::TexFeedback => "Mixes current input with persistent accumulation history.",
        ProjectNodeKind::TexBlend => {
            "Composites primary input with optional secondary blend texture."
        }
        ProjectNodeKind::SceneEntity => "Binds buffer geometry to scene transform/material state.",
        ProjectNodeKind::SceneBuild => "Aggregates scene entities into one renderable scene.",
        ProjectNodeKind::RenderCamera => {
            "Configures camera transform/projection for scene rendering."
        }
        ProjectNodeKind::RenderScenePass => "Renders scene data into a texture output pass.",
        ProjectNodeKind::CtlLfo => "Produces a looping scalar modulation signal.",
        ProjectNodeKind::IoWindowOut => "Final sink node that displays the texture output.",
    }
}

fn param_summary(kind: ProjectNodeKind, key: &str) -> &'static str {
    match (kind, key) {
        (ProjectNodeKind::TexFeedback, "accumulation_tex") => {
            "Optional external accumulation texture used as feedback history storage."
        }
        (_, "loop_mode") => "Selects free-running or timeline-locked seamless loop behavior.",
        (_, "loop_cyc") => "Loop cycle length in frames/cycles used by looped noise playback.",
        (_, "bg_mode") => "Controls whether scene pass keeps background or clips to alpha.",
        (_, "blend_mode") => "Selects blend equation used against blend_tex.",
        (_, "blend_tex") => "Optional secondary texture input used by tex.blend.",
        (_, "feedback") => "Mix factor where higher values preserve more accumulated history.",
        (_, "brightness") => "Global brightness multiplier for the incoming texture.",
        (_, "gamma") => "Non-linear gamma response for tone remapping.",
        (_, "alpha_mul") => "Scales output alpha after color operations.",
        (_, "amplitude") => "Signal/noise strength depending on node type.",
        (_, "frequency") => "Spatial frequency for procedural noise deformation.",
        (_, "speed_hz") => "Temporal phase speed for procedural animation.",
        (_, "rate_hz") => "Oscillation frequency for the LFO signal.",
        (_, "phase") => "Phase offset for periodic behavior.",
        (_, "bias") => "Constant offset applied after modulation.",
        (_, "opacity") => "Blend contribution of blend_tex into primary input.",
        (_, "res_width") => "Render pass width. 0 keeps project preview width.",
        (_, "res_height") => "Render pass height. 0 keeps project preview height.",
        (_, "edge_softness") => "Softens scene edges in render.scene_pass shading.",
        (_, "zoom") => "Camera zoom scale for scene rendering.",
        (_, "seed") => "Deterministic random seed for procedural generation.",
        (_, "radius") => "Primary radius/size control for shape geometry.",
        (_, "color_r") => "Red color channel scalar.",
        (_, "color_g") => "Green color channel scalar.",
        (_, "color_b") => "Blue color channel scalar.",
        (_, "alpha") => "Alpha output channel scalar.",
        (_, "center_x") => "Horizontal center position in normalized coordinates.",
        (_, "center_y") => "Vertical center position in normalized coordinates.",
        _ => "Controls this node's behavior within its documented range.",
    }
}

fn param_value_line(param: &NodeParamDescriptor) -> String {
    match param.widget {
        NodeParamWidget::TextureTarget => {
            if param.texture_source.is_some() {
                format!("Current Binding Label: {}", param.value_text)
            } else {
                "Current Binding Label: none".to_string()
            }
        }
        NodeParamWidget::Dropdown { .. } => format!("Current Option: {}", param.value_text),
        NodeParamWidget::Number => format!(
            "Current Value: {} (raw {:.4}, range {:.4}..{:.4}, step {:.4})",
            param.value_text, param.value, param.min, param.max, param.step
        ),
    }
}

fn param_widget_line(param: &NodeParamDescriptor) -> String {
    match param.widget {
        NodeParamWidget::Number => "Widget: numeric field".to_string(),
        NodeParamWidget::TextureTarget => {
            "Widget: texture target bind slot (drop a texture wire here)".to_string()
        }
        NodeParamWidget::Dropdown { options } => {
            let labels = options
                .iter()
                .map(|opt| opt.label)
                .collect::<Vec<_>>()
                .join(", ");
            format!("Widget: dropdown [{}]", labels)
        }
    }
}

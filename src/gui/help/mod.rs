//! Contextual help-model builders for the graph editor.
//!
//! `F1` opens a modal driven by hovered graph context. Help copy is loaded
//! from `docs/help/in_app_help.md` so in-app help and readable docs share one
//! source of truth.

mod catalog;

use super::project::{GuiProject, NodeParamDescriptor, NodeParamWidget, ResourceKind};
use catalog::help_catalog;

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
    let mut lines = help_catalog().global_lines.clone();
    if lines.is_empty() {
        lines = vec![
            "Hover a node or parameter, then press F1.".to_string(),
            "Press F1 again, or click, to close this help popup.".to_string(),
        ];
    }
    HelpModalContent {
        title: "Help".to_string(),
        lines,
    }
}

/// Build node-level help content for one node id.
pub(crate) fn build_node_help_modal(
    project: &GuiProject,
    node_id: u32,
) -> Option<HelpModalContent> {
    let node = project.node(node_id)?;
    let kind = node.kind();
    let node_doc = help_catalog().nodes.get(kind.stable_id());
    let mut lines = Vec::new();
    lines.push(format!("Node Id: {}#{}", kind.label(), node.id()));
    if let Some(doc) = node_doc {
        lines.extend(doc.lines.iter().cloned());
    } else {
        lines.push("No markdown docs found for this node yet.".to_string());
    }
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
            if let Some(doc_lines) = node_doc.and_then(|doc| doc.params.get(param.key)) {
                for line in doc_lines {
                    lines.push(format!("   {line}"));
                }
            }
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
    if let Some(doc) = help_catalog()
        .nodes
        .get(kind.stable_id())
        .and_then(|node_doc| node_doc.params.get(param.key))
    {
        lines.extend(doc.iter().cloned());
    } else {
        lines.push("No markdown docs found for this parameter yet.".to_string());
    }
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
            format!("Widget: dropdown [{labels}]")
        }
    }
}

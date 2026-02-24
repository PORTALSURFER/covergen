//! Extensible node-template catalog for V2 graph construction.
//!
//! This catalog decouples graph-building code from direct `NodeKind` matching by
//! exposing named node templates that can be looked up by key/alias. Presets and
//! subgraph modules can create nodes through this layer, and downstream runtime
//! semantics stay unchanged.

use std::collections::HashMap;

use crate::v2::graph::{
    BlendNode, GenerateLayerNode, GraphBuildError, GraphBuilder, MaskNode, NodeId, OutputNode,
    SourceNoiseNode, ToneMapNode, WarpTransformNode,
};

/// Payload used when instantiating a node template.
#[derive(Clone, Copy, Debug)]
pub enum NodePayload {
    GenerateLayer(GenerateLayerNode),
    SourceNoise(SourceNoiseNode),
    Mask(MaskNode),
    Blend(BlendNode),
    ToneMap(ToneMapNode),
    WarpTransform(WarpTransformNode),
    Output(OutputNode),
}

/// Metadata and constructor for one named node template.
#[derive(Clone, Copy, Debug)]
pub struct NodeTemplate {
    pub key: &'static str,
    pub aliases: &'static [&'static str],
    constructor: NodeConstructor,
}

type NodeConstructor = fn(&mut GraphBuilder, NodePayload) -> Result<NodeId, GraphBuildError>;

/// Mutable registry of node templates keyed by case-insensitive names.
#[derive(Debug, Default)]
pub struct NodeCatalog {
    templates: Vec<NodeTemplate>,
    lookup: HashMap<String, usize>,
}

impl NodeCatalog {
    /// Create an empty catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a catalog with all built-in V2 node templates.
    pub fn with_builtins() -> Result<Self, GraphBuildError> {
        let mut catalog = Self::new();
        register_builtin_templates(&mut catalog)?;
        Ok(catalog)
    }

    /// Register a template and all aliases.
    pub fn register(&mut self, template: NodeTemplate) -> Result<(), GraphBuildError> {
        let slot = self.templates.len();
        self.insert_lookup(template.key, slot)?;
        for alias in template.aliases {
            self.insert_lookup(alias, slot)?;
        }
        self.templates.push(template);
        Ok(())
    }

    /// Instantiate a node from a named template.
    pub fn create(
        &self,
        builder: &mut GraphBuilder,
        key: &str,
        payload: NodePayload,
    ) -> Result<NodeId, GraphBuildError> {
        let template = self.resolve(key)?;
        (template.constructor)(builder, payload)
    }

    /// Return sorted canonical template keys.
    pub fn keys(&self) -> Vec<&'static str> {
        let mut keys: Vec<&'static str> =
            self.templates.iter().map(|template| template.key).collect();
        keys.sort_unstable();
        keys
    }

    /// Resolve a template by key or alias.
    pub fn resolve(&self, key: &str) -> Result<NodeTemplate, GraphBuildError> {
        let lookup_key = normalize(key);
        let index = self.lookup.get(&lookup_key).copied().ok_or_else(|| {
            GraphBuildError::new(format!(
                "unknown node template '{key}', expected {}",
                self.keys().join("|")
            ))
        })?;
        Ok(self.templates[index])
    }

    fn insert_lookup(&mut self, raw: &str, slot: usize) -> Result<(), GraphBuildError> {
        let normalized = normalize(raw);
        if self.lookup.insert(normalized.clone(), slot).is_some() {
            return Err(GraphBuildError::new(format!(
                "duplicate node template key/alias '{raw}' ({normalized})"
            )));
        }
        Ok(())
    }
}

fn normalize(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn register_builtin_templates(catalog: &mut NodeCatalog) -> Result<(), GraphBuildError> {
    catalog.register(NodeTemplate {
        key: "generate-layer",
        aliases: &["layer", "fractal-layer"],
        constructor: create_generate_layer,
    })?;
    catalog.register(NodeTemplate {
        key: "source-noise",
        aliases: &["noise", "source"],
        constructor: create_source_noise,
    })?;
    catalog.register(NodeTemplate {
        key: "mask",
        aliases: &["threshold-mask"],
        constructor: create_mask,
    })?;
    catalog.register(NodeTemplate {
        key: "blend",
        aliases: &["mix", "composite"],
        constructor: create_blend,
    })?;
    catalog.register(NodeTemplate {
        key: "tone-map",
        aliases: &["tonemap", "tone"],
        constructor: create_tonemap,
    })?;
    catalog.register(NodeTemplate {
        key: "warp-transform",
        aliases: &["warp", "transform"],
        constructor: create_warp,
    })?;
    catalog.register(NodeTemplate {
        key: "output",
        aliases: &["out"],
        constructor: create_output,
    })?;
    Ok(())
}

fn create_generate_layer(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::GenerateLayer(spec) => Ok(builder.add_generate_layer(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'generate-layer' requires GenerateLayer payload",
        )),
    }
}

fn create_source_noise(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::SourceNoise(spec) => Ok(builder.add_source_noise(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'source-noise' requires SourceNoise payload",
        )),
    }
}

fn create_mask(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::Mask(spec) => Ok(builder.add_mask(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'mask' requires Mask payload",
        )),
    }
}

fn create_blend(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::Blend(spec) => Ok(builder.add_blend(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'blend' requires Blend payload",
        )),
    }
}

fn create_tonemap(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::ToneMap(spec) => Ok(builder.add_tonemap(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'tone-map' requires ToneMap payload",
        )),
    }
}

fn create_warp(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::WarpTransform(spec) => Ok(builder.add_warp_transform(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'warp-transform' requires WarpTransform payload",
        )),
    }
}

fn create_output(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::Output(spec) => match spec.role {
            super::super::node::OutputRole::Primary => Ok(builder.add_output()),
            super::super::node::OutputRole::Tap => Ok(builder.add_output_tap(spec.slot)),
        },
        _ => Err(GraphBuildError::new(
            "node template 'output' requires Output payload",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_alias_resolves_to_template() {
        let catalog = NodeCatalog::with_builtins().expect("builtins should register");
        let template = catalog.resolve("tone").expect("alias should resolve");
        assert_eq!(template.key, "tone-map");
    }

    #[test]
    fn payload_mismatch_reports_actionable_error() {
        let catalog = NodeCatalog::with_builtins().expect("builtins should register");
        let mut builder = GraphBuilder::new(64, 64, 7);
        let err = catalog
            .create(
                &mut builder,
                "mask",
                NodePayload::Output(OutputNode::primary()),
            )
            .expect_err("mismatched payload should fail");
        assert!(err.to_string().contains("requires Mask payload"));
    }
}

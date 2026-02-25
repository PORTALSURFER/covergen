//! Extensible node-template catalog for V2 graph construction.

use std::collections::HashMap;

use crate::chop::{ChopLfoNode, ChopMathNode, ChopRemapNode};
use crate::graph::{
    BlendNode, GenerateLayerNode, GraphBuildError, GraphBuilder, MaskNode, NodeId, OperatorFamily,
    OutputNode, SourceNoiseNode, StatefulFeedbackNode, ToneMapNode, WarpTransformNode,
};
use crate::sop::{SopCircleNode, SopGeometryNode, SopSphereNode, TopCameraRenderNode};

/// Payload used when instantiating a node template.
#[derive(Clone, Copy, Debug)]
pub enum NodePayload {
    GenerateLayer(GenerateLayerNode),
    SourceNoise(SourceNoiseNode),
    Mask(MaskNode),
    Blend(BlendNode),
    ToneMap(ToneMapNode),
    WarpTransform(WarpTransformNode),
    StatefulFeedback(StatefulFeedbackNode),
    ChopLfo(ChopLfoNode),
    ChopMath(ChopMathNode),
    ChopRemap(ChopRemapNode),
    SopCircle(SopCircleNode),
    SopSphere(SopSphereNode),
    SopGeometry(SopGeometryNode),
    TopCameraRender(TopCameraRenderNode),
    Output(OutputNode),
}

/// Metadata and constructor for one named node template.
#[derive(Clone, Copy, Debug)]
pub struct NodeTemplate {
    pub key: &'static str,
    pub aliases: &'static [&'static str],
    #[cfg_attr(not(test), allow(dead_code))]
    pub family: OperatorFamily,
    constructor: NodeConstructor,
}
type NodeConstructor = fn(&mut GraphBuilder, NodePayload) -> Result<NodeId, GraphBuildError>;

#[derive(Debug, Default)]
pub struct NodeCatalog {
    templates: Vec<NodeTemplate>,
    lookup: HashMap<String, usize>,
}
impl NodeCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtins() -> Result<Self, GraphBuildError> {
        let mut catalog = Self::new();
        register_builtin_templates(&mut catalog)?;
        Ok(catalog)
    }

    pub fn register(&mut self, template: NodeTemplate) -> Result<(), GraphBuildError> {
        let slot = self.templates.len();
        self.insert_lookup(template.key, slot)?;
        for alias in template.aliases {
            self.insert_lookup(alias, slot)?;
        }
        self.templates.push(template);
        Ok(())
    }

    pub fn create(
        &self,
        builder: &mut GraphBuilder,
        key: &str,
        payload: NodePayload,
    ) -> Result<NodeId, GraphBuildError> {
        let template = self.resolve(key)?;
        (template.constructor)(builder, payload)
    }

    pub fn keys(&self) -> Vec<&'static str> {
        let mut keys: Vec<&'static str> =
            self.templates.iter().map(|template| template.key).collect();
        keys.sort_unstable();
        keys
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn keys_for_family(&self, family: OperatorFamily) -> Vec<&'static str> {
        let mut keys: Vec<&'static str> = self
            .templates
            .iter()
            .filter(|template| template.family == family)
            .map(|template| template.key)
            .collect();
        keys.sort_unstable();
        keys
    }

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
        family: OperatorFamily::Top,
        constructor: create_generate_layer,
    })?;
    catalog.register(NodeTemplate {
        key: "source-noise",
        aliases: &["noise", "source"],
        family: OperatorFamily::Top,
        constructor: create_source_noise,
    })?;
    catalog.register(NodeTemplate {
        key: "mask",
        aliases: &["threshold-mask"],
        family: OperatorFamily::Top,
        constructor: create_mask,
    })?;
    catalog.register(NodeTemplate {
        key: "blend",
        aliases: &["mix", "composite"],
        family: OperatorFamily::Top,
        constructor: create_blend,
    })?;
    catalog.register(NodeTemplate {
        key: "tone-map",
        aliases: &["tonemap", "tone"],
        family: OperatorFamily::Top,
        constructor: create_tonemap,
    })?;
    catalog.register(NodeTemplate {
        key: "warp-transform",
        aliases: &["warp", "transform"],
        family: OperatorFamily::Top,
        constructor: create_warp,
    })?;
    catalog.register(NodeTemplate {
        key: "stateful-feedback",
        aliases: &["feedback"],
        family: OperatorFamily::Top,
        constructor: create_stateful_feedback,
    })?;
    catalog.register(NodeTemplate {
        key: "output",
        aliases: &["out"],
        family: OperatorFamily::Output,
        constructor: create_output,
    })?;
    catalog.register(NodeTemplate {
        key: "chop-lfo",
        aliases: &["lfo"],
        family: OperatorFamily::Chop,
        constructor: create_chop_lfo,
    })?;
    catalog.register(NodeTemplate {
        key: "chop-math",
        aliases: &["chop-math-op"],
        family: OperatorFamily::Chop,
        constructor: create_chop_math,
    })?;
    catalog.register(NodeTemplate {
        key: "chop-remap",
        aliases: &["remap"],
        family: OperatorFamily::Chop,
        constructor: create_chop_remap,
    })?;
    catalog.register(NodeTemplate {
        key: "sop-circle",
        aliases: &["circle"],
        family: OperatorFamily::Sop,
        constructor: create_sop_circle,
    })?;
    catalog.register(NodeTemplate {
        key: "sop-sphere",
        aliases: &["sphere"],
        family: OperatorFamily::Sop,
        constructor: create_sop_sphere,
    })?;
    catalog.register(NodeTemplate {
        key: "sop-geometry",
        aliases: &["geo", "sop-geo"],
        family: OperatorFamily::Sop,
        constructor: create_sop_geometry,
    })?;
    catalog.register(NodeTemplate {
        key: "top-camera-render",
        aliases: &["camera-render"],
        family: OperatorFamily::Top,
        constructor: create_top_camera_render,
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

fn create_stateful_feedback(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::StatefulFeedback(spec) => Ok(builder.add_stateful_feedback(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'stateful-feedback' requires StatefulFeedback payload",
        )),
    }
}

fn create_chop_lfo(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::ChopLfo(spec) => Ok(builder.add_chop_lfo(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'chop-lfo' requires ChopLfo payload",
        )),
    }
}

fn create_chop_math(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::ChopMath(spec) => Ok(builder.add_chop_math(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'chop-math' requires ChopMath payload",
        )),
    }
}

fn create_chop_remap(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::ChopRemap(spec) => Ok(builder.add_chop_remap(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'chop-remap' requires ChopRemap payload",
        )),
    }
}

fn create_sop_circle(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::SopCircle(spec) => Ok(builder.add_sop_circle(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'sop-circle' requires SopCircle payload",
        )),
    }
}

fn create_sop_sphere(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::SopSphere(spec) => Ok(builder.add_sop_sphere(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'sop-sphere' requires SopSphere payload",
        )),
    }
}

fn create_sop_geometry(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::SopGeometry(spec) => Ok(builder.add_sop_geometry(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'sop-geometry' requires SopGeometry payload",
        )),
    }
}

fn create_top_camera_render(
    builder: &mut GraphBuilder,
    payload: NodePayload,
) -> Result<NodeId, GraphBuildError> {
    match payload {
        NodePayload::TopCameraRender(spec) => Ok(builder.add_top_camera_render(spec)),
        _ => Err(GraphBuildError::new(
            "node template 'top-camera-render' requires TopCameraRender payload",
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
        assert_eq!(template.family, OperatorFamily::Top);
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

    #[test]
    fn builtin_keys_group_by_operator_family() {
        let catalog = NodeCatalog::with_builtins().expect("builtins should register");
        let top = catalog.keys_for_family(OperatorFamily::Top);
        let output = catalog.keys_for_family(OperatorFamily::Output);
        let chop = catalog.keys_for_family(OperatorFamily::Chop);
        let sop = catalog.keys_for_family(OperatorFamily::Sop);

        assert_eq!(top.len(), 8);
        assert_eq!(output, vec!["output"]);
        assert_eq!(chop.len(), 3);
        assert_eq!(sop.len(), 3);
    }
}

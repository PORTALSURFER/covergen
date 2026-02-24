//! Extensible reusable subgraph-module catalog for V2 presets.
//!
//! A subgraph module encapsulates a small graph pattern (for example mask
//! generation or masked blending) that can be reused across multiple presets.
//! Modules remain fully code-generated and do not require GUI authoring.

use std::collections::HashMap;

use crate::graph::{GraphBuildError, GraphBuilder, NodeId};
use crate::model::{LayerBlendMode, XorShift32};
use crate::node::{MaskTemporal, PortType, SourceNoiseTemporal};
use crate::runtime_config::V2Profile;

use super::node_catalog::{NodeCatalog, NodePayload};
use super::primitives::{random_blend, random_tonemap, random_warp};

/// Inputs for invoking a reusable subgraph module.
#[derive(Clone, Debug)]
pub struct ModuleRequest {
    pub seed: u32,
    pub profile: V2Profile,
    pub inputs: Vec<NodeId>,
}

/// Outputs produced by a reusable subgraph module.
#[derive(Clone, Debug)]
pub struct ModuleResult {
    pub primary: NodeId,
    pub extra_outputs: Vec<NodeId>,
}

impl ModuleResult {
    #[cfg(test)]
    /// Return total number of produced outputs (primary + extras).
    pub fn output_count(&self) -> usize {
        1 + self.extra_outputs.len()
    }
}

/// Constructor metadata for one reusable subgraph module.
#[derive(Clone, Copy, Debug)]
pub struct SubgraphTemplate {
    pub key: &'static str,
    pub aliases: &'static [&'static str],
    build: ModuleBuildFn,
}

type ModuleBuildFn =
    fn(&mut ModuleBuildContext<'_>, ModuleRequest) -> Result<ModuleResult, GraphBuildError>;

/// Build context provided to module constructors.
pub struct ModuleBuildContext<'a> {
    pub builder: &'a mut GraphBuilder,
    pub nodes: &'a NodeCatalog,
    pub rng: &'a mut XorShift32,
}

/// Registry of composable subgraph modules.
#[derive(Debug, Default)]
pub struct SubgraphCatalog {
    templates: Vec<SubgraphTemplate>,
    lookup: HashMap<String, usize>,
}

impl SubgraphCatalog {
    /// Create an empty module catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a catalog with default reusable modules.
    pub fn with_builtins() -> Result<Self, GraphBuildError> {
        let mut catalog = Self::new();
        register_builtin_modules(&mut catalog)?;
        Ok(catalog)
    }

    /// Register one module template with aliases.
    pub fn register(&mut self, template: SubgraphTemplate) -> Result<(), GraphBuildError> {
        let slot = self.templates.len();
        self.insert_lookup(template.key, slot)?;
        for alias in template.aliases {
            self.insert_lookup(alias, slot)?;
        }
        self.templates.push(template);
        Ok(())
    }

    /// Execute a named module.
    pub fn execute(
        &self,
        key: &str,
        context: &mut ModuleBuildContext<'_>,
        request: ModuleRequest,
    ) -> Result<ModuleResult, GraphBuildError> {
        let template = self.resolve(key)?;
        (template.build)(context, request)
    }

    /// Return sorted canonical module keys.
    pub fn keys(&self) -> Vec<&'static str> {
        let mut keys: Vec<&'static str> =
            self.templates.iter().map(|template| template.key).collect();
        keys.sort_unstable();
        keys
    }

    fn resolve(&self, key: &str) -> Result<SubgraphTemplate, GraphBuildError> {
        let normalized = key.trim().to_ascii_lowercase();
        let index = self.lookup.get(&normalized).copied().ok_or_else(|| {
            GraphBuildError::new(format!(
                "unknown subgraph module '{key}', expected {}",
                self.keys().join("|")
            ))
        })?;
        Ok(self.templates[index])
    }

    fn insert_lookup(&mut self, key: &str, slot: usize) -> Result<(), GraphBuildError> {
        let normalized = key.trim().to_ascii_lowercase();
        if self.lookup.insert(normalized.clone(), slot).is_some() {
            return Err(GraphBuildError::new(format!(
                "duplicate subgraph key/alias '{key}' ({normalized})"
            )));
        }
        Ok(())
    }
}

fn register_builtin_modules(catalog: &mut SubgraphCatalog) -> Result<(), GraphBuildError> {
    catalog.register(SubgraphTemplate {
        key: "noise-mask",
        aliases: &["mask-noise", "procedural-mask"],
        build: build_noise_mask,
    })?;
    catalog.register(SubgraphTemplate {
        key: "warp-tone",
        aliases: &["warp-and-tone"],
        build: build_warp_tone,
    })?;
    catalog.register(SubgraphTemplate {
        key: "masked-blend",
        aliases: &["blend-masked"],
        build: build_masked_blend,
    })?;
    Ok(())
}

fn build_noise_mask(
    context: &mut ModuleBuildContext<'_>,
    request: ModuleRequest,
) -> Result<ModuleResult, GraphBuildError> {
    let source = context.nodes.create(
        context.builder,
        "source-noise",
        NodePayload::SourceNoise(crate::graph::SourceNoiseNode {
            seed: context.rng.next_u32() ^ request.seed,
            scale: 1.4 + context.rng.next_f32() * 6.8,
            octaves: 3 + (context.rng.next_u32() % 3),
            amplitude: 0.65 + context.rng.next_f32() * 0.4,
            output_port: PortType::LumaTexture,
            temporal: SourceNoiseTemporal::default(),
        }),
    )?;

    let mask = context.nodes.create(
        context.builder,
        "mask",
        NodePayload::Mask(crate::graph::MaskNode {
            threshold: 0.33 + context.rng.next_f32() * 0.34,
            softness: 0.04 + context.rng.next_f32() * 0.24,
            invert: matches!(request.profile, V2Profile::Performance),
            temporal: MaskTemporal::default(),
        }),
    )?;

    context.builder.connect_luma(source, mask);
    Ok(ModuleResult {
        primary: mask,
        extra_outputs: vec![source],
    })
}

fn build_warp_tone(
    context: &mut ModuleBuildContext<'_>,
    request: ModuleRequest,
) -> Result<ModuleResult, GraphBuildError> {
    let input = request.inputs.first().copied().ok_or_else(|| {
        GraphBuildError::new("module 'warp-tone' expects one luma input in request.inputs[0]")
    })?;

    let warp = context.nodes.create(
        context.builder,
        "warp",
        NodePayload::WarpTransform(random_warp(context.rng, 1.0)),
    )?;
    context.builder.connect_luma(input, warp);

    let tone = context.nodes.create(
        context.builder,
        "tone",
        NodePayload::ToneMap(random_tonemap(context.rng)),
    )?;
    context.builder.connect_luma(warp, tone);

    Ok(ModuleResult {
        primary: tone,
        extra_outputs: vec![warp],
    })
}

fn build_masked_blend(
    context: &mut ModuleBuildContext<'_>,
    request: ModuleRequest,
) -> Result<ModuleResult, GraphBuildError> {
    if request.inputs.len() < 2 {
        return Err(GraphBuildError::new(
            "module 'masked-blend' expects base/top inputs in request.inputs",
        ));
    }

    let base = request.inputs[0];
    let top = request.inputs[1];
    let mask = match request.inputs.get(2).copied() {
        Some(node) => node,
        None => {
            let nested = ModuleRequest {
                seed: request.seed ^ 0xA53E_19D1,
                profile: request.profile,
                inputs: Vec::new(),
            };
            build_noise_mask(context, nested)?.primary
        }
    };

    let blend = context.nodes.create(
        context.builder,
        "blend",
        NodePayload::Blend(random_blend(
            context.rng,
            LayerBlendMode::Overlay,
            0.35,
            0.9,
        )),
    )?;
    context.builder.connect_luma_input(base, blend, 0);
    context.builder.connect_luma_input(top, blend, 1);
    context.builder.connect_mask_input(mask, blend, 2);

    Ok(ModuleResult {
        primary: blend,
        extra_outputs: vec![mask],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::OutputNode;

    #[test]
    fn masked_blend_module_wires_graph() {
        let nodes = NodeCatalog::with_builtins().expect("builtins should register");
        let modules = SubgraphCatalog::with_builtins().expect("builtins should register");
        let mut builder = GraphBuilder::new(128, 128, 7);
        let mut rng = XorShift32::new(9);

        let base = nodes
            .create(
                &mut builder,
                "source-noise",
                NodePayload::SourceNoise(crate::graph::SourceNoiseNode {
                    seed: 1,
                    scale: 1.0,
                    octaves: 3,
                    amplitude: 1.0,
                    output_port: PortType::LumaTexture,
                    temporal: SourceNoiseTemporal::default(),
                }),
            )
            .expect("base node");
        let top = nodes
            .create(
                &mut builder,
                "source-noise",
                NodePayload::SourceNoise(crate::graph::SourceNoiseNode {
                    seed: 2,
                    scale: 2.0,
                    octaves: 2,
                    amplitude: 1.0,
                    output_port: PortType::LumaTexture,
                    temporal: SourceNoiseTemporal::default(),
                }),
            )
            .expect("top node");

        let result = {
            let mut context = ModuleBuildContext {
                builder: &mut builder,
                nodes: &nodes,
                rng: &mut rng,
            };
            modules
                .execute(
                    "masked-blend",
                    &mut context,
                    ModuleRequest {
                        seed: 11,
                        profile: V2Profile::Quality,
                        inputs: vec![base, top],
                    },
                )
                .expect("module should build")
        };

        assert!(result.output_count() >= 1);
        let out = nodes
            .create(
                &mut builder,
                "output",
                NodePayload::Output(OutputNode::primary()),
            )
            .expect("output node");
        builder.connect_luma(result.primary, out);
        builder.build().expect("graph should validate");
    }
}

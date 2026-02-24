//! Regression tests for reusable subgraph module catalog behavior.

use super::node_catalog::{NodeCatalog, NodePayload};
use super::subgraph_catalog::{ModuleBuildContext, ModuleParams, ModuleRequest, SubgraphCatalog};
use crate::graph::GraphBuilder;
use crate::model::XorShift32;
use crate::node::{OutputNode, PortType, SourceNoiseTemporal};
use crate::runtime_config::V2Profile;

#[test]
fn motif_modules_are_registered() {
    let modules = SubgraphCatalog::with_builtins().expect("module catalog should register");
    let keys = modules.keys();
    assert!(keys.contains(&"motif-ribbon"));
    assert!(keys.contains(&"motif-echo"));
    assert!(keys.contains(&"motif-dual-tone"));
}

#[test]
fn motif_module_accepts_parameterized_request() {
    let nodes = NodeCatalog::with_builtins().expect("node catalog should register");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog should register");
    let mut builder = GraphBuilder::new(128, 128, 17);
    let mut rng = XorShift32::new(91);

    let input = nodes
        .create(
            &mut builder,
            "source-noise",
            NodePayload::SourceNoise(crate::graph::SourceNoiseNode {
                seed: 7,
                scale: 1.2,
                octaves: 3,
                amplitude: 0.9,
                output_port: PortType::LumaTexture,
                temporal: SourceNoiseTemporal::default(),
            }),
        )
        .expect("source node");

    let result = {
        let mut module_ctx = ModuleBuildContext {
            builder: &mut builder,
            nodes: &nodes,
            rng: &mut rng,
        };
        modules
            .execute(
                "motif-ribbon",
                &mut module_ctx,
                ModuleRequest::new(33, V2Profile::Quality, vec![input]).with_params(ModuleParams {
                    intensity: 1.35,
                    variation: 0.7,
                    blend_bias: 0.6,
                }),
            )
            .expect("motif module should build")
    };

    assert!(result.output_count() >= 1);
    let output = nodes
        .create(
            &mut builder,
            "output",
            NodePayload::Output(OutputNode::primary()),
        )
        .expect("output node");
    builder.connect_luma(result.primary, output);
    builder.build().expect("graph should validate");
}

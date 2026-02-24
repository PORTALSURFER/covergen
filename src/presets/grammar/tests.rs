use super::*;

use crate::graph::NodeKind;
use crate::presets::node_catalog::NodeCatalog;
use crate::presets::subgraph_catalog::SubgraphCatalog;
use crate::runtime_config::{AnimationConfig, AnimationMotion, V2Config};

fn grammar_config(seed: u32) -> V2Config {
    V2Config {
        width: 512,
        height: 512,
        seed,
        count: 1,
        output: "test.png".to_string(),
        layers: 5,
        antialias: 1,
        preset: "random-grammar".to_string(),
        profile: V2Profile::Quality,
        animation: AnimationConfig {
            enabled: false,
            seconds: 30,
            fps: 30,
            keep_frames: false,
            motion: AnimationMotion::Normal,
        },
        selection: crate::runtime_config::SelectionConfig {
            explore_candidates: 0,
            explore_size: 320,
        },
    }
}

#[test]
fn grammar_builder_is_seed_deterministic() {
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let config = grammar_config(7);
    let context = PresetContext {
        config: &config,
        nodes: &nodes,
        modules: &modules,
    };

    let first = build_constrained_random_grammar(context).expect("first graph");
    let second = build_constrained_random_grammar(context).expect("second graph");
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
}

#[test]
fn grammar_builder_emits_mixed_node_classes() {
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");
    let config = grammar_config(11);
    let context = PresetContext {
        config: &config,
        nodes: &nodes,
        modules: &modules,
    };

    let graph = build_constrained_random_grammar(context).expect("graph");

    let mut has_source_noise = false;
    let mut has_mask = false;
    let mut has_blend = false;
    for node in &graph.nodes {
        match node.kind {
            NodeKind::SourceNoise(_) => has_source_noise = true,
            NodeKind::Mask(_) => has_mask = true,
            NodeKind::Blend(_) => has_blend = true,
            _ => {}
        }
    }

    assert!(
        has_source_noise,
        "grammar graph should include source-noise"
    );
    assert!(has_mask, "grammar graph should include mask nodes");
    assert!(has_blend, "grammar graph should include blend nodes");
}

#[test]
fn grammar_builder_can_emit_stateful_feedback_nodes() {
    let nodes = NodeCatalog::with_builtins().expect("node catalog");
    let modules = SubgraphCatalog::with_builtins().expect("module catalog");

    let mut found_feedback = false;
    for seed in 1..=24 {
        let config = grammar_config(seed);
        let context = PresetContext {
            config: &config,
            nodes: &nodes,
            modules: &modules,
        };
        let graph = build_constrained_random_grammar(context).expect("graph");
        if graph
            .nodes
            .iter()
            .any(|node| matches!(node.kind, NodeKind::StatefulFeedback(_)))
        {
            found_feedback = true;
            break;
        }
    }

    assert!(
        found_feedback,
        "expected at least one seed to emit stateful feedback node"
    );
}

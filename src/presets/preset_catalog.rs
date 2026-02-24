//! Extensible preset registry for V2 graph generation.
//!
//! Presets register as named builders with aliases, so callers can provide
//! custom catalogs without editing central `match` statements.

use std::collections::HashMap;

use crate::graph::{GpuGraph, GraphBuildError};
use crate::runtime_config::V2Config;

use super::families;
use super::grammar;
use super::node_catalog::NodeCatalog;
use super::subgraph_catalog::SubgraphCatalog;
use super::touchdesigner;

/// Build context passed to preset builders.
#[derive(Clone, Copy)]
pub struct PresetContext<'a> {
    pub config: &'a V2Config,
    pub nodes: &'a NodeCatalog,
    pub modules: &'a SubgraphCatalog,
}

/// Function signature for preset graph builders.
pub type PresetBuilder = fn(PresetContext<'_>) -> Result<GpuGraph, GraphBuildError>;

/// Metadata and builder for one preset entry.
#[derive(Clone, Copy)]
pub struct PresetDescriptor {
    pub key: &'static str,
    pub aliases: &'static [&'static str],
    pub build: PresetBuilder,
}

/// Registry of available preset builders.
#[derive(Default)]
pub struct PresetCatalog {
    entries: Vec<PresetDescriptor>,
    lookup: HashMap<String, usize>,
}

impl PresetCatalog {
    /// Create an empty preset registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry containing all built-in preset families.
    pub fn with_builtins() -> Result<Self, GraphBuildError> {
        let mut catalog = Self::new();
        register_builtin_presets(&mut catalog)?;
        Ok(catalog)
    }

    /// Register one preset descriptor and aliases.
    pub fn register(&mut self, descriptor: PresetDescriptor) -> Result<(), GraphBuildError> {
        let slot = self.entries.len();
        self.insert_lookup(descriptor.key, slot)?;
        for alias in descriptor.aliases {
            self.insert_lookup(alias, slot)?;
        }
        self.entries.push(descriptor);
        Ok(())
    }

    /// Build one graph from a preset key/alias.
    pub fn build(
        &self,
        key: &str,
        context: PresetContext<'_>,
    ) -> Result<GpuGraph, GraphBuildError> {
        let descriptor = self.resolve(key)?;
        (descriptor.build)(context)
    }

    /// Return sorted canonical preset keys.
    pub fn keys(&self) -> Vec<&'static str> {
        let mut keys: Vec<&'static str> = self.entries.iter().map(|entry| entry.key).collect();
        keys.sort_unstable();
        keys
    }

    fn resolve(&self, key: &str) -> Result<PresetDescriptor, GraphBuildError> {
        let normalized = normalize(key);
        let index = self.lookup.get(&normalized).copied().ok_or_else(|| {
            GraphBuildError::new(format!(
                "unknown v2 preset '{key}', expected {}",
                self.keys().join("|")
            ))
        })?;
        Ok(self.entries[index])
    }

    fn insert_lookup(&mut self, key: &str, slot: usize) -> Result<(), GraphBuildError> {
        let normalized = normalize(key);
        if self.lookup.insert(normalized.clone(), slot).is_some() {
            return Err(GraphBuildError::new(format!(
                "duplicate preset key/alias '{key}' ({normalized})"
            )));
        }
        Ok(())
    }
}

fn normalize(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn register_builtin_presets(catalog: &mut PresetCatalog) -> Result<(), GraphBuildError> {
    catalog.register(PresetDescriptor {
        key: "hybrid-stack",
        aliases: &["hybrid"],
        build: families::build_hybrid_stack,
    })?;
    catalog.register(PresetDescriptor {
        key: "field-weave",
        aliases: &["field"],
        build: families::build_field_weave,
    })?;
    catalog.register(PresetDescriptor {
        key: "node-weave",
        aliases: &["node"],
        build: families::build_node_weave,
    })?;
    catalog.register(PresetDescriptor {
        key: "mask-atlas",
        aliases: &["atlas"],
        build: families::build_mask_atlas,
    })?;
    catalog.register(PresetDescriptor {
        key: "warp-grid",
        aliases: &["grid"],
        build: families::build_warp_grid,
    })?;
    catalog.register(PresetDescriptor {
        key: "random-grammar",
        aliases: &["grammar", "random"],
        build: grammar::build_constrained_random_grammar,
    })?;
    catalog.register(PresetDescriptor {
        key: "td-primitive-stage",
        aliases: &["td", "touchdesigner"],
        build: touchdesigner::build_td_primitive_stage,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_config::{AnimationConfig, AnimationMotion, V2Profile};

    fn config_for(preset: &str) -> V2Config {
        V2Config {
            width: 256,
            height: 256,
            seed: 7,
            count: 1,
            output: "test.png".to_string(),
            layers: 4,
            antialias: 1,
            preset: preset.to_string(),
            profile: V2Profile::Quality,
            animation: AnimationConfig {
                enabled: false,
                seconds: 30,
                fps: 30,
                keep_frames: false,
                motion: AnimationMotion::Normal,
            },
        }
    }

    #[test]
    fn builtins_resolve_aliases() {
        let presets = PresetCatalog::with_builtins().expect("builtins should register");
        let nodes = NodeCatalog::with_builtins().expect("node catalog should register");
        let modules = SubgraphCatalog::with_builtins().expect("module catalog should register");
        let ctx = PresetContext {
            config: &config_for("field"),
            nodes: &nodes,
            modules: &modules,
        };
        presets.build("field", ctx).expect("alias should build");
    }
}

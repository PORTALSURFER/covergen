//! Constrained random graph grammar for procedural preset generation.
//!
//! This builder creates seeded, non-chaotic random graphs by enforcing node
//! classes and wiring rules. It keeps exploration variety while preserving
//! stable visual structure through bounded modulation depth and core-source
//! ancestry constraints.

mod ops;

#[cfg(test)]
mod tests;

use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder, NodeId};
use crate::model::XorShift32;
use crate::runtime_config::V2Profile;

use super::node_catalog::NodePayload;
use super::preset_catalog::PresetContext;
use super::primitives::{generate_layer_node, render_size};

use ops::{
    add_blend_node, add_feedback_node, add_tone_node, add_warp_node, allows_mask_source,
    allows_modulation, blend_in_core_anchor, choose_index, random_mask_node, random_source_noise,
    take_merge_pair, wire_outputs, BlendNodeParams,
};

const GRAPH_SALT: u32 = 0x4A95_11E3;
const RNG_SALT: u32 = 0xB02F_7C19;

/// Build a deterministic graph from the constrained random grammar preset.
pub(super) fn build_constrained_random_grammar(
    ctx: PresetContext<'_>,
) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let limits = GrammarLimits::from_profile(ctx.config.layers, ctx.config.profile);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ GRAPH_SALT);
    let mut rng = XorShift32::new(ctx.config.seed ^ RNG_SALT);
    let mut state = GrammarState::default();

    seed_sources(&mut builder, ctx, &mut rng, &mut state, limits)?;
    seed_masks(&mut builder, ctx, &mut rng, &mut state, limits)?;
    apply_modulation_passes(&mut builder, ctx, &mut rng, &mut state, limits)?;
    apply_merge_passes(&mut builder, ctx, &mut rng, &mut state, limits)?;

    let primary = finalize_primary(&mut builder, ctx, &mut rng, &mut state, limits)?;
    wire_outputs(&mut builder, ctx, primary.id)?;
    builder.build()
}

#[derive(Clone, Copy)]
pub(super) struct GrammarLimits {
    source_layers: u32,
    source_noise: u32,
    mask_budget: u32,
    modulation_passes: u32,
    merge_passes: u32,
    max_mod_depth: u8,
    mask_connection_chance: f32,
}

impl GrammarLimits {
    fn from_profile(layers: u32, profile: V2Profile) -> Self {
        let base_layers = layers.clamp(2, 8);
        match profile {
            V2Profile::Performance => Self {
                source_layers: base_layers.min(4),
                source_noise: 1,
                mask_budget: 2,
                modulation_passes: base_layers + 1,
                merge_passes: base_layers + 1,
                max_mod_depth: 2,
                mask_connection_chance: 0.42,
            },
            V2Profile::Quality => Self {
                source_layers: (base_layers + 1).min(6),
                source_noise: if base_layers >= 4 { 2 } else { 1 },
                mask_budget: 3,
                modulation_passes: base_layers + 2,
                merge_passes: base_layers + 2,
                max_mod_depth: 3,
                mask_connection_chance: 0.60,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NodeClass {
    CoreSource,
    FieldSource,
    ModulateWarp,
    ModulateTone,
    Composite,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct LumaValue {
    pub(super) id: NodeId,
    pub(super) class: NodeClass,
    pub(super) mod_depth: u8,
    pub(super) has_core_ancestry: bool,
}

#[derive(Default)]
pub(super) struct GrammarState {
    luma_pool: Vec<LumaValue>,
    mask_pool: Vec<NodeId>,
    core_anchors: Vec<NodeId>,
}

impl GrammarState {
    fn push_luma(&mut self, value: LumaValue) {
        if value.has_core_ancestry {
            self.core_anchors.push(value.id);
        }
        self.luma_pool.push(value);
    }

    fn ensure_fallback_luma(&self) -> Result<LumaValue, GraphBuildError> {
        self.luma_pool
            .last()
            .copied()
            .ok_or_else(|| GraphBuildError::new("grammar produced no luma nodes"))
    }
}

fn seed_sources(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    state: &mut GrammarState,
    limits: GrammarLimits,
) -> Result<(), GraphBuildError> {
    for layer_index in 0..limits.source_layers {
        let node = generate_layer_node(
            layer_index,
            limits.source_layers,
            ctx.config.profile,
            rng,
            true,
        );
        let id = ctx
            .nodes
            .create(builder, "generate-layer", NodePayload::GenerateLayer(node))?;
        state.push_luma(LumaValue {
            id,
            class: NodeClass::CoreSource,
            mod_depth: 0,
            has_core_ancestry: true,
        });
    }

    for _ in 0..limits.source_noise {
        let id = ctx.nodes.create(
            builder,
            "source-noise",
            NodePayload::SourceNoise(random_source_noise(rng)),
        )?;
        state.push_luma(LumaValue {
            id,
            class: NodeClass::FieldSource,
            mod_depth: 0,
            has_core_ancestry: false,
        });
    }
    Ok(())
}

fn seed_masks(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    state: &mut GrammarState,
    limits: GrammarLimits,
) -> Result<(), GraphBuildError> {
    if state.luma_pool.is_empty() {
        return Ok(());
    }

    for _ in 0..limits.mask_budget {
        let Some(source_index) = choose_index(&state.luma_pool, rng, |_, value| {
            allows_mask_source(value.class)
        }) else {
            break;
        };
        let source = state.luma_pool[source_index];
        let mask = ctx
            .nodes
            .create(builder, "mask", NodePayload::Mask(random_mask_node(rng)))?;
        builder.connect_luma(source.id, mask);
        state.mask_pool.push(mask);
    }
    Ok(())
}

fn apply_modulation_passes(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    state: &mut GrammarState,
    limits: GrammarLimits,
) -> Result<(), GraphBuildError> {
    for _ in 0..limits.modulation_passes {
        let Some(source_index) = choose_index(&state.luma_pool, rng, |_, value| {
            allows_modulation(value.class, value.mod_depth, limits)
        }) else {
            break;
        };

        let source = state.luma_pool[source_index];
        let roll = rng.next_f32();
        let value = if roll < 0.36 {
            add_feedback_node(builder, ctx, rng, source)?
        } else if roll < 0.68 {
            add_warp_node(builder, ctx, rng, source)?
        } else {
            add_tone_node(builder, ctx, rng, source)?
        };

        if rng.next_f32() < 0.22 && state.mask_pool.len() < limits.mask_budget as usize + 2 {
            let mask =
                ctx.nodes
                    .create(builder, "mask", NodePayload::Mask(random_mask_node(rng)))?;
            builder.connect_luma(value.id, mask);
            state.mask_pool.push(mask);
        }

        state.push_luma(value);
    }
    Ok(())
}

fn apply_merge_passes(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    state: &mut GrammarState,
    limits: GrammarLimits,
) -> Result<(), GraphBuildError> {
    for _ in 0..limits.merge_passes {
        if state.luma_pool.len() < 2 {
            break;
        }
        let pair = take_merge_pair(state, rng)?;
        let merged = add_blend_node(
            builder,
            ctx,
            rng,
            BlendNodeParams {
                left: pair.0,
                right: pair.1,
                state,
                limits,
                collapse_mode: false,
            },
        )?;
        state.push_luma(merged);
    }

    while state.luma_pool.len() > 1 {
        let pair = take_merge_pair(state, rng)?;
        let merged = add_blend_node(
            builder,
            ctx,
            rng,
            BlendNodeParams {
                left: pair.0,
                right: pair.1,
                state,
                limits,
                collapse_mode: true,
            },
        )?;
        state.push_luma(merged);
    }
    Ok(())
}

fn finalize_primary(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    state: &mut GrammarState,
    limits: GrammarLimits,
) -> Result<LumaValue, GraphBuildError> {
    let mut current = state.ensure_fallback_luma()?;

    if !current.has_core_ancestry {
        current = blend_in_core_anchor(builder, ctx, rng, state, current, limits)?;
    }

    current = add_tone_node(builder, ctx, rng, current)?;

    if allows_modulation(current.class, current.mod_depth, limits) && rng.next_f32() < 0.35 {
        current = add_warp_node(builder, ctx, rng, current)?;
    }

    Ok(current)
}

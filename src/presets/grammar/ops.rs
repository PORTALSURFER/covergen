//! Low-level node construction and grammar-rule helpers.

use crate::graph::{
    GraphBuildError, GraphBuilder, MaskNode, NodeId, OutputNode, SourceNoiseNode,
    StatefulFeedbackNode,
};
use crate::model::{LayerBlendMode, XorShift32};
use crate::node::{MaskTemporal, PortType, SourceNoiseTemporal};

use super::super::node_catalog::NodePayload;
use super::super::preset_catalog::PresetContext;
use super::super::primitives::{random_blend, random_tonemap, random_warp};
use super::{GrammarLimits, GrammarState, LumaValue, NodeClass};

pub(super) fn blend_in_core_anchor(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    state: &GrammarState,
    value: LumaValue,
    limits: GrammarLimits,
) -> Result<LumaValue, GraphBuildError> {
    let core = state
        .core_anchors
        .get(pick_index(rng, state.core_anchors.len()))
        .copied()
        .ok_or_else(|| GraphBuildError::new("grammar expected at least one core source"))?;

    add_blend_node(
        builder,
        ctx,
        rng,
        LumaValue {
            id: core,
            class: NodeClass::CoreSource,
            mod_depth: 0,
            has_core_ancestry: true,
        },
        value,
        state,
        limits,
        true,
    )
}

pub(super) fn add_warp_node(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    source: LumaValue,
) -> Result<LumaValue, GraphBuildError> {
    let strength_scale = 0.82 + rng.next_f32() * 0.35;
    let node = ctx.nodes.create(
        builder,
        "warp",
        NodePayload::WarpTransform(random_warp(rng, strength_scale)),
    )?;
    builder.connect_luma(source.id, node);

    Ok(LumaValue {
        id: node,
        class: NodeClass::ModulateWarp,
        mod_depth: source.mod_depth.saturating_add(1),
        has_core_ancestry: source.has_core_ancestry,
    })
}

pub(super) fn add_tone_node(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    source: LumaValue,
) -> Result<LumaValue, GraphBuildError> {
    let node = ctx
        .nodes
        .create(builder, "tone", NodePayload::ToneMap(random_tonemap(rng)))?;
    builder.connect_luma(source.id, node);

    Ok(LumaValue {
        id: node,
        class: NodeClass::ModulateTone,
        mod_depth: source.mod_depth.saturating_add(1),
        has_core_ancestry: source.has_core_ancestry,
    })
}

pub(super) fn add_feedback_node(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    source: LumaValue,
) -> Result<LumaValue, GraphBuildError> {
    let mix = (0.18 + rng.next_f32() * 0.52).clamp(0.0, 0.95);
    let node = ctx.nodes.create(
        builder,
        "feedback",
        NodePayload::StatefulFeedback(StatefulFeedbackNode { mix }),
    )?;
    builder.connect_luma(source.id, node);

    Ok(LumaValue {
        id: node,
        class: NodeClass::Composite,
        mod_depth: source.mod_depth.saturating_add(1),
        has_core_ancestry: source.has_core_ancestry,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn add_blend_node(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    left: LumaValue,
    right: LumaValue,
    state: &GrammarState,
    limits: GrammarLimits,
    collapse_mode: bool,
) -> Result<LumaValue, GraphBuildError> {
    let (min_opacity, max_opacity) = if collapse_mode {
        (0.30, 0.72)
    } else {
        (0.38, 0.90)
    };
    let fallback_mode = if collapse_mode {
        LayerBlendMode::Screen
    } else {
        LayerBlendMode::Overlay
    };

    let node = ctx.nodes.create(
        builder,
        "blend",
        NodePayload::Blend(random_blend(rng, fallback_mode, min_opacity, max_opacity)),
    )?;

    builder.connect_luma_input(left.id, node, 0);
    builder.connect_luma_input(right.id, node, 1);

    if !state.mask_pool.is_empty() && rng.next_f32() < limits.mask_connection_chance {
        let mask = state.mask_pool[pick_index(rng, state.mask_pool.len())];
        builder.connect_mask_input(mask, node, 2);
    }

    Ok(LumaValue {
        id: node,
        class: NodeClass::Composite,
        mod_depth: left.mod_depth.max(right.mod_depth),
        has_core_ancestry: left.has_core_ancestry || right.has_core_ancestry,
    })
}

pub(super) fn take_merge_pair(
    state: &mut GrammarState,
    rng: &mut XorShift32,
) -> Result<(LumaValue, LumaValue), GraphBuildError> {
    if state.luma_pool.len() < 2 {
        return Err(GraphBuildError::new(
            "grammar merge requested with fewer than two luma nodes",
        ));
    }

    let first_index = choose_index(&state.luma_pool, rng, |_, value| {
        allows_blend_source(value.class)
    })
    .unwrap_or_else(|| pick_index(rng, state.luma_pool.len()));

    let first = state.luma_pool[first_index];
    let second_prefer_core = !first.has_core_ancestry;
    let second_index = choose_index(&state.luma_pool, rng, |index, value| {
        index != first_index
            && allows_blend_source(value.class)
            && (!second_prefer_core || value.has_core_ancestry)
    })
    .or_else(|| {
        choose_index(&state.luma_pool, rng, |index, value| {
            index != first_index && allows_blend_source(value.class)
        })
    })
    .ok_or_else(|| GraphBuildError::new("grammar could not select merge pair"))?;

    let (low, high) = if first_index < second_index {
        (first_index, second_index)
    } else {
        (second_index, first_index)
    };

    let right = state.luma_pool.swap_remove(high);
    let left = state.luma_pool.swap_remove(low);
    Ok((left, right))
}

pub(super) fn wire_outputs(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    input: NodeId,
) -> Result<(), GraphBuildError> {
    let tap = ctx
        .nodes
        .create(builder, "output", NodePayload::Output(OutputNode::tap(1)))?;
    builder.connect_luma(input, tap);

    let primary = ctx.nodes.create(
        builder,
        "output",
        NodePayload::Output(OutputNode::primary()),
    )?;
    builder.connect_luma(input, primary);
    Ok(())
}

pub(super) fn random_source_noise(rng: &mut XorShift32) -> SourceNoiseNode {
    SourceNoiseNode {
        seed: rng.next_u32(),
        scale: 0.9 + rng.next_f32() * 5.8,
        octaves: 3 + (rng.next_u32() % 3),
        amplitude: 0.5 + rng.next_f32() * 0.6,
        output_port: PortType::LumaTexture,
        temporal: SourceNoiseTemporal::default(),
    }
}

pub(super) fn random_mask_node(rng: &mut XorShift32) -> MaskNode {
    MaskNode {
        threshold: 0.28 + rng.next_f32() * 0.44,
        softness: 0.04 + rng.next_f32() * 0.20,
        invert: rng.next_f32() < 0.15,
        temporal: MaskTemporal::default(),
    }
}

pub(super) fn allows_modulation(class: NodeClass, mod_depth: u8, limits: GrammarLimits) -> bool {
    if mod_depth >= limits.max_mod_depth {
        return false;
    }
    matches!(
        class,
        NodeClass::CoreSource
            | NodeClass::FieldSource
            | NodeClass::ModulateWarp
            | NodeClass::ModulateTone
            | NodeClass::Composite
    )
}

pub(super) fn allows_mask_source(class: NodeClass) -> bool {
    matches!(
        class,
        NodeClass::CoreSource
            | NodeClass::FieldSource
            | NodeClass::ModulateWarp
            | NodeClass::ModulateTone
            | NodeClass::Composite
    )
}

fn allows_blend_source(class: NodeClass) -> bool {
    matches!(
        class,
        NodeClass::CoreSource
            | NodeClass::FieldSource
            | NodeClass::ModulateWarp
            | NodeClass::ModulateTone
            | NodeClass::Composite
    )
}

pub(super) fn choose_index<F>(
    values: &[LumaValue],
    rng: &mut XorShift32,
    mut predicate: F,
) -> Option<usize>
where
    F: FnMut(usize, &LumaValue) -> bool,
{
    let mut seen = 0u32;
    let mut selected = None;

    for (index, value) in values.iter().enumerate() {
        if !predicate(index, value) {
            continue;
        }
        seen = seen.saturating_add(1);
        if rng.next_u32().is_multiple_of(seen) {
            selected = Some(index);
        }
    }

    selected
}

fn pick_index(rng: &mut XorShift32, len: usize) -> usize {
    (rng.next_u32() as usize) % len.max(1)
}

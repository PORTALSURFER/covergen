//! Shared helper nodes for staged TouchDesigner-style preset builders.

use crate::chop::{ChopLfoNode, ChopRemapNode, ChopWave};
use crate::graph::{GraphBuildError, GraphBuilder, MaskNode, NodeId, SourceNoiseNode};
use crate::model::XorShift32;
use crate::node::{OutputNode, PortType};
use crate::sop::{SopCircleNode, SopSphereNode, TopCameraRenderNode};

use super::node_catalog::NodePayload;
use super::preset_catalog::PresetContext;

/// Texture + mask pair that can be used as a compositing source.
#[derive(Clone, Copy)]
pub(super) struct TextureFx {
    pub noise: NodeId,
    pub mask: NodeId,
}

pub(super) fn add_lfo(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
    wave: ChopWave,
    min_freq: f32,
    max_freq: f32,
) -> Result<NodeId, GraphBuildError> {
    ctx.nodes.create(
        builder,
        "chop-lfo",
        NodePayload::ChopLfo(ChopLfoNode {
            wave,
            frequency: min_freq + rng.next_f32() * (max_freq - min_freq),
            phase: rng.next_f32(),
            amplitude: 0.4 + rng.next_f32() * 0.6,
            offset: 0.75 + rng.next_f32() * 0.45,
        }),
    )
}

pub(super) fn add_remap(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    out_min: f32,
    out_max: f32,
) -> Result<NodeId, GraphBuildError> {
    ctx.nodes.create(
        builder,
        "chop-remap",
        NodePayload::ChopRemap(ChopRemapNode {
            in_min: -1.0,
            in_max: 1.0,
            out_min,
            out_max,
            clamp: true,
        }),
    )
}

pub(super) fn add_circle(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<NodeId, GraphBuildError> {
    ctx.nodes.create(
        builder,
        "sop-circle",
        NodePayload::SopCircle(SopCircleNode {
            radius: 0.14 + rng.next_f32() * 0.24,
            feather: 0.012 + rng.next_f32() * 0.05,
            center_x: (rng.next_f32() - 0.5) * 0.45,
            center_y: (rng.next_f32() - 0.5) * 0.45,
        }),
    )
}

pub(super) fn add_sphere(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<NodeId, GraphBuildError> {
    ctx.nodes.create(
        builder,
        "sop-sphere",
        NodePayload::SopSphere(SopSphereNode {
            radius: 0.18 + rng.next_f32() * 0.2,
            center_x: (rng.next_f32() - 0.5) * 0.35,
            center_y: (rng.next_f32() - 0.5) * 0.35,
            light_x: (rng.next_f32() - 0.5) * 2.0,
            light_y: (rng.next_f32() - 0.5) * 2.0,
            ambient: 0.12 + rng.next_f32() * 0.36,
            deform: 0.0,
            deform_freq: 2.5,
            deform_phase: 0.0,
        }),
    )
}

pub(super) fn add_camera(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<NodeId, GraphBuildError> {
    ctx.nodes.create(
        builder,
        "top-camera-render",
        NodePayload::TopCameraRender(TopCameraRenderNode {
            exposure: 0.85 + rng.next_f32() * 1.05,
            gamma: 0.82 + rng.next_f32() * 0.5,
            zoom: 0.78 + rng.next_f32() * 0.88,
            pan_x: (rng.next_f32() - 0.5) * 0.32,
            pan_y: (rng.next_f32() - 0.5) * 0.32,
            rotate: (rng.next_f32() - 0.5) * 1.35,
            invert: rng.next_f32() < 0.12,
        }),
    )
}

pub(super) fn add_noise_mask_pair(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    rng: &mut XorShift32,
) -> Result<TextureFx, GraphBuildError> {
    let noise = ctx.nodes.create(
        builder,
        "source-noise",
        NodePayload::SourceNoise(SourceNoiseNode {
            seed: rng.next_u32(),
            scale: 1.1 + rng.next_f32() * 6.8,
            octaves: 3 + (rng.next_u32() % 4),
            amplitude: 0.3 + rng.next_f32() * 0.9,
            output_port: PortType::LumaTexture,
            temporal: Default::default(),
        }),
    )?;
    let mask = ctx.nodes.create(
        builder,
        "mask",
        NodePayload::Mask(MaskNode {
            threshold: 0.32 + rng.next_f32() * 0.33,
            softness: 0.05 + rng.next_f32() * 0.22,
            invert: rng.next_f32() < 0.25,
            temporal: Default::default(),
        }),
    )?;
    builder.connect_luma(noise, mask);
    Ok(TextureFx { noise, mask })
}

pub(super) fn add_outputs(
    builder: &mut GraphBuilder,
    ctx: PresetContext<'_>,
    source: NodeId,
) -> Result<(), GraphBuildError> {
    let tap = ctx
        .nodes
        .create(builder, "output", NodePayload::Output(OutputNode::tap(1)))?;
    builder.connect_luma(source, tap);
    let output = ctx.nodes.create(
        builder,
        "output",
        NodePayload::Output(OutputNode::primary()),
    )?;
    builder.connect_luma(source, output);
    Ok(())
}

pub(super) fn pop_random(
    pool: &mut Vec<NodeId>,
    rng: &mut XorShift32,
) -> Result<NodeId, GraphBuildError> {
    if pool.is_empty() {
        return Err(GraphBuildError::new(
            "touchdesigner multi-stage preset has no candidate nodes",
        ));
    }
    let index = (rng.next_u32() as usize) % pool.len();
    Ok(pool.swap_remove(index))
}

pub(super) fn pop_optional(pool: &mut Vec<NodeId>, rng: &mut XorShift32) -> Option<NodeId> {
    if pool.is_empty() {
        None
    } else {
        let index = (rng.next_u32() as usize) % pool.len();
        Some(pool.swap_remove(index))
    }
}

pub(super) fn pick<T: Copy>(items: &[T], rng: &mut XorShift32) -> Result<T, GraphBuildError> {
    if items.is_empty() {
        return Err(GraphBuildError::new(
            "touchdesigner multi-stage preset has empty selection pool",
        ));
    }
    let index = (rng.next_u32() as usize) % items.len();
    Ok(items[index])
}

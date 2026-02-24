//! TouchDesigner-inspired preset builders using CHOP/SOP/TOP composition.

use crate::chop::{ChopLfoNode, ChopMathMode, ChopMathNode, ChopRemapNode, ChopWave};
use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder};
use crate::model::{LayerBlendMode, XorShift32};
use crate::node::OutputNode;
use crate::sop::{SopCircleNode, SopSphereNode, TopCameraRenderNode};

use super::node_catalog::NodePayload;
use super::preset_catalog::PresetContext;
use super::primitives::{random_blend, random_tonemap, render_size};

/// Build a CHOP/SOP/TOP chain with basic camera-rendered primitives.
pub(super) fn build_td_primitive_stage(
    ctx: PresetContext<'_>,
) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0xA7D1_2213);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0x4F92_13B1);

    let lfo_a = ctx.nodes.create(
        &mut builder,
        "chop-lfo",
        NodePayload::ChopLfo(ChopLfoNode {
            wave: ChopWave::Sine,
            frequency: 0.35 + rng.next_f32() * 0.9,
            phase: rng.next_f32(),
            amplitude: 0.65,
            offset: 1.0,
        }),
    )?;
    let lfo_b = ctx.nodes.create(
        &mut builder,
        "chop-lfo",
        NodePayload::ChopLfo(ChopLfoNode {
            wave: ChopWave::Triangle,
            frequency: 0.22 + rng.next_f32() * 0.6,
            phase: rng.next_f32(),
            amplitude: 0.5,
            offset: 0.8,
        }),
    )?;
    let zoom_mod = ctx.nodes.create(
        &mut builder,
        "chop-remap",
        NodePayload::ChopRemap(ChopRemapNode {
            in_min: -1.0,
            in_max: 1.0,
            out_min: 0.7,
            out_max: 1.6,
            clamp: true,
        }),
    )?;
    builder.connect_channel_input(lfo_a, zoom_mod, 0);

    let tone_mod = ctx.nodes.create(
        &mut builder,
        "chop-math",
        NodePayload::ChopMath(ChopMathNode {
            mode: ChopMathMode::Mix,
            value: 1.0,
            blend: 0.55,
        }),
    )?;
    builder.connect_channel_input(lfo_a, tone_mod, 0);
    builder.connect_channel_input(lfo_b, tone_mod, 1);

    let circle = ctx.nodes.create(
        &mut builder,
        "sop-circle",
        NodePayload::SopCircle(SopCircleNode {
            radius: 0.20 + rng.next_f32() * 0.18,
            feather: 0.02 + rng.next_f32() * 0.03,
            center_x: (rng.next_f32() - 0.5) * 0.35,
            center_y: (rng.next_f32() - 0.5) * 0.35,
        }),
    )?;

    let sphere = ctx.nodes.create(
        &mut builder,
        "sop-sphere",
        NodePayload::SopSphere(SopSphereNode {
            radius: 0.22 + rng.next_f32() * 0.16,
            center_x: (rng.next_f32() - 0.5) * 0.3,
            center_y: (rng.next_f32() - 0.5) * 0.3,
            light_x: (rng.next_f32() - 0.5) * 1.4,
            light_y: (rng.next_f32() - 0.5) * 1.4,
            ambient: 0.18 + rng.next_f32() * 0.28,
        }),
    )?;

    let camera_a = ctx.nodes.create(
        &mut builder,
        "top-camera-render",
        NodePayload::TopCameraRender(TopCameraRenderNode {
            exposure: 1.1 + rng.next_f32() * 0.6,
            gamma: 0.9 + rng.next_f32() * 0.4,
            zoom: 1.0 + rng.next_f32() * 0.5,
            pan_x: (rng.next_f32() - 0.5) * 0.2,
            pan_y: (rng.next_f32() - 0.5) * 0.2,
            rotate: (rng.next_f32() - 0.5) * 0.7,
            invert: false,
        }),
    )?;
    builder.connect_sop_input(circle, camera_a, 0);
    builder.connect_channel_input(zoom_mod, camera_a, 1);

    let camera_b = ctx.nodes.create(
        &mut builder,
        "top-camera-render",
        NodePayload::TopCameraRender(TopCameraRenderNode {
            exposure: 0.9 + rng.next_f32() * 0.7,
            gamma: 0.9 + rng.next_f32() * 0.3,
            zoom: 0.95 + rng.next_f32() * 0.55,
            pan_x: (rng.next_f32() - 0.5) * 0.24,
            pan_y: (rng.next_f32() - 0.5) * 0.24,
            rotate: (rng.next_f32() - 0.5) * 1.0,
            invert: rng.next_f32() < 0.12,
        }),
    )?;
    builder.connect_sop_input(sphere, camera_b, 0);
    builder.connect_channel_input(zoom_mod, camera_b, 1);

    let blend = ctx.nodes.create(
        &mut builder,
        "blend",
        NodePayload::Blend(random_blend(&mut rng, LayerBlendMode::Screen, 0.45, 0.90)),
    )?;
    builder.connect_luma_input(camera_a, blend, 0);
    builder.connect_luma_input(camera_b, blend, 1);

    let tone = ctx.nodes.create(
        &mut builder,
        "tone-map",
        NodePayload::ToneMap(random_tonemap(&mut rng)),
    )?;
    builder.connect_luma(blend, tone);
    builder.connect_channel_input(tone_mod, tone, 1);

    let tap = ctx.nodes.create(
        &mut builder,
        "output",
        NodePayload::Output(OutputNode::tap(1)),
    )?;
    builder.connect_luma(tone, tap);
    let output = ctx.nodes.create(
        &mut builder,
        "output",
        NodePayload::Output(OutputNode::primary()),
    )?;
    builder.connect_luma(tone, output);

    builder.build()
}

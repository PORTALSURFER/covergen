//! TouchDesigner-style preset: sphere + noise-driven SOP geometry + camera TOP.

use crate::graph::{GpuGraph, GraphBuildError, GraphBuilder};
use crate::model::XorShift32;
use crate::node::{OutputNode, PortType};
use crate::sop::{SopGeometryNode, SopSphereNode, TopCameraRenderNode};

use super::node_catalog::NodePayload;
use super::preset_catalog::PresetContext;
use super::primitives::render_size;

/// Build a graph-native pipeline:
/// `sop-sphere -> source-noise(channel) -> sop-geometry -> top-camera-render -> output`.
pub(super) fn build_td_sphere_noise_geo(
    ctx: PresetContext<'_>,
) -> Result<GpuGraph, GraphBuildError> {
    let (width, height) = render_size(ctx.config);
    let mut builder = GraphBuilder::new(width, height, ctx.config.seed ^ 0x2D31_8A79);
    let mut rng = XorShift32::new(ctx.config.seed ^ 0x71C9_E423);

    let sphere = ctx.nodes.create(
        &mut builder,
        "sop-sphere",
        NodePayload::SopSphere(SopSphereNode {
            radius: 0.22 + rng.next_f32() * 0.22,
            center_x: (rng.next_f32() - 0.5) * 0.28,
            center_y: (rng.next_f32() - 0.5) * 0.28,
            light_x: (rng.next_f32() - 0.5) * 1.8,
            light_y: (rng.next_f32() - 0.5) * 1.8,
            ambient: 0.15 + rng.next_f32() * 0.3,
        }),
    )?;
    let noise = ctx.nodes.create(
        &mut builder,
        "source-noise",
        NodePayload::SourceNoise(crate::graph::SourceNoiseNode {
            seed: rng.next_u32(),
            scale: 1.4 + rng.next_f32() * 4.0,
            octaves: 3 + (rng.next_u32() % 3),
            amplitude: 0.55 + rng.next_f32() * 0.5,
            output_port: PortType::ChannelScalar,
            temporal: Default::default(),
        }),
    )?;
    let geometry = ctx.nodes.create(
        &mut builder,
        "sop-geometry",
        NodePayload::SopGeometry(SopGeometryNode {
            radius_response: 0.55 + rng.next_f32() * 0.65,
            center_response: 0.25 + rng.next_f32() * 0.45,
            light_response: 0.4 + rng.next_f32() * 0.8,
            bias: (rng.next_f32() - 0.5) * 0.4,
        }),
    )?;
    let camera = ctx.nodes.create(
        &mut builder,
        "top-camera-render",
        NodePayload::TopCameraRender(TopCameraRenderNode {
            exposure: 0.95 + rng.next_f32() * 0.65,
            gamma: 0.9 + rng.next_f32() * 0.35,
            zoom: 0.88 + rng.next_f32() * 0.55,
            pan_x: (rng.next_f32() - 0.5) * 0.14,
            pan_y: (rng.next_f32() - 0.5) * 0.14,
            rotate: (rng.next_f32() - 0.5) * 0.45,
            invert: false,
        }),
    )?;
    let output = ctx.nodes.create(
        &mut builder,
        "output",
        NodePayload::Output(OutputNode::primary()),
    )?;
    let tap = ctx.nodes.create(
        &mut builder,
        "output",
        NodePayload::Output(OutputNode::tap(1)),
    )?;

    builder.connect_sop_input(sphere, geometry, 0);
    builder.connect_channel_input(noise, geometry, 1);
    builder.connect_sop_input(geometry, camera, 0);
    builder.connect_luma(camera, output);
    builder.connect_luma(camera, tap);
    builder.build()
}

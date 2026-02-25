//! GPU TOP preview execution and compositing.
//!
//! This module owns preview texture resources and executes GPU operation
//! chains emitted by `gui::top_view` directly on the device.

mod execution;
mod pipeline;

use bytemuck::{Pod, Zeroable};

use crate::gui::top_view::TopViewerOp;

use super::viewer;
use pipeline::{create_op_pipeline, OP_SHADER_SOURCE};

const PREVIEW_BG: wgpu::Color = wgpu::Color {
    r: 8.0 / 255.0,
    g: 8.0 / 255.0,
    b: 8.0 / 255.0,
    a: 1.0,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct TopOpUniform {
    p0: [f32; 4],
    p1: [f32; 4],
    p2: [f32; 4],
}

impl TopOpUniform {
    fn solid(op: TopViewerOp) -> Self {
        let TopViewerOp::Solid {
            center_x,
            center_y,
            radius,
            feather,
            color_r,
            color_g,
            color_b,
            alpha,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [center_x, center_y, radius, feather],
            p1: [color_r, color_g, color_b, alpha],
            p2: [0.0; 4],
        }
    }

    fn transform(op: TopViewerOp) -> Self {
        let TopViewerOp::Transform {
            brightness,
            gain_r,
            gain_g,
            gain_b,
            alpha_mul,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [brightness, gain_r, gain_g, gain_b],
            p1: [alpha_mul, 0.0, 0.0, 0.0],
            p2: [0.0; 4],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderTargetRef {
    Viewer,
    ScratchA,
    ScratchB,
}

/// GPU-backed TOP preview state for GUI rendering.
#[derive(Debug)]
pub(super) struct TopPreviewRenderer {
    viewer_pipeline: wgpu::RenderPipeline,
    viewer_texture_layout: wgpu::BindGroupLayout,
    viewer_sampler: wgpu::Sampler,
    viewer_bind_group: Option<wgpu::BindGroup>,
    viewer_texture: Option<wgpu::Texture>,
    viewer_texture_view: Option<wgpu::TextureView>,
    viewer_texture_size: (u32, u32),
    viewer_quad_buffer: wgpu::Buffer,
    viewer_visible: bool,

    op_uniform_buffer: wgpu::Buffer,
    op_uniform_bind_group: wgpu::BindGroup,
    op_solid_pipeline: wgpu::RenderPipeline,
    op_transform_pipeline: wgpu::RenderPipeline,

    dummy_texture: wgpu::Texture,
    dummy_bind_group: wgpu::BindGroup,

    scratch_texture_a: Option<wgpu::Texture>,
    scratch_view_a: Option<wgpu::TextureView>,
    scratch_bind_group_a: Option<wgpu::BindGroup>,
    scratch_texture_b: Option<wgpu::Texture>,
    scratch_view_b: Option<wgpu::TextureView>,
    scratch_bind_group_b: Option<wgpu::BindGroup>,
    scratch_texture_size: (u32, u32),
}

impl TopPreviewRenderer {
    /// Create a preview renderer that executes compiled GPU operation chains.
    pub(super) fn new(
        device: &wgpu::Device,
        uniform_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let viewer_texture_layout = viewer::create_texture_bind_group_layout(device);
        let viewer_sampler = viewer::create_texture_sampler(device);
        let viewer_shader = viewer::create_shader_module(device);
        let viewer_pipeline = viewer::create_pipeline(
            device,
            &viewer_shader,
            uniform_layout,
            &viewer_texture_layout,
            surface_format,
        );
        let viewer_quad_buffer = viewer::create_vertex_buffer(device);

        let op_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gui-top-preview-op-uniform"),
            size: std::mem::size_of::<TopOpUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let op_uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gui-top-preview-op-uniform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let op_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gui-top-preview-op-uniform-bind-group"),
            layout: &op_uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: op_uniform_buffer.as_entire_binding(),
            }],
        });

        let op_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gui-top-preview-op-shader"),
            source: wgpu::ShaderSource::Wgsl(OP_SHADER_SOURCE.into()),
        });
        let op_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gui-top-preview-op-pipeline-layout"),
            bind_group_layouts: &[&op_uniform_layout, &viewer_texture_layout],
            push_constant_ranges: &[],
        });
        let op_solid_pipeline = create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_solid",
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let op_transform_pipeline = create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_transform",
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );

        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui-top-preview-dummy-texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let dummy_view = dummy_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let dummy_bind_group = viewer::create_texture_bind_group(
            device,
            &viewer_texture_layout,
            &dummy_view,
            &viewer_sampler,
        );

        Self {
            viewer_pipeline,
            viewer_texture_layout,
            viewer_sampler,
            viewer_bind_group: None,
            viewer_texture: None,
            viewer_texture_view: None,
            viewer_texture_size: (0, 0),
            viewer_quad_buffer,
            viewer_visible: false,
            op_uniform_buffer,
            op_uniform_bind_group,
            op_solid_pipeline,
            op_transform_pipeline,
            dummy_texture,
            dummy_bind_group,
            scratch_texture_a: None,
            scratch_view_a: None,
            scratch_bind_group_a: None,
            scratch_texture_b: None,
            scratch_view_b: None,
            scratch_bind_group_b: None,
            scratch_texture_size: (0, 0),
        }
    }

    /// Draw prepared viewer texture into the right-side panel.
    pub(super) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bind_group: &'a wgpu::BindGroup,
    ) {
        if !self.viewer_visible {
            return;
        }
        let Some(bind_group) = self.viewer_bind_group.as_ref() else {
            return;
        };
        let _keep_dummy_alive = &self.dummy_texture;
        pass.set_pipeline(&self.viewer_pipeline);
        pass.set_bind_group(0, uniform_bind_group, &[]);
        pass.set_bind_group(1, bind_group, &[]);
        pass.set_vertex_buffer(0, self.viewer_quad_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }
}

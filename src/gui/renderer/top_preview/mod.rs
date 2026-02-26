//! GPU TOP preview execution and compositing.
//!
//! This module owns preview texture resources and executes GPU operation
//! chains emitted by `gui::top_view` directly on the device.

mod execution;
mod pipeline;

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use std::num::NonZeroU64;

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
    p3: [f32; 4],
    p4: [f32; 4],
}

impl TopOpUniform {
    fn solid(op: TopViewerOp) -> Self {
        let TopViewerOp::Solid {
            color_r,
            color_g,
            color_b,
            alpha,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [color_r, color_g, color_b, alpha],
            p1: [0.0; 4],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }

    fn circle(op: TopViewerOp) -> Self {
        let TopViewerOp::Circle {
            center_x,
            center_y,
            radius,
            feather,
            line_width,
            noise_amount,
            noise_freq,
            noise_phase,
            noise_twist,
            noise_stretch,
            arc_start_deg,
            arc_end_deg,
            segment_count,
            arc_open,
            color_r,
            color_g,
            color_b,
            alpha,
            ..
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [center_x, center_y, radius, feather],
            p1: [color_r, color_g, color_b, alpha],
            p2: [arc_start_deg, arc_end_deg, segment_count, arc_open],
            p3: [line_width, noise_amount, noise_freq, noise_phase],
            p4: [noise_twist, noise_stretch, 0.0, 0.0],
        }
    }

    fn sphere(op: TopViewerOp) -> Self {
        let TopViewerOp::Sphere {
            center_x,
            center_y,
            radius,
            edge_softness,
            noise_amount,
            noise_freq,
            noise_phase,
            noise_twist,
            noise_stretch,
            light_x,
            light_y,
            light_z,
            ambient,
            color_r,
            color_g,
            color_b,
            alpha,
            ..
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [center_x, center_y, radius, edge_softness],
            p1: [light_x, light_y, light_z, ambient],
            p2: [color_r, color_g, color_b, alpha],
            p3: [noise_amount, noise_freq, noise_phase, noise_twist],
            p4: [noise_stretch, 0.0, 0.0, 0.0],
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
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }

    fn feedback(op: TopViewerOp) -> Self {
        let TopViewerOp::Feedback { mix, .. } = op else {
            return Self::zeroed();
        };
        Self {
            p0: [mix, 0.0, 0.0, 0.0],
            p1: [0.0; 4],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderTargetRef {
    Viewer,
    ScratchA,
    ScratchB,
}

/// Persistent history texture for one feedback node.
#[derive(Debug)]
struct FeedbackHistorySlot {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
}

/// GPU-backed TOP preview state for GUI rendering.
#[derive(Debug)]
pub(super) struct TopPreviewRenderer {
    viewer_pipeline: wgpu::RenderPipeline,
    viewer_texture_layout: wgpu::BindGroupLayout,
    viewer_sampler: wgpu::Sampler,
    op_sampler: wgpu::Sampler,
    viewer_bind_group: Option<wgpu::BindGroup>,
    viewer_texture: Option<wgpu::Texture>,
    viewer_texture_view: Option<wgpu::TextureView>,
    viewer_texture_size: (u32, u32),
    viewer_quad_buffer: wgpu::Buffer,
    viewer_visible: bool,

    op_uniform_layout: wgpu::BindGroupLayout,
    op_uniform_buffer: wgpu::Buffer,
    op_uniform_bind_group: wgpu::BindGroup,
    op_uniform_stride: u64,
    op_uniform_capacity: usize,
    op_solid_pipeline: wgpu::RenderPipeline,
    op_circle_pipeline: wgpu::RenderPipeline,
    op_sphere_pipeline: wgpu::RenderPipeline,
    op_transform_pipeline: wgpu::RenderPipeline,
    op_feedback_pipeline: wgpu::RenderPipeline,

    dummy_texture: wgpu::Texture,
    dummy_bind_group: wgpu::BindGroup,

    scratch_texture_a: Option<wgpu::Texture>,
    scratch_view_a: Option<wgpu::TextureView>,
    scratch_bind_group_a: Option<wgpu::BindGroup>,
    scratch_texture_b: Option<wgpu::Texture>,
    scratch_view_b: Option<wgpu::TextureView>,
    scratch_bind_group_b: Option<wgpu::BindGroup>,
    scratch_texture_size: (u32, u32),
    feedback_history: HashMap<u32, FeedbackHistorySlot>,
}

impl TopPreviewRenderer {
    /// Return non-zero binding size for one operation uniform payload.
    fn op_uniform_binding_size() -> NonZeroU64 {
        NonZeroU64::new(std::mem::size_of::<TopOpUniform>() as u64)
            .expect("top op uniform size must be non-zero")
    }

    /// Return one dynamic-uniform stride aligned to device limits.
    fn op_uniform_stride(device: &wgpu::Device) -> u64 {
        let size = std::mem::size_of::<TopOpUniform>() as u64;
        let alignment = device.limits().min_uniform_buffer_offset_alignment as u64;
        if alignment <= 1 {
            return size;
        }
        let padded = size.saturating_add(alignment.saturating_sub(1));
        (padded / alignment) * alignment
    }

    /// Create bind group that exposes one dynamic uniform slice per op.
    fn create_op_uniform_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gui-top-preview-op-uniform-bind-group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    size: Some(Self::op_uniform_binding_size()),
                }),
            }],
        })
    }

    /// Ensure uniform storage can hold one payload for every operation.
    fn ensure_op_uniform_capacity(&mut self, device: &wgpu::Device, op_count: usize) {
        if op_count <= self.op_uniform_capacity {
            return;
        }
        let next_capacity = op_count.next_power_of_two();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gui-top-preview-op-uniform"),
            size: self.op_uniform_stride.saturating_mul(next_capacity as u64),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group =
            Self::create_op_uniform_bind_group(device, &self.op_uniform_layout, &buffer);
        self.op_uniform_buffer = buffer;
        self.op_uniform_bind_group = bind_group;
        self.op_uniform_capacity = next_capacity;
    }

    /// Return byte offset for one operation's dynamic uniform slice.
    fn op_uniform_offset(&self, op_index: usize) -> u64 {
        self.op_uniform_stride.saturating_mul(op_index as u64)
    }

    /// Create a preview renderer that executes compiled GPU operation chains.
    pub(super) fn new(
        device: &wgpu::Device,
        uniform_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let viewer_texture_layout = viewer::create_texture_bind_group_layout(device);
        let viewer_sampler = viewer::create_texture_sampler(device);
        let op_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gui-top-preview-op-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let viewer_shader = viewer::create_shader_module(device);
        let viewer_pipeline = viewer::create_pipeline(
            device,
            &viewer_shader,
            uniform_layout,
            &viewer_texture_layout,
            surface_format,
        );
        let viewer_quad_buffer = viewer::create_vertex_buffer(device);

        let op_uniform_stride = Self::op_uniform_stride(device);
        let op_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gui-top-preview-op-uniform"),
            size: op_uniform_stride,
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
                    has_dynamic_offset: true,
                    min_binding_size: Some(Self::op_uniform_binding_size()),
                },
                count: None,
            }],
        });
        let op_uniform_bind_group =
            Self::create_op_uniform_bind_group(device, &op_uniform_layout, &op_uniform_buffer);

        let op_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gui-top-preview-op-shader"),
            source: wgpu::ShaderSource::Wgsl(OP_SHADER_SOURCE.into()),
        });
        let op_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gui-top-preview-op-pipeline-layout"),
            bind_group_layouts: &[
                &op_uniform_layout,
                &viewer_texture_layout,
                &viewer_texture_layout,
            ],
            push_constant_ranges: &[],
        });
        let op_solid_pipeline = create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_solid",
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let op_circle_pipeline = create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_circle",
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let op_sphere_pipeline = create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_sphere",
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let op_transform_pipeline = create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_transform",
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let op_feedback_pipeline = create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_feedback",
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
            &op_sampler,
        );

        Self {
            viewer_pipeline,
            viewer_texture_layout,
            viewer_sampler,
            op_sampler,
            viewer_bind_group: None,
            viewer_texture: None,
            viewer_texture_view: None,
            viewer_texture_size: (0, 0),
            viewer_quad_buffer,
            viewer_visible: false,
            op_uniform_layout,
            op_uniform_buffer,
            op_uniform_bind_group,
            op_uniform_stride,
            op_uniform_capacity: 1,
            op_solid_pipeline,
            op_circle_pipeline,
            op_sphere_pipeline,
            op_transform_pipeline,
            op_feedback_pipeline,
            dummy_texture,
            dummy_bind_group,
            scratch_texture_a: None,
            scratch_view_a: None,
            scratch_bind_group_a: None,
            scratch_texture_b: None,
            scratch_view_b: None,
            scratch_bind_group_b: None,
            scratch_texture_size: (0, 0),
            feedback_history: HashMap::new(),
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

//! GPU tex preview execution and compositing.
//!
//! This module owns preview texture resources and executes GPU operation
//! chains emitted by `gui::tex_view` directly on the device.

mod execution;
mod execution_plan;
mod pipeline;

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use std::num::NonZeroU64;

use crate::gpu_timestamp::OptionalGpuTimestampQueries;
use crate::gui::runtime::TexRuntimeFeedbackHistoryBinding;
use crate::gui::tex_view::TexViewerOp;

use super::viewer;
use pipeline::{create_op_pipeline, OP_SHADER_SOURCE};

const PREVIEW_BG: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
pub(super) const TEX_PREVIEW_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct TexOpUniform {
    p0: [f32; 4],
    p1: [f32; 4],
    p2: [f32; 4],
    p3: [f32; 4],
    p4: [f32; 4],
}

impl TexOpUniform {
    fn solid(op: TexViewerOp) -> Self {
        let TexViewerOp::Solid {
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

    fn circle(op: TexViewerOp) -> Self {
        let TexViewerOp::Circle {
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

    fn sphere(op: TexViewerOp) -> Self {
        let TexViewerOp::Sphere {
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

    fn transform(op: TexViewerOp) -> Self {
        let TexViewerOp::Transform {
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

    fn level(op: TexViewerOp) -> Self {
        let TexViewerOp::Level {
            in_low,
            in_high,
            gamma,
            out_low,
            out_high,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [in_low, in_high, gamma, 0.0],
            p1: [out_low, out_high, 0.0, 0.0],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }

    fn feedback(op: TexViewerOp) -> Self {
        let TexViewerOp::Feedback { mix, .. } = op else {
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

    fn reaction_diffusion(op: TexViewerOp) -> Self {
        let TexViewerOp::ReactionDiffusion {
            diffusion_a,
            diffusion_b,
            feed,
            kill,
            dt,
            seed_mix,
            ..
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [diffusion_a, diffusion_b, feed, kill],
            p1: [seed_mix, dt, 0.0, 0.0],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }

    fn blend(op: TexViewerOp) -> Self {
        let TexViewerOp::Blend {
            mode,
            opacity,
            bg_r,
            bg_g,
            bg_b,
            bg_a,
            ..
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [mode, opacity, 0.0, 0.0],
            p1: [bg_r, bg_g, bg_b, bg_a],
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
    FeedbackHistory {
        key: FeedbackHistoryKey,
        slot_index: usize,
    },
}

/// Stable storage key for one feedback history texture slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum FeedbackHistoryKey {
    Internal { feedback_node_id: u32 },
    External { texture_node_id: u32 },
}

impl FeedbackHistoryKey {
    fn from_binding(binding: TexRuntimeFeedbackHistoryBinding) -> Self {
        match binding {
            TexRuntimeFeedbackHistoryBinding::Internal { feedback_node_id } => {
                Self::Internal { feedback_node_id }
            }
            TexRuntimeFeedbackHistoryBinding::External { texture_node_id } => {
                Self::External { texture_node_id }
            }
        }
    }
}

/// Cached texture slot used by preview runtime operations.
#[derive(Debug)]
struct CachedTextureSlot {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
}

/// Ping-pong history textures for one feedback storage key.
#[derive(Debug)]
struct FeedbackHistorySlot {
    slots: [CachedTextureSlot; 2],
    read_index: usize,
}

/// GPU-backed tex preview state for GUI rendering.
#[derive(Debug)]
pub(super) struct TexPreviewRenderer {
    viewer_pipeline: wgpu::RenderPipeline,
    viewer_texture_layout: wgpu::BindGroupLayout,
    viewer_sampler: wgpu::Sampler,
    op_sampler: wgpu::Sampler,
    op_surface_format: wgpu::TextureFormat,
    viewer_bind_group: Option<wgpu::BindGroup>,
    viewer_texture: Option<wgpu::Texture>,
    viewer_texture_view: Option<wgpu::TextureView>,
    viewer_texture_size: (u32, u32),
    viewer_quad_buffer: wgpu::Buffer,
    viewer_visible: bool,
    export_preview_quad_buffer: wgpu::Buffer,
    export_preview_visible: bool,

    op_uniform_layout: wgpu::BindGroupLayout,
    op_uniform_buffer: wgpu::Buffer,
    op_uniform_bind_group: wgpu::BindGroup,
    op_uniform_stride: u64,
    op_uniform_capacity: usize,
    op_uniform_staging: Vec<u8>,
    op_solid_pipeline: Option<wgpu::RenderPipeline>,
    op_circle_pipeline: Option<wgpu::RenderPipeline>,
    op_sphere_pipeline: Option<wgpu::RenderPipeline>,
    op_transform_pipeline: Option<wgpu::RenderPipeline>,
    op_level_pipeline: Option<wgpu::RenderPipeline>,
    op_transform_fused_pipeline: Option<wgpu::RenderPipeline>,
    op_feedback_pipeline: Option<wgpu::RenderPipeline>,
    op_reaction_diffusion_pipeline: Option<wgpu::RenderPipeline>,
    op_blend_pipeline: Option<wgpu::RenderPipeline>,

    dummy_texture: Option<wgpu::Texture>,
    dummy_bind_group: Option<wgpu::BindGroup>,

    scratch_texture_a: Option<wgpu::Texture>,
    scratch_view_a: Option<wgpu::TextureView>,
    scratch_bind_group_a: Option<wgpu::BindGroup>,
    scratch_texture_b: Option<wgpu::Texture>,
    scratch_view_b: Option<wgpu::TextureView>,
    scratch_bind_group_b: Option<wgpu::BindGroup>,
    scratch_texture_size: (u32, u32),
    feedback_history: HashMap<FeedbackHistoryKey, FeedbackHistorySlot>,
    blend_source_slots: HashMap<u32, CachedTextureSlot>,
    blend_source_aliases: HashMap<u32, RenderTargetRef>,
    op_pass_timestamps: OptionalGpuTimestampQueries,
}

impl TexPreviewRenderer {
    /// Return non-zero binding size for one operation uniform payload.
    fn op_uniform_binding_size() -> NonZeroU64 {
        NonZeroU64::new(std::mem::size_of::<TexOpUniform>() as u64)
            .expect("tex op uniform size must be non-zero")
    }

    /// Return one dynamic-uniform stride aligned to device limits.
    fn op_uniform_stride(device: &wgpu::Device) -> u64 {
        let size = std::mem::size_of::<TexOpUniform>() as u64;
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
            label: Some("gui-tex-preview-op-uniform-bind-group"),
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
            label: Some("gui-tex-preview-op-uniform"),
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

    /// Lazily create operation pipelines on first tex-op execution.
    fn ensure_op_pipelines(&mut self, device: &wgpu::Device) {
        if self.op_solid_pipeline.is_some()
            && self.op_circle_pipeline.is_some()
            && self.op_sphere_pipeline.is_some()
            && self.op_transform_pipeline.is_some()
            && self.op_level_pipeline.is_some()
            && self.op_transform_fused_pipeline.is_some()
            && self.op_feedback_pipeline.is_some()
            && self.op_reaction_diffusion_pipeline.is_some()
            && self.op_blend_pipeline.is_some()
        {
            return;
        }
        let op_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gui-tex-preview-op-shader"),
            source: wgpu::ShaderSource::Wgsl(OP_SHADER_SOURCE.into()),
        });
        let op_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gui-tex-preview-op-pipeline-layout"),
            bind_group_layouts: &[
                &self.op_uniform_layout,
                &self.viewer_texture_layout,
                &self.viewer_texture_layout,
            ],
            push_constant_ranges: &[],
        });
        self.op_solid_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_solid",
            self.op_surface_format,
        ));
        self.op_circle_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_circle",
            self.op_surface_format,
        ));
        self.op_sphere_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_sphere",
            self.op_surface_format,
        ));
        self.op_transform_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_transform",
            self.op_surface_format,
        ));
        self.op_level_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_level",
            self.op_surface_format,
        ));
        self.op_transform_fused_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_transform_fused",
            self.op_surface_format,
        ));
        self.op_feedback_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_feedback",
            self.op_surface_format,
        ));
        self.op_reaction_diffusion_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_reaction_diffusion",
            self.op_surface_format,
        ));
        self.op_blend_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_blend",
            self.op_surface_format,
        ));
    }

    /// Lazily create fallback bind group used when an op input is missing.
    fn ensure_dummy_bind_group(&mut self, device: &wgpu::Device) {
        if self.dummy_bind_group.is_some() {
            return;
        }
        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui-tex-preview-dummy-texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEX_PREVIEW_TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let dummy_view = dummy_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let dummy_bind_group = viewer::create_texture_bind_group(
            device,
            &self.viewer_texture_layout,
            &dummy_view,
            &self.op_sampler,
        );
        self.dummy_texture = Some(dummy_texture);
        self.dummy_bind_group = Some(dummy_bind_group);
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
            label: Some("gui-tex-preview-op-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
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
        let export_preview_quad_buffer = viewer::create_vertex_buffer(device);

        let op_uniform_stride = Self::op_uniform_stride(device);
        let op_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gui-tex-preview-op-uniform"),
            size: op_uniform_stride,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let op_uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gui-tex-preview-op-uniform-layout"),
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
        let op_pass_timestamps =
            OptionalGpuTimestampQueries::new(device, "gui-tex-preview-op-pass", 2048);

        Self {
            viewer_pipeline,
            viewer_texture_layout,
            viewer_sampler,
            op_sampler,
            op_surface_format: TEX_PREVIEW_TEXTURE_FORMAT,
            viewer_bind_group: None,
            viewer_texture: None,
            viewer_texture_view: None,
            viewer_texture_size: (0, 0),
            viewer_quad_buffer,
            viewer_visible: false,
            export_preview_quad_buffer,
            export_preview_visible: false,
            op_uniform_layout,
            op_uniform_buffer,
            op_uniform_bind_group,
            op_uniform_stride,
            op_uniform_capacity: 1,
            op_uniform_staging: Vec::new(),
            op_solid_pipeline: None,
            op_circle_pipeline: None,
            op_sphere_pipeline: None,
            op_transform_pipeline: None,
            op_level_pipeline: None,
            op_transform_fused_pipeline: None,
            op_feedback_pipeline: None,
            op_reaction_diffusion_pipeline: None,
            op_blend_pipeline: None,
            dummy_texture: None,
            dummy_bind_group: None,
            scratch_texture_a: None,
            scratch_view_a: None,
            scratch_bind_group_a: None,
            scratch_texture_b: None,
            scratch_view_b: None,
            scratch_bind_group_b: None,
            scratch_texture_size: (0, 0),
            feedback_history: HashMap::new(),
            blend_source_slots: HashMap::new(),
            blend_source_aliases: HashMap::new(),
            op_pass_timestamps,
        }
    }

    /// Draw prepared main viewer texture into the right-side panel.
    pub(super) fn draw_main_viewer<'a>(
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

    /// Draw prepared export preview texture clone into the export popup slot.
    pub(super) fn draw_export_preview<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bind_group: &'a wgpu::BindGroup,
    ) {
        if !self.viewer_visible || !self.export_preview_visible {
            return;
        }
        let Some(bind_group) = self.viewer_bind_group.as_ref() else {
            return;
        };
        pass.set_pipeline(&self.viewer_pipeline);
        pass.set_bind_group(0, uniform_bind_group, &[]);
        pass.set_bind_group(1, bind_group, &[]);
        pass.set_vertex_buffer(0, self.export_preview_quad_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }

    /// Return current viewer render target texture and size.
    pub(in crate::gui::renderer) fn viewer_texture_and_size(
        &self,
    ) -> Option<(&wgpu::Texture, (u32, u32))> {
        let texture = self.viewer_texture.as_ref()?;
        if self.viewer_texture_size.0 == 0 || self.viewer_texture_size.1 == 0 {
            return None;
        }
        Some((texture, self.viewer_texture_size))
    }
}

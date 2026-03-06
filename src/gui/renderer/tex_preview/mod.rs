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
pub(super) const TEX_PREVIEW_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

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

    fn box_shape(op: TexViewerOp) -> Self {
        let TexViewerOp::Box {
            center_x,
            center_y,
            size_x,
            size_y,
            corner_radius,
            edge_softness,
            noise_amount,
            noise_freq,
            noise_phase,
            noise_twist,
            noise_stretch,
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
            p0: [center_x, center_y, size_x, size_y],
            p1: [corner_radius, edge_softness, noise_amount, noise_freq],
            p2: [noise_phase, noise_twist, noise_stretch, 0.0],
            p3: [color_r, color_g, color_b, alpha],
            p4: [0.0; 4],
        }
    }

    fn grid(op: TexViewerOp) -> Self {
        let TexViewerOp::Grid {
            center_x,
            center_y,
            size_x,
            size_y,
            cells_x,
            cells_y,
            line_width,
            edge_softness,
            noise_amount,
            noise_freq,
            noise_phase,
            noise_twist,
            noise_stretch,
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
            p0: [center_x, center_y, size_x, size_y],
            p1: [cells_x, cells_y, line_width, edge_softness],
            p2: [noise_amount, noise_freq, noise_phase, noise_twist],
            p3: [noise_stretch, color_r, color_g, color_b],
            p4: [alpha, 0.0, 0.0, 0.0],
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

    fn source_noise(op: TexViewerOp) -> Self {
        let TexViewerOp::SourceNoise {
            seed,
            scale,
            octaves,
            amplitude,
            mode,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [seed, scale, octaves, amplitude],
            p1: [mode, 0.0, 0.0, 0.0],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }

    fn transform_2d(op: TexViewerOp) -> Self {
        let TexViewerOp::Transform2D {
            offset_x,
            offset_y,
            scale_x,
            scale_y,
            rotate_deg,
            pivot_x,
            pivot_y,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [offset_x, offset_y, scale_x, scale_y],
            p1: [rotate_deg, pivot_x, pivot_y, 0.0],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }

    fn color_adjust(op: TexViewerOp) -> Self {
        let TexViewerOp::ColorAdjust {
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

    fn mask(op: TexViewerOp) -> Self {
        let TexViewerOp::Mask {
            threshold,
            softness,
            invert,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [threshold, softness, invert, 0.0],
            p1: [0.0; 4],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }

    fn morphology(op: TexViewerOp) -> Self {
        let TexViewerOp::Morphology {
            mode,
            radius,
            amount,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [mode, radius, amount, 0.0],
            p1: [0.0; 4],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }

    fn tone_map(op: TexViewerOp) -> Self {
        let TexViewerOp::ToneMap {
            contrast,
            low_pct,
            high_pct,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [contrast, low_pct, high_pct, 0.0],
            p1: [0.0; 4],
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

    fn domain_warp(op: TexViewerOp) -> Self {
        let TexViewerOp::DomainWarp {
            strength,
            frequency,
            rotation,
            octaves,
            ..
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [strength, frequency, rotation, octaves],
            p1: [0.0; 4],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }

    fn directional_smear(op: TexViewerOp) -> Self {
        let TexViewerOp::DirectionalSmear {
            angle,
            length,
            jitter,
            amount,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [angle, length, jitter, amount],
            p1: [0.0; 4],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }

    fn warp_transform(op: TexViewerOp) -> Self {
        let TexViewerOp::WarpTransform {
            strength,
            frequency,
            phase,
        } = op
        else {
            return Self::zeroed();
        };
        Self {
            p0: [strength, frequency, phase, 0.0],
            p1: [0.0; 4],
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

    fn post_process(op: TexViewerOp) -> Self {
        let TexViewerOp::PostProcess {
            category,
            effect,
            amount,
            scale,
            threshold,
            speed,
            time,
            ..
        } = op
        else {
            return Self::zeroed();
        };
        let category_id = match category {
            crate::gui::runtime::PostProcessCategory::ColorTone => 0.0,
            crate::gui::runtime::PostProcessCategory::EdgeStructure => 1.0,
            crate::gui::runtime::PostProcessCategory::BlurDiffusion => 2.0,
            crate::gui::runtime::PostProcessCategory::Distortion => 3.0,
            crate::gui::runtime::PostProcessCategory::Temporal => 4.0,
            crate::gui::runtime::PostProcessCategory::NoiseTexture => 5.0,
            crate::gui::runtime::PostProcessCategory::Lighting => 6.0,
            crate::gui::runtime::PostProcessCategory::ScreenSpace => 7.0,
            crate::gui::runtime::PostProcessCategory::Experimental => 8.0,
        };
        Self {
            p0: [category_id, effect, amount, scale],
            p1: [threshold, speed, time, 0.0],
            p2: [0.0; 4],
            p3: [0.0; 4],
            p4: [0.0; 4],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    /// Countdown in feedback-op invocations until the next history write.
    write_cooldown: u32,
    /// Last applied frame-gap value used to detect parameter changes.
    configured_gap: u32,
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
    cached_viewer_quad_rect: Option<crate::gui::geometry::Rect>,
    export_preview_quad_buffer: wgpu::Buffer,
    export_preview_visible: bool,
    cached_export_preview_quad_rect: Option<crate::gui::geometry::Rect>,

    op_uniform_layout: wgpu::BindGroupLayout,
    op_uniform_buffer: wgpu::Buffer,
    op_uniform_bind_group: wgpu::BindGroup,
    op_uniform_stride: u64,
    op_uniform_capacity: usize,
    op_uniform_staging: Vec<u8>,
    cached_plan_ops: Vec<TexViewerOp>,
    cached_plan_steps: Vec<execution_plan::PlannedStep>,
    cached_plan_render_ops: Vec<execution_plan::PlannedRenderOp>,
    cached_plan_signature: Option<u64>,
    op_uniform_signature: Option<u64>,
    op_solid_pipeline: Option<wgpu::RenderPipeline>,
    op_circle_pipeline: Option<wgpu::RenderPipeline>,
    op_box_pipeline: Option<wgpu::RenderPipeline>,
    op_grid_pipeline: Option<wgpu::RenderPipeline>,
    op_sphere_pipeline: Option<wgpu::RenderPipeline>,
    op_source_noise_pipeline: Option<wgpu::RenderPipeline>,
    op_transform_2d_pipeline: Option<wgpu::RenderPipeline>,
    op_color_adjust_pipeline: Option<wgpu::RenderPipeline>,
    op_level_pipeline: Option<wgpu::RenderPipeline>,
    op_mask_pipeline: Option<wgpu::RenderPipeline>,
    op_morphology_pipeline: Option<wgpu::RenderPipeline>,
    op_tone_map_pipeline: Option<wgpu::RenderPipeline>,
    op_color_adjust_fused_pipeline: Option<wgpu::RenderPipeline>,
    op_feedback_pipeline: Option<wgpu::RenderPipeline>,
    op_reaction_diffusion_pipeline: Option<wgpu::RenderPipeline>,
    op_domain_warp_pipeline: Option<wgpu::RenderPipeline>,
    op_directional_smear_pipeline: Option<wgpu::RenderPipeline>,
    op_warp_transform_pipeline: Option<wgpu::RenderPipeline>,
    op_post_process_pipeline: Option<wgpu::RenderPipeline>,
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
    blend_source_aliases_by_target: HashMap<RenderTargetRef, Vec<u32>>,
    blend_alias_materialize_scratch: Vec<u32>,
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
            && self.op_box_pipeline.is_some()
            && self.op_grid_pipeline.is_some()
            && self.op_sphere_pipeline.is_some()
            && self.op_source_noise_pipeline.is_some()
            && self.op_transform_2d_pipeline.is_some()
            && self.op_color_adjust_pipeline.is_some()
            && self.op_level_pipeline.is_some()
            && self.op_mask_pipeline.is_some()
            && self.op_morphology_pipeline.is_some()
            && self.op_tone_map_pipeline.is_some()
            && self.op_color_adjust_fused_pipeline.is_some()
            && self.op_feedback_pipeline.is_some()
            && self.op_reaction_diffusion_pipeline.is_some()
            && self.op_domain_warp_pipeline.is_some()
            && self.op_directional_smear_pipeline.is_some()
            && self.op_warp_transform_pipeline.is_some()
            && self.op_post_process_pipeline.is_some()
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
        self.op_box_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_box",
            self.op_surface_format,
        ));
        self.op_grid_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_grid",
            self.op_surface_format,
        ));
        self.op_sphere_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_sphere",
            self.op_surface_format,
        ));
        self.op_source_noise_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_source_noise",
            self.op_surface_format,
        ));
        self.op_transform_2d_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_transform_2d",
            self.op_surface_format,
        ));
        self.op_color_adjust_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_color_adjust",
            self.op_surface_format,
        ));
        self.op_level_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_level",
            self.op_surface_format,
        ));
        self.op_mask_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_mask",
            self.op_surface_format,
        ));
        self.op_morphology_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_morphology",
            self.op_surface_format,
        ));
        self.op_tone_map_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_tone_map",
            self.op_surface_format,
        ));
        self.op_color_adjust_fused_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_color_adjust_fused",
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
        self.op_domain_warp_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_domain_warp",
            self.op_surface_format,
        ));
        self.op_directional_smear_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_directional_smear",
            self.op_surface_format,
        ));
        self.op_warp_transform_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_warp_transform",
            self.op_surface_format,
        ));
        self.op_post_process_pipeline = Some(create_op_pipeline(
            device,
            &op_shader,
            &op_pipeline_layout,
            "fs_post_process",
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
            cached_viewer_quad_rect: None,
            export_preview_quad_buffer,
            export_preview_visible: false,
            cached_export_preview_quad_rect: None,
            op_uniform_layout,
            op_uniform_buffer,
            op_uniform_bind_group,
            op_uniform_stride,
            op_uniform_capacity: 1,
            op_uniform_staging: Vec::new(),
            cached_plan_ops: Vec::new(),
            cached_plan_steps: Vec::new(),
            cached_plan_render_ops: Vec::new(),
            cached_plan_signature: None,
            op_uniform_signature: None,
            op_solid_pipeline: None,
            op_circle_pipeline: None,
            op_box_pipeline: None,
            op_grid_pipeline: None,
            op_sphere_pipeline: None,
            op_source_noise_pipeline: None,
            op_transform_2d_pipeline: None,
            op_color_adjust_pipeline: None,
            op_level_pipeline: None,
            op_mask_pipeline: None,
            op_morphology_pipeline: None,
            op_tone_map_pipeline: None,
            op_color_adjust_fused_pipeline: None,
            op_feedback_pipeline: None,
            op_reaction_diffusion_pipeline: None,
            op_domain_warp_pipeline: None,
            op_directional_smear_pipeline: None,
            op_warp_transform_pipeline: None,
            op_post_process_pipeline: None,
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
            blend_source_aliases_by_target: HashMap::new(),
            blend_alias_materialize_scratch: Vec::new(),
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

    /// Drop cached feedback history slots for one feedback node.
    pub(in crate::gui::renderer) fn reset_feedback_history(
        &mut self,
        feedback_node_id: u32,
        accumulation_texture_node_id: Option<u32>,
    ) -> bool {
        let mut cleared = self
            .feedback_history
            .remove(&FeedbackHistoryKey::Internal { feedback_node_id })
            .is_some();
        if let Some(texture_node_id) = accumulation_texture_node_id {
            cleared |= self
                .feedback_history
                .remove(&FeedbackHistoryKey::External { texture_node_id })
                .is_some();
        }
        cleared
    }

    /// Return current viewer render-target size when available.
    pub(in crate::gui::renderer) fn viewer_texture_size(&self) -> Option<(u32, u32)> {
        if self.viewer_texture.is_none()
            || self.viewer_texture_size.0 == 0
            || self.viewer_texture_size.1 == 0
        {
            return None;
        }
        Some(self.viewer_texture_size)
    }

    /// Return current viewer render target texture when available.
    pub(in crate::gui::renderer) fn viewer_texture(&self) -> Option<&wgpu::Texture> {
        self.viewer_texture.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::{FeedbackHistoryKey, TexOpUniform};
    use crate::gui::runtime::{TexRuntimeFeedbackHistoryBinding, TexRuntimeOp};

    #[test]
    fn feedback_history_key_maps_internal_and_external_bindings() {
        let internal =
            FeedbackHistoryKey::from_binding(TexRuntimeFeedbackHistoryBinding::Internal {
                feedback_node_id: 42,
            });
        let external =
            FeedbackHistoryKey::from_binding(TexRuntimeFeedbackHistoryBinding::External {
                texture_node_id: 7,
            });
        assert_eq!(
            internal,
            FeedbackHistoryKey::Internal {
                feedback_node_id: 42
            }
        );
        assert_eq!(
            external,
            FeedbackHistoryKey::External { texture_node_id: 7 }
        );
    }

    #[test]
    fn circle_uniform_maps_center_radius_and_color_fields() {
        let uniform = TexOpUniform::circle(TexRuntimeOp::Circle {
            center_x: 0.15,
            center_y: 0.65,
            radius: 0.33,
            feather: 0.12,
            line_width: 0.08,
            noise_amount: 0.45,
            noise_freq: 3.0,
            noise_phase: 0.5,
            noise_twist: 0.35,
            noise_stretch: 0.2,
            arc_start_deg: 15.0,
            arc_end_deg: 290.0,
            segment_count: 7.0,
            arc_open: 1.0,
            color_r: 0.2,
            color_g: 0.4,
            color_b: 0.9,
            alpha: 0.75,
            alpha_clip: false,
        });
        assert_eq!(uniform.p0, [0.15, 0.65, 0.33, 0.12]);
        assert_eq!(uniform.p1, [0.2, 0.4, 0.9, 0.75]);
        assert_eq!(uniform.p2, [15.0, 290.0, 7.0, 1.0]);
        assert_eq!(uniform.p3, [0.08, 0.45, 3.0, 0.5]);
        assert_eq!(uniform.p4, [0.35, 0.2, 0.0, 0.0]);
    }

    #[test]
    fn box_uniform_maps_size_corner_noise_and_color_fields() {
        let uniform = TexOpUniform::box_shape(TexRuntimeOp::Box {
            center_x: 0.5,
            center_y: 0.4,
            size_x: 0.7,
            size_y: 0.3,
            corner_radius: 0.08,
            edge_softness: 0.02,
            noise_amount: 0.4,
            noise_freq: 2.6,
            noise_phase: 0.7,
            noise_twist: 0.2,
            noise_stretch: 0.15,
            color_r: 0.8,
            color_g: 0.6,
            color_b: 0.4,
            alpha: 0.9,
            alpha_clip: false,
        });
        assert_eq!(uniform.p0, [0.5, 0.4, 0.7, 0.3]);
        assert_eq!(uniform.p1, [0.08, 0.02, 0.4, 2.6]);
        assert_eq!(uniform.p2, [0.7, 0.2, 0.15, 0.0]);
        assert_eq!(uniform.p3, [0.8, 0.6, 0.4, 0.9]);
    }

    #[test]
    fn grid_uniform_maps_size_cells_line_and_color_fields() {
        let uniform = TexOpUniform::grid(TexRuntimeOp::Grid {
            center_x: 0.5,
            center_y: 0.5,
            size_x: 0.8,
            size_y: 0.6,
            cells_x: 10.0,
            cells_y: 6.0,
            line_width: 0.015,
            edge_softness: 0.01,
            noise_amount: 0.35,
            noise_freq: 1.9,
            noise_phase: 0.4,
            noise_twist: 0.3,
            noise_stretch: 0.2,
            color_r: 0.7,
            color_g: 0.8,
            color_b: 0.9,
            alpha: 0.85,
            alpha_clip: true,
        });
        assert_eq!(uniform.p0, [0.5, 0.5, 0.8, 0.6]);
        assert_eq!(uniform.p1, [10.0, 6.0, 0.015, 0.01]);
        assert_eq!(uniform.p2, [0.35, 1.9, 0.4, 0.3]);
        assert_eq!(uniform.p3, [0.2, 0.7, 0.8, 0.9]);
        assert_eq!(uniform.p4, [0.85, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn sphere_uniform_maps_light_and_noise_fields() {
        let uniform = TexOpUniform::sphere(TexRuntimeOp::Sphere {
            center_x: 0.3,
            center_y: 0.4,
            radius: 0.5,
            edge_softness: 0.25,
            noise_amount: 0.6,
            noise_freq: 2.5,
            noise_phase: 0.8,
            noise_twist: 0.45,
            noise_stretch: 0.35,
            light_x: -0.2,
            light_y: 0.15,
            light_z: 0.9,
            ambient: 0.3,
            color_r: 0.7,
            color_g: 0.1,
            color_b: 0.5,
            alpha: 0.85,
            alpha_clip: true,
        });
        assert_eq!(uniform.p0, [0.3, 0.4, 0.5, 0.25]);
        assert_eq!(uniform.p1, [-0.2, 0.15, 0.9, 0.3]);
        assert_eq!(uniform.p2, [0.7, 0.1, 0.5, 0.85]);
        assert_eq!(uniform.p3, [0.6, 2.5, 0.8, 0.45]);
        assert_eq!(uniform.p4, [0.35, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn source_noise_uniform_maps_seed_scale_octaves_and_amplitude() {
        let uniform = TexOpUniform::source_noise(TexRuntimeOp::SourceNoise {
            seed: 13.0,
            scale: 2.8,
            octaves: 5.0,
            amplitude: 0.7,
            mode: 3.0,
        });
        assert_eq!(uniform.p0, [13.0, 2.8, 5.0, 0.7]);
        assert_eq!(uniform.p1, [3.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn morphology_uniform_maps_mode_radius_and_amount() {
        let uniform = TexOpUniform::morphology(TexRuntimeOp::Morphology {
            mode: 2.0,
            radius: 1.5,
            amount: 0.8,
        });
        assert_eq!(uniform.p0, [2.0, 1.5, 0.8, 0.0]);
    }

    #[test]
    fn tone_map_uniform_maps_contrast_and_percentiles() {
        let uniform = TexOpUniform::tone_map(TexRuntimeOp::ToneMap {
            contrast: 1.6,
            low_pct: 0.08,
            high_pct: 0.92,
        });
        assert_eq!(uniform.p0, [1.6, 0.08, 0.92, 0.0]);
    }

    #[test]
    fn domain_warp_uniform_maps_strength_frequency_rotation_and_octaves() {
        let uniform = TexOpUniform::domain_warp(TexRuntimeOp::DomainWarp {
            strength: 0.42,
            frequency: 3.2,
            rotation: 24.0,
            octaves: 4.0,
            base_texture_node_id: 7,
            warp_texture_node_id: Some(9),
        });
        assert_eq!(uniform.p0, [0.42, 3.2, 24.0, 4.0]);
    }

    #[test]
    fn directional_smear_uniform_maps_angle_length_jitter_and_amount() {
        let uniform = TexOpUniform::directional_smear(TexRuntimeOp::DirectionalSmear {
            angle: 90.0,
            length: 18.0,
            jitter: 0.2,
            amount: 0.55,
        });
        assert_eq!(uniform.p0, [90.0, 18.0, 0.2, 0.55]);
    }
}

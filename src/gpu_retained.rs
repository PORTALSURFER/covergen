use std::error::Error;
use std::sync::mpsc::{self, Receiver};

use bytemuck::{Pod, Zeroable};
use wgpu::{self, util::DeviceExt};

/// WGSL shader implementing retained post-processing passes.
const RETAINED_POST_SHADER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/gpu_retained_post.wgsl"
));

/// Uniforms for one retained post-process blend dispatch.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct RetainedPostParams {
    width: u32,
    height: u32,
    blend_mode: u32,
    _pad0: u32,
    opacity: f32,
    contrast: f32,
    _pad1: [f32; 2],
}

/// GPU resources used to retain and blend layer output without intermediate readbacks.
#[derive(Debug)]
pub(crate) struct RetainedGpuPost {
    blend_pipeline: wgpu::ComputePipeline,
    clear_pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    accum_buffer: wgpu::Buffer,
    post_uniform: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    width: u32,
    height: u32,
}

impl RetainedGpuPost {
    /// Build retained post-processing resources bound to the primary output buffer.
    pub(crate) fn new(
        device: &wgpu::Device,
        out_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> Self {
        let luma_size = (width as u64)
            .saturating_mul(height as u64)
            .saturating_mul(std::mem::size_of::<f32>() as u64);
        let accum_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("retained accum"),
            size: luma_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("retained staging"),
            size: luma_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let initial = RetainedPostParams {
            width,
            height,
            blend_mode: 0,
            _pad0: 0,
            opacity: 1.0,
            contrast: 1.0,
            _pad1: [0.0, 0.0],
        };
        let post_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("retained post uniforms"),
            contents: bytemuck::bytes_of(&initial),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("retained post shader"),
            source: wgpu::ShaderSource::Wgsl(RETAINED_POST_SHADER.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("retained post bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("retained post bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: out_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: accum_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: post_uniform.as_entire_binding(),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("retained post pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let clear_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("retained clear pipeline"),
            layout: Some(&layout),
            module: &shader_module,
            entry_point: "clear_accum",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        });
        let blend_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("retained blend pipeline"),
            layout: Some(&layout),
            module: &shader_module,
            entry_point: "blend_layer",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        });

        Self {
            blend_pipeline,
            clear_pipeline,
            bind_group,
            accum_buffer,
            post_uniform,
            staging_buffer,
            width,
            height,
        }
    }

    /// Dispatch clear pass for a new image.
    pub(crate) fn begin_image(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let clear = RetainedPostParams {
            width: self.width,
            height: self.height,
            blend_mode: 0,
            _pad0: 0,
            opacity: 1.0,
            contrast: 1.0,
            _pad1: [0.0, 0.0],
        };
        queue.write_buffer(&self.post_uniform, 0, bytemuck::bytes_of(&clear));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("retained clear encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained clear pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.clear_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(self.width.div_ceil(16), self.height.div_ceil(16), 1);
        }
        queue.submit(Some(encoder.finish()));
    }

    /// Encode one retained blend pass after the main fractal pass has populated `out_buffer`.
    pub(crate) fn encode_blend_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        opacity: f32,
        blend_mode: u32,
        contrast: f32,
    ) {
        let post = RetainedPostParams {
            width: self.width,
            height: self.height,
            blend_mode: blend_mode % 10,
            _pad0: 0,
            opacity: opacity.clamp(0.0, 1.0),
            contrast: contrast.clamp(1.0, 3.0),
            _pad1: [0.0, 0.0],
        };
        queue.write_buffer(&self.post_uniform, 0, bytemuck::bytes_of(&post));
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("retained blend pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.blend_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.width.div_ceil(16), self.height.div_ceil(16), 1);
    }

    /// Map retained output for one final readback at image end.
    pub(crate) fn begin_readback(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Receiver<Result<(), wgpu::BufferAsyncError>> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("retained readback encoder"),
        });
        let bytes = (self.expected_pixels() * std::mem::size_of::<f32>()) as u64;
        encoder.copy_buffer_to_buffer(&self.accum_buffer, 0, &self.staging_buffer, 0, bytes);
        queue.submit(Some(encoder.finish()));

        let slice = self.staging_buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        receiver
    }

    /// Copy mapped retained buffer into `out`.
    pub(crate) fn finish_readback(&self, out: &mut [f32]) -> Result<(), Box<dyn Error>> {
        let expected_pixels = self.expected_pixels();
        if out.len() != expected_pixels {
            return Err("output buffer length does not match render dimensions".into());
        }
        let slice = self.staging_buffer.slice(..);
        {
            let raw = slice.get_mapped_range();
            let mapped: &[f32] = bytemuck::cast_slice(&raw);
            out.copy_from_slice(&mapped[..expected_pixels]);
        }
        self.staging_buffer.unmap();
        Ok(())
    }

    /// Return expected number of output pixels for this renderer.
    pub(crate) fn expected_pixels(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }
}

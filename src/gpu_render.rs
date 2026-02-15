use std::error::Error;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use crate::image_ops::decode_luma;
use crate::model::Params;
use bytemuck::Zeroable;
use wgpu::{self, util::DeviceExt};

/// Default timeout used while waiting for the compute output buffer to map.
const MAP_TIMEOUT: Duration = Duration::from_secs(8);

/// GPU-backed compute renderer for one fixed output resolution.
#[derive(Debug)]
pub(crate) struct GpuLayerRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    out_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    output_size: u64,
}

impl GpuLayerRenderer {
    /// Build a compute renderer from an adapter and WGSL source.
    pub(crate) async fn new(
        adapter: &wgpu::Adapter,
        shader_source: &str,
        width: u32,
        height: u32,
    ) -> Result<Self, Box<dyn Error>> {
        let shader = wgpu::ShaderModuleDescriptor {
            label: Some("fractal shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        };

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await?;

        let shader_module = device.create_shader_module(shader);
        let output_size = (width as u64)
            .saturating_mul(height as u64)
            .saturating_mul(std::mem::size_of::<u32>() as u64);
        let out_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output storage"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let zero = Params::zeroed();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("uniforms"),
            contents: bytemuck::bytes_of(&zero),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: output_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
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
            label: Some("bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: out_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("compute pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: "main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group,
            out_buffer,
            uniform_buffer,
            staging_buffer,
            width,
            height,
            output_size,
        })
    }

    /// Render one layer into a grayscale float buffer.
    pub(crate) fn render_layer(
        &mut self,
        params: &Params,
        out: &mut [f32],
    ) -> Result<(), Box<dyn Error>> {
        let expected_pixels = (params.width as usize)
            .checked_mul(params.height as usize)
            .ok_or("invalid layer dimensions")?;
        if params.width != self.width || params.height != self.height {
            return Err("gpu params must match renderer resolution".into());
        }
        if out.len() != expected_pixels {
            return Err("output buffer length does not match render dimensions".into());
        }

        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(params));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("command encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            let work_x = params.width.div_ceil(16);
            let work_y = params.height.div_ceil(16);
            pass.dispatch_workgroups(work_x, work_y, 1);
        }

        encoder.copy_buffer_to_buffer(
            &self.out_buffer,
            0,
            &self.staging_buffer,
            0,
            self.output_size,
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = self.staging_buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);

        match receiver.recv_timeout(MAP_TIMEOUT) {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                return Err(format!("buffer map failed: {err:?}").into());
            }
            Err(RecvTimeoutError::Timeout) => {
                return Err("timeout waiting for GPU buffer mapping".into());
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err("gpu map callback disconnected before completion".into());
            }
        };

        {
            let raw = slice.get_mapped_range();
            decode_luma(&raw, out);
        }
        self.staging_buffer.unmap();
        Ok(())
    }
}

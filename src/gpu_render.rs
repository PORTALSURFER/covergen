use std::error::Error;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

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
    pending_readback: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
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
            pending_readback: None,
        })
    }

    /// Submit one GPU layer render and stage it for asynchronous readback.
    pub(crate) fn submit_layer(&mut self, params: &Params) -> Result<(), Box<dyn Error>> {
        self.validate_params(params)?;
        if self.pending_readback.is_some() {
            return Err("gpu readback already pending; collect before submitting again".into());
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
        self.pending_readback = Some(receiver);
        Ok(())
    }

    /// Complete a previously submitted GPU layer render and decode into `out`.
    pub(crate) fn collect_layer(&mut self, out: &mut [f32]) -> Result<(), Box<dyn Error>> {
        let expected_pixels = self
            .width
            .checked_mul(self.height)
            .ok_or("invalid renderer dimensions")? as usize;
        if out.len() != expected_pixels {
            return Err("output buffer length does not match render dimensions".into());
        }

        let receiver = self
            .pending_readback
            .take()
            .ok_or("no gpu readback pending for collect")?;

        let deadline = Instant::now() + MAP_TIMEOUT;
        loop {
            self.device.poll(wgpu::Maintain::Poll);
            match receiver.try_recv() {
                Ok(Ok(())) => break,
                Ok(Err(err)) => {
                    return Err(format!("buffer map failed: {err:?}").into());
                }
                Err(TryRecvError::Empty) => {
                    if Instant::now() >= deadline {
                        return Err("timeout waiting for GPU buffer mapping".into());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(TryRecvError::Disconnected) => {
                    return Err("gpu map callback disconnected before completion".into());
                }
            }
        }

        let slice = self.staging_buffer.slice(..);
        {
            let raw = slice.get_mapped_range();
            decode_luma(&raw, out);
        }
        self.staging_buffer.unmap();
        Ok(())
    }

    /// Render one layer into a grayscale float buffer.
    pub(crate) fn render_layer(
        &mut self,
        params: &Params,
        out: &mut [f32],
    ) -> Result<(), Box<dyn Error>> {
        self.submit_layer(params)?;
        self.collect_layer(out)
    }

    fn validate_params(&self, params: &Params) -> Result<(), Box<dyn Error>> {
        let _expected_pixels = (params.width as usize)
            .checked_mul(params.height as usize)
            .ok_or("invalid layer dimensions")?;
        if params.width != self.width || params.height != self.height {
            return Err("gpu params must match renderer resolution".into());
        }
        Ok(())
    }
}

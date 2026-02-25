use std::error::Error;
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::gpu_retained::RetainedGpuPost;
use crate::image_ops::decode_luma;
use crate::model::Params;
use crate::shaders::{create_shader_module, ShaderProgram};
use bytemuck::Zeroable;
use wgpu::{self, util::DeviceExt};

mod graph_exec;
mod graph_ops;
use graph_ops::GpuGraphOps;

/// Default timeout used while waiting for mapped GPU buffers.
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
    readback_slots: Vec<ReadbackSlot>,
    width: u32,
    height: u32,
    output_size: u64,
    pending_readbacks: VecDeque<usize>,
    next_readback_slot: usize,
    retained: RetainedGpuPost,
    graph_ops: GpuGraphOps,
    node_layer_temp_buffer: wgpu::Buffer,
    node_composite_temp_buffer: wgpu::Buffer,
    node_alias_luma_buffers: Vec<wgpu::Buffer>,
    node_alias_mask_buffers: Vec<wgpu::Buffer>,
    node_feedback_buffers: Vec<wgpu::Buffer>,
    node_feedback_clear_buffer: wgpu::Buffer,
}

#[derive(Debug)]
struct ReadbackSlot {
    staging_buffer: wgpu::Buffer,
    pending: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
}

/// Frame-scoped command recording context for graph execution.
///
/// The context accumulates all graph node passes into one command encoder so
/// the caller can submit once per frame.
#[derive(Debug)]
pub(crate) struct GraphFrameContext {
    encoder: wgpu::CommandEncoder,
    encoded_ops: u32,
}

impl GpuLayerRenderer {
    /// Build a compute renderer from an adapter and the configured shader backend.
    pub(crate) async fn new(
        adapter: &wgpu::Adapter,
        width: u32,
        height: u32,
    ) -> Result<Self, Box<dyn Error>> {
        Self::new_with_output(adapter, width, height, width, height).await
    }

    /// Build a compute renderer with an explicit output size for retained finalization.
    pub(crate) async fn new_with_output(
        adapter: &wgpu::Adapter,
        width: u32,
        height: u32,
        output_width: u32,
        output_height: u32,
    ) -> Result<Self, Box<dyn Error>> {
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

        let shader_module = create_shader_module(&device, ShaderProgram::FractalMain)?;
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
        let mut readback_slots = Vec::with_capacity(2);
        for slot in 0..2 {
            let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("staging-{slot}")),
                size: output_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            readback_slots.push(ReadbackSlot {
                staging_buffer,
                pending: None,
            });
        }
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("compute pipeline"),
            layout: Some(&layout),
            module: &shader_module,
            entry_point: "main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        });
        let retained = RetainedGpuPost::new_with_output(
            &device,
            &out_buffer,
            width,
            height,
            output_width,
            output_height,
        )?;
        let graph_ops = GpuGraphOps::new(&device)?;
        let node_layer_temp_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("v2 node layer temp"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let node_composite_temp_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("v2 node composite temp"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let node_feedback_clear_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("v2 node feedback clear"),
            size: output_size,
            usage: wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group,
            out_buffer,
            uniform_buffer,
            readback_slots,
            width,
            height,
            output_size,
            pending_readbacks: VecDeque::with_capacity(2),
            next_readback_slot: 0,
            retained,
            graph_ops,
            node_layer_temp_buffer,
            node_composite_temp_buffer,
            node_alias_luma_buffers: Vec::new(),
            node_alias_mask_buffers: Vec::new(),
            node_feedback_buffers: Vec::new(),
            node_feedback_clear_buffer,
        })
    }

    /// Reset retained accumulation buffers for the next image.
    pub(crate) fn begin_retained_image(&mut self) -> Result<(), Box<dyn Error>> {
        if self.has_pending_layer_readbacks() {
            return Err("cannot begin retained image while readback is pending".into());
        }
        self.retained.begin_image(&self.device, &self.queue);
        Ok(())
    }

    /// Read retained output after on-GPU finalization into grayscale output bytes.
    pub(crate) fn collect_retained_output_gray(
        &mut self,
        out_gray: &mut [u8],
        contrast: f32,
        low_pct: f32,
        high_pct: f32,
        fast_mode: bool,
    ) -> Result<(), Box<dyn Error>> {
        if self.has_pending_layer_readbacks() {
            return Err("cannot collect retained output while layer readback is pending".into());
        }
        let receiver = self.retained.begin_final_readback(
            &self.device,
            &self.queue,
            contrast,
            low_pct,
            high_pct,
            fast_mode,
        );
        self.wait_for_map(receiver)?;
        self.retained.finish_final_readback_gray(out_gray)
    }

    /// Submit one layer into retained GPU post-processing accumulation.
    pub(crate) fn submit_retained_layer(
        &mut self,
        params: &Params,
        opacity: f32,
        blend_mode: u32,
        contrast: f32,
    ) -> Result<(), Box<dyn Error>> {
        self.validate_params(params)?;
        if self.has_pending_layer_readbacks() {
            return Err("gpu readback already pending; collect before submitting again".into());
        }

        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(params));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("retained layer encoder"),
            });
        self.dispatch_main_pass(&mut encoder, params);
        self.retained
            .encode_blend_pass(&mut encoder, &self.queue, opacity, blend_mode, contrast);
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Read the retained accumulation into `out` using a single map/readback.
    pub(crate) fn collect_retained_image(&mut self, out: &mut [f32]) -> Result<(), Box<dyn Error>> {
        let receiver = self.retained.begin_readback(&self.device, &self.queue);
        self.wait_for_map(receiver)?;
        self.retained.finish_readback(out)
    }

    /// Submit one GPU layer render and stage it for asynchronous readback.
    pub(crate) fn submit_layer(&mut self, params: &Params) -> Result<(), Box<dyn Error>> {
        self.validate_params(params)?;
        if self.pending_readbacks.len() >= self.readback_slots.len() {
            return Err("gpu readback queue is full; collect before submitting again".into());
        }
        let slot_index = self.next_readback_slot % self.readback_slots.len();
        if self.readback_slots[slot_index].pending.is_some() {
            return Err("readback slot is unexpectedly busy".into());
        }

        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(params));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("command encoder"),
            });
        self.dispatch_main_pass(&mut encoder, params);
        encoder.copy_buffer_to_buffer(
            &self.out_buffer,
            0,
            &self.readback_slots[slot_index].staging_buffer,
            0,
            self.output_size,
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = self.readback_slots[slot_index].staging_buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.readback_slots[slot_index].pending = Some(receiver);
        self.pending_readbacks.push_back(slot_index);
        self.next_readback_slot = (slot_index + 1) % self.readback_slots.len();
        Ok(())
    }

    /// Complete a previously submitted GPU layer render and decode into `out`.
    pub(crate) fn collect_layer(&mut self, out: &mut [f32]) -> Result<(), Box<dyn Error>> {
        if out.len() != self.expected_pixels()? {
            return Err("output buffer length does not match render dimensions".into());
        }
        let slot_index = self
            .pending_readbacks
            .pop_front()
            .ok_or("no gpu readback pending for collect")?;
        let receiver = self
            .readback_slots
            .get_mut(slot_index)
            .and_then(|slot| slot.pending.take())
            .ok_or("readback slot had no pending map")?;
        self.wait_for_map(receiver)?;

        let slot = self
            .readback_slots
            .get_mut(slot_index)
            .ok_or("readback slot index out of range")?;
        let slice = slot.staging_buffer.slice(..);
        {
            let raw = slice.get_mapped_range();
            decode_luma(&raw, out);
        }
        slot.staging_buffer.unmap();
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

    fn dispatch_main_pass(&self, encoder: &mut wgpu::CommandEncoder, params: &Params) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("compute pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(params.width.div_ceil(16), params.height.div_ceil(16), 1);
    }

    fn wait_for_map(
        &self,
        receiver: Receiver<Result<(), wgpu::BufferAsyncError>>,
    ) -> Result<(), Box<dyn Error>> {
        let deadline = Instant::now() + MAP_TIMEOUT;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err("timeout waiting for GPU buffer mapping".into());
            }
            let wait = (deadline - now).min(Duration::from_millis(2));
            self.device.poll(wgpu::Maintain::Poll);
            match receiver.recv_timeout(wait) {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(err)) => return Err(format!("buffer map failed: {err:?}").into()),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("gpu map callback disconnected before completion".into())
                }
            }
        }
    }

    fn has_pending_layer_readbacks(&self) -> bool {
        !self.pending_readbacks.is_empty()
    }

    fn expected_pixels(&self) -> Result<usize, Box<dyn Error>> {
        self.width
            .checked_mul(self.height)
            .map(|pixels| pixels as usize)
            .ok_or("invalid renderer dimensions".into())
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

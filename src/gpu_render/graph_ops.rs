//! GPU compute pipelines for V2 graph-native node operations.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::shaders::{create_shader_module, ShaderProgram};

mod layout;
use layout::{create_decode_bind_group_layout, create_graph_bind_group_layout, create_pipeline};

const GRAPH_BIND_GROUP_CACHE_SOFT_MAX: usize = 512;
const DECODE_BIND_GROUP_CACHE_SOFT_MAX: usize = 128;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct GraphOpUniforms {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) mode: u32,
    pub(super) flags: u32,
    pub(super) seed: u32,
    pub(super) octaves: u32,
    pub(super) _pad0: u32,
    pub(super) _pad1: u32,
    pub(super) p0: f32,
    pub(super) p1: f32,
    pub(super) p2: f32,
    pub(super) p3: f32,
    pub(super) p4: f32,
    pub(super) p5: f32,
    pub(super) p6: f32,
    pub(super) p7: f32,
    pub(super) p8: f32,
    pub(super) p9: f32,
    pub(super) p10: f32,
    pub(super) p11: f32,
    pub(super) p12: f32,
    pub(super) p13: f32,
    pub(super) p14: f32,
    pub(super) p15: f32,
}

impl GraphOpUniforms {
    pub(super) fn sized(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            mode: 0,
            flags: 0,
            seed: 0,
            octaves: 1,
            _pad0: 0,
            _pad1: 0,
            p0: 0.0,
            p1: 0.0,
            p2: 0.0,
            p3: 0.0,
            p4: 0.0,
            p5: 0.0,
            p6: 0.0,
            p7: 0.0,
            p8: 0.0,
            p9: 0.0,
            p10: 0.0,
            p11: 0.0,
            p12: 0.0,
            p13: 0.0,
            p14: 0.0,
            p15: 0.0,
        }
    }
}

pub(super) struct GraphBuffers<'a> {
    pub(super) src0: &'a wgpu::Buffer,
    pub(super) src1: &'a wgpu::Buffer,
    pub(super) src2: &'a wgpu::Buffer,
    pub(super) dst: &'a wgpu::Buffer,
}

pub(super) struct DecodeBuffers<'a> {
    pub(super) src_u32: &'a wgpu::Buffer,
    pub(super) dst: &'a wgpu::Buffer,
}

/// Cache key for graph-op bind groups that share the same storage bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct GraphBindGroupKey {
    src0: usize,
    src1: usize,
    src2: usize,
    dst: usize,
}

impl GraphBindGroupKey {
    fn from_buffers(buffers: &GraphBuffers<'_>) -> Self {
        Self {
            src0: buffer_key(buffers.src0),
            src1: buffer_key(buffers.src1),
            src2: buffer_key(buffers.src2),
            dst: buffer_key(buffers.dst),
        }
    }
}

/// Cache key for decode-pass bind groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct DecodeBindGroupKey {
    src_u32: usize,
    dst: usize,
}

impl DecodeBindGroupKey {
    fn from_buffers(buffers: &DecodeBuffers<'_>) -> Self {
        Self {
            src_u32: buffer_key(buffers.src_u32),
            dst: buffer_key(buffers.dst),
        }
    }
}

/// GPU dispatch helpers for graph-node operations over aliased buffers.
#[derive(Debug)]
pub(super) struct GpuGraphOps {
    graph_bind_group_layout: wgpu::BindGroupLayout,
    decode_bind_group_layout: wgpu::BindGroupLayout,
    graph_uniform_buffer: wgpu::Buffer,
    decode_uniform_buffer: wgpu::Buffer,
    copy_pipeline: wgpu::ComputePipeline,
    decode_pipeline: wgpu::ComputePipeline,
    source_noise_pipeline: wgpu::ComputePipeline,
    mask_pipeline: wgpu::ComputePipeline,
    blend_pipeline: wgpu::ComputePipeline,
    top_camera_pipeline: wgpu::ComputePipeline,
    tone_map_pipeline: wgpu::ComputePipeline,
    warp_pipeline: wgpu::ComputePipeline,
    graph_bind_group_cache: RefCell<HashMap<GraphBindGroupKey, Arc<wgpu::BindGroup>>>,
    decode_bind_group_cache: RefCell<HashMap<DecodeBindGroupKey, Arc<wgpu::BindGroup>>>,
    frame_bind_group_creates: Cell<u64>,
}

impl GpuGraphOps {
    pub(super) fn new(device: &wgpu::Device) -> Result<Self, Box<dyn Error>> {
        let graph_bind_group_layout = create_graph_bind_group_layout(device);
        let decode_bind_group_layout = create_decode_bind_group_layout(device);
        let graph_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("v2 graph ops layout"),
            bind_group_layouts: &[&graph_bind_group_layout],
            push_constant_ranges: &[],
        });
        let decode_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("v2 graph decode layout"),
            bind_group_layouts: &[&decode_bind_group_layout],
            push_constant_ranges: &[],
        });
        let shader_module = create_shader_module(device, ShaderProgram::GraphOps)?;
        let decode_shader_module = create_shader_module(device, ShaderProgram::GraphDecode)?;
        let graph_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("v2 graph ops uniforms"),
            contents: bytemuck::bytes_of(&GraphOpUniforms::sized(1, 1)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let decode_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("v2 graph decode uniforms"),
            contents: bytemuck::bytes_of(&GraphOpUniforms::sized(1, 1)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Ok(Self {
            graph_bind_group_layout,
            decode_bind_group_layout,
            graph_uniform_buffer,
            decode_uniform_buffer,
            copy_pipeline: create_pipeline(device, &graph_layout, &shader_module, "copy_luma"),
            decode_pipeline: create_pipeline(
                device,
                &decode_layout,
                &decode_shader_module,
                "decode_layer_u32",
            ),
            source_noise_pipeline: create_pipeline(
                device,
                &graph_layout,
                &shader_module,
                "source_noise",
            ),
            mask_pipeline: create_pipeline(device, &graph_layout, &shader_module, "build_mask"),
            blend_pipeline: create_pipeline(device, &graph_layout, &shader_module, "blend_luma"),
            top_camera_pipeline: create_pipeline(
                device,
                &graph_layout,
                &shader_module,
                "top_camera_render",
            ),
            tone_map_pipeline: create_pipeline(device, &graph_layout, &shader_module, "tone_map"),
            warp_pipeline: create_pipeline(device, &graph_layout, &shader_module, "warp_luma"),
            graph_bind_group_cache: RefCell::new(HashMap::with_capacity(64)),
            decode_bind_group_cache: RefCell::new(HashMap::with_capacity(16)),
            frame_bind_group_creates: Cell::new(0),
        })
    }

    /// Reset frame-scoped bind-group caches before a new graph frame begins.
    pub(super) fn begin_frame(&self) {
        let graph_cache_len = self.graph_bind_group_cache.borrow().len();
        if graph_cache_len > GRAPH_BIND_GROUP_CACHE_SOFT_MAX {
            self.graph_bind_group_cache.borrow_mut().clear();
        }
        let decode_cache_len = self.decode_bind_group_cache.borrow().len();
        if decode_cache_len > DECODE_BIND_GROUP_CACHE_SOFT_MAX {
            self.decode_bind_group_cache.borrow_mut().clear();
        }
        self.frame_bind_group_creates.set(0);
    }

    /// Return the number of bind groups created in the current graph frame.
    pub(super) fn frame_bind_group_creates(&self) -> u64 {
        self.frame_bind_group_creates.get()
    }

    pub(super) fn encode_copy(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        buffers: GraphBuffers<'_>,
        width: u32,
        height: u32,
    ) -> u64 {
        self.encode_graph_pass(
            device,
            queue,
            encoder,
            &self.copy_pipeline,
            buffers,
            GraphOpUniforms::sized(width, height),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn encode_decode_layer(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        buffers: DecodeBuffers<'_>,
        width: u32,
        height: u32,
        contrast: f32,
    ) -> u64 {
        let mut uniforms = GraphOpUniforms::sized(width, height);
        uniforms.p0 = contrast;
        self.encode_decode_pass(device, queue, encoder, buffers, uniforms)
    }

    pub(super) fn encode_source_noise(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        buffers: GraphBuffers<'_>,
        uniforms: GraphOpUniforms,
    ) -> u64 {
        self.encode_graph_pass(
            device,
            queue,
            encoder,
            &self.source_noise_pipeline,
            buffers,
            uniforms,
        )
    }

    pub(super) fn encode_mask(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        buffers: GraphBuffers<'_>,
        uniforms: GraphOpUniforms,
    ) -> u64 {
        self.encode_graph_pass(
            device,
            queue,
            encoder,
            &self.mask_pipeline,
            buffers,
            uniforms,
        )
    }

    pub(super) fn encode_blend(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        buffers: GraphBuffers<'_>,
        uniforms: GraphOpUniforms,
    ) -> u64 {
        self.encode_graph_pass(
            device,
            queue,
            encoder,
            &self.blend_pipeline,
            buffers,
            uniforms,
        )
    }

    pub(super) fn encode_tone_map(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        buffers: GraphBuffers<'_>,
        uniforms: GraphOpUniforms,
    ) -> u64 {
        self.encode_graph_pass(
            device,
            queue,
            encoder,
            &self.tone_map_pipeline,
            buffers,
            uniforms,
        )
    }

    pub(super) fn encode_feedback_mix(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        buffers: GraphBuffers<'_>,
        mut uniforms: GraphOpUniforms,
    ) -> u64 {
        // Compatibility path: feedback is equivalent to normal blend with
        // alpha=mix and no mask, so we can reuse `blend_luma`.
        uniforms.mode = 0;
        uniforms.flags &= !0x1;
        self.encode_graph_pass(
            device,
            queue,
            encoder,
            &self.blend_pipeline,
            buffers,
            uniforms,
        )
    }

    pub(super) fn encode_top_camera(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        buffers: GraphBuffers<'_>,
        uniforms: GraphOpUniforms,
    ) -> u64 {
        self.encode_graph_pass(
            device,
            queue,
            encoder,
            &self.top_camera_pipeline,
            buffers,
            uniforms,
        )
    }

    pub(super) fn encode_warp(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        buffers: GraphBuffers<'_>,
        uniforms: GraphOpUniforms,
    ) -> u64 {
        self.encode_graph_pass(
            device,
            queue,
            encoder,
            &self.warp_pipeline,
            buffers,
            uniforms,
        )
    }

    fn encode_graph_pass(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::ComputePipeline,
        buffers: GraphBuffers<'_>,
        uniforms: GraphOpUniforms,
    ) -> u64 {
        let uploaded = std::mem::size_of::<GraphOpUniforms>() as u64;
        queue.write_buffer(&self.graph_uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let bind_group = self.cached_graph_bind_group(device, buffers);

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("v2 graph op pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(uniforms.width.div_ceil(16), uniforms.height.div_ceil(16), 1);
        uploaded
    }

    fn encode_decode_pass(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        buffers: DecodeBuffers<'_>,
        uniforms: GraphOpUniforms,
    ) -> u64 {
        let uploaded = std::mem::size_of::<GraphOpUniforms>() as u64;
        queue.write_buffer(
            &self.decode_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );
        let bind_group = self.cached_decode_bind_group(device, buffers);

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("v2 graph decode pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.decode_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(uniforms.width.div_ceil(16), uniforms.height.div_ceil(16), 1);
        uploaded
    }

    fn cached_graph_bind_group(
        &self,
        device: &wgpu::Device,
        buffers: GraphBuffers<'_>,
    ) -> Arc<wgpu::BindGroup> {
        let key = GraphBindGroupKey::from_buffers(&buffers);
        let mut cache = self.graph_bind_group_cache.borrow_mut();
        if let Some(bind_group) = cache.get(&key) {
            return bind_group.clone();
        }
        let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("v2 graph op bind group"),
            layout: &self.graph_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.src0.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffers.src1.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffers.src2.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: buffers.dst.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.graph_uniform_buffer.as_entire_binding(),
                },
            ],
        }));
        self.frame_bind_group_creates
            .set(self.frame_bind_group_creates.get().saturating_add(1));
        cache.insert(key, bind_group.clone());
        bind_group
    }

    fn cached_decode_bind_group(
        &self,
        device: &wgpu::Device,
        buffers: DecodeBuffers<'_>,
    ) -> Arc<wgpu::BindGroup> {
        let key = DecodeBindGroupKey::from_buffers(&buffers);
        let mut cache = self.decode_bind_group_cache.borrow_mut();
        if let Some(bind_group) = cache.get(&key) {
            return bind_group.clone();
        }
        let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("v2 graph decode bind group"),
            layout: &self.decode_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.src_u32.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffers.dst.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.decode_uniform_buffer.as_entire_binding(),
                },
            ],
        }));
        self.frame_bind_group_creates
            .set(self.frame_bind_group_creates.get().saturating_add(1));
        cache.insert(key, bind_group.clone());
        bind_group
    }
}

fn buffer_key(buffer: &wgpu::Buffer) -> usize {
    std::ptr::from_ref(buffer).cast::<()>() as usize
}

#[cfg(test)]
#[path = "graph_ops_tests.rs"]
mod tests;

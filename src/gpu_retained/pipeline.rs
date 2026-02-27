use std::error::Error;
use std::sync::mpsc::{self, Receiver};

use wgpu::util::DeviceExt;

use super::{RetainedFinalizeParams, RetainedPostParams};
use crate::shaders::{create_shader_module, ShaderProgram};

/// All retained resources created together so `mod.rs` can stay focused on behavior.
pub(super) struct RetainedSetup {
    pub(super) clear_pipeline: wgpu::ComputePipeline,
    pub(super) blend_pipeline: wgpu::ComputePipeline,
    pub(super) clear_hist_pipeline: wgpu::ComputePipeline,
    pub(super) histogram_pipeline: wgpu::ComputePipeline,
    pub(super) thresholds_pipeline: wgpu::ComputePipeline,
    pub(super) finalize_pipeline: wgpu::ComputePipeline,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) accum_buffer: wgpu::Buffer,
    pub(super) post_uniform: wgpu::Buffer,
    pub(super) finalize_uniform: wgpu::Buffer,
    pub(super) final_output_buffer: wgpu::Buffer,
    pub(super) staging_buffer: wgpu::Buffer,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_setup(
    device: &wgpu::Device,
    out_buffer: &wgpu::Buffer,
    width: u32,
    height: u32,
    output_width: u32,
    output_height: u32,
    post_init: RetainedPostParams,
    finalize_init: RetainedFinalizeParams,
) -> Result<RetainedSetup, Box<dyn Error>> {
    let src_bytes = (width as u64)
        .saturating_mul(height as u64)
        .saturating_mul(std::mem::size_of::<f32>() as u64);
    let output_bytes = (output_width as u64)
        .saturating_mul(output_height as u64)
        .saturating_mul(std::mem::size_of::<u32>() as u64);

    let accum_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("retained accum"),
        size: src_bytes,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("retained staging"),
        size: src_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let final_output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("retained final output"),
        size: output_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let histogram_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("retained histogram"),
        size: (256usize * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let stretch_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("retained stretch thresholds"),
        contents: bytemuck::cast_slice(&[0.0f32, 1.0f32]),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let post_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("retained post uniforms"),
        contents: bytemuck::bytes_of(&post_init),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let finalize_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("retained finalize uniforms"),
        contents: bytemuck::bytes_of(&finalize_init),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let shader_module = create_shader_module(device, ShaderProgram::RetainedPost)?;

    let bind_group_layout = create_bind_group_layout(device);
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
            wgpu::BindGroupEntry {
                binding: 3,
                resource: histogram_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: stretch_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: final_output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: finalize_uniform.as_entire_binding(),
            },
        ],
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("retained post pipeline layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    Ok(RetainedSetup {
        clear_pipeline: create_compute_pipeline(
            device,
            &layout,
            &shader_module,
            "clear_accum",
            "retained clear pipeline",
        ),
        blend_pipeline: create_compute_pipeline(
            device,
            &layout,
            &shader_module,
            "blend_layer",
            "retained blend pipeline",
        ),
        clear_hist_pipeline: create_compute_pipeline(
            device,
            &layout,
            &shader_module,
            "clear_histogram",
            "retained clear histogram pipeline",
        ),
        histogram_pipeline: create_compute_pipeline(
            device,
            &layout,
            &shader_module,
            "accumulate_histogram",
            "retained histogram pipeline",
        ),
        thresholds_pipeline: create_compute_pipeline(
            device,
            &layout,
            &shader_module,
            "compute_thresholds",
            "retained threshold pipeline",
        ),
        finalize_pipeline: create_compute_pipeline(
            device,
            &layout,
            &shader_module,
            "finalize_to_u8",
            "retained finalize pipeline",
        ),
        bind_group,
        accum_buffer,
        post_uniform,
        finalize_uniform,
        final_output_buffer,
        staging_buffer,
    })
}

fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("retained post bind group layout"),
        entries: &[
            storage_layout_entry(0, true),
            storage_layout_entry(1, false),
            uniform_layout_entry(2),
            storage_layout_entry(3, false),
            storage_layout_entry(4, false),
            storage_layout_entry(5, false),
            uniform_layout_entry(6),
        ],
    })
}

fn storage_layout_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn create_compute_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    entry: &str,
    label: &str,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        module: shader,
        entry_point: entry,
        compilation_options: wgpu::PipelineCompilationOptions::default(),
    })
}

pub(super) fn map_buffer_async(
    buffer: &wgpu::Buffer,
) -> Receiver<Result<(), wgpu::BufferAsyncError>> {
    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    receiver
}

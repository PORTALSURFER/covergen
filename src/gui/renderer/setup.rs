//! Shader/pipeline setup and geometry upload helpers for the GUI renderer.

use std::error::Error;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::runtime_config::GuiVsync;

use super::super::geometry::Rect;
use super::super::scene::Color;

/// Vertex payload consumed by the GUI WGSL shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl Vertex {
    /// Build one vertex from pixel-space position and RGBA color.
    pub(super) fn new(x: i32, y: i32, color: Color) -> Self {
        Self {
            position: [x as f32, y as f32],
            color: [color.r, color.g, color.b, color.a],
        }
    }

    /// Return vertex buffer layout for render pipeline creation.
    pub(super) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as u64,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Uniform payload containing current viewport size.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct ViewportUniform {
    viewport_size: [f32; 2],
    _pad: [f32; 2],
}

impl ViewportUniform {
    /// Build viewport uniform from current surface dimensions.
    pub(super) fn new(width: u32, height: u32) -> Self {
        Self {
            viewport_size: [width.max(1) as f32, height.max(1) as f32],
            _pad: [0.0, 0.0],
        }
    }
}

/// Create a WGSL shader module for GUI line/rectangle drawing.
pub(super) fn create_shader_module(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("gui-shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
    })
}

/// Create one initialized viewport-uniform buffer.
pub(super) fn create_uniform_buffer(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gui-uniform-buffer"),
        contents: bytemuck::bytes_of(&ViewportUniform::new(width, height)),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

/// Create bind-group layout and bind-group for viewport uniforms.
pub(super) fn create_uniform_bind_group(
    device: &wgpu::Device,
    uniform_buffer: &wgpu::Buffer,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("gui-uniform-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gui-uniform-bind-group"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });
    (layout, bind_group)
}

/// Create one render pipeline for either triangle or line topology.
pub(super) fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    uniform_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    topology: wgpu::PrimitiveTopology,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gui-pipeline-layout"),
        bind_group_layouts: &[uniform_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("gui-render-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: "vs_main",
            buffers: &[Vertex::layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    })
}

/// Create one dynamic vertex buffer with `capacity` vertices.
pub(super) fn create_vertex_buffer(
    device: &wgpu::Device,
    capacity: usize,
    label: &str,
) -> wgpu::Buffer {
    let bytes = (capacity.max(1) * std::mem::size_of::<Vertex>()) as u64;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Append two triangles for one filled rectangle.
pub(super) fn push_rect_triangles(out: &mut Vec<Vertex>, rect: Rect, color: Color) {
    if rect.w <= 0 || rect.h <= 0 {
        return;
    }
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x + rect.w;
    let y1 = rect.y + rect.h;
    out.push(Vertex::new(x0, y0, color));
    out.push(Vertex::new(x1, y0, color));
    out.push(Vertex::new(x1, y1, color));
    out.push(Vertex::new(x0, y0, color));
    out.push(Vertex::new(x1, y1, color));
    out.push(Vertex::new(x0, y1, color));
}

/// Pick preferred srgb surface format.
pub(super) fn preferred_surface_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .unwrap_or(formats[0])
}

/// Pick present mode based on requested GUI vsync policy.
pub(super) fn select_present_mode(
    modes: &[wgpu::PresentMode],
    vsync: GuiVsync,
) -> wgpu::PresentMode {
    let preferred = match vsync {
        GuiVsync::On => wgpu::PresentMode::AutoVsync,
        GuiVsync::Off => wgpu::PresentMode::AutoNoVsync,
        GuiVsync::Adaptive => wgpu::PresentMode::Mailbox,
    };
    modes
        .iter()
        .copied()
        .find(|mode| *mode == preferred)
        .unwrap_or(wgpu::PresentMode::Fifo)
}

/// Request a non-software adapter compatible with the window surface.
pub(super) async fn request_hardware_adapter(
    instance: &wgpu::Instance,
    surface: &wgpu::Surface<'_>,
) -> Result<wgpu::Adapter, Box<dyn Error>> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(surface),
        })
        .await
        .ok_or_else(|| {
            "covergen requires a hardware GPU adapter; no GPU adapter was detected. \
             install GPU drivers and run on a machine with an available hardware GPU."
        })?;
    let info = adapter.get_info();
    if is_software_adapter(info.device_type, &info.name) {
        return Err(format!(
            "covergen requires a hardware GPU adapter; software adapter '{} ({:?})' is not supported.",
            info.name, info.device_type
        )
        .into());
    }
    Ok(adapter)
}

/// Grow dynamic vertex capacity to next power-of-two bucket.
pub(super) fn grow_capacity(min_required: usize) -> usize {
    min_required.next_power_of_two().max(1024)
}

fn is_software_adapter(device_type: wgpu::DeviceType, adapter_name: &str) -> bool {
    if matches!(
        device_type,
        wgpu::DeviceType::Cpu | wgpu::DeviceType::VirtualGpu
    ) {
        return true;
    }
    let lower = adapter_name.to_ascii_lowercase();
    ["swiftshader", "llvmpipe", "lavapipe", "softpipe", "warp"]
        .iter()
        .any(|needle| lower.contains(needle))
}

const SHADER_SOURCE: &str = r#"
struct ViewportUniform {
    viewport_size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> u_view: ViewportUniform;

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    let ndc_x = (v.position.x / u_view.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (v.position.y / u_view.viewport_size.y) * 2.0;
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = v.color;
    return out;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    return v.color;
}
"#;

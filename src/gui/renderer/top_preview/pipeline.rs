//! Pipeline/texture helpers for GPU TOP preview execution.

use super::super::viewer;

/// Create one render pipeline for a fullscreen TOP preview operation.
pub(super) fn create_op_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    fragment_entry: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("gui-top-preview-op-pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: "vs_fullscreen",
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: fragment_entry,
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
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

/// Create sampled + renderable preview texture resources.
pub(super) fn create_preview_texture_bundle(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    label: &str,
    texture_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = viewer::create_texture_bind_group(device, texture_layout, &view, sampler);
    (texture, view, bind_group)
}

pub(super) const OP_SHADER_SOURCE: &str = r#"
struct TopOpUniform {
    p0: vec4<f32>,
    p1: vec4<f32>,
    p2: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u_op: TopOpUniform;

@group(1) @binding(0)
var t_src: texture_2d<f32>;
@group(1) @binding(1)
var s_src: sampler;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> VertexOut {
    var out: VertexOut;
    if (vi == 0u) {
        out.clip_pos = vec4<f32>(-1.0, -1.0, 0.0, 1.0);
        out.uv = vec2<f32>(0.0, 1.0);
    } else if (vi == 1u) {
        out.clip_pos = vec4<f32>(1.0, -1.0, 0.0, 1.0);
        out.uv = vec2<f32>(1.0, 1.0);
    } else if (vi == 2u) {
        out.clip_pos = vec4<f32>(1.0, 1.0, 0.0, 1.0);
        out.uv = vec2<f32>(1.0, 0.0);
    } else if (vi == 3u) {
        out.clip_pos = vec4<f32>(-1.0, -1.0, 0.0, 1.0);
        out.uv = vec2<f32>(0.0, 1.0);
    } else if (vi == 4u) {
        out.clip_pos = vec4<f32>(1.0, 1.0, 0.0, 1.0);
        out.uv = vec2<f32>(1.0, 0.0);
    } else {
        out.clip_pos = vec4<f32>(-1.0, 1.0, 0.0, 1.0);
        out.uv = vec2<f32>(0.0, 0.0);
    }
    return out;
}

@fragment
fn fs_solid(v: VertexOut) -> @location(0) vec4<f32> {
    let bg = vec3<f32>(8.0 / 255.0, 8.0 / 255.0, 8.0 / 255.0);
    let center = u_op.p0.xy;
    let radius = max(u_op.p0.z, 0.01);
    let feather = max(u_op.p0.w, 0.0);
    let dist = distance(v.uv, center);
    let edge = smoothstep(radius + feather, radius - feather, dist);
    let alpha = clamp(edge * clamp(u_op.p1.w, 0.0, 1.0), 0.0, 1.0);
    let fg = clamp(u_op.p1.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
    let rgb = mix(bg, fg, alpha);
    return vec4<f32>(rgb, 1.0);
}

@fragment
fn fs_transform(v: VertexOut) -> @location(0) vec4<f32> {
    let src = textureSample(t_src, s_src, v.uv);
    let brightness = u_op.p0.x;
    let gain_r = u_op.p0.y;
    let gain_g = u_op.p0.z;
    let gain_b = u_op.p0.w;
    let alpha_mul = u_op.p1.x;
    let rgb = vec3<f32>(
        clamp(src.r * gain_r * brightness, 0.0, 1.0),
        clamp(src.g * gain_g * brightness, 0.0, 1.0),
        clamp(src.b * gain_b * brightness, 0.0, 1.0)
    );
    let a = clamp(src.a * alpha_mul, 0.0, 1.0);
    return vec4<f32>(rgb, a);
}
"#;

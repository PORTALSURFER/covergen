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
    // Operation passes write a full replacement texture each step.
    // Blending at this stage introduces unintended compositing artifacts.
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
                blend: None,
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
    let fg = clamp(u_op.p0.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
    let alpha = clamp(u_op.p0.w, 0.0, 1.0);
    return vec4<f32>(fg, alpha);
}

@fragment
fn fs_circle(v: VertexOut) -> @location(0) vec4<f32> {
    let center = u_op.p0.xy;
    let radius = max(u_op.p0.z, 0.01);
    let feather = max(u_op.p0.w, 0.0001);
    let delta = v.uv - center;
    let dist = length(delta);
    let pi = 3.14159265359;
    let tau = 6.28318530718;

    var boundary = radius;
    let segments = u_op.p2.z;
    if (segments >= 3.0) {
        let n = floor(segments);
        let half_sector = pi / n;
        let sector = tau / n;
        let theta = atan2(delta.y, delta.x);
        let wrapped = fract((theta + pi) / sector) * sector;
        let local = abs(wrapped - half_sector);
        boundary = radius * cos(half_sector) / max(cos(local), 0.0001);
    }

    let edge = smoothstep(boundary + feather, boundary - feather, dist);
    let theta_norm = fract((atan2(delta.y, delta.x) + pi) / tau);
    let start_norm = fract(u_op.p2.x / 360.0);
    let end_norm = fract(u_op.p2.y / 360.0);
    let arc_span = abs(u_op.p2.y - u_op.p2.x);
    var arc_mask = 1.0;
    if (arc_span < 359.9) {
        if (start_norm <= end_norm) {
            arc_mask = select(0.0, 1.0, theta_norm >= start_norm && theta_norm <= end_norm);
        } else {
            arc_mask = select(0.0, 1.0, theta_norm >= start_norm || theta_norm <= end_norm);
        }
    }

    let alpha = clamp(edge * arc_mask * clamp(u_op.p1.w, 0.0, 1.0), 0.0, 1.0);
    let fg = clamp(u_op.p1.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(fg, alpha);
}

@fragment
fn fs_sphere(v: VertexOut) -> @location(0) vec4<f32> {
    let center = u_op.p0.xy;
    let radius = max(u_op.p0.z, 0.01);
    let edge_softness = max(u_op.p0.w, 0.0);
    let dist = distance(v.uv, center);
    let edge = smoothstep(radius + edge_softness, radius - edge_softness, dist);
    if (edge <= 0.0001) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let rel = (v.uv - center) / radius;
    let rr = dot(rel, rel);
    let z = sqrt(max(1.0 - rr, 0.0));
    let n = normalize(vec3<f32>(rel.x, rel.y, z));
    let l = normalize(vec3<f32>(u_op.p1.x, u_op.p1.y, max(u_op.p1.z, 0.001)));
    let ambient = clamp(u_op.p1.w, 0.0, 1.0);
    let ndotl = max(dot(n, l), 0.0);
    let diffuse = ambient + (1.0 - ambient) * ndotl;

    let vdir = vec3<f32>(0.0, 0.0, 1.0);
    let h = normalize(l + vdir);
    let spec = pow(max(dot(n, h), 0.0), 32.0) * 0.2;

    let base = clamp(u_op.p2.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
    let lit = clamp(base * diffuse + vec3<f32>(spec), vec3<f32>(0.0), vec3<f32>(1.0));
    let alpha = clamp(edge * clamp(u_op.p2.w, 0.0, 1.0), 0.0, 1.0);
    return vec4<f32>(lit, alpha);
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

//! Pipeline/texture helpers for GPU tex preview execution.

use super::super::viewer;
use super::TEX_PREVIEW_TEXTURE_FORMAT;

/// Create one render pipeline for a fullscreen tex preview operation.
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
        label: Some("gui-tex-preview-op-pipeline"),
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
        format: TEX_PREVIEW_TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = viewer::create_texture_bind_group(device, texture_layout, &view, sampler);
    (texture, view, bind_group)
}

pub(super) const OP_SHADER_SOURCE: &str = r#"
struct TexOpUniform {
    p0: vec4<f32>,
    p1: vec4<f32>,
    p2: vec4<f32>,
    p3: vec4<f32>,
    p4: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u_op: TexOpUniform;

@group(1) @binding(0)
var t_src: texture_2d<f32>;
@group(1) @binding(1)
var s_src: sampler;
@group(2) @binding(0)
var t_feedback: texture_2d<f32>;
@group(2) @binding(1)
var s_feedback: sampler;

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

fn seamless_angular_noise(theta: f32, freq: f32, phase: f32) -> f32 {
    let safe_freq = max(freq, 0.01);
    let low_freq = max(floor(safe_freq), 1.0);
    let high_freq = low_freq + 1.0;
    let freq_blend = fract(safe_freq);
    // Use integer harmonics only so the wave is guaranteed 2*pi periodic.
    let low_wave = sin(theta * low_freq + phase) * 0.7
        + sin(theta * (low_freq * 2.0) - phase * 2.0) * 0.3;
    let high_wave = sin(theta * high_freq + phase) * 0.7
        + sin(theta * (high_freq * 2.0) - phase * 2.0) * 0.3;
    return mix(low_wave, high_wave, freq_blend);
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
    let noise_amount = clamp(u_op.p3.y, 0.0, 2.0);
    let noise_freq = max(u_op.p3.z, 0.01);
    let noise_phase = u_op.p3.w;
    let noise_twist = u_op.p4.x;
    let noise_stretch = clamp(u_op.p4.y, 0.0, 1.0);
    let src_delta = v.uv - center;
    let pi = 3.14159265359;
    let tau = 6.28318530718;
    let src_dist = length(src_delta);
    let src_theta = atan2(src_delta.y, src_delta.x);
    let safe_radius = max(radius, 0.0001);
    let twist_theta = src_theta + noise_twist * (src_dist / safe_radius);
    let c = cos(twist_theta);
    let s = sin(twist_theta);
    let local = vec2(
        c * src_delta.x + s * src_delta.y,
        -s * src_delta.x + c * src_delta.y
    );
    let stretch_scale = max(0.2, 1.0 + noise_stretch * sin(twist_theta + noise_phase));
    let stretched_local = vec2(local.x * stretch_scale, local.y / stretch_scale);
    let delta = vec2(
        c * stretched_local.x - s * stretched_local.y,
        s * stretched_local.x + c * stretched_local.y
    );
    let dist = length(delta);
    let theta = atan2(delta.y, delta.x);
    let noise_wave = seamless_angular_noise(theta, noise_freq, noise_phase);
    let noisy_radius = radius * (1.0 + noise_amount * 0.35 * noise_wave);

    var boundary = noisy_radius;
    let segments = u_op.p2.z;
    if (segments >= 3.0) {
        let n = floor(segments);
        let half_sector = pi / n;
        let sector = tau / n;
        let wrapped = fract((theta + pi) / sector) * sector;
        let local = abs(wrapped - half_sector);
        boundary = noisy_radius * cos(half_sector) / max(cos(local), 0.0001);
    }

    let edge = select(0.0, 1.0, dist <= boundary);
    let theta_norm = fract((theta + pi) / tau);
    let start_norm = fract(u_op.p2.x / 360.0);
    let end_norm = fract(u_op.p2.y / 360.0);
    let arc_span = abs(u_op.p2.y - u_op.p2.x);
    let arc_open = u_op.p2.w >= 0.5;
    let line_width = max(u_op.p3.x, 0.0005);
    var arc_mask = 1.0;
    if (arc_span < 359.9) {
        if (start_norm <= end_norm) {
            arc_mask = select(0.0, 1.0, theta_norm >= start_norm && theta_norm <= end_norm);
        } else {
            arc_mask = select(0.0, 1.0, theta_norm >= start_norm || theta_norm <= end_norm);
        }
    }

    var shape_alpha = edge;
    if (arc_open) {
        let inner = max(boundary - line_width, 0.0);
        let inner_edge = select(0.0, 1.0, dist <= inner);
        shape_alpha = clamp(edge - inner_edge, 0.0, 1.0);
    }

    let alpha = clamp(shape_alpha * arc_mask * clamp(u_op.p1.w, 0.0, 1.0), 0.0, 1.0);
    let fg = clamp(u_op.p1.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(fg, alpha);
}

@fragment
fn fs_sphere(v: VertexOut) -> @location(0) vec4<f32> {
    let center = u_op.p0.xy;
    let radius = max(u_op.p0.z, 0.01);
    let noise_amount = clamp(u_op.p3.x, 0.0, 2.0);
    let noise_freq = max(u_op.p3.y, 0.01);
    let noise_phase = u_op.p3.z;
    let noise_twist = u_op.p3.w;
    let noise_stretch = clamp(u_op.p4.x, 0.0, 1.0);
    let src_delta = v.uv - center;
    let src_dist = length(src_delta);
    let src_theta = atan2(src_delta.y, src_delta.x);
    let safe_radius = max(radius, 0.0001);
    let twist_theta = src_theta + noise_twist * (src_dist / safe_radius);
    let c = cos(twist_theta);
    let s = sin(twist_theta);
    let local = vec2(
        c * src_delta.x + s * src_delta.y,
        -s * src_delta.x + c * src_delta.y
    );
    let stretch_scale = max(0.2, 1.0 + noise_stretch * sin(twist_theta + noise_phase));
    let stretched_local = vec2(local.x * stretch_scale, local.y / stretch_scale);
    let delta = vec2(
        c * stretched_local.x - s * stretched_local.y,
        s * stretched_local.x + c * stretched_local.y
    );
    let dist = length(delta);
    let theta = atan2(delta.y, delta.x);
    let noise_wave = seamless_angular_noise(theta, noise_freq, noise_phase);
    let boundary = radius * (1.0 + noise_amount * 0.35 * noise_wave);
    let edge = select(0.0, 1.0, dist <= boundary);
    if (edge <= 0.0001) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let rel = delta / max(boundary, 0.001);
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

@fragment
fn fs_level(v: VertexOut) -> @location(0) vec4<f32> {
    let src = textureSample(t_src, s_src, v.uv);
    let in_low = clamp(u_op.p0.x, 0.0, 1.0);
    let in_high = clamp(u_op.p0.y, 0.0, 1.0);
    let gamma = clamp(u_op.p0.z, 0.1, 8.0);
    let out_low = clamp(u_op.p1.x, 0.0, 1.0);
    let out_high = clamp(u_op.p1.y, 0.0, 1.0);
    let input_range = in_high - in_low;
    let safe_range = select(
        min(input_range, -1e-5),
        max(input_range, 1e-5),
        input_range >= 0.0
    );
    let normalized = clamp((src.rgb - vec3<f32>(in_low)) / vec3<f32>(safe_range), vec3<f32>(0.0), vec3<f32>(1.0));
    let shaped = pow(normalized, vec3<f32>(1.0 / gamma));
    let leveled = mix(vec3<f32>(out_low), vec3<f32>(out_high), shaped);
    return vec4<f32>(leveled, src.a);
}

fn apply_transform_step(src: vec4<f32>, transform: vec4<f32>, alpha_mul: f32) -> vec4<f32> {
    let brightness = transform.x;
    let gain_r = transform.y;
    let gain_g = transform.z;
    let gain_b = transform.w;
    let rgb = vec3<f32>(
        clamp(src.r * gain_r * brightness, 0.0, 1.0),
        clamp(src.g * gain_g * brightness, 0.0, 1.0),
        clamp(src.b * gain_b * brightness, 0.0, 1.0)
    );
    let a = clamp(src.a * alpha_mul, 0.0, 1.0);
    return vec4<f32>(rgb, a);
}

@fragment
fn fs_transform_fused(v: VertexOut) -> @location(0) vec4<f32> {
    let src = textureSample(t_src, s_src, v.uv);
    let first = apply_transform_step(src, u_op.p0, u_op.p1.x);
    let second = apply_transform_step(first, u_op.p2, u_op.p3.x);
    return second;
}

@fragment
fn fs_feedback(v: VertexOut) -> @location(0) vec4<f32> {
    let src = textureSample(t_src, s_src, v.uv);
    let history = textureSample(t_feedback, s_feedback, v.uv);
    let mix_amount = clamp(u_op.p0.x, 0.0, 1.0);
    let src_pm = src.rgb * src.a;
    let history_pm = history.rgb * history.a;
    let out_a = mix(src.a, history.a, mix_amount);
    let out_pm = mix(src_pm, history_pm, mix_amount);
    // Quantized history buffers can leave tiny residual alpha that never
    // reaches exact zero; clamp near-black tails so feedback fades to black.
    let fade_epsilon = 1.5 / 255.0;
    if (out_a <= fade_epsilon || max(max(out_pm.r, out_pm.g), out_pm.b) <= fade_epsilon) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let safe_a = max(out_a, 1e-6);
    let out_rgb = select(vec3<f32>(0.0), out_pm / safe_a, out_a > 1e-6);
    return vec4<f32>(out_rgb, out_a);
}

fn blend_mode_rgb(base: vec3<f32>, layer: vec3<f32>, mode: i32) -> vec3<f32> {
    if (mode == 1) {
        return clamp(base + layer, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    if (mode == 2) {
        return clamp(base - layer, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    if (mode == 3) {
        return base * layer;
    }
    if (mode == 4) {
        return vec3<f32>(1.0) - (vec3<f32>(1.0) - base) * (vec3<f32>(1.0) - layer);
    }
    if (mode == 5) {
        let low = 2.0 * base * layer;
        let high = vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - base) * (vec3<f32>(1.0) - layer);
        let mask = step(vec3<f32>(0.5), base);
        return mix(low, high, mask);
    }
    if (mode == 6) {
        return min(base, layer);
    }
    if (mode == 7) {
        return max(base, layer);
    }
    if (mode == 8) {
        return abs(base - layer);
    }
    return layer;
}

@fragment
fn fs_blend(v: VertexOut) -> @location(0) vec4<f32> {
    let base = textureSample(t_src, s_src, v.uv);
    let layer = textureSample(t_feedback, s_feedback, v.uv);
    let mode = i32(round(clamp(u_op.p0.x, 0.0, 8.0)));
    let opacity = clamp(u_op.p0.y, 0.0, 1.0);
    let bg = vec4<f32>(clamp(u_op.p1.xyz, vec3<f32>(0.0), vec3<f32>(1.0)), clamp(u_op.p1.w, 0.0, 1.0));

    let base_pm = base.rgb * base.a;
    let blend_rgb = blend_mode_rgb(base.rgb, layer.rgb, mode);
    let layer_pm = blend_rgb * layer.a;
    let over_pm = layer_pm + base_pm * (1.0 - layer.a);
    let over_a = layer.a + base.a * (1.0 - layer.a);

    let composite_a = mix(base.a, over_a, opacity);
    let composite_pm = mix(base_pm, over_pm, opacity);
    // Optional background fill behind the blend result.
    let bg_pm = bg.rgb * bg.a;
    let out_a = composite_a + bg.a * (1.0 - composite_a);
    let out_pm = composite_pm + bg_pm * (1.0 - composite_a);
    let safe_a = max(out_a, 1e-6);
    let out_rgb = select(vec3<f32>(0.0), out_pm / safe_a, out_a > 1e-6);
    return vec4<f32>(out_rgb, out_a);
}
"#;

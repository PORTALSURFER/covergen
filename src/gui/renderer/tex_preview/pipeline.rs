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
    // Quantized RGBA8 feedback can leave residual tails at high mix values.
    // Increase the fade floor as feedback approaches 1.0 so stale history
    // converges to black instead of lingering as gray patches.
    let tail_boost = smoothstep(0.75, 1.0, mix_amount);
    let fade_epsilon = mix(1.5 / 255.0, 6.0 / 255.0, tail_boost);
    if (out_a <= fade_epsilon || max(max(out_pm.r, out_pm.g), out_pm.b) <= fade_epsilon) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let safe_a = max(out_a, 1e-6);
    let out_rgb = clamp(
        select(vec3<f32>(0.0), out_pm / safe_a, out_a > 1e-6),
        vec3<f32>(0.0),
        vec3<f32>(1.0)
    );
    return vec4<f32>(out_rgb, out_a);
}

fn rd_concentrations(uv: vec2<f32>) -> vec2<f32> {
    let src = textureSample(t_src, s_src, uv);
    let history = textureSample(t_feedback, s_feedback, uv);
    let history_weight = select(0.0, 1.0, history.a > 0.001);
    let state = mix(src, history, history_weight);
    return clamp(state.rg, vec2<f32>(0.0), vec2<f32>(1.0));
}

@fragment
fn fs_reaction_diffusion(v: VertexOut) -> @location(0) vec4<f32> {
    let diffusion_a = clamp(u_op.p0.x, 0.0, 2.0);
    let diffusion_b = clamp(u_op.p0.y, 0.0, 2.0);
    let feed = clamp(u_op.p0.z, 0.0, 0.12);
    let kill = clamp(u_op.p0.w, 0.0, 0.12);
    let seed_mix = clamp(u_op.p1.x, 0.0, 1.0);
    let dt = clamp(u_op.p1.y, 0.0, 2.0);

    let size = vec2<f32>(textureDimensions(t_feedback));
    let texel = vec2<f32>(1.0 / max(size.x, 1.0), 1.0 / max(size.y, 1.0));
    let center = rd_concentrations(v.uv);
    let north = rd_concentrations(v.uv + vec2<f32>(0.0, -texel.y));
    let south = rd_concentrations(v.uv + vec2<f32>(0.0, texel.y));
    let west = rd_concentrations(v.uv + vec2<f32>(-texel.x, 0.0));
    let east = rd_concentrations(v.uv + vec2<f32>(texel.x, 0.0));
    let north_west = rd_concentrations(v.uv + vec2<f32>(-texel.x, -texel.y));
    let north_east = rd_concentrations(v.uv + vec2<f32>(texel.x, -texel.y));
    let south_west = rd_concentrations(v.uv + vec2<f32>(-texel.x, texel.y));
    let south_east = rd_concentrations(v.uv + vec2<f32>(texel.x, texel.y));

    let laplacian = (north + south + west + east) * 0.2
        + (north_west + north_east + south_west + south_east) * 0.05
        - center;
    let a = center.x;
    let b = center.y;
    let reaction = a * b * b;
    var next_a = a + (diffusion_a * laplacian.x - reaction + feed * (1.0 - a)) * dt;
    var next_b = b + (diffusion_b * laplacian.y + reaction - (kill + feed) * b) * dt;
    next_a = clamp(next_a, 0.0, 1.0);
    next_b = clamp(next_b, 0.0, 1.0);

    let seed = clamp(textureSample(t_src, s_src, v.uv).rg, vec2<f32>(0.0), vec2<f32>(1.0));
    let next = mix(vec2<f32>(next_a, next_b), seed, seed_mix);
    let display = vec3<f32>(next.x, next.y, clamp(next.x - next.y * 0.5, 0.0, 1.0));
    return vec4<f32>(display, 1.0);
}

fn pp_luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn pp_hash21(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn pp_blur9(uv: vec2<f32>, texel: vec2<f32>, radius: f32) -> vec4<f32> {
    let r = texel * radius;
    let c0 = textureSample(t_src, s_src, uv);
    let c1 = textureSample(t_src, s_src, uv + vec2<f32>(r.x, 0.0));
    let c2 = textureSample(t_src, s_src, uv - vec2<f32>(r.x, 0.0));
    let c3 = textureSample(t_src, s_src, uv + vec2<f32>(0.0, r.y));
    let c4 = textureSample(t_src, s_src, uv - vec2<f32>(0.0, r.y));
    let c5 = textureSample(t_src, s_src, uv + vec2<f32>(r.x, r.y));
    let c6 = textureSample(t_src, s_src, uv + vec2<f32>(-r.x, r.y));
    let c7 = textureSample(t_src, s_src, uv + vec2<f32>(r.x, -r.y));
    let c8 = textureSample(t_src, s_src, uv + vec2<f32>(-r.x, -r.y));
    return (c0 * 0.2) + (c1 + c2 + c3 + c4) * 0.12 + (c5 + c6 + c7 + c8) * 0.08;
}

@fragment
fn fs_post_process(v: VertexOut) -> @location(0) vec4<f32> {
    let category = i32(round(clamp(u_op.p0.x, 0.0, 8.0)));
    let effect = i32(round(clamp(u_op.p0.y, 0.0, 15.0)));
    let amount = clamp(u_op.p0.z, 0.0, 1.0);
    let scale = clamp(u_op.p0.w, 0.0, 8.0);
    let threshold = clamp(u_op.p1.x, 0.0, 1.0);
    let speed = clamp(u_op.p1.y, 0.0, 8.0);
    let time = u_op.p1.z;
    let src = textureSample(t_src, s_src, v.uv);
    let history = textureSample(t_feedback, s_feedback, v.uv);
    let size = vec2<f32>(textureDimensions(t_src));
    let texel = vec2<f32>(1.0 / max(size.x, 1.0), 1.0 / max(size.y, 1.0));
    let blur = pp_blur9(v.uv, texel, max(scale, 0.5));
    let luma = pp_luma(src.rgb);

    if (category == 0) {
        if (effect == 0) {
            let bright = max(luma - threshold, 0.0);
            let glow = blur.rgb * bright * (1.0 + scale * 0.5);
            return vec4<f32>(clamp(src.rgb + glow * amount, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);
        } else if (effect == 8) {
            let levels = max(2.0, 16.0 - amount * 14.0);
            let q = floor(src.rgb * levels) / levels;
            return vec4<f32>(q, src.a);
        } else if (effect == 9) {
            let a = vec3<f32>(0.1, 0.2, 0.6);
            let b = vec3<f32>(1.0, 0.85, 0.35);
            let t = pow(luma, mix(2.0, 0.6, amount));
            return vec4<f32>(mix(a, b, t), src.a);
        }
        let contrast = mix(1.0, 2.5, amount);
        let graded = clamp((src.rgb - 0.5) * contrast + 0.5, vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(graded, src.a);
    }

    if (category == 1) {
        let gx = pp_luma(textureSample(t_src, s_src, v.uv + vec2<f32>(texel.x, 0.0)).rgb)
            - pp_luma(textureSample(t_src, s_src, v.uv - vec2<f32>(texel.x, 0.0)).rgb);
        let gy = pp_luma(textureSample(t_src, s_src, v.uv + vec2<f32>(0.0, texel.y)).rgb)
            - pp_luma(textureSample(t_src, s_src, v.uv - vec2<f32>(0.0, texel.y)).rgb);
        let edge = clamp(length(vec2<f32>(gx, gy)) * (1.0 + scale), 0.0, 1.0);
        if (effect == 1) {
            let line = select(0.0, 1.0, edge > threshold);
            return vec4<f32>(mix(src.rgb, src.rgb * (1.0 - line), amount), src.a);
        }
        if (effect == 2) {
            let e = vec3<f32>(0.5 + gx * 1.5, 0.5 + gy * 1.5, 0.5);
            return vec4<f32>(mix(src.rgb, clamp(e, vec3<f32>(0.0), vec3<f32>(1.0)), amount), src.a);
        }
        if (effect == 3) {
            let sharp = clamp(src.rgb + (src.rgb - blur.rgb) * (amount * 2.5), vec3<f32>(0.0), vec3<f32>(1.0));
            return vec4<f32>(sharp, src.a);
        }
        return vec4<f32>(mix(src.rgb, vec3<f32>(edge), amount), src.a);
    }

    if (category == 2) {
        let radial_dir = normalize(v.uv - vec2<f32>(0.5, 0.5) + vec2<f32>(1e-4, 1e-4));
        let radial_uv = v.uv + radial_dir * texel * scale * 6.0;
        let radial = textureSample(t_src, s_src, radial_uv);
        let blend = select(blur, radial, effect == 3);
        return vec4<f32>(mix(src.rgb, blend.rgb, amount), src.a);
    }

    if (category == 3) {
        let centered = v.uv - vec2<f32>(0.5, 0.5);
        let r2 = dot(centered, centered);
        let pulse = sin(time * speed + r2 * 60.0);
        let warp = centered * (1.0 + amount * (r2 * 2.0 + pulse * 0.05));
        let uv = warp + vec2<f32>(0.5, 0.5);
        if (effect == 0) {
            let off = texel * amount * 3.0;
            let cr = textureSample(t_src, s_src, uv + off).r;
            let cg = textureSample(t_src, s_src, uv).g;
            let cb = textureSample(t_src, s_src, uv - off).b;
            return vec4<f32>(vec3<f32>(cr, cg, cb), src.a);
        }
        return textureSample(t_src, s_src, uv);
    }

    if (category == 4) {
        let mix_amount = clamp(amount * (0.65 + threshold * 0.35), 0.0, 1.0);
        if (effect == 2) {
            let moshed = vec3<f32>(history.r, src.g, history.b);
            return vec4<f32>(mix(src.rgb, moshed, mix_amount), 1.0);
        }
        if (effect == 4) {
            let delayed_uv = v.uv + vec2<f32>(sin(time * speed) * texel.x * scale * 12.0, 0.0);
            let delayed = textureSample(t_feedback, s_feedback, delayed_uv);
            return vec4<f32>(mix(src.rgb, delayed.rgb, mix_amount), 1.0);
        }
        return vec4<f32>(mix(src.rgb, history.rgb, mix_amount), 1.0);
    }

    if (category == 5) {
        let n = pp_hash21(v.uv * size + vec2<f32>(time * speed * 37.0, time * speed * 11.0)) - 0.5;
        if (effect == 4) {
            let cell = max(1.0, floor(mix(1.0, 48.0, amount) * (1.0 + scale * 0.1)));
            let uv = (floor(v.uv * size / cell) * cell) / size;
            return textureSample(t_src, s_src, uv);
        }
        let noisy = clamp(src.rgb + n * amount * 0.2, vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(noisy, src.a);
    }

    if (category == 6) {
        if (effect == 2) {
            let d = distance(v.uv, vec2<f32>(0.5, 0.5));
            let vig = smoothstep(0.15 + threshold * 0.3, 0.9, d);
            return vec4<f32>(src.rgb * (1.0 - vig * amount), src.a);
        }
        let bright = max(pp_luma(src.rgb) - threshold, 0.0);
        let glow = blur.rgb * bright * amount;
        let tint = vec3<f32>(1.0, 0.65, 0.45);
        return vec4<f32>(clamp(src.rgb + glow * tint, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);
    }

    if (category == 7) {
        let occ = clamp(pp_luma(blur.rgb) - luma, -1.0, 1.0);
        if (effect == 3) {
            let fog = smoothstep(0.0, 1.0, v.uv.y);
            let fog_color = mix(src.rgb, vec3<f32>(0.2, 0.25, 0.3), fog * amount);
            return vec4<f32>(fog_color, src.a);
        }
        let shaded = clamp(src.rgb - occ * amount * 0.5, vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(shaded, src.a);
    }

    // Experimental
    if (effect == 0) {
        let cell = rd_concentrations(v.uv);
        let n = rd_concentrations(v.uv + vec2<f32>(0.0, -texel.y));
        let s = rd_concentrations(v.uv + vec2<f32>(0.0, texel.y));
        let w = rd_concentrations(v.uv + vec2<f32>(-texel.x, 0.0));
        let e = rd_concentrations(v.uv + vec2<f32>(texel.x, 0.0));
        let lap = (n + s + w + e) * 0.25 - cell;
        let a = clamp(cell.x + (lap.x - cell.x * cell.y * cell.y) * 0.8, 0.0, 1.0);
        let b = clamp(cell.y + (lap.y + cell.x * cell.y * cell.y - cell.y * 0.05) * 0.8, 0.0, 1.0);
        let rd = vec3<f32>(a, b, clamp(a - b * 0.5, 0.0, 1.0));
        return vec4<f32>(mix(src.rgb, rd, amount), 1.0);
    }
    if (effect == 2) {
        let centered = (v.uv - vec2<f32>(0.5, 0.5)) * (1.0 - amount * 0.15);
        let zoomed = textureSample(t_feedback, s_feedback, centered + vec2<f32>(0.5, 0.5));
        return vec4<f32>(mix(src.rgb, zoomed.rgb, amount), 1.0);
    }
    if (effect == 3) {
        let p = abs(fract(v.uv * (2.0 + floor(scale))) - 0.5);
        let uv = abs(vec2<f32>(p.x, p.y)) * 2.0;
        return textureSample(t_src, s_src, uv);
    }
    let advect = textureSample(
        t_src,
        s_src,
        v.uv + vec2<f32>(sin(time * speed + v.uv.y * 24.0), cos(time * speed + v.uv.x * 24.0))
            * texel
            * amount
            * (4.0 + scale),
    );
    return vec4<f32>(mix(src.rgb, advect.rgb, amount), src.a);
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

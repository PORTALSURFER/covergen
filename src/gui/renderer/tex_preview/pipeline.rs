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

pub(super) const OP_SHADER_SOURCE: &str = include_str!("op_shader.wgsl");

#[cfg(test)]
mod tests {
    use super::OP_SHADER_SOURCE;

    #[test]
    fn op_shader_declares_expected_pipeline_entry_points() {
        let entries = [
            "fn vs_fullscreen(",
            "fn fs_solid(",
            "fn fs_circle(",
            "fn fs_sphere(",
            "fn fs_transform(",
            "fn fs_level(",
            "fn fs_transform_fused(",
            "fn fs_feedback(",
            "fn fs_reaction_diffusion(",
            "fn fs_post_process(",
            "fn fs_blend(",
        ];
        for entry in entries {
            assert!(
                OP_SHADER_SOURCE.contains(entry),
                "missing shader entry point: {entry}"
            );
        }
    }

    #[test]
    fn fullscreen_vertex_shader_entry_is_unique() {
        let count = OP_SHADER_SOURCE.match_indices("fn vs_fullscreen(").count();
        assert_eq!(count, 1, "expected exactly one fullscreen vertex entry");
    }
}

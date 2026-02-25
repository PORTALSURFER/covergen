//! Frame-time TOP preview preparation and GPU operation execution.

use crate::gui::geometry::Rect;
use crate::gui::top_view::{TopViewerFrame, TopViewerOp, TopViewerPayload};

use super::super::viewer;
use super::pipeline::create_preview_texture_bundle;
use super::{RenderTargetRef, TopOpUniform, TopPreviewRenderer, PREVIEW_BG};

impl TopPreviewRenderer {
    /// Prepare viewer resources and content for the current frame.
    pub(in crate::gui::renderer) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        top_view: Option<TopViewerFrame<'_>>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> u64 {
        let Some(top_view) = top_view else {
            self.viewer_visible = false;
            return 0;
        };
        if top_view.width == 0 || top_view.height == 0 {
            self.viewer_visible = false;
            return 0;
        }
        let mut upload_bytes = 0u64;
        self.ensure_viewer_texture(device, top_view.width, top_view.height);
        let rect = Rect::new(
            top_view.x,
            top_view.y,
            top_view.width as i32,
            top_view.height as i32,
        );
        let quad = viewer::quad_vertices(rect);
        upload_bytes = upload_bytes.saturating_add(std::mem::size_of_val(&quad) as u64);
        queue.write_buffer(&self.viewer_quad_buffer, 0, bytemuck::cast_slice(&quad));

        let ops = match top_view.payload {
            TopViewerPayload::GpuOps(ops) => ops,
        };
        if let Some(op_upload_bytes) =
            self.encode_gpu_ops(device, queue, encoder, ops, top_view.width, top_view.height)
        {
            upload_bytes = upload_bytes.saturating_add(op_upload_bytes);
        } else {
            self.clear_viewer_target(encoder);
        }
        self.viewer_visible = true;
        upload_bytes
    }

    fn encode_gpu_ops(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        ops: &[TopViewerOp],
        width: u32,
        height: u32,
    ) -> Option<u64> {
        if ops.is_empty() {
            return None;
        }
        let mut upload_bytes = 0u64;
        if ops.len() > 1 {
            self.ensure_scratch_textures(device, width, height);
        }
        let mut source_target = None;
        let mut scratch_flip = false;
        for (index, op) in ops.iter().copied().enumerate() {
            let last = index + 1 == ops.len();
            let target = if last {
                RenderTargetRef::Viewer
            } else if scratch_flip {
                scratch_flip = false;
                RenderTargetRef::ScratchB
            } else {
                scratch_flip = true;
                RenderTargetRef::ScratchA
            };
            let Some(target_view) = self.target_view(target) else {
                return None;
            };

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui-top-preview-op-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(PREVIEW_BG),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            match op {
                TopViewerOp::Solid { .. } => {
                    upload_bytes =
                        upload_bytes.saturating_add(std::mem::size_of::<TopOpUniform>() as u64);
                    queue.write_buffer(
                        &self.op_uniform_buffer,
                        0,
                        bytemuck::bytes_of(&TopOpUniform::solid(op)),
                    );
                    pass.set_pipeline(&self.op_solid_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.dummy_bind_group, &[]);
                }
                TopViewerOp::Circle { .. } => {
                    upload_bytes =
                        upload_bytes.saturating_add(std::mem::size_of::<TopOpUniform>() as u64);
                    queue.write_buffer(
                        &self.op_uniform_buffer,
                        0,
                        bytemuck::bytes_of(&TopOpUniform::circle(op)),
                    );
                    pass.set_pipeline(&self.op_circle_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.dummy_bind_group, &[]);
                }
                TopViewerOp::Transform { .. } => {
                    let Some(src_target) = source_target else {
                        return None;
                    };
                    let Some(src_bind_group) = self.target_bind_group(src_target) else {
                        return None;
                    };
                    upload_bytes =
                        upload_bytes.saturating_add(std::mem::size_of::<TopOpUniform>() as u64);
                    queue.write_buffer(
                        &self.op_uniform_buffer,
                        0,
                        bytemuck::bytes_of(&TopOpUniform::transform(op)),
                    );
                    pass.set_pipeline(&self.op_transform_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[]);
                    pass.set_bind_group(1, src_bind_group, &[]);
                }
            }
            pass.draw(0..6, 0..1);
            source_target = Some(target);
        }
        Some(upload_bytes)
    }

    fn clear_viewer_target(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(view) = self.viewer_texture_view.as_ref() else {
            return;
        };
        let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gui-top-preview-clear-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(PREVIEW_BG),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
    }

    fn ensure_viewer_texture(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.viewer_texture_size == (width, height) && self.viewer_bind_group.is_some() {
            return;
        }
        self.viewer_texture_size = (width, height);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui-top-viewer-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = viewer::create_texture_bind_group(
            device,
            &self.viewer_texture_layout,
            &texture_view,
            &self.viewer_sampler,
        );
        self.viewer_texture = Some(texture);
        self.viewer_texture_view = Some(texture_view);
        self.viewer_bind_group = Some(bind_group);
    }

    fn ensure_scratch_textures(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.scratch_texture_size == (width, height)
            && self.scratch_bind_group_a.is_some()
            && self.scratch_bind_group_b.is_some()
        {
            return;
        }
        self.scratch_texture_size = (width, height);
        let (a_texture, a_view, a_bind) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-top-preview-scratch-a",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        let (b_texture, b_view, b_bind) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-top-preview-scratch-b",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        self.scratch_texture_a = Some(a_texture);
        self.scratch_view_a = Some(a_view);
        self.scratch_bind_group_a = Some(a_bind);
        self.scratch_texture_b = Some(b_texture);
        self.scratch_view_b = Some(b_view);
        self.scratch_bind_group_b = Some(b_bind);
    }

    fn target_view(&self, target: RenderTargetRef) -> Option<&wgpu::TextureView> {
        match target {
            RenderTargetRef::Viewer => self.viewer_texture_view.as_ref(),
            RenderTargetRef::ScratchA => self.scratch_view_a.as_ref(),
            RenderTargetRef::ScratchB => self.scratch_view_b.as_ref(),
        }
    }

    fn target_bind_group(&self, target: RenderTargetRef) -> Option<&wgpu::BindGroup> {
        match target {
            RenderTargetRef::Viewer => self.viewer_bind_group.as_ref(),
            RenderTargetRef::ScratchA => self.scratch_bind_group_a.as_ref(),
            RenderTargetRef::ScratchB => self.scratch_bind_group_b.as_ref(),
        }
    }
}

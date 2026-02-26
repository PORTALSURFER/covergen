//! Frame-time TOP preview preparation and GPU operation execution.

use crate::gui::geometry::Rect;
use crate::gui::top_view::{TopViewerFrame, TopViewerOp, TopViewerPayload};

use super::super::viewer;
use super::pipeline::create_preview_texture_bundle;
use super::{RenderTargetRef, TopOpUniform, TopPreviewRenderer, PREVIEW_BG};

const TRANSPARENT_BG: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

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
        if top_view.width == 0
            || top_view.height == 0
            || top_view.texture_width == 0
            || top_view.texture_height == 0
        {
            self.viewer_visible = false;
            return 0;
        }
        let mut upload_bytes = 0u64;
        self.ensure_viewer_texture(device, top_view.texture_width, top_view.texture_height);
        let rect = Rect::new(
            top_view.x,
            top_view.y,
            top_view.width as i32,
            top_view.height as i32,
        );
        let quad = viewer::quad_vertices(rect);
        upload_bytes = upload_bytes.saturating_add(std::mem::size_of_val(&quad) as u64);
        queue.write_buffer(&self.viewer_quad_buffer, 0, bytemuck::cast_slice(&quad));

        let TopViewerPayload::GpuOps(ops) = top_view.payload;
        if let Some(op_upload_bytes) = self.encode_gpu_ops(
            device,
            queue,
            encoder,
            ops,
            top_view.texture_width,
            top_view.texture_height,
        ) {
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
        self.ensure_op_uniform_capacity(device, ops.len());
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
            if let TopViewerOp::Feedback { node_id, .. } = op {
                self.ensure_feedback_history_slot(device, encoder, node_id, width, height);
            }
            let target_view = self.target_view(target)?;
            let uniform_offset = self.op_uniform_offset(index);
            let Ok(dynamic_offset) = u32::try_from(uniform_offset) else {
                return None;
            };
            let clear_color = op_clear_color(op);

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui-top-preview-op-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
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
                        uniform_offset,
                        bytemuck::bytes_of(&TopOpUniform::solid(op)),
                    );
                    pass.set_pipeline(&self.op_solid_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, &self.dummy_bind_group, &[]);
                    pass.set_bind_group(2, &self.dummy_bind_group, &[]);
                }
                TopViewerOp::Circle { .. } => {
                    upload_bytes =
                        upload_bytes.saturating_add(std::mem::size_of::<TopOpUniform>() as u64);
                    queue.write_buffer(
                        &self.op_uniform_buffer,
                        uniform_offset,
                        bytemuck::bytes_of(&TopOpUniform::circle(op)),
                    );
                    pass.set_pipeline(&self.op_circle_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, &self.dummy_bind_group, &[]);
                    pass.set_bind_group(2, &self.dummy_bind_group, &[]);
                }
                TopViewerOp::Sphere { .. } => {
                    upload_bytes =
                        upload_bytes.saturating_add(std::mem::size_of::<TopOpUniform>() as u64);
                    queue.write_buffer(
                        &self.op_uniform_buffer,
                        uniform_offset,
                        bytemuck::bytes_of(&TopOpUniform::sphere(op)),
                    );
                    pass.set_pipeline(&self.op_sphere_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, &self.dummy_bind_group, &[]);
                    pass.set_bind_group(2, &self.dummy_bind_group, &[]);
                }
                TopViewerOp::Transform { .. } => {
                    let src_target = source_target?;
                    let src_bind_group = self.target_bind_group(src_target)?;
                    upload_bytes =
                        upload_bytes.saturating_add(std::mem::size_of::<TopOpUniform>() as u64);
                    queue.write_buffer(
                        &self.op_uniform_buffer,
                        uniform_offset,
                        bytemuck::bytes_of(&TopOpUniform::transform(op)),
                    );
                    pass.set_pipeline(&self.op_transform_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, src_bind_group, &[]);
                    pass.set_bind_group(2, &self.dummy_bind_group, &[]);
                }
                TopViewerOp::Feedback { node_id, .. } => {
                    let src_target = source_target?;
                    let src_bind_group = self.target_bind_group(src_target)?;
                    let history_bind_group = self.feedback_history_bind_group(node_id)?;
                    upload_bytes =
                        upload_bytes.saturating_add(std::mem::size_of::<TopOpUniform>() as u64);
                    queue.write_buffer(
                        &self.op_uniform_buffer,
                        uniform_offset,
                        bytemuck::bytes_of(&TopOpUniform::feedback(op)),
                    );
                    pass.set_pipeline(&self.op_feedback_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, src_bind_group, &[]);
                    pass.set_bind_group(2, history_bind_group, &[]);
                }
            }
            pass.draw(0..6, 0..1);
            drop(pass);
            if let TopViewerOp::Feedback { node_id, .. } = op {
                self.copy_target_to_feedback_history(encoder, target, node_id, width, height);
            }
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
                | wgpu::TextureUsages::COPY_SRC
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

    fn target_texture(&self, target: RenderTargetRef) -> Option<&wgpu::Texture> {
        match target {
            RenderTargetRef::Viewer => self.viewer_texture.as_ref(),
            RenderTargetRef::ScratchA => self.scratch_texture_a.as_ref(),
            RenderTargetRef::ScratchB => self.scratch_texture_b.as_ref(),
        }
    }

    fn ensure_feedback_history_slot(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        node_id: u32,
        width: u32,
        height: u32,
    ) {
        if self
            .feedback_history
            .get(&node_id)
            .map(|slot| slot.size == (width, height))
            .unwrap_or(false)
        {
            return;
        }
        let (texture, view, bind_group) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-top-preview-feedback-history",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gui-top-preview-feedback-history-clear-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
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
        self.feedback_history.insert(
            node_id,
            super::FeedbackHistorySlot {
                texture,
                bind_group,
                size: (width, height),
            },
        );
    }

    fn feedback_history_bind_group(&self, node_id: u32) -> Option<&wgpu::BindGroup> {
        self.feedback_history
            .get(&node_id)
            .map(|slot| &slot.bind_group)
    }

    fn copy_target_to_feedback_history(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: RenderTargetRef,
        node_id: u32,
        width: u32,
        height: u32,
    ) {
        let Some(src_texture) = self.target_texture(target) else {
            return;
        };
        let Some(slot) = self.feedback_history.get(&node_id) else {
            return;
        };
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: src_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: &slot.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

fn op_clear_color(op: TopViewerOp) -> wgpu::Color {
    match op {
        TopViewerOp::Sphere {
            alpha_clip: true, ..
        }
        | TopViewerOp::Circle {
            alpha_clip: true, ..
        } => TRANSPARENT_BG,
        _ => PREVIEW_BG,
    }
}

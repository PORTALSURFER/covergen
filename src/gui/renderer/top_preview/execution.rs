//! Frame-time TOP preview preparation and GPU operation execution.

use crate::gui::geometry::Rect;
use crate::gui::top_view::{TopViewerFrame, TopViewerOp, TopViewerPayload};

use super::super::viewer;
use super::pipeline::create_preview_texture_bundle;
use super::{
    CachedTextureSlot, FeedbackHistoryKey, FeedbackHistorySlot, RenderTargetRef, TopOpUniform,
    TopPreviewRenderer, PREVIEW_BG,
};

const TRANSPARENT_BG: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

const TOP_OP_UNIFORM_SIZE: usize = std::mem::size_of::<TopOpUniform>();

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
        self.ensure_op_uniform_capacity(device, ops.len());
        let upload_bytes = self.write_op_uniforms(queue, ops);
        if ops.len() > 1 {
            self.ensure_scratch_textures(device, width, height);
        }
        let mut source_target: Option<RenderTargetRef> = None;
        let mut scratch_flip = false;
        for (index, op) in ops.iter().copied().enumerate() {
            if let TopViewerOp::StoreTexture { texture_node_id } = op {
                let src_target = source_target?;
                self.ensure_blend_source_slot(device, encoder, texture_node_id, width, height);
                self.copy_target_to_blend_source(
                    encoder,
                    src_target,
                    texture_node_id,
                    width,
                    height,
                );
                continue;
            }
            let feedback_history_key = match op {
                TopViewerOp::Feedback { history, .. } => {
                    Some(FeedbackHistoryKey::from_binding(history))
                }
                _ => None,
            };
            if let Some(history_key) = feedback_history_key {
                self.ensure_feedback_history_slot(device, encoder, history_key, width, height);
            }
            let last = index + 1 == ops.len();
            let mut target = if last {
                RenderTargetRef::Viewer
            } else if scratch_flip {
                scratch_flip = false;
                RenderTargetRef::ScratchB
            } else {
                scratch_flip = true;
                RenderTargetRef::ScratchA
            };
            if let Some(history_key) = feedback_history_key {
                target = self.feedback_history_write_target(history_key)?;
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
                    pass.set_pipeline(&self.op_solid_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, &self.dummy_bind_group, &[]);
                    pass.set_bind_group(2, &self.dummy_bind_group, &[]);
                }
                TopViewerOp::Circle { .. } => {
                    pass.set_pipeline(&self.op_circle_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, &self.dummy_bind_group, &[]);
                    pass.set_bind_group(2, &self.dummy_bind_group, &[]);
                }
                TopViewerOp::Sphere { .. } => {
                    pass.set_pipeline(&self.op_sphere_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, &self.dummy_bind_group, &[]);
                    pass.set_bind_group(2, &self.dummy_bind_group, &[]);
                }
                TopViewerOp::Transform { .. } => {
                    let src_target = source_target?;
                    let src_bind_group = self.target_bind_group(src_target)?;
                    pass.set_pipeline(&self.op_transform_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, src_bind_group, &[]);
                    pass.set_bind_group(2, &self.dummy_bind_group, &[]);
                }
                TopViewerOp::Feedback { .. } => {
                    let src_target = source_target?;
                    let src_bind_group = self.target_bind_group(src_target)?;
                    let history_key = feedback_history_key?;
                    let history_bind_group = self.feedback_history_read_bind_group(history_key)?;
                    pass.set_pipeline(&self.op_feedback_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, src_bind_group, &[]);
                    pass.set_bind_group(2, history_bind_group, &[]);
                }
                TopViewerOp::Blend {
                    base_texture_node_id,
                    layer_texture_node_id,
                    ..
                } => {
                    let base_bind_group = self
                        .blend_source_bind_group(base_texture_node_id)
                        .or_else(|| {
                            source_target.and_then(|target_ref| self.target_bind_group(target_ref))
                        })?;
                    let layer_bind_group = layer_texture_node_id
                        .and_then(|id| self.blend_source_bind_group(id))
                        .unwrap_or(&self.dummy_bind_group);
                    pass.set_pipeline(&self.op_blend_pipeline);
                    pass.set_bind_group(0, &self.op_uniform_bind_group, &[dynamic_offset]);
                    pass.set_bind_group(1, base_bind_group, &[]);
                    pass.set_bind_group(2, layer_bind_group, &[]);
                }
                TopViewerOp::StoreTexture { .. } => {
                    return None;
                }
            }
            pass.draw(0..6, 0..1);
            drop(pass);
            source_target = if let Some(history_key) = feedback_history_key {
                self.swap_feedback_history(history_key)
            } else {
                Some(target)
            };
        }
        if let Some(final_target) = source_target {
            self.copy_target_to_viewer(encoder, final_target, width, height);
        } else {
            return None;
        }
        Some(upload_bytes)
    }

    fn op_uniform_for(op: TopViewerOp) -> TopOpUniform {
        match op {
            TopViewerOp::Solid { .. } => TopOpUniform::solid(op),
            TopViewerOp::Circle { .. } => TopOpUniform::circle(op),
            TopViewerOp::Sphere { .. } => TopOpUniform::sphere(op),
            TopViewerOp::Transform { .. } => TopOpUniform::transform(op),
            TopViewerOp::Feedback { .. } => TopOpUniform::feedback(op),
            TopViewerOp::Blend { .. } => TopOpUniform::blend(op),
            TopViewerOp::StoreTexture { .. } => TopOpUniform::solid(op),
        }
    }

    fn write_op_uniforms(&mut self, queue: &wgpu::Queue, ops: &[TopViewerOp]) -> u64 {
        if ops.is_empty() {
            return 0;
        }
        let stride = self.op_uniform_stride as usize;
        let upload_len = stride.saturating_mul(ops.len());
        self.op_uniform_staging.resize(upload_len, 0);
        for (index, op) in ops.iter().copied().enumerate() {
            let offset = stride.saturating_mul(index);
            let chunk = &mut self.op_uniform_staging[offset..offset + stride];
            chunk.fill(0);
            let uniform = Self::op_uniform_for(op);
            chunk[..TOP_OP_UNIFORM_SIZE].copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        queue.write_buffer(
            &self.op_uniform_buffer,
            0,
            &self.op_uniform_staging[..upload_len],
        );
        upload_len as u64
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
            RenderTargetRef::FeedbackHistory { key, slot_index } => self
                .feedback_history
                .get(&key)
                .and_then(|history| history.slots.get(slot_index))
                .map(|slot| &slot.view),
        }
    }

    fn target_bind_group(&self, target: RenderTargetRef) -> Option<&wgpu::BindGroup> {
        match target {
            RenderTargetRef::Viewer => self.viewer_bind_group.as_ref(),
            RenderTargetRef::ScratchA => self.scratch_bind_group_a.as_ref(),
            RenderTargetRef::ScratchB => self.scratch_bind_group_b.as_ref(),
            RenderTargetRef::FeedbackHistory { key, slot_index } => self
                .feedback_history
                .get(&key)
                .and_then(|history| history.slots.get(slot_index))
                .map(|slot| &slot.bind_group),
        }
    }

    fn target_texture(&self, target: RenderTargetRef) -> Option<&wgpu::Texture> {
        match target {
            RenderTargetRef::Viewer => self.viewer_texture.as_ref(),
            RenderTargetRef::ScratchA => self.scratch_texture_a.as_ref(),
            RenderTargetRef::ScratchB => self.scratch_texture_b.as_ref(),
            RenderTargetRef::FeedbackHistory { key, slot_index } => self
                .feedback_history
                .get(&key)
                .and_then(|history| history.slots.get(slot_index))
                .map(|slot| &slot.texture),
        }
    }

    fn ensure_feedback_history_slot(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        key: FeedbackHistoryKey,
        width: u32,
        height: u32,
    ) {
        if self
            .feedback_history
            .get(&key)
            .map(|history| history.slots[0].size == (width, height))
            .unwrap_or(false)
        {
            return;
        }
        let (texture_a, view_a, bind_group_a) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-top-preview-feedback-history-a",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        let (texture_b, view_b, bind_group_b) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-top-preview-feedback-history-b",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        for view in [&view_a, &view_b] {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui-top-preview-feedback-history-clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(TRANSPARENT_BG),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }
        self.feedback_history.insert(
            key,
            FeedbackHistorySlot {
                slots: [
                    CachedTextureSlot {
                        texture: texture_a,
                        view: view_a,
                        bind_group: bind_group_a,
                        size: (width, height),
                    },
                    CachedTextureSlot {
                        texture: texture_b,
                        view: view_b,
                        bind_group: bind_group_b,
                        size: (width, height),
                    },
                ],
                read_index: 0,
            },
        );
    }

    fn feedback_history_read_bind_group(
        &self,
        key: FeedbackHistoryKey,
    ) -> Option<&wgpu::BindGroup> {
        let history = self.feedback_history.get(&key)?;
        history
            .slots
            .get(history.read_index)
            .map(|slot| &slot.bind_group)
    }

    fn feedback_history_write_target(&self, key: FeedbackHistoryKey) -> Option<RenderTargetRef> {
        let history = self.feedback_history.get(&key)?;
        let write_index = 1usize.saturating_sub(history.read_index);
        Some(RenderTargetRef::FeedbackHistory {
            key,
            slot_index: write_index,
        })
    }

    fn swap_feedback_history(&mut self, key: FeedbackHistoryKey) -> Option<RenderTargetRef> {
        let history = self.feedback_history.get_mut(&key)?;
        history.read_index = 1usize.saturating_sub(history.read_index);
        Some(RenderTargetRef::FeedbackHistory {
            key,
            slot_index: history.read_index,
        })
    }

    fn copy_target_to_viewer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: RenderTargetRef,
        width: u32,
        height: u32,
    ) {
        let Some(src_texture) = self.target_texture(target) else {
            return;
        };
        let Some(dst_texture) = self.viewer_texture.as_ref() else {
            return;
        };
        if matches!(target, RenderTargetRef::Viewer) {
            return;
        }
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: src_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: dst_texture,
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

    fn copy_target_to_blend_source(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: RenderTargetRef,
        texture_node_id: u32,
        width: u32,
        height: u32,
    ) {
        let Some(src_texture) = self.target_texture(target) else {
            return;
        };
        let Some(slot) = self.blend_source_slots.get(&texture_node_id) else {
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

    fn ensure_blend_source_slot(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture_node_id: u32,
        width: u32,
        height: u32,
    ) {
        if self
            .blend_source_slots
            .get(&texture_node_id)
            .map(|slot| slot.size == (width, height))
            .unwrap_or(false)
        {
            return;
        }
        let (texture, view, bind_group) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-top-preview-blend-source",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui-top-preview-blend-source-clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(TRANSPARENT_BG),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }
        self.blend_source_slots.insert(
            texture_node_id,
            CachedTextureSlot {
                texture,
                view,
                bind_group,
                size: (width, height),
            },
        );
    }

    fn blend_source_bind_group(&self, texture_node_id: u32) -> Option<&wgpu::BindGroup> {
        self.blend_source_slots
            .get(&texture_node_id)
            .map(|slot| &slot.bind_group)
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

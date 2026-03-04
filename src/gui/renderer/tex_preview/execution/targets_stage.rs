//! Texture target and copy helpers for staged tex-preview execution.

use super::*;

impl TexPreviewRenderer {
    pub(super) fn ensure_viewer_texture(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.viewer_texture_size == (width, height) && self.viewer_bind_group.is_some() {
            return;
        }
        self.viewer_texture_size = (width, height);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui-tex-viewer-texture"),
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

    pub(super) fn ensure_scratch_textures(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) {
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
            "gui-tex-preview-scratch-a",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        let (b_texture, b_view, b_bind) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-tex-preview-scratch-b",
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

    pub(super) fn target_view(&self, target: RenderTargetRef) -> Option<&wgpu::TextureView> {
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

    pub(super) fn target_bind_group(&self, target: RenderTargetRef) -> Option<&wgpu::BindGroup> {
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

    pub(super) fn target_texture(&self, target: RenderTargetRef) -> Option<&wgpu::Texture> {
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

    pub(super) fn copy_target_to_viewer(
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

    pub(super) fn copy_target_to_blend_source(
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

    pub(super) fn copy_target_to_target(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        src_target: RenderTargetRef,
        dst_target: RenderTargetRef,
        width: u32,
        height: u32,
    ) {
        if src_target == dst_target {
            return;
        }
        let Some(src_texture) = self.target_texture(src_target) else {
            return;
        };
        let Some(dst_texture) = self.target_texture(dst_target) else {
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

    pub(super) fn ensure_blend_source_slot(
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
            "gui-tex-preview-blend-source",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui-tex-preview-blend-source-clear-pass"),
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

    pub(super) fn blend_source_bind_group_for_texture(
        &self,
        texture_node_id: u32,
    ) -> Option<&wgpu::BindGroup> {
        self.blend_source_aliases
            .get(&texture_node_id)
            .copied()
            .and_then(|target| self.target_bind_group(target))
            .or_else(|| self.blend_source_bind_group(texture_node_id))
    }
}

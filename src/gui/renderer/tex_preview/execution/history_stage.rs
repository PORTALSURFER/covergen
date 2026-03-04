//! Feedback-history cache lifecycle helpers.

use super::*;

impl TexPreviewRenderer {
    pub(super) fn ensure_feedback_history_slot(
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
            "gui-tex-preview-feedback-history-a",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        let (texture_b, view_b, bind_group_b) = create_preview_texture_bundle(
            device,
            width,
            height,
            "gui-tex-preview-feedback-history-b",
            &self.viewer_texture_layout,
            &self.op_sampler,
        );
        for view in [&view_a, &view_b] {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui-tex-preview-feedback-history-clear-pass"),
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
                write_cooldown: 0,
                configured_gap: 0,
            },
        );
    }

    pub(super) fn feedback_history_read_bind_group(
        &self,
        key: FeedbackHistoryKey,
    ) -> Option<&wgpu::BindGroup> {
        let history = self.feedback_history.get(&key)?;
        history
            .slots
            .get(history.read_index)
            .map(|slot| &slot.bind_group)
    }

    pub(super) fn feedback_history_write_target(
        &self,
        key: FeedbackHistoryKey,
    ) -> Option<RenderTargetRef> {
        let history = self.feedback_history.get(&key)?;
        let write_index = 1usize.saturating_sub(history.read_index);
        Some(RenderTargetRef::FeedbackHistory {
            key,
            slot_index: write_index,
        })
    }

    pub(super) fn swap_feedback_history(
        &mut self,
        key: FeedbackHistoryKey,
    ) -> Option<RenderTargetRef> {
        let history = self.feedback_history.get_mut(&key)?;
        history.read_index = 1usize.saturating_sub(history.read_index);
        Some(RenderTargetRef::FeedbackHistory {
            key,
            slot_index: history.read_index,
        })
    }
}

pub(super) fn consume_feedback_write_cooldown(write_cooldown: &mut u32, frame_gap: u32) -> bool {
    if *write_cooldown == 0 {
        *write_cooldown = frame_gap;
        true
    } else {
        *write_cooldown = (*write_cooldown).saturating_sub(1);
        false
    }
}

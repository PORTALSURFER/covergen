//! Frame-scoped dynamic-uniform staging for graph compute passes.

use super::*;

/// Result of staging one uniform block for a later frame upload.
#[derive(Clone, Copy, Debug)]
pub(super) struct PreparedUniform {
    pub(super) dynamic_offset: u32,
    pub(super) resized_buffer: bool,
}

/// CPU-side staging + GPU backing buffer for one dynamic-uniform batch.
#[derive(Debug)]
pub(super) struct UniformBatchState {
    buffer: wgpu::Buffer,
    staging: Vec<u8>,
    stride: usize,
    used: usize,
    capacity: usize,
    label: &'static str,
}

impl UniformBatchState {
    /// Create one uniform batch with capacity for at least one dynamic slice.
    pub(super) fn new(device: &wgpu::Device, label: &'static str) -> Self {
        let stride = aligned_uniform_stride(device.limits().min_uniform_buffer_offset_alignment);
        Self {
            buffer: create_uniform_buffer(device, label, stride),
            staging: Vec::new(),
            stride,
            used: 0,
            capacity: 1,
            label,
        }
    }

    /// Reset frame-local staging while retaining allocated capacity.
    pub(super) fn begin_frame(&mut self) {
        self.used = 0;
    }

    /// Stage one uniform block and return the dynamic offset used by the pass.
    pub(super) fn push(
        &mut self,
        device: &wgpu::Device,
        uniform: GraphOpUniforms,
    ) -> PreparedUniform {
        let resized_buffer = self.ensure_capacity(device, self.used.saturating_add(1));
        let slot = self.used;
        write_uniform_slot(&mut self.staging, self.stride, slot, &uniform);
        self.used = self.used.saturating_add(1);
        PreparedUniform {
            dynamic_offset: u32::try_from(slot.saturating_mul(self.stride))
                .expect("dynamic uniform offset should fit in u32"),
            resized_buffer,
        }
    }

    /// Upload the staged uniform bytes for the current frame in one queue write.
    pub(super) fn flush(&mut self, queue: &wgpu::Queue) {
        if self.used == 0 {
            return;
        }
        let upload_len = self.used.saturating_mul(self.stride);
        queue.write_buffer(&self.buffer, 0, &self.staging[..upload_len]);
    }

    /// Return the GPU buffer backing this batch.
    pub(super) fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    fn ensure_capacity(&mut self, device: &wgpu::Device, required: usize) -> bool {
        if required <= self.capacity {
            return false;
        }
        let mut next = self.capacity.max(1);
        while next < required {
            next = next.saturating_mul(2);
        }
        self.buffer = create_uniform_buffer(device, self.label, self.stride.saturating_mul(next));
        self.capacity = next;
        true
    }
}

fn create_uniform_buffer(device: &wgpu::Device, label: &'static str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(1) as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn aligned_uniform_stride(min_alignment: u32) -> usize {
    let uniform_size = std::mem::size_of::<GraphOpUniforms>();
    let alignment = usize::try_from(min_alignment.max(1)).unwrap_or(uniform_size.max(1));
    align_up(uniform_size, alignment)
}

fn align_up(size: usize, alignment: usize) -> usize {
    let remainder = size % alignment;
    if remainder == 0 {
        return size;
    }
    size.saturating_add(alignment.saturating_sub(remainder))
}

fn write_uniform_slot(
    staging: &mut Vec<u8>,
    stride: usize,
    slot: usize,
    uniform: &GraphOpUniforms,
) {
    let offset = slot.saturating_mul(stride);
    let required_len = offset.saturating_add(stride);
    if staging.len() < required_len {
        staging.resize(required_len, 0);
    }
    staging[offset..required_len].fill(0);
    let uniform_bytes = bytemuck::bytes_of(uniform);
    let end = offset.saturating_add(uniform_bytes.len());
    staging[offset..end].copy_from_slice(uniform_bytes);
}

#[cfg(test)]
mod tests {
    use super::{align_up, aligned_uniform_stride, write_uniform_slot};
    use crate::gpu_render::graph_ops::GraphOpUniforms;

    #[test]
    fn aligned_stride_rounds_uniform_size_up_to_device_alignment() {
        let stride = aligned_uniform_stride(256);
        assert_eq!(stride, 256);
        assert!(stride >= std::mem::size_of::<GraphOpUniforms>());
    }

    #[test]
    fn align_up_keeps_aligned_values_unchanged() {
        assert_eq!(align_up(128, 64), 128);
        assert_eq!(align_up(129, 64), 192);
    }

    #[test]
    fn write_uniform_slot_zeroes_padding_between_dynamic_offsets() {
        let mut staging = vec![255; 512];
        let mut uniform = GraphOpUniforms::sized(64, 32);
        uniform.mode = 7;
        write_uniform_slot(&mut staging, 256, 1, &uniform);

        assert!(staging[..256].iter().all(|byte| *byte == 255));
        assert!(staging[256 + std::mem::size_of::<GraphOpUniforms>()..512]
            .iter()
            .all(|byte| *byte == 0));
    }
}

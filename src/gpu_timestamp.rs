//! Optional GPU timestamp query helpers for pass-level attribution.
//!
//! The helper is inert unless `TIMESTAMP_QUERY` is enabled on the device.
//! When enabled, callers can attach per-pass begin/end writes and resolve all
//! recorded queries at the end of command encoding.

use std::sync::Arc;

/// Optional timestamp query state for one command-encoding owner.
#[derive(Debug)]
pub(crate) struct OptionalGpuTimestampQueries {
    query_set: Option<Arc<wgpu::QuerySet>>,
    resolve_buffer: Option<wgpu::Buffer>,
    query_capacity: u32,
    next_query: u32,
}

impl OptionalGpuTimestampQueries {
    /// Create an optional timestamp query set with fixed query capacity.
    pub(crate) fn new(device: &wgpu::Device, label: &str, query_capacity: u32) -> Self {
        if query_capacity < 2 || !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return Self {
                query_set: None,
                resolve_buffer: None,
                query_capacity: 0,
                next_query: 0,
            };
        }
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some(&format!("{label}-query-set")),
            ty: wgpu::QueryType::Timestamp,
            count: query_capacity,
        });
        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label}-query-resolve")),
            size: (query_capacity as u64).saturating_mul(std::mem::size_of::<u64>() as u64),
            usage: wgpu::BufferUsages::QUERY_RESOLVE,
            mapped_at_creation: false,
        });
        Self {
            query_set: Some(Arc::new(query_set)),
            resolve_buffer: Some(resolve_buffer),
            query_capacity,
            next_query: 0,
        }
    }

    /// Reset query allocation for a fresh command-encoding sequence.
    pub(crate) fn begin_frame(&mut self) {
        self.next_query = 0;
    }

    /// Allocate timestamp-write parts for one render pass, if capacity permits.
    pub(crate) fn next_render_pass_parts(&mut self) -> Option<(Arc<wgpu::QuerySet>, u32, u32)> {
        let (begin, end) = self.reserve_pair()?;
        let query_set = self.query_set.as_ref()?.clone();
        Some((query_set, begin, end))
    }

    /// Allocate timestamp-write parts for one compute pass, if capacity permits.
    pub(crate) fn next_compute_pass_parts(&mut self) -> Option<(Arc<wgpu::QuerySet>, u32, u32)> {
        let (begin, end) = self.reserve_pair()?;
        let query_set = self.query_set.as_ref()?.clone();
        Some((query_set, begin, end))
    }

    /// Resolve all recorded timestamps into the internal resolve buffer.
    pub(crate) fn resolve_and_reset(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let query_count = self.next_query;
        if query_count == 0 {
            return;
        }
        let (Some(query_set), Some(resolve_buffer)) =
            (self.query_set.as_ref(), self.resolve_buffer.as_ref())
        else {
            self.next_query = 0;
            return;
        };
        encoder.resolve_query_set(query_set.as_ref(), 0..query_count, resolve_buffer, 0);
        self.next_query = 0;
    }

    fn reserve_pair(&mut self) -> Option<(u32, u32)> {
        if self.query_set.is_none() || self.next_query.saturating_add(1) >= self.query_capacity {
            return None;
        }
        let begin = self.next_query;
        let end = begin + 1;
        self.next_query = end + 1;
        Some((begin, end))
    }
}

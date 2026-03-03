//! Label-fit cache controls for menu and node text rendering.

/// Maximum number of fitted-label cache buckets by width/zoom tuple.
pub(super) const FITTED_LABEL_CACHE_MAX_BUCKETS: usize = 32;
/// Maximum number of cached text fits per bucket.
pub(super) const FITTED_LABEL_CACHE_MAX_ENTRIES_PER_BUCKET: usize = 512;

/// Label-fit cache partition key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct FittedLabelCacheBucketKey {
    pub(super) max_width: i32,
    pub(super) zoom_bits: u32,
}

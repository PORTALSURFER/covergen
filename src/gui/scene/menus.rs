//! Label-fit cache controls for menu and node text rendering.

use std::collections::{HashMap, VecDeque};

/// Maximum number of fitted-label cache buckets by width/zoom tuple.
pub(super) const FITTED_LABEL_CACHE_MAX_BUCKETS: usize = 32;
/// Maximum number of cached text fits per bucket.
pub(super) const FITTED_LABEL_CACHE_MAX_ENTRIES_PER_BUCKET: usize = 512;
/// Maximum number of prefix-width cache buckets by zoom tuple.
pub(super) const TEXT_WIDTH_PREFIX_CACHE_MAX_BUCKETS: usize = 16;
/// Maximum number of cached prefix-width entries per zoom bucket.
pub(super) const TEXT_WIDTH_PREFIX_CACHE_MAX_ENTRIES_PER_BUCKET: usize = 512;

/// Label-fit cache partition key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct FittedLabelCacheBucketKey {
    pub(super) max_width: i32,
    pub(super) zoom_bits: u32,
}

/// Cached fitted-label entries for one width/zoom bucket.
#[derive(Clone, Debug, Default)]
pub(super) struct FittedLabelCacheBucket {
    entries: HashMap<String, String>,
    entry_order: VecDeque<String>,
}

impl FittedLabelCacheBucket {
    /// Return one cached fitted label, if present.
    pub(super) fn get(&self, text: &str) -> Option<&str> {
        self.entries.get(text).map(String::as_str)
    }

    /// Return true when this bucket already contains `text`.
    #[cfg(test)]
    pub(super) fn contains_key(&self, text: &str) -> bool {
        self.entries.contains_key(text)
    }

    /// Return the number of cached fitted labels in this bucket.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Insert or update one fitted label while evicting at most one oldest entry.
    pub(super) fn insert_bounded(&mut self, text: &str, fitted: &str, max_entries: usize) {
        if !self.entries.contains_key(text) {
            if self.entries.len() >= max_entries {
                while let Some(oldest) = self.entry_order.pop_front() {
                    if self.entries.remove(oldest.as_str()).is_some() {
                        break;
                    }
                }
            }
            self.entry_order.push_back(text.to_owned());
        }
        self.entries.insert(text.to_owned(), fitted.to_owned());
    }
}

/// Prefix-width cache partition key for one text zoom bucket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct TextWidthPrefixCacheBucketKey {
    pub(super) zoom_bits: u32,
}

/// Cached cumulative widths for one UTF-8 label at one zoom level.
#[derive(Clone, Debug, Default)]
pub(super) struct TextWidthPrefixEntry {
    pub(super) byte_ends: Vec<usize>,
    pub(super) prefix_widths: Vec<i32>,
}

impl TextWidthPrefixEntry {
    /// Return the full measured width of the source text.
    pub(super) fn full_width(&self) -> i32 {
        self.prefix_widths.last().copied().unwrap_or_default()
    }

    /// Return the UTF-8 byte end for the widest prefix that fits `max_width`.
    pub(super) fn fitted_byte_end(&self, max_width: i32) -> usize {
        let prefix_len = self
            .prefix_widths
            .partition_point(|width| *width <= max_width);
        if prefix_len == 0 {
            0
        } else {
            self.byte_ends
                .get(prefix_len.saturating_sub(1))
                .copied()
                .unwrap_or(0)
        }
    }
}

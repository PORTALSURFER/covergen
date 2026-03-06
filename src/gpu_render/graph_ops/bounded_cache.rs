//! Small frame-recency cache used by graph-op bind-group reuse.

use std::collections::HashMap;
use std::hash::Hash;

#[derive(Clone, Debug)]
struct CacheEntry<V> {
    value: V,
    last_used_stamp: u64,
}

/// Bounded cache that evicts the stalest entry once it reaches capacity.
#[derive(Debug)]
pub(super) struct BoundedFrameCache<K, V> {
    entries: HashMap<K, CacheEntry<V>>,
    max_len: usize,
}

impl<K, V> BoundedFrameCache<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    /// Create one bounded cache with the requested maximum size.
    pub(super) fn with_capacity(initial_capacity: usize, max_len: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(initial_capacity),
            max_len: max_len.max(1),
        }
    }

    /// Return one cached value and refresh its recency stamp.
    pub(super) fn get(&mut self, key: &K, stamp: u64) -> Option<V> {
        let entry = self.entries.get_mut(key)?;
        entry.last_used_stamp = stamp;
        Some(entry.value.clone())
    }

    /// Insert one value and evict the stalest entry when the cache is full.
    pub(super) fn insert(&mut self, key: K, value: V, stamp: u64) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.value = value;
            entry.last_used_stamp = stamp;
            return;
        }
        if self.entries.len() >= self.max_len {
            self.evict_stalest();
        }
        self.entries.insert(
            key,
            CacheEntry {
                value,
                last_used_stamp: stamp,
            },
        );
    }

    /// Clear all cached entries.
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    fn evict_stalest(&mut self) {
        let stale_key = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used_stamp)
            .map(|(key, _)| key.clone());
        if let Some(stale_key) = stale_key {
            self.entries.remove(&stale_key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BoundedFrameCache;

    #[test]
    fn bounded_cache_evicts_oldest_entry_when_full() {
        let mut cache = BoundedFrameCache::with_capacity(2, 2);
        cache.insert(1u32, "a", 1);
        cache.insert(2u32, "b", 2);
        cache.insert(3u32, "c", 3);

        assert!(cache.get(&1, 4).is_none());
        assert_eq!(cache.get(&2, 4), Some("b"));
        assert_eq!(cache.get(&3, 4), Some("c"));
    }

    #[test]
    fn cache_hit_refreshes_recency_before_eviction() {
        let mut cache = BoundedFrameCache::with_capacity(2, 2);
        cache.insert(1u32, "a", 1);
        cache.insert(2u32, "b", 2);
        assert_eq!(cache.get(&1, 3), Some("a"));

        cache.insert(3u32, "c", 4);

        assert_eq!(cache.get(&1, 5), Some("a"));
        assert!(cache.get(&2, 5).is_none());
        assert_eq!(cache.get(&3, 5), Some("c"));
    }
}

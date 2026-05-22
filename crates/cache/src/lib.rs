use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct CachedPrefix {
    pub blocks: Vec<u8>,
    pub token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub hit_rate: f64,
    pub block_count: usize,
    pub total_size_mb: f64,
}

pub trait KvCache: Send + Sync {
    fn lookup(&self, prefix_hash: u64) -> Option<CachedPrefix>;
    fn store(&self, prefix_hash: u64, blocks: CachedPrefix);
    fn stats(&self) -> CacheStats;
}

pub struct NoopCache;

impl KvCache for NoopCache {
    fn lookup(&self, _hash: u64) -> Option<CachedPrefix> {
        None
    }
    fn store(&self, _hash: u64, _blocks: CachedPrefix) {}

    fn stats(&self) -> CacheStats {
        CacheStats {
            hit_rate: 0.0,
            block_count: 0,
            total_size_mb: 0.0,
        }
    }
}

/// Q4 KV Cache using safetensors block pool.
/// Currently returns cache miss; full implementation in Phase 3B+.
pub struct SafetensorsCache {
    _cache_dir: std::path::PathBuf,
    _max_blocks: usize,
    _total_budget_mb: usize,
}

impl SafetensorsCache {
    pub fn new(cache_dir: std::path::PathBuf, max_blocks: usize, total_budget_mb: usize) -> Self {
        Self {
            _cache_dir: cache_dir,
            _max_blocks: max_blocks,
            _total_budget_mb: total_budget_mb,
        }
    }
}

impl KvCache for SafetensorsCache {
    fn lookup(&self, _prefix_hash: u64) -> Option<CachedPrefix> {
        None
    }
    fn store(&self, _prefix_hash: u64, _blocks: CachedPrefix) {}
    fn stats(&self) -> CacheStats {
        CacheStats {
            hit_rate: 0.0,
            block_count: 0,
            total_size_mb: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_cache_always_miss() {
        let cache = NoopCache;
        assert!(cache.lookup(0).is_none());
    }

    #[test]
    fn test_noop_cache_store_is_noop() {
        let cache = NoopCache;
        let blocks = CachedPrefix {
            blocks: vec![1, 2, 3],
            token_count: 10,
        };
        cache.store(42, blocks);
        assert!(cache.lookup(42).is_none());
    }

    #[test]
    fn test_noop_cache_stats() {
        let cache = NoopCache;
        let stats = cache.stats();
        assert_eq!(stats.hit_rate, 0.0);
        assert_eq!(stats.block_count, 0);
        assert_eq!(stats.total_size_mb, 0.0);
    }

    #[test]
    fn test_cache_stats_serialization() {
        let stats = CacheStats {
            hit_rate: 0.85,
            block_count: 12,
            total_size_mb: 600.0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("hit_rate"));
        let decoded: CacheStats = serde_json::from_str(&json).unwrap();
        assert!((decoded.hit_rate - 0.85).abs() < 0.01);
    }
}

#[derive(Debug, Clone)]
pub struct CachedPrefix {
    pub blocks: Vec<u8>,
    pub token_count: usize,
}

pub trait KvCache: Send + Sync {
    fn lookup(&self, prefix_hash: u64) -> Option<CachedPrefix>;
    fn store(&self, prefix_hash: u64, blocks: CachedPrefix);
}

pub struct NoopCache;

impl KvCache for NoopCache {
    fn lookup(&self, _hash: u64) -> Option<CachedPrefix> {
        None
    }
    fn store(&self, _hash: u64, _blocks: CachedPrefix) {}
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
}

//! Prefix hash tracker for local model KV cache observability.
//!
//! llama.cpp (LM Studio / Ollama) reuses KV blocks when consecutive requests
//! share a common prefix (system prompt + tool definitions). This module tracks
//! prefix hashes and counts hits/misses — purely observational, no KV block I/O.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use loom_types::Message;

#[derive(Debug, Clone, Default)]
pub struct PrefixCacheStats {
    pub hits: u64,
    pub misses: u64,
}

/// Tracks prefix hashes across requests to detect cache reuse.
pub struct PrefixCache {
    last_hash: Mutex<Option<u64>>,
    stats: Mutex<PrefixCacheStats>,
    prefix_message_count: usize,
}

impl PrefixCache {
    pub fn new(prefix_message_count: usize) -> Self {
        Self {
            last_hash: Mutex::new(None),
            stats: Mutex::new(PrefixCacheStats::default()),
            prefix_message_count,
        }
    }

    /// Check whether the prefix of `all_messages` matches the last request.
    /// Returns (is_hit, hash).
    pub fn check(&self, all_messages: &[Message]) -> (bool, u64) {
        let prefix_end = self.prefix_message_count.min(all_messages.len());
        let hash = hash_prefix(&all_messages[..prefix_end]);

        let mut last = self.last_hash.lock().unwrap();
        let is_hit = last.map_or(false, |h| h == hash);

        let mut stats = self.stats.lock().unwrap();
        if is_hit { stats.hits += 1; } else { stats.misses += 1; }

        *last = Some(hash);
        (is_hit, hash)
    }

    /// Reset per-turn stats only (prefix hash carries over).
    pub fn reset_turn(&self) {
        *self.stats.lock().unwrap() = PrefixCacheStats::default();
    }

    pub fn stats(&self) -> PrefixCacheStats {
        self.stats.lock().unwrap().clone()
    }
}

fn hash_prefix(messages: &[Message]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for msg in messages {
        msg.role.as_str().hash(&mut hasher);
        for part in &msg.content {
            std::mem::discriminant(part).hash(&mut hasher);
            match part {
                loom_types::ContentPart::Text { text } => text.hash(&mut hasher),
                loom_types::ContentPart::ToolCall { id, name, arguments } => {
                    id.hash(&mut hasher); name.hash(&mut hasher);
                    arguments.to_string().hash(&mut hasher);
                }
                loom_types::ContentPart::ToolResult { tool_call_id, result, .. } => {
                    tool_call_id.hash(&mut hasher); result.hash(&mut hasher);
                }
                loom_types::ContentPart::Thinking { text } => text.hash(&mut hasher),
                loom_types::ContentPart::Image { .. } => {}
            }
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_types::{ContentPart, Message, Role};

    fn make_msg(role: Role, text: &str) -> Message {
        Message {
            role,
            content: vec![ContentPart::Text { text: text.into() }],
            timestamp: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_prefix_hit_same_messages() {
        let cache = PrefixCache::new(2);
        let msgs = vec![make_msg(Role::System, "sys"), make_msg(Role::User, "hello")];
        let (hit, _) = cache.check(&msgs);
        assert!(!hit, "first request should be miss");
        let (hit, _) = cache.check(&msgs);
        assert!(hit, "second request with same prefix should be hit");
    }

    #[test]
    fn test_prefix_miss_different_messages() {
        let cache = PrefixCache::new(2);
        cache.check(&vec![make_msg(Role::System, "sys A"), make_msg(Role::User, "hello")]);
        let (hit, _) = cache.check(&vec![make_msg(Role::System, "sys B"), make_msg(Role::User, "hello")]);
        assert!(!hit);
    }

    #[test]
    fn test_reset_turn_keeps_prefix() {
        let cache = PrefixCache::new(2);
        let msgs = vec![make_msg(Role::System, "sys"), make_msg(Role::User, "hello")];
        cache.check(&msgs); // miss
        cache.reset_turn();
        let (hit, _) = cache.check(&msgs); // still hit — prefix persisted
        assert!(hit);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 0);
    }

    #[test]
    fn test_stats_accumulate() {
        let cache = PrefixCache::new(2);
        let msgs = vec![make_msg(Role::System, "sys"), make_msg(Role::User, "hello")];
        cache.check(&msgs);
        cache.check(&msgs);
        cache.check(&msgs);
        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
    }
}

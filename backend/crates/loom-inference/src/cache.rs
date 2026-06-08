//! Prefix hash tracker for KV cache observability.
//!
//! Tracks prefix fingerprints across requests to detect KV-cache reuse.
//! V2: SHA256-based prefix digest with per-component drift detection
//! and cache-break classification (breaking vs additive).
//!
//! llama.cpp (LM Studio / Ollama) reuses KV blocks when consecutive requests
//! share a common prefix (system prompt + tool definitions). This module tracks
//! prefix hashes and counts hits/misses -- purely observational, no KV block I/O.

use loom_context::PrefixDigest;
use loom_types::Message;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

#[derive(Debug, Clone, Default)]
pub struct PrefixCacheStats {
    pub hits: u64,
    pub misses: u64,
    /// Count of additive misses (prefix same, suffix grew -- prefix reusable).
    pub additive_misses: u64,
    /// Count of breaking misses (prefix changed -- full KV-cache flush).
    pub breaking_misses: u64,
}

/// Classification of a cache-check result against a previous digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    /// First request -- no previous digest to compare against.
    ColdStart,
    /// Stable prefix is identical to the last request.
    Hit,
    /// Stable prefix changed in a way that invalidates the entire KV cache.
    BreakingMiss,
    /// Stable prefix is identical but the dynamic suffix grew.
    AdditiveMiss,
}

/// Tracks prefix fingerprints across requests to detect cache reuse.
///
/// V2 upgrade: stores `PrefixDigest` instead of raw `u64` hashes.
/// Supports per-component drift detection and cache-break classification.
pub struct PrefixCache {
    /// SHA256 digest of the last known stable prefix (agent loop path).
    last_digest: Mutex<Option<PrefixDigest>>,
    /// Legacy u64 hash for backward-compatible check_legacy() path.
    legacy_hash: Mutex<Option<u64>>,
    stats: Mutex<PrefixCacheStats>,
    prefix_message_count: usize,
    last_hit: Mutex<Option<bool>>,
    last_prefix_tokens: Mutex<usize>,
    last_drift_additive: Mutex<bool>,
}

impl PrefixCache {
    pub fn new(prefix_message_count: usize) -> Self {
        Self {
            last_digest: Mutex::new(None),
            legacy_hash: Mutex::new(None),
            stats: Mutex::new(PrefixCacheStats::default()),
            prefix_message_count,
            last_hit: Mutex::new(None),
            last_prefix_tokens: Mutex::new(0),
            last_drift_additive: Mutex::new(false),
        }
    }

    // -- Primary digest-based API (agent loop path) --

    /// Check whether the incoming `digest` matches the last known prefix.
    ///
    /// Returns `(CacheStatus, Option<PrefixDigest>, Vec<&'static str>)`:
    /// - CacheStatus: the cache-hit classification
    /// - Option<PrefixDigest>: the incoming digest (for logging)
    /// - Vec<&'static str>: drift reasons (empty on Hit/ColdStart, component names on miss)
    pub fn check_digest(
        &self,
        digest: &Option<PrefixDigest>,
    ) -> (CacheStatus, Option<PrefixDigest>, Vec<&'static str>) {
        let Some(incoming) = digest else {
            *self.last_hit.lock().unwrap() = Some(false);
            *self.last_prefix_tokens.lock().unwrap() = 0;
            *self.last_drift_additive.lock().unwrap() = false;
            return (CacheStatus::ColdStart, None, vec![]);
        };

        let mut last = self.last_digest.lock().unwrap();
        let prev = last.clone();

        // Compute drift reasons BEFORE updating last_digest.
        let reasons: Vec<&'static str> = match &prev {
            None => vec![],
            Some(prev_digest) => {
                let mut r = Vec::new();
                if prev_digest.system_hash != incoming.system_hash {
                    r.push("system_prompt");
                }
                if prev_digest.persona_hash != incoming.persona_hash {
                    r.push("persona");
                }
                if prev_digest.summary_hash != incoming.summary_hash {
                    r.push("summary");
                }
                if prev_digest.kg_hash != incoming.kg_hash {
                    r.push("kg_context");
                }
                if prev_digest.catalog_hash != incoming.catalog_hash {
                    r.push("tool_catalog");
                }
                if r.is_empty() && prev_digest.combined_hash != incoming.combined_hash {
                    r.push("unknown");
                }
                r
            }
        };

        let result = match &prev {
            None => {
                *last = Some(incoming.clone());
                *self.last_hit.lock().unwrap() = Some(false);
                *self.last_prefix_tokens.lock().unwrap() = 0;
                *self.last_drift_additive.lock().unwrap() = false;
                self.stats.lock().unwrap().misses += 1;
                CacheStatus::ColdStart
            }
            Some(prev_digest) => {
                if prev_digest.combined_hash == incoming.combined_hash {
                    *last = Some(incoming.clone());
                    *self.last_hit.lock().unwrap() = Some(true);
                    *self.last_prefix_tokens.lock().unwrap() = incoming.prefix_token_count;
                    *self.last_drift_additive.lock().unwrap() = false;
                    self.stats.lock().unwrap().hits += 1;
                    CacheStatus::Hit
                } else {
                    let breaking = !reasons.is_empty() && reasons != vec!["unknown"];
                    *last = Some(incoming.clone());
                    *self.last_hit.lock().unwrap() = Some(false);
                    *self.last_prefix_tokens.lock().unwrap() = 0;
                    let mut stats = self.stats.lock().unwrap();
                    stats.misses += 1;
                    if breaking {
                        *self.last_drift_additive.lock().unwrap() = false;
                        stats.breaking_misses += 1;
                        CacheStatus::BreakingMiss
                    } else {
                        *self.last_drift_additive.lock().unwrap() = true;
                        stats.additive_misses += 1;
                        CacheStatus::AdditiveMiss
                    }
                }
            }
        };

        (result, Some(incoming.clone()), reasons)
    }

    // -- Snapshot / restore --

    pub fn snapshot_digest(&self) -> Option<PrefixDigest> {
        self.last_digest.lock().unwrap().clone()
    }

    pub fn restore_digest(&self, saved: Option<PrefixDigest>) {
        *self.last_digest.lock().unwrap() = saved;
    }

    /// Force the next check to be a cache miss (clears both digest and legacy hash).
    pub fn reset_prefix(&self) {
        *self.last_digest.lock().unwrap() = None;
        *self.legacy_hash.lock().unwrap() = None;
    }

    // -- Backward-compatible check --

    /// Primary check method. Uses stored PrefixDigest when available;
    /// falls through to check_legacy() for orchestrator internal calls.
    pub fn check(&self, all_messages: &[Message]) -> (bool, u64) {
        let last = self.last_digest.lock().unwrap();
        if last.is_some() {
            let prefix_end = self.prefix_message_count.min(all_messages.len());
            let prefix = &all_messages[..prefix_end];
            let hash = hash_prefix(prefix);
            let is_hit = self.last_hit.lock().unwrap().unwrap_or(false);
            return (is_hit, hash);
        }
        drop(last);
        self.check_legacy(all_messages)
    }

    /// Legacy hash check using DefaultHasher (SipHash-1-3).
    pub fn check_legacy(&self, all_messages: &[Message]) -> (bool, u64) {
        let prefix_end = self.prefix_message_count.min(all_messages.len());
        let prefix = &all_messages[..prefix_end];
        let hash = hash_prefix(prefix);

        let mut last = self.legacy_hash.lock().unwrap();
        let is_hit = last.is_some_and(|h| h == hash);

        let mut stats = self.stats.lock().unwrap();
        if is_hit {
            stats.hits += 1;
        } else {
            stats.misses += 1;
        }

        *last = Some(hash);
        *self.last_hit.lock().unwrap() = Some(is_hit);
        if !is_hit {
            *self.last_prefix_tokens.lock().unwrap() = 0;
        }
        (is_hit, hash)
    }

    // -- Accessors --

    pub fn last_cached_tokens(&self) -> usize {
        if self.last_hit.lock().unwrap().unwrap_or(false) {
            *self.last_prefix_tokens.lock().unwrap()
        } else {
            0
        }
    }

    pub fn reset_turn(&self) {
        *self.stats.lock().unwrap() = PrefixCacheStats::default();
        *self.last_hit.lock().unwrap() = None;
    }

    pub fn snapshot_hash(&self) -> Option<u64> {
        *self.legacy_hash.lock().unwrap()
    }

    pub fn restore_hash(&self, saved: Option<u64>) {
        *self.legacy_hash.lock().unwrap() = saved;
    }

    pub fn last_check_was_hit(&self) -> Option<bool> {
        *self.last_hit.lock().unwrap()
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
                loom_types::ContentPart::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    id.hash(&mut hasher);
                    name.hash(&mut hasher);
                    arguments.to_string().hash(&mut hasher);
                }
                loom_types::ContentPart::ToolResult {
                    tool_call_id,
                    result,
                    ..
                } => {
                    tool_call_id.hash(&mut hasher);
                    result.hash(&mut hasher);
                }
                loom_types::ContentPart::Thinking { text } => text.hash(&mut hasher),
                loom_types::ContentPart::Image { .. } => {}
                loom_types::ContentPart::ImageRef { .. } => {}
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
            content: vec![ContentPart::Text {
                text: text.into(),
            }],
            timestamp: chrono::Utc::now(),
            usage: None,
        }
    }

    // -- Legacy tests (preserved) --

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
        cache.check(&vec![
            make_msg(Role::System, "sys A"),
            make_msg(Role::User, "hello"),
        ]);
        let (hit, _) = cache.check(&vec![
            make_msg(Role::System, "sys B"),
            make_msg(Role::User, "hello"),
        ]);
        assert!(!hit);
    }

    #[test]
    fn test_reset_turn_keeps_prefix() {
        let cache = PrefixCache::new(2);
        let msgs = vec![make_msg(Role::System, "sys"), make_msg(Role::User, "hello")];
        cache.check(&msgs); // miss
        cache.reset_turn();
        let (hit, _) = cache.check(&msgs); // still hit -- prefix persisted
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

    // -- Digest-based tests (new) --

    fn make_digest(combined: &str) -> PrefixDigest {
        PrefixDigest {
            combined_hash: combined.into(),
            system_hash: String::new(),
            persona_hash: String::new(),
            summary_hash: String::new(),
            kg_hash: String::new(),
            catalog_hash: String::new(),
            prefix_token_count: 100,
        }
    }

    #[test]
    fn test_digest_cold_start() {
        let cache = PrefixCache::new(2);
        let (status, _, _) = cache.check_digest(&Some(make_digest("abc123")));
        assert_eq!(status, CacheStatus::ColdStart);
    }

    #[test]
    fn test_digest_hit() {
        let cache = PrefixCache::new(2);
        let digest = Some(make_digest("abc123"));
        cache.check_digest(&digest); // cold start
        let (status, _, _) = cache.check_digest(&digest); // same digest
        assert_eq!(status, CacheStatus::Hit);
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn test_digest_breaking_miss() {
        let cache = PrefixCache::new(2);
        cache.check_digest(&Some(PrefixDigest {
            combined_hash: "abc".into(),
            system_hash: "old_sys".into(),
            persona_hash: String::new(),
            summary_hash: String::new(),
            kg_hash: String::new(),
            catalog_hash: String::new(),
            prefix_token_count: 100,
        }));
        let (status, _, _) = cache.check_digest(&Some(PrefixDigest {
            combined_hash: "def".into(),
            system_hash: "new_sys".into(),
            persona_hash: String::new(),
            summary_hash: String::new(),
            kg_hash: String::new(),
            catalog_hash: String::new(),
            prefix_token_count: 100,
        }));
        assert_eq!(status, CacheStatus::BreakingMiss);
    }

    #[test]
    fn test_digest_drift_reasons() {
        let cache = PrefixCache::new(2);
        cache.check_digest(&Some(PrefixDigest {
            combined_hash: "abc".into(),
            system_hash: "sys_a".into(),
            persona_hash: "persona_a".into(),
            summary_hash: String::new(),
            kg_hash: String::new(),
            catalog_hash: String::new(),
            prefix_token_count: 100,
        }));
        let incoming = Some(PrefixDigest {
            combined_hash: "def".into(),
            system_hash: "sys_b".into(),
            persona_hash: "persona_a".into(),
            summary_hash: String::new(),
            kg_hash: String::new(),
            catalog_hash: String::new(),
            prefix_token_count: 100,
        });
        let (_status, _digest, reasons) = cache.check_digest(&incoming);
        assert!(reasons.contains(&"system_prompt"));
        assert!(!reasons.contains(&"persona"));
    }

    #[test]
    fn test_reset_prefix_forces_miss() {
        let cache = PrefixCache::new(2);
        let digest = Some(make_digest("abc123"));
        cache.check_digest(&digest); // cold start
        cache.check_digest(&digest); // hit
        assert_eq!(cache.stats().hits, 1);

        cache.reset_prefix();
        let (status, _, _) = cache.check_digest(&digest);
        assert_eq!(status, CacheStatus::ColdStart);
    }

    #[test]
    fn test_digest_snapshot_restore() {
        let cache = PrefixCache::new(2);
        let digest = Some(make_digest("abc123"));
        cache.check_digest(&digest); // cold start

        let snapshot = cache.snapshot_digest();
        assert!(snapshot.is_some());

        cache.restore_digest(None);
        cache.restore_digest(snapshot);

        let (status, _, _) = cache.check_digest(&digest);
        assert_eq!(status, CacheStatus::Hit);
    }
}

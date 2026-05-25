use std::collections::{HashMap, VecDeque};
use std::time::Instant;

/// Per-user rate limiter using a sliding window
pub struct RateLimiter {
    max_per_minute: u32,
    windows: HashMap<String, VecDeque<Instant>>,
}

impl RateLimiter {
    pub fn new(max_per_minute: u32) -> Self {
        Self {
            max_per_minute,
            windows: HashMap::new(),
        }
    }

    /// Returns `true` if the user is within rate limits.
    /// Records the attempt regardless of result.
    pub fn check(&mut self, user_key: &str) -> bool {
        let now = Instant::now();
        let window = self.windows.entry(user_key.to_string()).or_default();

        // Remove entries older than 60 seconds
        while let Some(front) = window.front() {
            if now.duration_since(*front).as_secs() >= 60 {
                window.pop_front();
            } else {
                break;
            }
        }

        if window.len() >= self.max_per_minute as usize {
            return false;
        }

        window.push_back(now);
        true
    }
}

/// Deduplicates messages by external_message_id using a bounded LRU cache
pub struct MessageDedup {
    seen: VecDeque<String>,
    capacity: usize,
}

impl MessageDedup {
    pub fn new(capacity: usize) -> Self {
        Self {
            seen: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Returns `true` if this message_id has been seen before
    pub fn is_duplicate(&mut self, message_id: &str) -> bool {
        if self.seen.iter().any(|id| id == message_id) {
            return true;
        }
        if self.seen.len() >= self.capacity {
            self.seen.pop_front();
        }
        self.seen.push_back(message_id.to_string());
        false
    }
}

/// Detects bot-to-bot reply loops by tracking consecutive bot messages
pub struct LoopDetector {
    consecutive_bot_messages: HashMap<String, u32>,
    threshold: u32,
}

impl LoopDetector {
    pub fn new(threshold: u32) -> Self {
        Self {
            consecutive_bot_messages: HashMap::new(),
            threshold,
        }
    }

    /// Record a message in a chat. Returns `true` if a loop is detected.
    pub fn check(&mut self, chat_id: &str, is_bot: bool) -> bool {
        let count = self
            .consecutive_bot_messages
            .entry(chat_id.to_string())
            .or_insert(0);

        if is_bot {
            *count += 1;
            *count >= self.threshold
        } else {
            *count = 0;
            false
        }
    }

    /// Reset counter for a chat (e.g., when a human sends a message)
    pub fn reset(&mut self, chat_id: &str) {
        self.consecutive_bot_messages.insert(chat_id.to_string(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_under_limit() {
        let mut limiter = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.check("user_1"));
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let mut limiter = RateLimiter::new(3);
        assert!(limiter.check("user_1"));
        assert!(limiter.check("user_1"));
        assert!(limiter.check("user_1"));
        assert!(!limiter.check("user_1"));
    }

    #[test]
    fn test_rate_limiter_separate_users() {
        let mut limiter = RateLimiter::new(1);
        assert!(limiter.check("user_1"));
        assert!(limiter.check("user_2"));
        assert!(!limiter.check("user_1"));
        assert!(!limiter.check("user_2"));
    }

    #[test]
    fn test_dedup_first_seen_not_duplicate() {
        let mut dedup = MessageDedup::new(100);
        assert!(!dedup.is_duplicate("msg_1"));
    }

    #[test]
    fn test_dedup_second_seen_is_duplicate() {
        let mut dedup = MessageDedup::new(100);
        assert!(!dedup.is_duplicate("msg_1"));
        assert!(dedup.is_duplicate("msg_1"));
    }

    #[test]
    fn test_dedup_eviction() {
        let mut dedup = MessageDedup::new(2);
        assert!(!dedup.is_duplicate("msg_1"));
        assert!(!dedup.is_duplicate("msg_2"));
        assert!(!dedup.is_duplicate("msg_3")); // evicts msg_1
        assert!(!dedup.is_duplicate("msg_1")); // msg_1 was evicted, so not duplicate
    }

    #[test]
    fn test_loop_detector_no_loop_with_human() {
        let mut detector = LoopDetector::new(3);
        assert!(!detector.check("chat_1", true));
        assert!(!detector.check("chat_1", false)); // resets
        assert!(!detector.check("chat_1", true));
    }

    #[test]
    fn test_loop_detector_detects_loop() {
        let mut detector = LoopDetector::new(3);
        assert!(!detector.check("chat_1", true));
        assert!(!detector.check("chat_1", true));
        assert!(detector.check("chat_1", true)); // 3 consecutive → loop!
    }

    #[test]
    fn test_loop_detector_separate_chats() {
        let mut detector = LoopDetector::new(2);
        assert!(!detector.check("chat_1", true));
        assert!(!detector.check("chat_2", true));
        assert!(detector.check("chat_1", true));
        assert!(detector.check("chat_2", true));
    }

    #[test]
    fn test_loop_detector_reset() {
        let mut detector = LoopDetector::new(2);
        assert!(!detector.check("chat_1", true));
        detector.reset("chat_1");
        assert!(!detector.check("chat_1", true));
    }
}

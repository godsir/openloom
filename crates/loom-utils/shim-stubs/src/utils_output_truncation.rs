// Stub for codex-utils-output-truncation types.

use serde::{Deserialize, Serialize};

/// Truncation policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TruncationPolicy {
    Tokens(usize),
    Chars(usize),
    Bytes(usize),
}

/// Stub: returns a rough token count (4 bytes per token).
pub fn approx_token_count(text: &str) -> usize {
    text.len() / 4
}

/// Stub: truncates text to fit the policy by returning the input unchanged.
pub fn formatted_truncate_text(text: &str, policy: TruncationPolicy) -> String {
    let limit = match policy {
        TruncationPolicy::Tokens(n) => n * 4,
        TruncationPolicy::Chars(n) | TruncationPolicy::Bytes(n) => n,
    };
    if text.len() <= limit {
        text.to_string()
    } else {
        let mut truncated = text[..limit].to_string();
        truncated.push_str("...");
        truncated
    }
}

/// Stub: converts bytes to approximate tokens (divide by 4).
pub fn approx_tokens_from_byte_count_i64(byte_count: i64) -> i64 {
    byte_count / 4
}

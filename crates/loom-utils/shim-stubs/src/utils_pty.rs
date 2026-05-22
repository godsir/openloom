// Stub for codex-utils-pty types.

/// Stub: always returns false on non-Windows (ConPTY is Windows-only).
pub fn conpty_supported() -> bool {
    false
}

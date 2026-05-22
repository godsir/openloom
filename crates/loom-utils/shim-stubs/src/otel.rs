// Stub for codex-otel types. Re-exports from loom-otel-stub and adds metric constants.

pub use loom_otel_stub::*;

/// Stub metric constants (no-op string identifiers).
pub const CURATED_PLUGINS_STARTUP_SYNC_FINAL_METRIC: &str = "curated_plugins_startup_sync_final";
pub const CURATED_PLUGINS_STARTUP_SYNC_METRIC: &str = "curated_plugins_startup_sync";
pub const THREAD_SKILLS_DESCRIPTION_TRUNCATED_CHARS_METRIC: &str =
    "thread_skills_description_truncated_chars";
pub const THREAD_SKILLS_ENABLED_TOTAL_METRIC: &str = "thread_skills_enabled_total";
pub const THREAD_SKILLS_KEPT_TOTAL_METRIC: &str = "thread_skills_kept_total";
pub const THREAD_SKILLS_TRUNCATED_METRIC: &str = "thread_skills_truncated";

/// Stub global metrics accessor. Always returns None.
pub fn global() -> Option<MetricsStub> {
    None
}

/// Minimal stub for global metrics handle.
pub struct MetricsStub;

impl MetricsStub {
    pub fn counter(&self, _name: &str, _inc: u64, _attrs: &[(&str, &str)]) {}
    pub fn gauge(&self, _name: &str, _val: f64, _attrs: &[(&str, &str)]) {}
    pub fn histogram(&self, _name: &str, _val: f64, _attrs: &[(&str, &str)]) {}
    pub fn record_duration(&self, _name: &str, _duration: std::time::Duration, _attrs: &[(&str, &str)]) {}
}

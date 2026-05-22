// Stub for codex-analytics types.

/// Stub analytics client. All methods are no-ops.
#[derive(Clone, Debug)]
pub struct AnalyticsEventsClient;

impl AnalyticsEventsClient {
    pub fn new() -> Self {
        Self
    }

    pub fn track_plugin_installed(&self, _metadata: crate::plugin::PluginTelemetryMetadata) {
        // no-op stub
    }

    pub fn track_plugin_uninstalled(&self, _metadata: crate::plugin::PluginTelemetryMetadata) {
        // no-op stub
    }
}

impl Default for AnalyticsEventsClient {
    fn default() -> Self {
        Self
    }
}

/// Stub invocation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationType {
    Skill,
}

/// Stub skill invocation event.
#[derive(Debug, Clone)]
pub struct SkillInvocation {
    pub skill_name: String,
    pub invocation_type: InvocationType,
}

/// Stub track events context.
#[derive(Debug, Clone)]
pub struct TrackEventsContext {
    pub source: String,
}

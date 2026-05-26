//! Persona provider trait for user profiling from cognition data.
//!
//! Consumers: loom-core (agent loop context assembly), loom-memory (CognitionsPersonaProvider)

/// Provides a human-readable summary of the user's persona traits.
///
/// The persona is assembled from accumulated cognition data and injected
/// into the agent's system prompt at the start of each conversation turn.
///
/// Consumers: loom-core (agent_loop context assembly), loom-memory (provider impl)
#[async_trait::async_trait]
pub trait PersonaProvider: Send + Sync {
    /// Generate the current persona summary string.
    async fn summarize(&self) -> anyhow::Result<String>;

    /// Invalidate any cached persona data (called when new cognitions arrive).
    fn invalidate(&self);
}

/// No-op provider used when persona tracking is disabled.
///
/// Consumers: loom-core (default engine setup)
pub struct NoopPersonaProvider;

#[async_trait::async_trait]
impl PersonaProvider for NoopPersonaProvider {
    async fn summarize(&self) -> anyhow::Result<String> {
        Ok(String::new())
    }

    fn invalidate(&self) {}
}

//! AgentPool — manages concurrent agent instances with lifecycle control.
//!
//! One AgentPool per process. Each agent runs as an independent tokio task.
//! The pool handles spawn, pause, resume, kill, observe, and sub-agent delegation.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use loom_types::{AgentConfig, AgentId, SessionId};
use tokio::sync::RwLock;

use crate::agent::{Agent, AgentStatus};
use crate::event_bus::{AgentEvent, EventBus};

/// Summary of an agent's current state for UI display.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentSummary {
    pub id: AgentId,
    pub name: String,
    pub status: AgentStatus,
    pub session_id: String,
    pub iteration: usize,
    pub parent_id: Option<AgentId>,
    pub child_count: usize,
}

/// Manages all agent instances within a process.
pub struct AgentPool {
    agents: Arc<RwLock<HashMap<AgentId, Agent>>>,
    event_bus: EventBus,
    max_depth: usize,
}

impl AgentPool {
    pub fn new(
        max_depth: usize,
        _default_max_iterations: usize,
        _default_timeout_secs: u64,
    ) -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            event_bus: EventBus::new(65536),
            max_depth,
        }
    }

    /// Get a reference to the event bus for external subscribers.
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// Register a new agent in the pool.
    pub async fn register(&self, mut agent: Agent) -> Result<AgentId> {
        // Check depth limit
        if let Some(ref parent_id) = agent.parent_id {
            let depth = self.compute_depth(parent_id).await;
            if depth >= self.max_depth {
                return Err(anyhow!(
                    "max agent nesting depth ({}) exceeded",
                    self.max_depth
                ));
            }
        }

        let id = agent.id.clone();
        agent.status = AgentStatus::Idle;
        self.agents.write().await.insert(id.clone(), agent);

        self.event_bus.publish(AgentEvent::StateChanged {
            agent_id: id.clone(),
            old_status: AgentStatus::Idle,
            new_status: AgentStatus::Idle,
        });

        tracing::info!(agent_id = %id, "agent registered");
        Ok(id)
    }

    /// Spawn a new agent from config, optionally as a child of another agent.
    pub async fn spawn(
        &self,
        config: AgentConfig,
        parent_id: Option<AgentId>,
        session_id: SessionId,
    ) -> Result<AgentId> {
        let agent = Agent::new(config, session_id, parent_id.clone());

        if let Some(ref pid) = parent_id {
            if let Some(parent) = self.agents.write().await.get_mut(pid) {
                parent.add_child(agent.id.clone());
            }
            self.event_bus.publish(AgentEvent::SubagentSpawned {
                parent_id: pid.clone(),
                child_id: agent.id.clone(),
                child_name: agent.config.name.clone(),
            });
        }

        self.register(agent).await
    }

    /// Get a read-only snapshot of an agent's summary.
    pub async fn summary(&self, agent_id: &AgentId) -> Result<AgentSummary> {
        let agents = self.agents.read().await;
        let agent = agents
            .get(agent_id)
            .ok_or_else(|| anyhow!("agent not found: {}", agent_id))?;
        Ok(AgentSummary {
            id: agent.id.clone(),
            name: agent.config.name.clone(),
            status: agent.status.clone(),
            session_id: agent.session_id.to_string(),
            iteration: agent.iteration,
            parent_id: agent.parent_id.clone(),
            child_count: agent.children.len(),
        })
    }

    /// List all agents in the pool.
    pub async fn list(&self) -> Vec<AgentSummary> {
        self.agents
            .read()
            .await
            .values()
            .map(|a| AgentSummary {
                id: a.id.clone(),
                name: a.config.name.clone(),
                status: a.status.clone(),
                session_id: a.session_id.to_string(),
                iteration: a.iteration,
                parent_id: a.parent_id.clone(),
                child_count: a.children.len(),
            })
            .collect()
    }

    /// Transition an agent to a new status.
    pub async fn transition(
        &self,
        agent_id: &AgentId,
        new_status: AgentStatus,
        msg: Option<String>,
    ) -> Result<()> {
        let mut agents = self.agents.write().await;
        let agent = agents
            .get_mut(agent_id)
            .ok_or_else(|| anyhow!("agent not found: {}", agent_id))?;
        if let Some(event) = agent.transition(new_status, msg) {
            self.event_bus.publish(event);
        }
        Ok(())
    }

    /// Get the cancel token for an agent so the agent loop can check for interruption.
    pub async fn cancel_token(
        &self,
        agent_id: &AgentId,
    ) -> Result<tokio_util::sync::CancellationToken> {
        let agents = self.agents.read().await;
        let agent = agents
            .get(agent_id)
            .ok_or_else(|| anyhow!("agent not found: {}", agent_id))?;
        Ok(agent.cancel_token.clone())
    }

    /// Kill an agent immediately.
    pub async fn kill(&self, agent_id: &AgentId) -> Result<()> {
        let mut agents = self.agents.write().await;
        let agent = agents
            .get_mut(agent_id)
            .ok_or_else(|| anyhow!("agent not found: {}", agent_id))?;
        agent.cancel_token.cancel();
        if let Some(event) = agent.transition(AgentStatus::Killed, Some("killed by user".into())) {
            drop(agents);
            self.event_bus.publish(event);
        }
        tracing::info!(agent_id = %agent_id, "agent killed");
        Ok(())
    }

    /// Remove a completed/killed agent from the pool.
    pub async fn remove(&self, agent_id: &AgentId) -> Result<()> {
        let mut agents = self.agents.write().await;
        if let Some(agent) = agents.remove(agent_id) {
            // Remove from parent's children list
            if let Some(ref pid) = agent.parent_id
                && let Some(parent) = agents.get_mut(pid)
            {
                parent.remove_child(agent_id);
            }
        }
        Ok(())
    }

    /// Compute the nesting depth of an agent by walking up parent chain.
    async fn compute_depth(&self, parent_id: &AgentId) -> usize {
        let agents = self.agents.read().await;
        let mut depth = 0;
        let mut current = Some(parent_id.clone());
        while let Some(id) = current {
            if let Some(agent) = agents.get(&id) {
                depth += 1;
                current = agent.parent_id.clone();
            } else {
                break;
            }
        }
        depth
    }

    /// Notify that a subagent completed and transition parent back to Thinking.
    pub async fn subagent_completed(
        &self,
        parent_id: &AgentId,
        child_id: &AgentId,
        result: String,
    ) -> Result<()> {
        self.event_bus.publish(AgentEvent::SubagentCompleted {
            parent_id: parent_id.clone(),
            child_id: child_id.clone(),
            result: result.clone(),
        });

        let mut agents = self.agents.write().await;
        let parent = agents
            .get_mut(parent_id)
            .ok_or_else(|| anyhow!("parent agent not found: {}", parent_id))?;
        parent.remove_child(child_id);
        parent.push_history(loom_types::Message::assistant(format!(
            "[subagent result] {result}"
        )));
        parent.transition(AgentStatus::Thinking, None);
        Ok(())
    }
}

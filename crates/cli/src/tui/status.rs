use openloom_models::AgentState;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StatusLine {
    pub model: String,
    pub agent_state: AgentState,
    pub context_pct: f64,
    pub turn_tokens: usize,
    pub git_branch: String,
    pub cwd: String,
    pub context_max: usize,
}

impl StatusLine {
    pub fn state_icon(&self) -> &str {
        match self.agent_state {
            AgentState::Idle => "○",
            AgentState::Thinking => "●",
            AgentState::Acting => "◆",
        }
    }
}

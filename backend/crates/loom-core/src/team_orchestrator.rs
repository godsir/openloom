// SPDX-License-Identifier: Apache-2.0
//! 团队编排核心 — 构造团长 system prompt，驱动团队执行流程。

use loom_types::config::team::{MemberSource, TeamConfig, TeamStrategy};

/// 为团队团长构造 system prompt
pub fn build_captain_system_prompt(
    team: &TeamConfig,
    member_configs: &[(String, String, Option<String>)], // (name, persona, model)
) -> String {
    let strategy_instruction = match team.strategy {
        TeamStrategy::Synthesize => SYNTHESIZE_INSTRUCTION.to_string(),
        TeamStrategy::Debate => DEBATE_INSTRUCTION.to_string(),
    };

    let member_list = member_configs
        .iter()
        .map(|(name, persona, model)| {
            let model_note = model
                .as_ref()
                .map(|m| format!(" (model: {})", m))
                .unwrap_or_default();
            format!("- **{}**: {}{}", name, persona, model_note)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let captain_override = team
        .captain
        .system_prompt_override
        .as_ref()
        .map(|s| format!("\n## Captain Instructions\n{}\n", s))
        .unwrap_or_default();

    format!(
        r#"You are the captain of expert team "{}".

## Team Members
{}

## Your Role
{}
{captain_override}
## Important Rules
- Use the spawn_agent tool to start ALL team members in parallel.
- Members cannot communicate with each other — only through you.
- After all members complete, synthesize their findings into one comprehensive answer.
- Highlight agreements, disagreements, and your own judgment where appropriate.
"#,
        team.name, member_list, strategy_instruction
    )
}

const SYNTHESIZE_INSTRUCTION: &str = r#"Synthesize Mode:
1. Spawn all members with their specific personas.
2. Wait for all to complete.
3. Read each member's response carefully.
4. Produce a unified conclusion that integrates all perspectives.
5. Explicitly note any conflicting viewpoints and your resolution."#;

const DEBATE_INSTRUCTION: &str = r#"Debate Mode (Two Rounds):

Round 1:
1. Spawn all members with their specific personas.
2. Collect all Round 1 responses.

Round 2:
3. For each member, spawn them again with this additional context:
   "Here are the other experts' opinions from Round 1. Critically examine your own conclusion:
   identify points you agree with, points you disagree with, and either revise or defend your position."

Round 2 Prompt for each member:
---
Other experts' Round 1 responses:
{other_responses}

Please critically examine your own conclusion from Round 1. For each point raised by others:
- If you agree, acknowledge it and integrate it.
- If you disagree, explain why and defend your position.
- If you discover a flaw in your own reasoning, correct it.

Provide your revised (or reaffirmed) analysis.
---

4. After all Round 2 responses are collected, synthesize everything into a final conclusion.
5. Highlight: points of consensus, remaining disagreements, and your own recommendation."#;

/// 从团队配置解析成员 agent config
pub fn resolve_member_configs(
    team: &TeamConfig,
    existing_agents: &[loom_types::AgentConfig],
) -> Vec<(String, loom_types::AgentConfig)> {
    let mut results = Vec::new();

    for member in &team.members {
        match &member.source {
            MemberSource::AgentRef(config_name) => {
                if let Some(agent) = existing_agents.iter().find(|a| a.name == *config_name) {
                    results.push((config_name.clone(), agent.clone()));
                } else {
                    tracing::warn!(
                        team_id = %team.id,
                        member = %member.name,
                        ref_name = %config_name,
                        "team member references non-existent agent config — skipping"
                    );
                }
            }
            MemberSource::Inline {
                persona,
                model,
                temperature,
            } => {
                let config_name = format!("__team_{}_{}", team.id, member.name);
                let config = loom_types::AgentConfig {
                    name: config_name.clone(),
                    persona: persona.clone(),
                    model: model.clone(),
                    temperature: *temperature,
                    ..Default::default()
                };
                results.push((config_name, config));
            }
        }
    }

    results
}

/// 构造 captain 的 AgentConfig
pub fn build_captain_config(
    team: &TeamConfig,
    system_prompt: String,
    default_model: Option<String>,
) -> loom_types::AgentConfig {
    loom_types::AgentConfig {
        name: format!("__team_captain_{}", team.id),
        persona: format!("Team captain for '{}'", team.name),
        system_prompt_override: Some(system_prompt),
        model: team.captain.model.clone().or(default_model),
        cc_dispatch: true,
        auto_continue: false,
        ..Default::default()
    }
}

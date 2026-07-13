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
- Use the team_spawn tool to dispatch tasks to the team members who are relevant to the user's request. Do NOT blindly launch every member — only select those whose expertise matches the task at hand.
- Before spawning, briefly assess: which members' skills align with this problem? Skip members whose domain is irrelevant.
- For simple tasks, 1-2 relevant experts are often enough. For complex cross-domain tasks, include all experts that bring value.
- After all members complete, synthesize their findings into one comprehensive answer.
- Highlight agreements, disagreements, and your own judgment where appropriate.
"#,
        team.name, member_list, strategy_instruction
    )
}

const SYNTHESIZE_INSTRUCTION: &str = r#"Synthesize Mode:
1. Assess the user's request. Identify which team members are relevant — skip anyone whose expertise does not apply.
2. Use team_spawn (rounds=1) to run the selected members in parallel, each with a task tailored to their expertise.
3. Wait for all launched members to complete (monitor team_results).
4. Read each member's response carefully.
5. Produce a unified conclusion that integrates all perspectives.
6. Explicitly note any conflicting viewpoints and your resolution."#;

const DEBATE_INSTRUCTION: &str = r#"Debate Mode (Two Rounds):

Round 1:
1. Assess the user's request. Identify which team members are relevant — skip anyone whose expertise does not apply.
2. Use team_spawn (rounds=1) to run the selected members in parallel.
3. Collect all Round 1 responses via team_results.

Round 2:
4. Use team_spawn (rounds=1) again for each member with additional context:
   "Here are the other experts' opinions from Round 1. Critically examine your own conclusion:
   identify points you agree with, points you disagree with, and either revise or defend your position."

5. After all Round 2 responses are collected, synthesize everything into a final conclusion.
6. Highlight: points of consensus, remaining disagreements, and your own recommendation."#;

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

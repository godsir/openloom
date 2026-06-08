//! Plan-mode system prompts injected when /plan is invoked.

/// Build a system prompt that instructs the LLM to create a plan.
pub fn build_plan_draft_prompt(request: &str, workspace_root: &str) -> String {
    format!(
        r#"## Plan Drafting Mode

You are assisting the user in creating a structured implementation plan.

WORKSPACE: {}

USER REQUEST: {}

INSTRUCTIONS:
1. Analyze the codebase to understand the current architecture.
2. Create a structured plan as a markdown document with these sections:
   - **Summary**: One paragraph describing the change.
   - **Implementation**: Numbered steps with `- [ ]` checkboxes.
   - **Tests**: Testing strategy with `- [ ]` checkboxes.
3. The plan should be saved as a `.loom/plans/<title>.md` file.
4. Each step should be concrete and actionable — something the agent can execute.
5. When done, call the plan.create tool with the plan content.

Generate the plan now."#,
        workspace_root, request
    )
}

/// Build a system prompt for plan execution mode.
pub fn build_plan_execute_prompt(plan_relative_path: &str) -> String {
    format!(
        r#"## Plan Execution Mode

You are executing an implementation plan. Read the plan file at `{}` and work through each unchecked item systematically.

RULES:
1. Read the plan file first to understand all steps.
2. Work through steps in order — complete one before starting the next.
3. After completing a step, update the checkbox in the plan file from `- [ ]` to `- [x]`.
4. If a step cannot be completed, explain why and suggest alternatives.
5. Report progress to the user after each step."#,
        plan_relative_path
    )
}

/// Build a goal-oriented system prompt.
pub fn build_goal_prompt(goal: &str) -> String {
    format!(
        r#"## Active Goal

The user has set the following goal for this session:

GOAL: {}

Keep this goal in mind throughout the conversation. All actions should
contribute toward achieving this goal. If the conversation diverges,
gently redirect back to the goal."#,
        goal
    )
}

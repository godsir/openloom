use loom_memory::todo_store::TodoItem;

/// Build a formatted todo list string for injection into the system prompt.
/// Always returns content — even when the list is empty, so the LLM knows
/// `todo_write` is available and can populate the list from a plan or user request.
pub fn build_todo_continuation_instruction(todos: &[TodoItem]) -> Option<String> {
    let total = todos.len();
    let completed = todos.iter().filter(|t| t.status == "completed").count();

    let mut lines = vec![
        "## Todo".to_string(),
        "**IMPORTANT**: After completing each task, immediately call `todo_write` to mark it as completed. The todo panel is the user's primary progress tracker — keep it in sync with your actual progress in real time.".to_string(),
        String::new(),
        "Use `todo_write` to replace the list. Rules:".to_string(),
        "- At most ONE item in_progress at a time.".to_string(),
        "- Mark items complete ONLY after verifying the work is truly done.".to_string(),
        "- After finishing one in_progress item, move the next pending item to in_progress BEFORE starting it.".to_string(),
    ];

    if total > 0 {
        lines.push(String::new());
        lines.push("Current status:".to_string());
        for (i, todo) in todos.iter().enumerate() {
            let icon = match todo.status.as_str() {
                "in_progress" => "[in_progress]",
                "completed" => "[completed]",
                _ => "[pending]",
            };
            lines.push(format!("{}. {} {}", i + 1, icon, todo.content));
        }
    } else {
        lines.push(String::new());
        lines.push("(empty — populate with `todo_write` when you create a plan or the user asks for tasks)".to_string());
    }

    lines.push(String::new());
    if total > 0 {
        lines.push(format!("Progress: {} / {}", completed, total));
    }

    Some(lines.join("\n"))
}

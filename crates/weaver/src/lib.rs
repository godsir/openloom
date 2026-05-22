use openloom_cache::KvCache;
use openloom_models::{ChatMessage, ContentPart, Message, Role};
use std::sync::Arc;

pub struct AssembledPrompt {
    pub prompt: String,
    pub messages: Vec<Message>,
    pub static_prefix_len: usize,
}

pub struct ContextWeaver {
    cache: Arc<dyn KvCache>,
}

impl ContextWeaver {
    pub fn new(cache: Arc<dyn KvCache>) -> Self {
        Self { cache }
    }

    pub fn cache(&self) -> &Arc<dyn KvCache> {
        &self.cache
    }

    /// Assemble structured messages array for native tool calling.
    pub fn assemble_messages(
        &self,
        system_instruction: &str,
        user_message: &str,
        persona_summary: &str,
        skill_context: Option<&str>,
        working_memory: &[ChatMessage],
        max_context_chars: usize,
    ) -> Vec<Message> {
        let mut messages: Vec<Message> = Vec::new();

        let mut system_text = system_instruction.to_string();
        if !persona_summary.is_empty() {
            system_text.push_str("\n\n## Persona\n");
            system_text.push_str(persona_summary);
        }
        if let Some(ctx) = skill_context
            && !ctx.is_empty()
        {
            system_text.push_str("\n\n## Available Tools\n");
            system_text.push_str(ctx);
        }
        let static_prefix_len = system_text.len();
        messages.push(Message {
            role: Role::System,
            content: vec![ContentPart::Text { text: system_text }],
            timestamp: chrono::Utc::now(),
        });

        // Conversation history
        let history_msgs = if max_context_chars > 0 && !working_memory.is_empty() {
            let overhead = static_prefix_len + user_message.len() + 200;
            let budget = max_context_chars.saturating_sub(overhead);
            compact_memory_messages(working_memory, budget)
        } else {
            working_memory.iter().map(Message::from_legacy).collect()
        };
        messages.extend(history_msgs);

        // Current user message (skip if empty — query is already in history)
        if !user_message.is_empty() {
            messages.push(Message::user(user_message));
        }

        messages
    }

    pub fn assemble(
        &self,
        system_instruction: &str,
        user_message: &str,
        persona_summary: &str,
        skill_context: Option<&str>,
        working_memory: &[ChatMessage],
    ) -> AssembledPrompt {
        self.assemble_with_limit(
            system_instruction,
            user_message,
            persona_summary,
            skill_context,
            working_memory,
            0,
        )
    }

    pub fn assemble_with_limit(
        &self,
        system_instruction: &str,
        user_message: &str,
        persona_summary: &str,
        skill_context: Option<&str>,
        working_memory: &[ChatMessage],
        max_context_chars: usize,
    ) -> AssembledPrompt {
        let messages = self.assemble_messages(
            system_instruction,
            user_message,
            persona_summary,
            skill_context,
            working_memory,
            max_context_chars,
        );
        // Flatten to string for backward compat
        let prompt: String = messages
            .iter()
            .map(|m| format!("[{}] {}", m.role.as_str(), m.text_content()))
            .collect::<Vec<_>>()
            .join("\n");
        AssembledPrompt {
            prompt,
            messages,
            static_prefix_len: 0,
        }
    }
}

fn compact_memory_messages(messages: &[ChatMessage], budget_chars: usize) -> Vec<Message> {
    let total_chars: usize = messages
        .iter()
        .map(|m| m.role.len() + m.content.len() + 3)
        .sum();
    if total_chars <= budget_chars {
        return messages.iter().map(Message::from_legacy).collect();
    }

    let mut kept: Vec<ChatMessage> = Vec::new();
    let mut used = 0usize;
    let note_size = 80;
    let mut prev_was_tool = false;

    for msg in messages.iter().rev() {
        let msg_size = msg.role.len() + msg.content.len() + 3;
        // If we kept a tool message, we MUST also keep its preceding assistant
        let must_keep = prev_was_tool && msg.role == "assistant";
        if !must_keep && used + msg_size + note_size > budget_chars && !kept.is_empty() {
            break;
        }
        used += msg_size;
        kept.push(msg.clone());
        prev_was_tool = msg.role == "tool";
    }
    kept.reverse();

    let mut result: Vec<Message> = Vec::new();
    if kept.len() < messages.len() {
        result.push(Message {
            role: Role::System,
            content: vec![ContentPart::Text {
                text: "[Earlier messages were compacted to fit context window]".into(),
            }],
            timestamp: chrono::Utc::now(),
        });
    }
    result.extend(kept.iter().map(Message::from_legacy));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use openloom_cache::NoopCache;

    const SYSTEM_INSTRUCTION: &str = "You are openLoom, a private AI assistant.";

    fn make_weaver() -> ContextWeaver {
        ContextWeaver::new(Arc::new(NoopCache))
    }

    fn make_msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: content.into(),
            timestamp: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_assemble_returns_messages_array() {
        let weaver = make_weaver();
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "hello", "", None, &[]);
        assert!(result.messages.len() >= 2);
        assert_eq!(result.messages[0].role, Role::System);
    }

    #[test]
    fn test_assemble_user_message_present() {
        let weaver = make_weaver();
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "hello world", "", None, &[]);
        assert!(result.prompt.contains("hello world"));
    }

    #[test]
    fn test_assemble_with_persona() {
        let weaver = make_weaver();
        let persona = "短线交易";
        let result = weaver.assemble_messages(SYSTEM_INSTRUCTION, "hello", persona, None, &[], 0);
        assert!(result[0].text_content().contains("短线交易"));
    }

    #[test]
    fn test_assemble_with_working_memory_preserves_roles() {
        let weaver = make_weaver();
        let memory = vec![make_msg("user", "hi"), make_msg("assistant", "hello")];
        let result =
            weaver.assemble_messages(SYSTEM_INSTRUCTION, "how are you", "", None, &memory, 0);
        assert!(result.len() >= 4);
        assert_eq!(result[1].role, Role::User);
        assert_eq!(result[2].role, Role::Assistant);
    }

    #[test]
    fn test_assemble_messages_output() {
        let weaver = make_weaver();
        let result = weaver.assemble_messages(SYSTEM_INSTRUCTION, "test", "", None, &[], 0);
        assert_eq!(result[0].role, Role::System);
        assert_eq!(result.last().unwrap().role, Role::User);
        assert!(result[0].text_content().contains("openLoom"));
    }
}

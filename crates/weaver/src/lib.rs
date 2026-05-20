use openloom_cache::KvCache;
use openloom_models::ChatMessage;
use std::sync::Arc;

pub struct AssembledPrompt {
    pub prompt: String,
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

    pub fn assemble(
        &self,
        system_instruction: &str,
        user_message: &str,
        persona_summary: &str,
        skill_context: Option<&str>,
        working_memory: &[ChatMessage],
    ) -> AssembledPrompt {
        let prefix_hash = 0u64;
        let _ = self.cache.lookup(prefix_hash);

        let static_prefix = if persona_summary.is_empty() {
            system_instruction.to_string()
        } else {
            format!("{}\n{}", system_instruction, persona_summary)
        };
        let static_prefix_len = static_prefix.len();

        let skill_section = match skill_context {
            Some(ctx) if !ctx.is_empty() => format!("\n[Skill Context]\n{}\n", ctx),
            _ => String::new(),
        };

        let memory_section = if working_memory.is_empty() {
            String::new()
        } else {
            let memory_text: String = working_memory
                .iter()
                .map(|m| format!("{}: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n");
            format!("\n[Conversation History]\n{}\n", memory_text)
        };

        let dynamic_section = format!(
            "{}{}\n[User Message]\n{}",
            skill_section, memory_section, user_message
        );
        let prompt = format!("{}\n{}", static_prefix, dynamic_section);

        AssembledPrompt {
            prompt,
            static_prefix_len,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openloom_cache::NoopCache;

    const SYSTEM_INSTRUCTION: &str = "You are openLoom, a private AI assistant running locally.";

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
    fn test_assemble_basic_message() {
        let weaver = make_weaver();
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "hello", "", None, &[]);
        assert!(result.prompt.contains("hello"));
        assert!(result.prompt.contains(SYSTEM_INSTRUCTION));
        assert!(result.static_prefix_len > 0);
    }

    #[test]
    fn test_assemble_with_persona() {
        let weaver = make_weaver();
        let persona = "用户画像：短线交易；追高倾向。";
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "hello", persona, None, &[]);
        assert!(result.prompt.contains(persona));
        let static_part = &result.prompt[..result.static_prefix_len];
        assert!(static_part.contains(persona));
    }

    #[test]
    fn test_assemble_with_skill_context() {
        let weaver = make_weaver();
        let result = weaver.assemble(
            SYSTEM_INSTRUCTION,
            "open file",
            "",
            Some("file-manager: list/read/write files"),
            &[],
        );
        assert!(result.prompt.contains("[Skill Context]"));
        assert!(result.prompt.contains("file-manager"));
    }

    #[test]
    fn test_assemble_with_working_memory() {
        let weaver = make_weaver();
        let memory = vec![make_msg("user", "hi"), make_msg("assistant", "hello")];
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "how are you", "", None, &memory);
        assert!(result.prompt.contains("[Conversation History]"));
        assert!(result.prompt.contains("user: hi"));
        assert!(result.prompt.contains("assistant: hello"));
    }

    #[test]
    fn test_static_prefix_before_dynamic() {
        let weaver = make_weaver();
        let result = weaver.assemble(
            SYSTEM_INSTRUCTION,
            "test message",
            "用户画像：测试。",
            Some("skill context"),
            &[],
        );
        let static_part = &result.prompt[..result.static_prefix_len];
        let dynamic_part = &result.prompt[result.static_prefix_len..];
        assert!(static_part.contains(SYSTEM_INSTRUCTION));
        assert!(static_part.contains("用户画像"));
        assert!(!static_part.contains("test message"));
        assert!(dynamic_part.contains("test message"));
        assert!(dynamic_part.contains("[Skill Context]"));
    }
}

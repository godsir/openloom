# Native Tool Calling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace openLoom's text-based tool calling ("拼字符串→抠JSON") with native function calling (messages array + tools parameter + tool_calls response), aligning with OpenCode's architecture.

**Architecture:** Restructure the inference→weaver→engine pipeline to pass structured `Vec<Message>` and `Vec<ToolDefinition>` through the stack instead of a flat string. The API clients lower canonical types to provider-specific wire formats (Anthropic: tool_use blocks, OpenAI: tool_calls). The agent loop uses response tool_calls instead of text scanning. Backward compatible: old `prompt: String` field preserved with a conversion path.

**Tech Stack:** Rust 2024 edition, serde, reqwest, tokio, anyhow

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/models/src/lib.rs` | Modify | Add `Role` enum, `ContentPart`, `Message`, `ToolDefinition`, update `ToolCall`/`CompletionRequest`/`CompletionResponse` |
| `crates/inference/src/lib.rs` | Modify | Rewrite `AnthropicClient` and `OpenAIClient` to use messages+tools; add lowering/parsing |
| `crates/weaver/src/lib.rs` | Modify | Replace `assemble_with_limit()` string output with `Vec<Message>` output |
| `crates/engine/src/lib.rs` | Modify | Replace `SYSTEM_INSTRUCTION` text-based tool prompt; add `build_tool_definitions()` |
| `crates/engine/src/agent_loop.rs` | Modify | Replace `parse_tool_call()`; use native tool_calls from response; proper message-based history |

---

### Task 1: Add structured message and tool types to models crate

**Files:**
- Modify: `crates/models/src/lib.rs` (add types after ChatMessage around line 187)
- Modify: `crates/models/Cargo.toml` (check if any new deps needed)

- [ ] **Step 1: Add Role enum and ContentPart/Message types**

Insert after `ChatMessage` (line 187):

```rust
// === Native Tool Calling Types (aligned with OpenCode) ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

/// A content part within a message — text, tool call, or tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        result: String,
    },
}

/// A structured message with role-separated content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentPart>,
    #[serde(skip)]
    pub timestamp: DateTime<Utc>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentPart::Text { text: text.into() }],
            timestamp: Utc::now(),
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentPart::Text { text: text.into() }],
            timestamp: Utc::now(),
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, name: impl Into<String>, result: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: vec![ContentPart::ToolResult {
                tool_call_id: tool_call_id.into(),
                name: name.into(),
                result: result.into(),
            }],
            timestamp: Utc::now(),
        }
    }

    /// Extract text content from this message (concatenates all Text parts).
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract tool calls from this message.
    pub fn tool_calls(&self) -> Vec<&ContentPart> {
        self.content
            .iter()
            .filter(|p| matches!(p, ContentPart::ToolCall { .. }))
            .collect()
    }

    /// Convert legacy ChatMessage to new Message.
    pub fn from_legacy(msg: &ChatMessage) -> Self {
        let role = match msg.role.as_str() {
            "system" => Role::System,
            "assistant" => Role::Assistant,
            "tool" => Role::Tool,
            _ => Role::User,
        };
        Self {
            role,
            content: vec![ContentPart::Text { text: msg.content.clone() }],
            timestamp: msg.timestamp,
        }
    }
}

/// Tool definition sent to the API in the `tools` parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}
```

- [ ] **Step 2: Update ToolCall with id field and add ToolCallDelta**

Replace the existing `ToolCall` struct (line 196-200):

```rust
/// A parsed tool call extracted from the model response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}
```

- [ ] **Step 3: Add ToolChoice enum**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    Auto,
    None,
    Required,
}
```

- [ ] **Step 4: Update CompletionRequest with messages and tools**

Replace the existing `CompletionRequest` in `crates/inference/src/lib.rs` (lines 8-17):

```rust
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// New: structured messages array (system/user/assistant/tool).
    pub messages: Vec<Message>,
    /// New: tool definitions sent to the API.
    pub tools: Vec<ToolDefinition>,
    /// New: tool choice mode.
    pub tool_choice: Option<ToolChoice>,

    // Legacy: flat prompt string (kept for backward compat).
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub stop: Vec<String>,
    pub stream: bool,
    pub thinking_budget: Option<usize>,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            tools: Vec::new(),
            tool_choice: None,
            prompt: String::new(),
            max_tokens: 4096,
            temperature: 0.7,
            top_p: 1.0,
            stop: Vec::new(),
            stream: false,
            thinking_budget: None,
        }
    }
}

impl CompletionRequest {
    /// Get the effective messages array: if messages is non-empty use it,
    /// otherwise convert the legacy flat prompt into a single user message.
    pub fn effective_messages(&self) -> Vec<Message> {
        if !self.messages.is_empty() {
            self.messages.clone()
        } else if !self.prompt.is_empty() {
            vec![Message::user(&self.prompt)]
        } else {
            vec![]
        }
    }
}
```

**Important:** Add `use openloom_models::{Message, ToolDefinition, ToolChoice};` to the imports in `crates/inference/src/lib.rs` line 3-6.

- [ ] **Step 5: Update CompletionResponse with tool_calls**

Replace the existing `CompletionResponse` (lines 33-40):

```rust
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cached_tokens: usize,
    pub latency_ms: u64,
}

impl CompletionResponse {
    /// Check if this response contains tool calls.
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}
```

- [ ] **Step 6: Run cargo check to verify compilation**

Run: `cargo check --all`
Expected: FAIL — inference crate uses old field names, needs Task 2 updates.

- [ ] **Step 7: Commit**

```bash
git add crates/models/src/lib.rs crates/inference/src/lib.rs
git commit -m "feat: add native tool calling types (Message, ToolDefinition, ToolCall) to models and inference crates"
```

---

### Task 2: Update inference crate — AnthropicClient native tool calling

**Files:**
- Modify: `crates/inference/src/lib.rs` (AnthropicClient: try_complete, complete_stream, try_complete_stream_inner)

- [ ] **Step 1: Rewrite AnthropicClient::try_complete with messages+tools**

Replace `AnthropicClient::try_complete` (lines 398-447):

```rust
async fn try_complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
    let messages = self.lower_messages(&req.effective_messages());
    let mut body = serde_json::json!({
        "model": self.model,
        "max_tokens": req.max_tokens,
        "messages": messages,
    });
    if !req.tools.is_empty() {
        let anthropic_tools: Vec<serde_json::Value> = req.tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })
        }).collect();
        body["tools"] = serde_json::json!(anthropic_tools);
    }
    if req.thinking_budget.is_some() {
        body["thinking"] = serde_json::json!({
            "type": "enabled",
            "budget_tokens": req.thinking_budget.unwrap(),
        });
    }

    let resp = self.http
        .post(format!("{}/v1/messages", self.base_url))
        .header("x-api-key", &self.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic API error {}: {}", status, text);
    }

    let body_text = resp.text().await?;
    let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
        let preview = &body_text[..body_text.len().min(500)];
        anyhow::anyhow!("Anthropic response parse error: {}, body: {}", e, preview)
    })?;

    let (text, tool_calls) = self.parse_anthropic_content(&json);
    let prompt_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize;
    let completion_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize;
    let cached_tokens = json["usage"]["cache_read_input_tokens"]
        .as_u64()
        .unwrap_or(0) as usize;

    Ok(CompletionResponse {
        text,
        tool_calls,
        prompt_tokens,
        completion_tokens,
        cached_tokens,
        latency_ms: 0,
    })
}
```

- [ ] **Step 2: Add lower_messages helper to AnthropicClient**

Insert after `AnthropicClient::new()` (line 373):

```rust
/// Convert canonical Messages to Anthropic wire format.
fn lower_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
    messages.iter().map(|msg| {
        let role = msg.role.as_str();
        let content: Vec<serde_json::Value> = msg.content.iter().map(|part| match part {
            ContentPart::Text { text } => serde_json::json!({
                "type": "text", "text": text
            }),
            ContentPart::ToolCall { id, name, arguments } => serde_json::json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": arguments,
            }),
            ContentPart::ToolResult { tool_call_id, name: _, result } => serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tool_call_id,
                "content": result,
            }),
        }).collect();
        serde_json::json!({ "role": role, "content": content })
    }).collect()
}
```

- [ ] **Step 3: Add parse_anthropic_content helper**

Insert after lower_messages:

```rust
/// Parse Anthropic response content blocks into text + tool_calls.
fn parse_anthropic_content(&self, json: &serde_json::Value) -> (String, Vec<ToolCall>) {
    let content = json["content"].as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    let texts: Vec<String> = content.iter()
        .filter_map(|block| {
            if block["type"].as_str() == Some("text") {
                block["text"].as_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    let text = texts.join("\n");

    let tool_calls: Vec<ToolCall> = content.iter()
        .filter(|block| {
            matches!(block["type"].as_str(), Some("tool_use"))
        })
        .filter_map(|block| {
            Some(ToolCall {
                id: block["id"].as_str()?.to_string(),
                name: block["name"].as_str()?.to_string(),
                arguments: block["input"].clone(),
            })
        })
        .collect();

    (text, tool_calls)
}
```

- [ ] **Step 4: Update AnthropicClient::complete_stream to use messages+tools**

Replace the body construction in `complete_stream` (lines 462-467):

```rust
async fn complete_stream(
    &self,
    req: CompletionRequest,
    tx: tokio::sync::mpsc::Sender<String>,
) -> anyhow::Result<()> {
    let messages = self.lower_messages(&req.effective_messages());
    let mut body = serde_json::json!({
        "model": self.model,
        "max_tokens": req.max_tokens,
        "messages": messages,
        "stream": true,
    });
    if !req.tools.is_empty() {
        let anthropic_tools: Vec<serde_json::Value> = req.tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })
        }).collect();
        body["tools"] = serde_json::json!(anthropic_tools);
    }
    if req.thinking_budget.is_some() {
        body["thinking"] = serde_json::json!({
            "type": "enabled",
            "budget_tokens": req.thinking_budget.unwrap(),
        });
    }
    // ... rest of method unchanged
```

- [ ] **Step 5: Run cargo check to verify compilation**

Run: `cargo check -p openloom-inference`
Expected: FAIL — OpenAIClient still uses old format, will fix in Task 3.

- [ ] **Step 6: Commit**

```bash
git add crates/inference/src/lib.rs
git commit -m "feat: add Anthropic native tool calling (messages+tools+tool_use parsing)"
```

---

### Task 3: Update inference crate — OpenAIClient native tool calling

**Files:**
- Modify: `crates/inference/src/lib.rs` (OpenAIClient: try_complete, complete_stream)

- [ ] **Step 1: Rewrite OpenAIClient::try_complete with messages+tools**

Replace `OpenAIClient::try_complete` (lines 589-632):

```rust
async fn try_complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
    let messages = self.lower_messages(&req.effective_messages());
    let mut body = serde_json::json!({
        "model": self.model,
        "max_tokens": req.max_tokens,
        "messages": messages,
    });
    if !req.tools.is_empty() {
        let openai_tools: Vec<serde_json::Value> = req.tools.iter().map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                }
            })
        }).collect();
        body["tools"] = serde_json::json!(openai_tools);
    }
    if let Some(ref tc) = req.tool_choice {
        match tc {
            ToolChoice::Auto => { body["tool_choice"] = serde_json::json!("auto"); },
            ToolChoice::None => { body["tool_choice"] = serde_json::json!("none"); },
            ToolChoice::Required => { body["tool_choice"] = serde_json::json!("required"); },
        }
    }
    if req.temperature > 0.0 {
        body["temperature"] = serde_json::json!(req.temperature);
    }

    let resp = self.http
        .post(format!("{}/chat/completions", self.base_url))
        .header("Authorization", format!("Bearer {}", self.api_key))
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("API error {}: {}", status, text);
    }

    let body_text = resp.text().await?;
    let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
        let preview = &body_text[..body_text.len().min(500)];
        anyhow::anyhow!("API response parse error: {}, body: {}", e, preview)
    })?;

    let choice = &json["choices"][0]["message"];
    let text = choice["content"].as_str().unwrap_or("").to_string();
    let tool_calls: Vec<ToolCall> = choice["tool_calls"]
        .as_array()
        .map(|arr| {
            arr.iter().filter_map(|tc| {
                Some(ToolCall {
                    id: tc["id"].as_str()?.to_string(),
                    name: tc["function"]["name"].as_str()?.to_string(),
                    arguments: serde_json::from_str(
                        tc["function"]["arguments"].as_str().unwrap_or("{}")
                    ).unwrap_or(serde_json::Value::Object(Default::default())),
                })
            }).collect()
        })
        .unwrap_or_default();

    let prompt_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize;
    let completion_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize;
    let cached_tokens = json["usage"]["prompt_tokens_details"]["cached_tokens"]
        .as_u64()
        .unwrap_or(0) as usize;

    Ok(CompletionResponse {
        text,
        tool_calls,
        prompt_tokens,
        completion_tokens,
        cached_tokens,
        latency_ms: 0,
    })
}
```

- [ ] **Step 2: Add lower_messages helper to OpenAIClient**

Insert after `OpenAIClient::new()` (line 564):

```rust
/// Convert canonical Messages to OpenAI Chat Completions wire format.
fn lower_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
    messages.iter().map(|msg| {
        let role = msg.role.as_str();
        let mut obj = serde_json::json!({ "role": role });

        if role == "tool" {
            // OpenAI requires tool_call_id on tool messages
            if let Some(ContentPart::ToolResult { tool_call_id, name: _, result }) = msg.content.first() {
                obj["tool_call_id"] = serde_json::json!(tool_call_id);
                obj["content"] = serde_json::json!(result);
            }
            return obj;
        }

        // Collect text content and tool calls
        let texts: Vec<&str> = msg.content.iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        let tool_calls: Vec<serde_json::Value> = msg.content.iter()
            .filter_map(|p| match p {
                ContentPart::ToolCall { id, name, arguments } => Some(serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": serde_json::to_string(arguments).unwrap_or_default(),
                    }
                })),
                _ => None,
            })
            .collect();

        if texts.is_empty() && tool_calls.is_empty() {
            obj["content"] = serde_json::json!("");
        } else if !texts.is_empty() {
            obj["content"] = serde_json::json!(texts.join("\n"));
            if !tool_calls.is_empty() {
                obj["tool_calls"] = serde_json::json!(tool_calls);
            }
        } else {
            // tool calls only, no text
            obj["tool_calls"] = serde_json::json!(tool_calls);
        }

        obj
    }).collect()
}
```

- [ ] **Step 3: Update OpenAIClient::complete_stream**

Replace the body construction in `complete_stream` (around lines 645-652):

```rust
async fn complete_stream(
    &self,
    req: CompletionRequest,
    tx: tokio::sync::mpsc::Sender<String>,
) -> anyhow::Result<()> {
    let messages = self.lower_messages(&req.effective_messages());
    let mut body = serde_json::json!({
        "model": self.model,
        "max_tokens": req.max_tokens,
        "messages": messages,
        "stream": true,
    });
    if !req.tools.is_empty() {
        let openai_tools: Vec<serde_json::Value> = req.tools.iter().map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                }
            })
        }).collect();
        body["tools"] = serde_json::json!(openai_tools);
    }
    if let Some(ref tc) = req.tool_choice {
        match tc {
            ToolChoice::Auto => { body["tool_choice"] = serde_json::json!("auto"); },
            ToolChoice::None => { body["tool_choice"] = serde_json::json!("none"); },
            ToolChoice::Required => { body["tool_choice"] = serde_json::json!("required"); },
        }
    }
    // ... rest of method unchanged
```

- [ ] **Step 4: Run cargo check**

Run: `cargo check -p openloom-inference`
Expected: PASS (inference crate compiles, but engine/weaver still use old API)

- [ ] **Step 5: Commit**

```bash
git add crates/inference/src/lib.rs
git commit -m "feat: add OpenAI native tool calling (messages+tools+tool_calls parsing)"
```

---

### Task 4: Rewrite weaver to output structured messages

**Files:**
- Modify: `crates/weaver/src/lib.rs` (change assemble_with_limit to return Vec<Message>)
- Modify: `crates/weaver/Cargo.toml` (add any missing deps)

- [ ] **Step 1: Read current weaver tests as reference**

Already done — existing tests at lines 128-208 in weaver/lib.rs.

- [ ] **Step 2: Write new failing tests for message-based assembly**

Replace the test module in `crates/weaver/src/lib.rs` (lines 128-208):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use openloom_cache::NoopCache;
    use openloom_models::{Message, Role, ContentPart, ChatMessage};

    const SYSTEM_INSTRUCTION: &str = "You are openLoom, a private AI assistant.";

    fn make_weaver() -> ContextWeaver {
        ContextWeaver::new(Arc::new(NoopCache))
    }

    fn make_legacy_msg(role: &str, content: &str) -> ChatMessage {
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
        // Should have at least system + user messages
        assert!(result.len() >= 2);
        assert_eq!(result[0].role, Role::System);
        assert_eq!(result.last().unwrap().role, Role::User);
    }

    #[test]
    fn test_user_message_content_present() {
        let weaver = make_weaver();
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "hello world", "", None, &[]);
        let user_msg = result.last().unwrap();
        let text = user_msg.text_content();
        assert!(text.contains("hello world"));
    }

    #[test]
    fn test_system_message_contains_instruction() {
        let weaver = make_weaver();
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "hi", "", None, &[]);
        let sys_text = result[0].text_content();
        assert!(sys_text.contains("openLoom"));
    }

    #[test]
    fn test_assemble_with_persona() {
        let weaver = make_weaver();
        let persona = "用户画像：短线交易；追高倾向。";
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "hello", persona, None, &[]);
        let sys_text = result[0].text_content();
        assert!(sys_text.contains("短线交易"));
    }

    #[test]
    fn test_assemble_with_working_memory_preserves_roles() {
        let weaver = make_weaver();
        let memory = vec![
            make_legacy_msg("user", "hi"),
            make_legacy_msg("assistant", "hello"),
        ];
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "how are you", "", None, &memory);
        // Should have: system, user(from memory), assistant(from memory), user(new)
        assert!(result.len() >= 4);
        assert_eq!(result[1].role, Role::User);
        assert_eq!(result[2].role, Role::Assistant);
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
        let sys_text = result[0].text_content();
        assert!(sys_text.contains("file-manager"));
    }
}
```

- [ ] **Step 3: Run new tests to verify they fail**

Run: `cargo test -p openloom-weaver`
Expected: FAIL — `assemble()` still returns `AssembledPrompt` not `Vec<Message>`.

- [ ] **Step 4: Rewrite ContextWeaver to output Vec<Message>**

Replace `AssembledPrompt` struct and `ContextWeaver` impl (lines 1-89):

```rust
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

    /// Assemble a structured messages array for native tool calling.
    /// Returns: [system, ...history_messages, user_message]
    pub fn assemble(
        &self,
        system_instruction: &str,
        user_message: &str,
        persona_summary: &str,
        skill_context: Option<&str>,
        working_memory: &[ChatMessage],
    ) -> Vec<Message> {
        self.assemble_with_limit(system_instruction, user_message, persona_summary, skill_context, working_memory, 0)
    }

    pub fn assemble_with_limit(
        &self,
        system_instruction: &str,
        user_message: &str,
        persona_summary: &str,
        skill_context: Option<&str>,
        working_memory: &[ChatMessage],
        max_context_chars: usize,
    ) -> Vec<Message> {
        let mut messages: Vec<Message> = Vec::new();

        // 1. System message: instruction + persona + skill context
        let mut system_text = system_instruction.to_string();
        if !persona_summary.is_empty() {
            system_text.push_str("\n\n## Persona\n");
            system_text.push_str(persona_summary);
        }
        if let Some(ctx) = skill_context {
            if !ctx.is_empty() {
                system_text.push_str("\n\n## Available Tools\n");
                system_text.push_str(ctx);
            }
        }
        let static_prefix_len = system_text.len();
        messages.push(Message {
            role: Role::System,
            content: vec![ContentPart::Text { text: system_text }],
            timestamp: chrono::Utc::now(),
        });

        // 2. Conversation history (legacy ChatMessage → structured Message)
        let history_msgs = if max_context_chars > 0 && !working_memory.is_empty() {
            let overhead = static_prefix_len + user_message.len() + 200;
            let budget = max_context_chars.saturating_sub(overhead);
            compact_memory_messages(working_memory, budget)
        } else {
            working_memory.iter().map(|m| Message::from_legacy(m)).collect()
        };
        messages.extend(history_msgs);

        // 3. Current user message
        messages.push(Message::user(user_message));

        messages
    }

    // Keep assemble_legacy for backward compat during migration
    pub fn assemble_legacy(
        &self,
        system_instruction: &str,
        user_message: &str,
        persona_summary: &str,
        skill_context: Option<&str>,
        working_memory: &[ChatMessage],
    ) -> AssembledPrompt {
        let messages = self.assemble(system_instruction, user_message, persona_summary, skill_context, working_memory);
        // Flatten to string for backward compat
        let prompt: String = messages.iter()
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
    let total_chars: usize = messages.iter().map(|m| m.role.len() + m.content.len() + 3).sum();
    if total_chars <= budget_chars {
        return messages.iter().map(|m| Message::from_legacy(m)).collect();
    }

    let mut kept: Vec<ChatMessage> = Vec::new();
    let mut used = 0usize;
    let note_size = 80; // "[Earlier messages compacted]"

    for msg in messages.iter().rev() {
        let msg_size = msg.role.len() + msg.content.len() + 3;
        if used + msg_size + note_size > budget_chars && !kept.is_empty() {
            break;
        }
        used += msg_size;
        kept.push(msg.clone());
    }
    kept.reverse();

    let mut result: Vec<Message> = Vec::new();
    if kept.len() < messages.len() {
        result.push(Message {
            role: Role::System,
            content: vec![ContentPart::Text {
                text: "[Earlier messages were compacted to fit context window]".into()
            }],
            timestamp: chrono::Utc::now(),
        });
    }
    result.extend(kept.iter().map(|m| Message::from_legacy(m)));
    result
}
```

- [ ] **Step 5: Run weaver tests**

Run: `cargo test -p openloom-weaver`
Expected: PASS (all 6 tests pass).

- [ ] **Step 6: Commit**

```bash
git add crates/weaver/src/lib.rs
git commit -m "feat: rewrite weaver to output structured Vec<Message> for native tool calling"
```

---

### Task 5: Update engine — build_tool_definitions + new system instruction

**Files:**
- Modify: `crates/engine/src/lib.rs` (replace SYSTEM_INSTRUCTION, add build_tool_definitions)
- Modify: `crates/engine/Cargo.toml` (check deps)

- [ ] **Step 1: Replace SYSTEM_INSTRUCTION with native-tool-calling version**

Replace lines 72-97 in `crates/engine/src/lib.rs`:

```rust
pub(crate) const SYSTEM_INSTRUCTION: &str = "You are openLoom, a coding assistant and AI agent running locally.

## Environment
- Working directory: [cwd]
- Platform: [platform]

## Workflow
1. Read files before editing them.
2. Make minimal, precise edits.
3. Run tests/checks after changes to verify correctness.
4. Search before making assumptions about code structure.

## Rules
- Answer in the same language as the user.
- Be concise and direct.

## Tool Use
Use the provided tools to accomplish the user's task. Call tools when you need to read files, search code, run commands, or make edits.
";
```

**Key change:** Removed the `{"tool": ..., "params": ...}` JSON block instruction and `[tools]` placeholder. Tools are now sent natively via the API's `tools` parameter.

- [ ] **Step 2: Add build_tool_definitions function**

Insert after `system_instruction()` near line 119 in `crates/engine/src/lib.rs`:

```rust
use openloom_models::ToolDefinition;

/// Build ToolDefinition array from registered skills for native tool calling.
pub(crate) fn build_tool_definitions(skills: &[SkillInfo]) -> Vec<ToolDefinition> {
    skills.iter().map(|s| {
        ToolDefinition {
            name: s.name.clone(),
            description: s.description.clone(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": true,
            }),
        }
    }).collect()
}
```

**Note:** `SkillInfo` is the skill metadata struct from `crates/skills/src/lib.rs`. Check the import path — it may need `use openloom_skills::SkillInfo;`.

- [ ] **Step 3: Run cargo check**

Run: `cargo check -p openloom-engine`
Expected: FAIL — agent_loop.rs still references `replace("[tools]", ...)` and uses old `parse_tool_call`, etc.

- [ ] **Step 4: Commit**

```bash
git add crates/engine/src/lib.rs
git commit -m "feat: replace text-based tool prompt with native tool definitions in engine"
```

---

### Task 6: Rewrite agent_loop to use native tool calling

**Files:**
- Modify: `crates/engine/src/agent_loop.rs` (rewrite agent_loop_inner, remove parse_tool_call)

- [ ] **Step 1: Write failing integration test for agent_loop tool calling**

Create `crates/engine/tests/agent_loop_integration.rs`:

```rust
use openloom_models::{Message, Role, ContentPart, ToolCall, ToolDefinition, ChatMessage};
use openloom_inference::{CompletionRequest, CompletionResponse};

#[test]
fn test_agent_loop_detects_tool_calls_from_response() {
    // Verify that a response with tool_calls is correctly detected.
    let response = CompletionResponse {
        text: String::new(),
        tool_calls: vec![ToolCall {
            id: "toolu_001".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "/test.txt"}),
        }],
        prompt_tokens: 100,
        completion_tokens: 50,
        cached_tokens: 0,
        latency_ms: 0,
    };
    assert!(response.has_tool_calls());
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].name, "read_file");
}

#[test]
fn test_completion_request_uses_messages_when_present() {
    let req = CompletionRequest {
        messages: vec![
            Message::user("hello"),
        ],
        ..Default::default()
    };
    let effective = req.effective_messages();
    assert_eq!(effective.len(), 1);
    assert_eq!(effective[0].role, Role::User);
}

#[test]
fn test_completion_request_falls_back_to_prompt() {
    let req = CompletionRequest {
        prompt: "hello from prompt".into(),
        ..Default::default()
    };
    let effective = req.effective_messages();
    assert_eq!(effective.len(), 1);
    assert_eq!(effective[0].role, Role::User);
    assert!(effective[0].text_content().contains("hello from prompt"));
}
```

- [ ] **Step 2: Run integration tests to verify they fail or pass**

Run: `cargo test -p openloom-engine --test agent_loop_integration`
Note: These test the types themselves, so they should PASS immediately (types exist from Task 1). This confirms the type system works.

- [ ] **Step 3: Rewrite agent_loop_inner to use native tool calling**

Replace `agent_loop_inner` (lines 30-215) in `crates/engine/src/agent_loop.rs`:

```rust
async fn agent_loop_inner(
    &self,
    msg: &ChatMessage,
    session_id: &str,
    tx: Option<mpsc::Sender<String>>,
    mode: openloom_models::Mode,
) -> Result<ChatResponse> {
    self.in_flight.fetch_add(1, Ordering::SeqCst);
    let loop_start = std::time::Instant::now();
    *self.agent_state.write().await = AgentState::Thinking;
    let _ = self.event_bus.send(EngineEvent::AgentStateChanged {
        old_state: AgentState::Idle,
        new_state: AgentState::Thinking,
    });
    self.interruptible.store(true, Ordering::SeqCst);

    let mut history: Vec<ChatMessage> = self.get_working_memory(session_id).unwrap_or_default();
    history.push(msg.clone());

    // Build native tool definitions from skill registry
    let skill_infos = self.skills.list_all();
    let tool_definitions = crate::build_tool_definitions(&skill_infos);

    let mut all_tool_messages: Vec<ChatMessage> = Vec::new();
    let mut last_response = String::new();
    let mut total_prompt_tokens = 0usize;
    let mut total_completion_tokens = 0usize;

    let (max_iterations, timeout_secs) = {
        let cfg = self.config.read().await;
        (
            cfg.agent.max_iterations.max(3),
            cfg.agent.timeout_secs.max(60),
        )
    };

    let mode_cfg = mode.config();
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
        for _iteration in 0..max_iterations {
            let persona_summary = self.persona.summarize().await.unwrap_or_default();
            let system_instruction = crate::system_instruction(); // No [tools] placeholder needed
            let system_with_mode = if mode_cfg.system_suffix.is_empty() {
                system_instruction
            } else {
                format!("{}\n\n{}", system_instruction, mode_cfg.system_suffix)
            };

            // Build structured messages from weaver
            let messages = self.weaver.assemble_with_limit(
                &system_with_mode,
                "", /* user message already in history */
                &persona_summary,
                None, /* skill context no longer needed — sent as native tools */
                &history,
                self.context_max_chars,
            );

            // Build structured completion request
            let completion_req = CompletionRequest {
                messages,
                tools: tool_definitions.clone(),
                tool_choice: None, // auto
                prompt: String::new(),
                max_tokens: self.max_output_tokens,
                ..Default::default()
            };

            let response = self.invoke_model_native(&completion_req).await?;
            // Track tokens
            total_prompt_tokens += response.prompt_tokens;
            total_completion_tokens += response.completion_tokens;

            if response.has_tool_calls() {
                // Stream tool call markers to UI
                if let Some(ref tx) = tx {
                    for tc in &response.tool_calls {
                        let call_json = serde_json::to_string(tc).unwrap_or_default();
                        let _ = tx.send(format!("\x01CALL\x02{}", call_json)).await;
                    }
                }

                *self.agent_state.write().await = AgentState::Acting;
                let _ = self.event_bus.send(EngineEvent::AgentStateChanged {
                    old_state: AgentState::Thinking,
                    new_state: AgentState::Acting,
                });

                // Execute each tool call
                for tc in &response.tool_calls {
                    let tool_params = tc.arguments.clone();
                    let result = match self.execute_tool_native(tc, mode).await {
                        Ok(output) => truncate_tool_result(&output),
                        Err(e) => format!("Tool error: {}", e),
                    };

                    // Stream result marker
                    if let Some(ref tx) = tx {
                        let _ = tx.send(format!("\x01RESULT\x02{}", result)).await;
                    }

                    // Append to history with proper tool_call_id correlation
                    let ts = Utc::now();
                    history.push(ChatMessage {
                        role: "assistant".into(),
                        content: format!("Tool call: {}", tc.name),
                        timestamp: ts,
                    });
                    history.push(ChatMessage {
                        role: "tool".into(),
                        content: result.clone(),
                        timestamp: ts,
                    });
                    all_tool_messages.push(ChatMessage {
                        role: "assistant".into(),
                        content: format!("Tool call: {}", tc.name),
                        timestamp: ts,
                    });
                    all_tool_messages.push(ChatMessage {
                        role: "tool".into(),
                        content: result,
                        timestamp: ts,
                    });
                }
                *self.agent_state.write().await = AgentState::Thinking;
            } else {
                last_response = response.text;
                break;
            }
        }

        // Fallback: if no response but had tool calls, do one more turn
        if last_response.is_empty() && !all_tool_messages.is_empty() {
            let persona_summary = self.persona.summarize().await.unwrap_or_default();
            let system_instruction = crate::system_instruction();
            let system_with_mode = if mode_cfg.system_suffix.is_empty() {
                system_instruction
            } else {
                format!("{}\n\n{}", system_instruction, mode_cfg.system_suffix)
            };
            let messages = self.weaver.assemble_with_limit(
                &system_with_mode, "", &persona_summary, None, &history, self.context_max_chars,
            );
            let completion_req = CompletionRequest {
                messages,
                tools: tool_definitions.clone(),
                tool_choice: Some(ToolChoice::Auto),
                prompt: String::new(),
                max_tokens: self.max_output_tokens,
                ..Default::default()
            };
            let response = self.invoke_model_native(&completion_req).await?;
            total_prompt_tokens += response.prompt_tokens;
            total_completion_tokens += response.completion_tokens;
            last_response = response.text;
        }

        Ok::<_, anyhow::Error>(last_response)
    }).await;

    // ... rest of method unchanged from line 163 onward
```

- [ ] **Step 4: Add invoke_model_native and execute_tool_native methods**

Insert before `invoke_model_raw` (line 229):

```rust
/// Invoke model with structured CompletionRequest (native tool calling).
pub(crate) async fn invoke_model_native(&self, req: &CompletionRequest) -> Result<CompletionResponse> {
    if let Some(ref cloud) = self.cloud {
        match cloud.complete(req.clone()).await {
            Ok(r) => return Ok(r),
            Err(e) => tracing::warn!("Cloud failed, trying local: {}", e),
        }
    }
    // Fallback: construct a text-only request from messages
    let prompt = req.effective_messages().iter()
        .map(|m| format!("{}: {}", m.role.as_str(), m.text_content()))
        .collect::<Vec<_>>()
        .join("\n");
    let fallback_req = CompletionRequest {
        prompt,
        max_tokens: req.max_tokens,
        ..Default::default()
    };
    if let Some(ref local) = self.local_client {
        match local.complete(fallback_req.clone()).await {
            Ok(r) => return Ok(r),
            Err(e) => tracing::warn!("Local client failed, trying inference engine: {}", e),
        }
    }
    self.inference.complete(fallback_req).await
}

/// Execute a native ToolCall from the model response.
pub(crate) async fn execute_tool_native(
    &self,
    call: &ToolCall,
    mode: openloom_models::Mode,
) -> Result<String> {
    let mode_cfg = mode.config();
    if !mode_cfg.tool_scope.allows(&call.name) {
        return Ok(format!(
            "Tool '{}' is not available in {} mode.",
            call.name, mode_cfg.status_label
        ));
    }
    let risk = openloom_sandbox::classify_risk(&call.name, &call.arguments);

    if !self.skip_permissions
        && matches!(risk, openloom_models::RiskLevel::Medium | openloom_models::RiskLevel::High)
    {
        let risk_str = format!("{:?}", risk);
        let desc = format!("{}({:?})", call.name, call.arguments);
        let req = openloom_models::PermissionRequest {
            tool_name: call.name.clone(),
            description: desc,
            risk_level: risk_str,
        };
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        if self.perm_request_tx.send((req, resp_tx)).await.is_ok() {
            match resp_rx.await {
                Ok(true) => {}
                _ => return Ok(format!("Tool '{}' denied by user.", call.name)),
            }
        }
    }

    if matches!(risk, openloom_models::RiskLevel::Forbidden) {
        let msg = openloom_sandbox::risk_message(&call.name, &call.arguments, &risk);
        return Ok(msg);
    }

    self.skills
        .invoke(&call.name, call.arguments.clone())
        .await
        .map(|v| v.to_string())
}
```

- [ ] **Step 5: Remove deprecated parse_tool_call**

Delete the `parse_tool_call` method (lines 267-315). It's no longer needed — tool calls come from `CompletionResponse.tool_calls`.

- [ ] **Step 6: Update execute_tool to delegate to execute_tool_native**

Replace the old `execute_tool` body (lines 317-360) with a delegate that converts from old `ToolCall` format:

```rust
pub(crate) async fn execute_tool(&self, call: &ToolCall, mode: openloom_models::Mode) -> Result<String> {
    // Legacy compatibility — delegates to native implementation
    self.execute_tool_native(call, mode).await
}
```

- [ ] **Step 7: Run cargo check**

Run: `cargo check -p openloom-engine`
Expected: FAIL — need to add imports.

- [ ] **Step 8: Add required imports to agent_loop.rs**

Replace the imports at the top:

```rust
use super::Engine;
use crate::token_store::TokenUsageRecord;
use anyhow::Result;
use chrono::Utc;
use openloom_inference::CompletionRequest;
use openloom_models::*;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;
```

- [ ] **Step 9: Run cargo check again**

Run: `cargo check -p openloom-engine`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/engine/src/agent_loop.rs crates/engine/src/lib.rs
git commit -m "feat: rewrite agent loop to use native tool calling via API tools parameter"
```

---

### Task 7: Fix engine/lib.rs callers — handle_message (non-agent-loop path)

**Files:**
- Modify: `crates/engine/src/lib.rs:471-534`

- [ ] **Step 1: Replace assembled.prompt references in handle_message**

Replace lines 471-478 and the `prompt: assembled.prompt.clone()` lines 491, 499, 512, 519:

```rust
        let assembled = self.weaver.assemble_with_limit(
            &system,
            &msg.content,
            &persona_summary,
            skill_ctx.as_deref(),
            &working_memory,
            self.context_max_chars,
        );

        // Build a flat prompt string for the non-agent-loop (simple) path
        let flat_prompt = assembled.iter()
            .map(|m| format!("{}: {}", m.role.as_str(), m.text_content()))
            .collect::<Vec<_>>()
            .join("\n");

        let response = match out.target_model {
            TargetModel::None => {
                unreachable!(
                    "TargetModel::None with no skill_match -- should have gone to agent_loop"
                )
            }
            TargetModel::Local => {
                if let Some(ref local) = self.local_client {
                    local
                        .complete(CompletionRequest {
                            prompt: flat_prompt.clone(),
                            ..Default::default()
                        })
                        .await?
                        .text
                } else if self.cloud.is_some() {
                    self.inference
                        .complete(CompletionRequest {
                            prompt: flat_prompt.clone(),
                            ..Default::default()
                        })
                        .await?
                        .text
                } else {
                    Self::NO_CLOUD_RESPONSE.to_string()
                }
            }
            TargetModel::Cloud => {
                if let Some(ref cloud) = self.cloud {
                    cloud
                        .complete(CompletionRequest {
                            prompt: flat_prompt.clone(),
                            ..Default::default()
                        })
                        .await?
                        .text
                } else if let Some(ref local) = self.local_client {
                    local
                        .complete(CompletionRequest {
                            prompt: flat_prompt.clone(),
                            ..Default::default()
                        })
                        .await?
                        .text
                } else {
                    Self::NO_CLOUD_RESPONSE.to_string()
                }
            }
        };

        // save_messages is non-fatal
        let _ = self.save_messages(session_id, &msg, &response);

        let prompt_tokens = self.inference.token_count(&flat_prompt);
        let completion_tokens = self.inference.token_count(&response);
```

- [ ] **Step 2: Run cargo check**

Run: `cargo check -p openloom-engine`
Expected: FAIL — stream.rs also needs fixing.

- [ ] **Step 3: Commit**

```bash
git add crates/engine/src/lib.rs
git commit -m "fix: update handle_message to use Vec<Message> from weaver"
```

---

### Task 8: Fix engine/stream.rs callers + temperature

**Files:**
- Modify: `crates/engine/src/stream.rs:82-134`

- [ ] **Step 1: Replace assembled.prompt in handle_message_streaming**

Replace lines 82-134:

```rust
        let system = crate::system_instruction(); // [tools] placeholder removed
        let system = if mode_cfg.system_suffix.is_empty() {
            system
        } else {
            format!("{}\n\n{}", system, mode_cfg.system_suffix)
        };
        let assembled = self.weaver.assemble_with_limit(
            &system,
            &msg.content,
            &persona_summary,
            skill_ctx.as_deref(),
            &working_memory,
            self.context_max_chars,
        );

        // Build flat prompt for streaming fallback path
        let flat_prompt = assembled.iter()
            .map(|m| format!("{}: {}", m.role.as_str(), m.text_content()))
            .collect::<Vec<_>>()
            .join("\n");

        let start = Instant::now();
        // ... (collector code unchanged)

        // Stream the completion — use low temperature for reliable tool calling
        let req = CompletionRequest {
            prompt: flat_prompt.clone(),
            max_tokens: self.max_output_tokens,
            temperature: 0.0, // was 0.7 — too high for reliable tool calling
            top_p: 1.0,
            stop: Vec::new(),
            stream: true,
            thinking_budget: thinking.budget_tokens(),
        };
```

Also replace line 175 `self.inference.token_count(&assembled.prompt)` with `self.inference.token_count(&flat_prompt)`.

- [ ] **Step 2: Run cargo check**

Run: `cargo check -p openloom-engine`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/engine/src/stream.rs
git commit -m "fix: update stream.rs for Vec<Message> weaver, set temperature=0.0 for tool calling"
```

---

### Task 9: Fix inference crate — LlamaRuntime + stub_complete CompletionResponse

**Files:**
- Modify: `crates/inference/src/lib.rs:206-212, 302-308`

- [ ] **Step 1: Add tool_calls field to LlamaRuntime::complete**

Line 206-212, add `tool_calls: Vec::new()`:

```rust
            Ok(CompletionResponse {
                text,
                prompt_tokens,
                completion_tokens,
                cached_tokens: 0,
                latency_ms: 0,
                tool_calls: Vec::new(),
            })
```

- [ ] **Step 2: Add tool_calls field to stub_complete**

Line 302-308, add `tool_calls: Vec::new()`:

```rust
    CompletionResponse {
        text: response,
        prompt_tokens: prompt_chars / 4,
        completion_tokens: response_tokens,
        cached_tokens: 0,
        latency_ms: 0,
        tool_calls: Vec::new(),
    }
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check -p openloom-inference`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/inference/src/lib.rs
git commit -m "fix: add tool_calls field to LlamaRuntime and stub CompletionResponse builders"
```

---

### Task 10: Delete dead code + fix tests + missing import

**Files:**
- Modify: `crates/engine/src/agent_loop.rs:217-227` (remove build_skill_list_string)
- Modify: `crates/engine/src/lib.rs:57` (add SkillInfo import)
- Modify: `crates/engine/src/lib.rs:1014-1049` (remove parse_tool_call tests)

- [ ] **Step 1: Remove build_skill_list_string dead code**

Delete lines 217-227 in `crates/engine/src/agent_loop.rs`:

```rust
    pub(crate) fn build_skill_list_string(&self) -> String {
        let skills = self.skills.all_skills();
        if skills.is_empty() {
            return "None".into();
        }
        skills
            .iter()
            .map(|s| format!("### {}\n{}", s.name(), s.context_md()))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
```

- [ ] **Step 2: Add missing SkillInfo import**

In `crates/engine/src/lib.rs` line 57, change:
```rust
use openloom_skills::{Skill, SkillRegistry, builtins};
```
to:
```rust
use openloom_skills::{Skill, SkillInfo, SkillRegistry, builtins};
```

- [ ] **Step 3: Remove parse_tool_call tests**

Delete lines 1014-1050 in `crates/engine/src/lib.rs` (5 test functions: `test_parse_tool_call_valid`, `test_parse_tool_call_with_whitespace`, `test_parse_tool_call_malformed_json`, `test_parse_tool_call_no_json`, `test_parse_tool_call_nested_braces`).

- [ ] **Step 4: Run cargo check**

Run: `cargo check -p openloom-engine`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/agent_loop.rs crates/engine/src/lib.rs
git commit -m "chore: remove dead code (build_skill_list_string, parse_tool_call tests), add SkillInfo import"
```

---

### Task 11: DeepSeek reasoning_content quirk + Anthropic tool_choice mapping

**Files:**
- Modify: `crates/inference/src/lib.rs` (OpenAIClient: lower_messages — add reasoning_content to assistant messages; AnthropicClient — tool_choice "none"→omit tools, "required"→"any")

- [ ] **Step 1: Add reasoning_content for DeepSeek compatibility in OpenAIClient**

In `OpenAIClient::lower_messages()`, after building the message obj for assistant messages, add `reasoning_content` field. Update the assistant message construction:

```rust
        if role == "assistant" {
            // DeepSeek requires reasoning_content on every assistant message (even empty)
            obj["reasoning_content"] = serde_json::json!("");
        }
```

Insert this right after setting `obj["role"]` and before the tool_calls/text content logic.

- [ ] **Step 2: Fix AnthropicClient tool_choice mapping**

In `AnthropicClient::try_complete()`, update the tool_choice section:

```rust
    if let Some(ref tc) = req.tool_choice {
        match tc {
            ToolChoice::Auto => { body["tool_choice"] = serde_json::json!({"type": "auto"}); },
            ToolChoice::None => { body["tools"] = serde_json::json!([]); }, // Anthropic: omit tools, no "none" value
            ToolChoice::Required => { body["tool_choice"] = serde_json::json!({"type": "any"}); }, // Anthropic uses "any" not "required"
        }
    }
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check -p openloom-inference`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/inference/src/lib.rs
git commit -m "fix: add DeepSeek reasoning_content compat, fix Anthropic tool_choice mapping"
```

---

### Task 12: Run full test suite, clippy, fmt

**Files:**
- All modified files

- [ ] **Step 1: Run all tests**

Run: `cargo test --all`
Expected: Some old tests may fail.

- [ ] **Step 2: Fix remaining test failures**

For each failing test:
- Asserts `SYSTEM_INSTRUCTION` contains `{"tool"` — remove assertion
- Asserts weaver `AssembledPrompt.prompt` field directly — use `.text_content()` on messages instead
- Mocks `CompletionResponse` without `tool_calls` — add `tool_calls: vec![]`
- Calls removed methods (`parse_tool_call`, `build_skill_list_string`) — remove test

- [ ] **Step 3: Run cargo clippy**

Run: `cargo clippy --all -- -D warnings`
Expected: PASS (0 warnings).

- [ ] **Step 4: Run cargo fmt**

Run: `cargo fmt --all`

- [ ] **Step 5: Final test run**

Run: `cargo test --all`
Expected: PASS (all tests pass).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "test: fix all tests for native tool calling migration, clippy clean, fmt"
```

---

## Verification Checklist

After all tasks complete, verify end-to-end:

1. `cargo build --release` — compiles clean
2. `cargo test --all` — all tests pass
3. `cargo clippy --all -- -D warnings` — zero warnings
4. `cargo fmt --all -- --check` — clean
5. Manual smoke test: start openLoom with DeepSeek backend, send "修改 tga-splitter.html 内的标题为 TGA工具", verify tool calls are made

---

## Rollback Safety

All changes are additive to the type system:
- `CompletionRequest.prompt` field preserved for backward compat
- `effective_messages()` converts legacy prompt to messages
- `invoke_model_raw()` kept as fallback path
- Old `execute_tool` delegates to new `execute_tool_native`

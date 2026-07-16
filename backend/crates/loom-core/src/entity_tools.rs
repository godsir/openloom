//! Entity management tools - CRUD wrappers for agent, model, and team configs.
//! Each tool delegates to the MemoryStore trait which persists to SQLite.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use loom_types::{ModelBackend, ModelType, ToolDefinition, ToolProgress};
use tokio::sync::RwLock;
use tokio::sync::mpsc::UnboundedSender;

use crate::orchestrator::MemoryStore;
use crate::tool_context::ToolContext;
use crate::tool_registry::{AgentTool, ToolProvenance, ToolResult};

fn resolve_ms<'a>(
    guard: &'a tokio::sync::RwLockReadGuard<'_, Option<Box<dyn MemoryStore>>>,
) -> Result<&'a (dyn MemoryStore + 'static)> {
    guard
        .as_ref()
        .map(|b| b.as_ref() as &(dyn MemoryStore + 'static))
        .ok_or_else(|| anyhow::anyhow!("MemoryStore not initialized"))
}

// ============================================================================
// manage_agent
// ============================================================================

pub struct ManageAgentTool {
    pub memory_store: Arc<RwLock<Option<Box<dyn MemoryStore>>>>,
    pub cache: Arc<RwLock<HashMap<String, loom_types::AgentConfig>>>,
}

#[async_trait]
impl AgentTool for ManageAgentTool {
    fn tool_name(&self) -> &str {
        "manage_agent"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "manage_agent".into(),
            description: "Manage AI agent configs. Use when user says create/delete/change an agent.\n\nActions: list, get, create, update, delete. Required: action + name (except list). Optional: persona, model, thinking_level, temperature, system_prompt_override, tool_scope, is_primary, memory_enabled, max_iterations, timeout_secs.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "get", "create", "update", "delete"] },
                    "name": { "type": "string" },
                    "prev_name": { "type": "string" },
                    "persona": { "type": "string" },
                    "model": { "type": "string" },
                    "thinking_level": { "type": "string", "enum": ["low", "medium", "high", "xhigh", "max"] },
                    "temperature": { "type": "number" },
                    "system_prompt_override": { "type": "string" },
                    "tool_scope": { "type": "string" },
                    "is_primary": { "type": "boolean" },
                    "memory_enabled": { "type": "boolean" },
                    "max_iterations": { "type": "integer" },
                    "timeout_secs": { "type": "integer" }
                },
                "required": ["action"]
            }),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        _ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let action = arguments["action"].as_str().unwrap_or("");
        let guard = self.memory_store.read().await;
        let ms = resolve_ms(&guard)?;
        let result = exec_agent(action, &arguments, ms, &self.cache).await?;
        Ok(ToolResult {
            content: result,
            is_error: false,
            structured_content: None,
        })
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

async fn exec_agent(
    action: &str,
    args: &serde_json::Value,
    ms: &dyn MemoryStore,
    cache: &Arc<RwLock<HashMap<String, loom_types::AgentConfig>>>,
) -> Result<String> {
    match action {
        "list" => {
            let mut configs = ms.list_agent_configs().await?;
            // Hide __team_* synthetic configs — these are runtime artifacts for
            // inline team members and should not be visible to the LLM or user.
            configs.retain(|c| !c.name.starts_with("__team_"));
            Ok(serde_json::to_string_pretty(&configs).unwrap_or_else(|e| e.to_string()))
        }
        "get" => {
            let name = req_str(args, "name")?;
            match ms.get_agent_config(name).await? {
                Some(c) => Ok(serde_json::to_string_pretty(&c).unwrap_or_else(|e| e.to_string())),
                None => Ok(format!("Agent \"{name}\" not found.")),
            }
        }
        "create" | "update" => {
            let name = req_str(args, "name")?;
            let prev = args["prev_name"].as_str();
            let lookup = prev.unwrap_or(name);
            let mut cfg = match ms.get_agent_config(lookup).await? {
                Some(c) => c,
                None if action == "update" => {
                    return Err(anyhow::anyhow!("Agent \"{lookup}\" not found"))
                }
                None => {
                    let mut c = loom_types::AgentConfig::default();
                    c.name = name.to_string();
                    c
                }
            };
            patch_agent(&mut cfg, args);
            cfg.name = name.to_string();
            ms.save_agent_config(&cfg).await?;
            cache.write().await.insert(name.to_string(), cfg.clone());
            Ok(format!(
                "Agent \"{name}\" {}d.",
                if action == "create" { "create" } else { "update" }
            ))
        }
        "delete" => {
            let name = req_str(args, "name")?;
            if name == "default" {
                return Err(anyhow::anyhow!("cannot delete the 'default' agent"));
            }
            ms.delete_agent_config(name).await?;
            cache.write().await.remove(name);
            Ok(format!("Agent \"{name}\" deleted."))
        }
        _ => Err(anyhow::anyhow!(
            "Unknown action: {action}. Use list | get | create | update | delete."
        )),
    }
}

fn patch_agent(cfg: &mut loom_types::AgentConfig, args: &serde_json::Value) {
    if let Some(v) = args["persona"].as_str() { cfg.persona = v.to_string(); }
    if let Some(v) = args["model"].as_str() { cfg.model = Some(v.to_string()); }
    if let Some(v) = args["thinking_level"].as_str() { cfg.thinking_level = Some(v.to_string()); }
    if let Some(v) = args["temperature"].as_f64() { cfg.temperature = Some(v as f32); }
    if let Some(v) = args["system_prompt_override"].as_str() { cfg.system_prompt_override = Some(v.to_string()); }
    if let Some(v) = args["tool_scope"].as_str() { cfg.tool_scope = Some(v.to_string()); }
    if let Some(v) = args["is_primary"].as_bool() { cfg.is_primary = v; }
    if let Some(v) = args["memory_enabled"].as_bool() { cfg.memory_enabled = v; }
    if let Some(v) = args["max_iterations"].as_u64() { cfg.max_iterations = Some(v as usize); }
    if let Some(v) = args["timeout_secs"].as_u64() { cfg.timeout_secs = Some(v); }
}

// ============================================================================
// manage_model
// ============================================================================

pub struct ManageModelTool {
    pub memory_store: Arc<RwLock<Option<Box<dyn MemoryStore>>>>,
    pub cache: Arc<RwLock<HashMap<String, loom_types::ModelConfig>>>,
    pub active_model_name: Arc<RwLock<Option<String>>>,
}

#[async_trait]
impl AgentTool for ManageModelTool {
    fn tool_name(&self) -> &str {
        "manage_model"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "manage_model".into(),
            description: "Manage AI model configs. Use when user says add/switch/delete a model provider.\n\nActions: list, get, create, update, delete, set_active, get_active. Required: action + name (except list/get_active). Optional: model, model_type (Router/Summarizer/Reasoning), backend (Anthropic/OpenAI/DeepSeek/LmStudio/Ollama/Custom), base_url, api_key_env, context_size, max_output_tokens.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "get", "create", "update", "delete", "set_active", "get_active"] },
                    "name": { "type": "string" },
                    "prev_name": { "type": "string" },
                    "model": { "type": "string" },
                    "model_type": { "type": "string", "enum": ["Router", "Summarizer", "Reasoning"] },
                    "backend": { "type": "string", "enum": ["Anthropic", "OpenAI", "DeepSeek", "LmStudio", "Ollama", "Custom"] },
                    "base_url": { "type": "string" },
                    "api_key_env": { "type": "string" },
                    "context_size": { "type": "integer" },
                    "max_output_tokens": { "type": "integer" }
                },
                "required": ["action"]
            }),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        _ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let action = arguments["action"].as_str().unwrap_or("");
        let guard = self.memory_store.read().await;
        let ms = resolve_ms(&guard)?;
        let result = exec_model(action, &arguments, ms, &self.cache, &self.active_model_name).await?;
        Ok(ToolResult {
            content: result,
            is_error: false,
            structured_content: None,
        })
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

async fn exec_model(
    action: &str,
    args: &serde_json::Value,
    ms: &dyn MemoryStore,
    cache: &Arc<RwLock<HashMap<String, loom_types::ModelConfig>>>,
    active_model_name: &Arc<RwLock<Option<String>>>,
) -> Result<String> {
    match action {
        "list" => {
            let configs = ms.list_model_configs().await?;
            Ok(serde_json::to_string_pretty(&configs).unwrap_or_else(|e| e.to_string()))
        }
        "get" => {
            let name = req_str(args, "name")?;
            match ms.get_model_config(name).await? {
                Some(c) => Ok(serde_json::to_string_pretty(&c).unwrap_or_else(|e| e.to_string())),
                None => Ok(format!("Model \"{name}\" not found.")),
            }
        }
        "get_active" => match ms.get_active_model().await? {
            Some(c) => Ok(format!(
                "Active: {} ({} / {})",
                c.name,
                c.model.as_deref().unwrap_or("auto"),
                c.backend.name()
            )),
            None => Ok("No active model set.".into()),
        },
        "create" | "update" => {
            let name = req_str(args, "name")?;
            let prev = args["prev_name"].as_str();
            let lookup = prev.unwrap_or(name);
            let mut cfg = match ms.get_model_config(lookup).await? {
                Some(c) => c,
                None if action == "update" => {
                    return Err(anyhow::anyhow!("Model \"{lookup}\" not found"))
                }
                None => loom_types::ModelConfig {
                    name: String::new(),
                    ..Default::default()
                },
            };
            patch_model(&mut cfg, args);
            cfg.name = name.to_string();
            ms.save_model_config(&cfg).await?;
            cache.write().await.insert(name.to_string(), cfg.clone());
            Ok(format!(
                "Model \"{name}\" {}d.",
                if action == "create" { "create" } else { "update" }
            ))
        }
        "delete" => {
            let name = req_str(args, "name")?;
            ms.delete_model_config(name).await?;
            cache.write().await.remove(name);
            Ok(format!("Model \"{name}\" deleted."))
        }
        "set_active" => {
            let name = req_str(args, "name")?;
            ms.set_active_model(name).await?;
            active_model_name.write().await.replace(name.to_string());
            Ok(format!("Active model set to \"{name}\"."))
        }
        _ => Err(anyhow::anyhow!("Unknown action: {action}.")),
    }
}

fn patch_model(cfg: &mut loom_types::ModelConfig, args: &serde_json::Value) {
    if let Some(v) = args["model"].as_str() { cfg.model = Some(v.to_string()); }
    if let Some(v) = args["base_url"].as_str() { cfg.base_url = Some(v.to_string()); }
    if let Some(v) = args["api_key_env"].as_str() { cfg.api_key_env = Some(v.to_string()); }
    if let Some(v) = args["context_size"].as_u64() { cfg.context_size = v as usize; }
    if let Some(v) = args["max_output_tokens"].as_u64() { cfg.max_output_tokens = Some(v as usize); }
    if let Some(v) = args["model_type"].as_str() {
        cfg.model_type = match v {
            "Summarizer" => ModelType::Summarizer,
            "Reasoning" => ModelType::Reasoning,
            _ => ModelType::Router,
        };
    }
    if let Some(v) = args["backend"].as_str() {
        cfg.backend = match v {
            "Anthropic" => ModelBackend::Anthropic,
            "OpenAI" => ModelBackend::OpenAI,
            "DeepSeek" => ModelBackend::DeepSeek,
            "Ollama" => ModelBackend::Ollama,
            "Custom" => ModelBackend::Custom,
            _ => ModelBackend::LmStudio,
        };
    }
}

// ============================================================================
// manage_team
// ============================================================================

pub struct ManageTeamTool {
    pub memory_store: Arc<RwLock<Option<Box<dyn MemoryStore>>>>,
    pub cache: Arc<RwLock<HashMap<String, loom_types::TeamConfig>>>,
}

#[async_trait]
impl AgentTool for ManageTeamTool {
    fn tool_name(&self) -> &str {
        "manage_team"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "manage_team".into(),
            description: "Manage expert teams (专家团). Use when user wants to create/delete a team, add members, remove members, change strategy, or list teams.\n\nCommon: 给XX团队添加成员 -> action=add_members, id + members. 从XX团队移除YY -> action=remove_member, id + member_name. 创建团队 -> action=create, name + members. 列出团队 -> action=list.\n\nActions: list | get | create | update | delete | add_members | remove_member.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "get", "create", "update", "delete", "add_members", "remove_member"] },
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "strategy": { "type": "string", "enum": ["synthesize", "debate"] },
                    "captain_model": { "type": "string" },
                    "captain_persona": { "type": "string" },
                    "member_name": { "type": "string", "description": "Name of member to remove (remove_member only)" },
                    "members": { "type": "array" }
                },
                "required": ["action"]
            }),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        _ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let action = arguments["action"].as_str().unwrap_or("");
        let guard = self.memory_store.read().await;
        let ms = resolve_ms(&guard)?;
        let result = exec_team(action, &arguments, ms, &self.cache).await?;
        Ok(ToolResult {
            content: result,
            is_error: false,
            structured_content: None,
        })
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

async fn exec_team(
    action: &str,
    args: &serde_json::Value,
    ms: &dyn MemoryStore,
    cache: &Arc<RwLock<HashMap<String, loom_types::TeamConfig>>>,
) -> Result<String> {
    match action {
        "list" => {
            let configs = ms.list_team_configs().await?;
            Ok(serde_json::to_string_pretty(&configs).unwrap_or_else(|e| e.to_string()))
        }
        "get" => {
            let id = req_str(args, "id")?;
            match ms.get_team_config(id).await? {
                Some(c) => Ok(serde_json::to_string_pretty(&c).unwrap_or_else(|e| e.to_string())),
                None => Ok(format!("Team \"{id}\" not found.")),
            }
        }
        "create" => {
            let cfg = build_team(args)?;
            let name = cfg.name.clone();
            ms.save_team_config(&cfg).await?;
            cache.write().await.insert(cfg.id.clone(), cfg.clone());
            Ok(format!("Team \"{name}\" created (id: {}).", cfg.id))
        }
        "update" => {
            let id = req_str(args, "id")?;
            let mut cfg = ms
                .get_team_config(id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Team \"{id}\" not found"))?;
            patch_team(&mut cfg, args);
            ms.save_team_config(&cfg).await?;
            cache.write().await.insert(id.to_string(), cfg.clone());
            Ok(format!("Team \"{}\" updated.", cfg.name))
        }
        "delete" => {
            let id = req_str(args, "id")?;
            ms.delete_team_config(id).await?;
            cache.write().await.remove(id);
            Ok(format!("Team \"{id}\" deleted."))
        }
        "add_members" => {
            let id = req_str(args, "id")?;
            let mut cfg = ms.get_team_config(id).await?
                .ok_or_else(|| anyhow::anyhow!("Team \"{id}\" not found"))?;
            if let Some(arr) = args["members"].as_array() {
                let new = parse_members(arr);
                cfg.members.extend(new);
            }
            ms.save_team_config(&cfg).await?;
            cache.write().await.insert(id.to_string(), cfg.clone());
            Ok(format!("Team \"{}\" updated ({} members).", cfg.name, cfg.members.len()))
        }
        "remove_member" => {
            let id = req_str(args, "id")?;
            let mname = req_str(args, "member_name")?;
            let mut cfg = ms.get_team_config(id).await?
                .ok_or_else(|| anyhow::anyhow!("Team \"{id}\" not found"))?;
            let before = cfg.members.len();
            cfg.members.retain(|m| m.name != mname);
            let removed = before - cfg.members.len();
            ms.save_team_config(&cfg).await?;
            cache.write().await.insert(id.to_string(), cfg.clone());
            Ok(format!("Removed {} member(s) from team \"{}.", removed, cfg.name))
        }
        _ => Err(anyhow::anyhow!("Unknown action: {action}.")),
    }
}

fn build_team(v: &serde_json::Value) -> Result<loom_types::TeamConfig> {
    use loom_types::config::team::{TeamStrategy, TeamCaptain};
    let id = v["id"]
        .as_str()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    Ok(loom_types::TeamConfig {
        id,
        name: v["name"].as_str().unwrap_or("Unnamed Team").to_string(),
        description: v["description"].as_str().unwrap_or("").to_string(),
        strategy: match v["strategy"].as_str().unwrap_or("synthesize") {
            "debate" => TeamStrategy::Debate,
            _ => TeamStrategy::Synthesize,
        },
        captain: TeamCaptain {
            model: v["captain_model"].as_str().map(|s| s.to_string()),
            system_prompt_override: v["captain_persona"].as_str().map(|s| s.to_string()),
        },
        members: v["members"].as_array().map(parse_members).unwrap_or_default(),
    })
}

fn patch_team(cfg: &mut loom_types::TeamConfig, v: &serde_json::Value) {
    use loom_types::config::team::TeamStrategy;
    if let Some(s) = v["name"].as_str() { cfg.name = s.to_string(); }
    if let Some(s) = v["description"].as_str() { cfg.description = s.to_string(); }
    if let Some(s) = v["strategy"].as_str() {
        cfg.strategy = if s == "debate" { TeamStrategy::Debate } else { TeamStrategy::Synthesize };
    }
    if let Some(s) = v["captain_model"].as_str() { cfg.captain.model = Some(s.to_string()); }
    if let Some(s) = v["captain_persona"].as_str() { cfg.captain.system_prompt_override = Some(s.to_string()); }
    if let Some(arr) = v["members"].as_array() { cfg.members = parse_members(arr); }
}

fn parse_members(arr: &Vec<serde_json::Value>) -> Vec<loom_types::config::team::TeamMember> {
    use loom_types::config::team::{TeamMember, MemberSource};
    arr.iter()
        .map(|item| {
            let name = item["name"].as_str().unwrap_or("member").to_string();
            let source = if let Some(agent) = item["source"]["AgentRef"].as_str() {
                MemberSource::AgentRef(agent.to_string())
            } else {
                MemberSource::Inline {
                    persona: item["source"]["Inline"]["persona"].as_str().unwrap_or("").to_string(),
                    model: item["source"]["Inline"]["model"].as_str().map(|s| s.to_string()),
                    temperature: item["source"]["Inline"]["temperature"].as_f64().map(|v| v as f32),
                }
            };
            TeamMember { name, source }
        })
        .collect()
}

// Helpers

fn req_str<'a>(args: &'a serde_json::Value, field: &str) -> Result<&'a str> {
    args[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("{field} required"))
}

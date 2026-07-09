# 专家团（Expert Team）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 openloom 现有 Agent 体系上叠加专家团编排层，支持合成/辩论两种协作策略。

**Architecture:** 后端复用 AgentPool + cc_dispatch 子 Agent 派发机制。团长 Agent 带着团队 system prompt 处理用户消息，通过 spawn_agent 工具并行启动成员，收集结果后综合输出。团队配置存储复用现有 SQLite 的 agent_configs 表模式，新增 team_configs 表。

**Tech Stack:** Rust (loom-types/loom-memory/loom-core/loom-server), TypeScript + React + Zustand (frontend renderer)

## Global Constraints

- 团队配置与 AgentConfig 同级存储，复用现有 SQLite
- 成员支持两种来源：Inline（内联定义）和 AgentRef（引用已有 Agent config name）
- 团长和成员都走现有 agent_loop + cc_dispatch 派发机制
- 成员并行执行，不可直接通信
- 前端 Zustand store 模式与现有 agent.ts 保持一致
- UI 组件复用现有 Select、AgentConfigPanel 等共享组件样式
- 所有用户可见文案走 i18n（zh-CN / en-US）

---

## 文件结构总览

```
新增:
  backend/crates/loom-types/src/config/team.rs       # TeamConfig 类型
  backend/crates/loom-core/src/team_orchestrator.rs   # 团队编排核心
  backend/crates/loom-server/src/dispatch/team.rs     # team.config.* RPC
  frontend/src/renderer/src/stores/team.ts            # Zustand team slice
  frontend/src/renderer/src/components/input/EntitySelector.tsx  # 合并选择器
  frontend/src/renderer/src/components/input/EntitySelector.module.css
  frontend/src/renderer/src/components/settings/TeamTab.tsx       # 设置页 Tab
  frontend/src/renderer/src/components/chat/TeamCard.tsx          # 聊天区卡片

修改:
  backend/crates/loom-types/src/config/mod.rs   # 导出 team 模块
  backend/crates/loom-types/src/lib.rs          # 重导出 TeamConfig 等类型
  backend/crates/loom-core/src/lib.rs           # 导出 team_orchestrator
  backend/crates/loom-core/src/event_bus.rs      # 新增 Team* 事件
  backend/crates/loom-core/src/orchestrator.rs   # 团队配置缓存 + process_message_with_team
  backend/crates/loom-memory/src/store.rs        # team_configs 表 CRUD
  backend/crates/loom-server/src/dispatch/mod.rs # 注册 team 路由
  backend/crates/loom-server/src/dispatch/chat.rs# chat.send 支持 team_config_id
  backend/crates/loom-server/src/dispatch/session.rs # 绑定团队
  frontend/src/renderer/src/stores/index.ts      # 注册 team slice
  frontend/src/renderer/src/components/input/AgentSelector.tsx  # 废弃（替换为 EntitySelector）
  frontend/src/renderer/src/components/settings/SettingsPage.tsx # 新增专家团 Tab
  frontend/src/renderer/src/i18n/zh-CN.ts        # 新增团队文案
  frontend/src/renderer/src/i18n/en-US.ts        # 新增团队文案
```

---

### Task 1: TeamConfig 类型定义

**Files:**
- Create: `backend/crates/loom-types/src/config/team.rs`
- Modify: `backend/crates/loom-types/src/config/mod.rs`
- Modify: `backend/crates/loom-types/src/lib.rs`

**Interfaces:**
- Produces: `TeamStrategy`, `TeamCaptain`, `TeamMember`, `MemberSource`, `TeamConfig` — 供所有后续任务使用

- [ ] **Step 1: 创建 `config/team.rs`**

```rust
// backend/crates/loom-types/src/config/team.rs
//! 专家团（Expert Team）配置类型。

use serde::{Deserialize, Serialize};

/// 协作策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TeamStrategy {
    /// 合成模式：各成员并行回答 → 团长综合结论
    #[default]
    Synthesize,
    /// 辩论模式：两轮互相质疑后综合结论
    Debate,
}

/// 团长配置（负责汇总/协调）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeamCaptain {
    /// 团长模型，None 表示跟随全局
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 团长 system prompt 覆写
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_override: Option<String>,
}

/// 成员来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MemberSource {
    /// 内联定义：直接在团队表单中填写配置
    Inline {
        persona: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
    },
    /// 引用已有 Agent 的 config name
    AgentRef(String),
}

/// 团队成员
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub name: String,
    pub source: MemberSource,
}

/// 专家团配置，与 AgentConfig 同级存储。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    /// 唯一标识（UUID v7）
    pub id: String,
    /// 团队名称，如"代码审查团"
    pub name: String,
    /// 用途说明
    #[serde(default)]
    pub description: String,
    /// 协作策略
    #[serde(default)]
    pub strategy: TeamStrategy,
    /// 团长配置
    #[serde(default)]
    pub captain: TeamCaptain,
    /// 成员列表
    #[serde(default)]
    pub members: Vec<TeamMember>,
}
```

- [ ] **Step 2: 从 `config/mod.rs` 导出 team 模块**

```rust
// backend/crates/loom-types/src/config/mod.rs — 末尾追加
pub mod team;
```

- [ ] **Step 3: 从 `lib.rs` 重导出 team 类型**

```rust
// backend/crates/loom-types/src/lib.rs — 在现有 config 重导出区域追加
pub use config::team::{MemberSource, TeamCaptain, TeamConfig, TeamMember, TeamStrategy};
```

- [ ] **Step 4: 编译验证**

```bash
cargo check -p loom-types
```

- [ ] **Step 5: Commit**

```bash
git add backend/crates/loom-types/src/config/team.rs backend/crates/loom-types/src/config/mod.rs backend/crates/loom-types/src/lib.rs
git commit -m "feat(loom-types): add TeamConfig types for expert team"
```

---

### Task 2: team_configs 表 SQLite CRUD

**Files:**
- Modify: `backend/crates/loom-memory/src/store.rs`

**Interfaces:**
- Consumes: `TeamConfig` from Task 1
- Produces: `save_team_config()`, `get_team_config()`, `list_team_configs()`, `delete_team_config()` — 供 Task 3 Orchestrator 调用

- [ ] **Step 1: 在 store 初始化方法中添加建表 SQL**

找到现有 `agent_configs` 建表位置，在其下方添加：

```rust
conn.execute_batch("
    CREATE TABLE IF NOT EXISTS team_configs (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        config_json TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
")?;
```

- [ ] **Step 2: 添加 CRUD 方法**

在 `impl` 块中 `agent_configs` 方法附近添加：

```rust
/// 保存团队配置（INSERT OR REPLACE 语义）
pub async fn save_team_config(&self, config: &loom_types::TeamConfig) -> Result<()> {
    let config_json = serde_json::to_string(config)?;
    let conn = self.conn.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO team_configs (id, name, config_json, updated_at)
         VALUES (?1, ?2, ?3, datetime('now'))",
        params![config.id, config.name, config_json],
    )?;
    Ok(())
}

/// 获取单个团队配置
pub async fn get_team_config(&self, id: &str) -> Result<Option<loom_types::TeamConfig>> {
    let conn = self.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT config_json FROM team_configs WHERE id = ?1"
    )?;
    let result: Option<String> = stmt
        .query_row(params![id], |row| row.get(0))
        .optional()?;
    match result {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

/// 列出所有团队配置
pub async fn list_team_configs(&self) -> Result<Vec<loom_types::TeamConfig>> {
    let conn = self.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT config_json FROM team_configs ORDER BY name"
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut configs = Vec::new();
    for row in rows {
        configs.push(serde_json::from_str(&row?)?);
    }
    Ok(configs)
}

/// 删除团队配置
pub async fn delete_team_config(&self, id: &str) -> Result<()> {
    let conn = self.conn.lock().unwrap();
    conn.execute("DELETE FROM team_configs WHERE id = ?1", params![id])?;
    Ok(())
}
```

注意：确认 `use rusqlite::OptionalExtension` 已在文件顶部。如未导入则添加。

- [ ] **Step 3: 编译验证**

```bash
cargo check -p loom-memory
```

- [ ] **Step 4: Commit**

---

### Task 3: EventBus 扩展 + Orchestrator 团队方法

**Files:**
- Create: `backend/crates/loom-core/src/team_orchestrator.rs`
- Modify: `backend/crates/loom-core/src/event_bus.rs`
- Modify: `backend/crates/loom-core/src/orchestrator.rs`
- Modify: `backend/crates/loom-core/src/lib.rs`

**Interfaces:**
- Consumes: `TeamConfig` from Task 1, store CRUD from Task 2
- Produces: `TeamStarted/TeamCompleted` 事件, `process_message_with_team()`, `team_config_*()` CRUD — 供 Task 4 RPC

- [ ] **Step 1: 新增 EventBus 事件**

在 `backend/crates/loom-core/src/event_bus.rs` 的 `AgentEvent` enum 末尾添加：

```rust
    /// 团队启动
    TeamStarted {
        team_id: String,
        team_name: String,
        captain_id: AgentId,
        member_ids: Vec<AgentId>,
    },
    /// 团队成员完成一轮
    TeamMemberDone {
        team_id: String,
        member_id: AgentId,
        member_name: String,
        round: usize,
    },
    /// 团队一轮完成
    TeamRoundComplete {
        team_id: String,
        round: usize,
    },
    /// 团队执行完毕
    TeamCompleted {
        team_id: String,
        session_id: String,
        summary: String,
    },
```

- [ ] **Step 2: 创建 `team_orchestrator.rs`**

```rust
// backend/crates/loom-core/src/team_orchestrator.rs
//! 团队编排核心 — 构造团长 system prompt，驱协团队执行流程。

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
```

- [ ] **Step 3: 在 Orchestrator 中添加团队配置缓存和 CRUD**

在 `Orchestrator` struct 的 `agent_configs` 字段下方添加：

```rust
    team_configs: Arc<RwLock<std::collections::HashMap<String, loom_types::TeamConfig>>>,
```

在构造方法中初始化：

```rust
    team_configs: Arc::new(RwLock::new(HashMap::new())),
```

在 `impl Orchestrator` 块中添加方法（放在 `agent_config_*` 方法附近）：

```rust
    pub async fn team_config_list(&self) -> Vec<loom_types::TeamConfig> {
        self.team_configs.read().await.values().cloned().collect()
    }

    pub async fn team_config_get(&self, id: &str) -> Result<loom_types::TeamConfig> {
        {
            let cache = self.team_configs.read().await;
            if let Some(cfg) = cache.get(id).cloned() {
                return Ok(cfg);
            }
        }
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store
            && let Some(cfg) = s.get_team_config(id).await?
        {
            self.team_configs.write().await.insert(id.to_string(), cfg.clone());
            return Ok(cfg);
        }
        anyhow::bail!("team config '{}' not found", id)
    }

    pub async fn team_config_create(&self, config: loom_types::TeamConfig) -> Result<()> {
        let id = config.id.clone();
        {
            let cache = self.team_configs.read().await;
            if cache.contains_key(&id) {
                anyhow::bail!("team config '{}' already exists", id);
            }
        }
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            if s.get_team_config(&id).await?.is_some() {
                anyhow::bail!("team config '{}' already exists", id);
            }
            s.save_team_config(&config).await?;
        }
        self.team_configs.write().await.insert(id, config);
        Ok(())
    }

    pub async fn team_config_update(&self, config: loom_types::TeamConfig) -> Result<()> {
        let id = config.id.clone();
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.save_team_config(&config).await?;
        }
        self.team_configs.write().await.insert(id, config);
        Ok(())
    }

    pub async fn team_config_delete(&self, id: &str) -> Result<()> {
        self.team_configs.write().await.remove(id);
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.delete_team_config(id).await?;
        }
        Ok(())
    }

    pub async fn load_team_configs(&self) -> Result<()> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            let configs = s.list_team_configs().await?;
            let mut cache = self.team_configs.write().await;
            for cfg in configs {
                cache.insert(cfg.id.clone(), cfg);
            }
        }
        Ok(())
    }

    pub async fn process_message_with_team(
        &self,
        user_message: &str,
        session_id: &str,
        team_config_id: &str,
        thinking_budget: Option<usize>,
        attached_images: Vec<ContentPart>,
        selected_skills: Vec<String>,
        permission_mode: &str,
        skip_user_message: bool,
    ) -> Result<TurnResult> {
        let team = self.team_config_get(team_config_id).await?;
        let existing_agents = self.agent_config_list().await;
        let member_configs = crate::team_orchestrator::resolve_member_configs(&team, &existing_agents);

        if member_configs.is_empty() {
            anyhow::bail!("team '{}' has no valid members", team.name);
        }

        for (config_name, config) in &member_configs {
            if config_name.starts_with("__team_") {
                if self.agent_config_get(config_name).await.is_err() {
                    let _ = self.agent_config_create(config.clone()).await;
                }
            }
        }

        let member_info: Vec<(String, String, Option<String>)> = member_configs
            .iter()
            .map(|(name, cfg)| (name.clone(), cfg.persona.clone(), cfg.model.clone()))
            .collect();
        let captain_system_prompt =
            crate::team_orchestrator::build_captain_system_prompt(&team, &member_info);

        let default_model = self.active_model_name().await;
        let captain_config = crate::team_orchestrator::build_captain_config(
            &team,
            captain_system_prompt,
            default_model,
        );

        let member_ids: Vec<loom_types::AgentId> = member_configs
            .iter()
            .map(|_| loom_types::AgentId::new())
            .collect();
        self.pool.event_bus().publish(crate::event_bus::AgentEvent::TeamStarted {
            team_id: team.id.clone(),
            team_name: team.name.clone(),
            captain_id: loom_types::AgentId::new(),
            member_ids: member_ids.clone(),
        });

        let result = self
            .process_message_with_config(
                user_message,
                session_id,
                &captain_config,
                thinking_budget,
                attached_images,
                selected_skills,
                permission_mode,
                skip_user_message,
            )
            .await;

        for (config_name, _) in &member_configs {
            if config_name.starts_with("__team_") {
                let _ = self.agent_config_delete(config_name).await;
            }
        }

        let summary = result.as_ref().map(|r| r.response.clone()).unwrap_or_default();
        self.pool.event_bus().publish(crate::event_bus::AgentEvent::TeamCompleted {
            team_id: team.id.clone(),
            session_id: session_id.to_string(),
            summary: summary.clone(),
        });

        result
    }
```

- [ ] **Step 4: 从 `lib.rs` 导出**

```rust
pub mod team_orchestrator;
```

- [ ] **Step 5: 启动时加载团队配置**

找到 `Orchestrator` 初始化位置（server 初始化代码中），在 `load_agent_configs` 调用附近添加 `load_team_configs()`。

- [ ] **Step 6: 编译验证**

```bash
cargo check -p loom-core
```

- [ ] **Step 7: Commit**

---

### Task 4: team RPC handlers + chat.send/session 扩展

**Files:**
- Create: `backend/crates/loom-server/src/dispatch/team.rs`
- Modify: `backend/crates/loom-server/src/dispatch/mod.rs`
- Modify: `backend/crates/loom-server/src/dispatch/chat.rs`
- Modify: `backend/crates/loom-server/src/dispatch/session.rs`

**Interfaces:**
- Consumes: Task 3 Orchestrator 方法
- Produces: `team.config.*` RPC 端点, `chat.send` 支持 `team_config_id`, `session.bind_team` 端点

- [ ] **Step 1: 创建 `dispatch/team.rs`**

```rust
// backend/crates/loom-server/src/dispatch/team.rs
//! Team dispatch handlers — team.config.*

use loom_types::config::team::TeamConfig;
use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};
use uuid::Uuid;

use super::err;
use crate::AppState;

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "team.config.list" => Some(handle_team_config_list(state).await),
        "team.config.get" => Some(handle_team_config_get(state, p).await),
        "team.config.create" => Some(handle_team_config_create(state, p).await),
        "team.config.update" => Some(handle_team_config_update(state, p).await),
        "team.config.delete" => Some(handle_team_config_delete(state, p).await),
        _ => None,
    }
}

async fn handle_team_config_list(state: &AppState) -> Result<Value, JsonRpcError> {
    let configs = state.orchestrator.team_config_list().await;
    Ok(json!({ "teams": configs }))
}

async fn handle_team_config_get(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "id required"));
    }
    let config = state.orchestrator.team_config_get(id).await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(serde_json::to_value(config).unwrap_or_default())
}

async fn handle_team_config_create(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let mut config: TeamConfig = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    if config.name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    if config.id.is_empty() {
        config.id = Uuid::now_v7().to_string();
    }
    state.orchestrator.team_config_create(config.clone()).await
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    Ok(json!({ "id": config.id }))
}

async fn handle_team_config_update(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let config: TeamConfig = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    if config.id.is_empty() || config.name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "id and name required"));
    }
    state.orchestrator.team_config_update(config).await
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

async fn handle_team_config_delete(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "id required"));
    }
    state.orchestrator.team_config_delete(id).await
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}
```

- [ ] **Step 2: 注册 team 路由**

在 `dispatch/mod.rs` 中添加模块声明和路由：

```rust
// mod 声明
mod team;

// dispatch_method 中添加（在现有 handler 链中）
if let Some(result) = team::handle(state, method, &p).await {
    return result;
}
```

- [ ] **Step 3: chat.send 支持 team_config_id**

修改 `chat.rs` 的 `handle_chat_send`：在 `config_name` 解析之前，先检查 `team_config_id`。如果存在则走 `process_message_with_team`，跳过单 Agent 流程。

- [ ] **Step 4: session.bind_team 端点**

修改 `session.rs`：
- `SessionData` 添加 `team_config_id: Option<String>` 字段
- `SessionStore` 添加 `bind_team` / `get_bound_team` 方法
- 新增 `handle_session_bind_team` handler
- 绑定团队时清空 agent 绑定（互斥）
- 所有 `SessionData` 构造处添加 `team_config_id: None`

- [ ] **Step 5: 编译验证**

```bash
cargo check -p loom-server
```

- [ ] **Step 6: Commit**

---

### Task 5: 前端 Zustand Team Slice + EntitySelector

**Files:**
- Create: `frontend/src/renderer/src/stores/team.ts`
- Create: `frontend/src/renderer/src/components/input/EntitySelector.tsx`
- Create: `frontend/src/renderer/src/components/input/EntitySelector.module.css`
- Modify: `frontend/src/renderer/src/stores/index.ts`
- Modify: `frontend/src/renderer/src/i18n/zh-CN.ts`
- Modify: `frontend/src/renderer/src/i18n/en-US.ts`

**Interfaces:**
- Consumes: Task 4 RPC 端点
- Produces: `TeamSlice` Zustand state, `EntitySelector` 组件 — 供 Task 6/7 使用

- [ ] **Step 1: 创建 `stores/team.ts`**

```typescript
// frontend/src/renderer/src/stores/team.ts
import { StateCreator } from 'zustand'

export interface TeamConfig {
  id: string
  name: string
  description: string
  strategy: 'synthesize' | 'debate'
  captain: { model?: string; system_prompt_override?: string }
  members: TeamMember[]
}

export type TeamMember =
  | { name: string; source: { persona: string; model?: string; temperature?: number } }
  | { name: string; source: string }

export interface TeamSlice {
  teams: TeamConfig[]
  currentTeamId: string | null
  sessionTeamBindings: Record<string, string>
  setTeams: (teams: TeamConfig[]) => void
  setCurrentTeamId: (id: string | null) => void
  setSessionTeamBinding: (sessionId: string, teamId: string) => void
  getSessionTeam: (sessionId: string) => TeamConfig | undefined
}

export const createTeamSlice: StateCreator<TeamSlice> = (set, get) => ({
  teams: [],
  currentTeamId: null,
  sessionTeamBindings: {},

  setTeams: (teams) => set({ teams }),
  setCurrentTeamId: (currentTeamId) => set({ currentTeamId }),

  setSessionTeamBinding: (sessionId, teamId) => {
    const next = { ...get().sessionTeamBindings }
    if (!teamId) {
      delete next[sessionId]
    } else {
      next[sessionId] = teamId
    }
    set({ sessionTeamBindings: next })
  },

  getSessionTeam: (sessionId) => {
    const state = get()
    const id = state.sessionTeamBindings[sessionId]
    if (id) return state.teams.find((t) => t.id === id)
    return undefined
  },
})
```

- [ ] **Step 2: 注册到 Zustand store**

修改 `stores/index.ts`：导入 `createTeamSlice` + `TeamSlice`，加入 `AppStore` 类型和 `create` 调用。

- [ ] **Step 3: 创建 EntitySelector 组件**

```typescript
// frontend/src/renderer/src/components/input/EntitySelector.tsx
// 两个 Tab（Agent / 专家团）的 Popover 选择器
// 选中互斥：选 Agent 清空团队，选团队切到 default Agent
// 底部入口跳设置页
```

组件结构：trigger 按钮 → 点击弹出 Popover → 两个 Tab 切换 → 各自列表 → 底部管理入口。实现细节参考设计文档第 5.1 节。

- [ ] **Step 4: 替换 AgentSelector 引用**

在输入区组件中将 `<AgentSelector />` 替换为 `<EntitySelector />`。

- [ ] **Step 5: 添加 i18n 文案**

zh-CN / en-US 新增约 30 条团队相关文案键。

- [ ] **Step 6: 类型检查**

```bash
cd frontend && npx tsc --noEmit
```

- [ ] **Step 7: Commit**

---

### Task 6: 前端 TeamTab（设置页）

**Files:**
- Create: `frontend/src/renderer/src/components/settings/TeamTab.tsx`
- Modify: `frontend/src/renderer/src/components/settings/SettingsPage.tsx`

**Interfaces:**
- Consumes: TeamSlice from Task 5, `team.config.*` RPC
- Produces: 设置页专家团 Tab UI

- [ ] **Step 1: 创建 TeamTab.tsx**

参考 `AgentConfigPanel.tsx` 结构实现：
- 团队列表（卡片，显示名称 + 描述 + 策略标签 + 成员数）
- 创建/编辑表单：名称、描述、策略下拉、团长模型、成员编辑器
- 成员支持两种添加方式："从 Agent 添加"下拉 + "自定义成员"内联表单
- 删除确认 Modal

- [ ] **Step 2: 集成到 SettingsPage**

在助手组新增「专家团」Tab 项，Tab 内容渲染 `<TeamTab />`。

- [ ] **Step 3: 类型检查**

```bash
cd frontend && npx tsc --noEmit
```

- [ ] **Step 4: Commit**

---

### Task 7: 前端 TeamCard（聊天区） + 端到端集成

**Files:**
- Create: `frontend/src/renderer/src/components/chat/TeamCard.tsx`
- Modify: 聊天消息渲染组件

**Interfaces:**
- Consumes: WebSocket 事件流中的 TeamStarted/TeamMemberDone/TeamCompleted
- Produces: 聊天区团队执行状态卡片

- [ ] **Step 1: 创建 TeamCard.tsx**

显示团队名称 + 各成员的状态（running/done/errored）+ 简要结果摘要。复用 SubagentCard 样式。

- [ ] **Step 2: 集成到聊天消息流**

在消息渲染组件中，用户消息和 AI 回复之间插入 TeamCard。通过 streaming/chat store 跟踪当前活跃团队的执行状态。

- [ ] **Step 3: 端到端测试**

1. 启动后端 `cargo run -p loom-cli -- serve --port 8080`
2. 启动前端 `cd frontend && npm run dev`
3. 创建团队 → 选中团队 → 发送消息
4. 验证合成模式：所有成员并行，团长输出综合结论
5. 验证辩论模式：两轮辩论后团长输出结论
6. 验证停止功能
7. 验证团队/Agent 互斥切换

- [ ] **Step 4: Commit**

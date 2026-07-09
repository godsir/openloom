# 专家团（Expert Team）设计文档

## 1. 概述

### 1.1 背景

openloom 现有 Agent 体系支持创建多个 Agent（配置不同 persona/model/tools），但一次对话只能绑定一个 Agent。当用户需要多个专业视角分析同一问题时，必须手动切换 Agent 分别提问，效率低且无法产生协作效应。

专家团在现有 Agent 体系之上叠加一层编排抽象：用户创建团队后，一键启动多 Agent 协作——各成员并行执行，团长负责汇总、辩论、综合，输出比单 Agent 更高质量的结论。

### 1.2 目标

1. 用户可创建和管理专家团配置（name、成员、策略）
2. 支持两种协作策略：合成模式（Synthesize）和辩论模式（Debate）
3. 成员支持两种创建方式：内联定义字段、引用已有 Agent
4. 输入区整合 Agent 选择和团队选择为统一入口
5. 聊天区展示团队成员执行卡片和团长汇总结果

---

## 2. 数据模型

### 2.1 TeamConfig（后端）

```rust
/// 专家团配置，与 AgentConfig 同级存储。
pub struct TeamConfig {
    /// 唯一标识
    pub id: String,
    /// 团队名称，如"代码审查团"
    pub name: String,
    /// 用途说明
    #[serde(default)]
    pub description: String,
    /// 协作策略
    #[serde(default)]
    pub strategy: TeamStrategy,
    /// 团长配置（负责汇总/协调）
    #[serde(default)]
    pub captain: TeamCaptain,
    /// 成员列表
    #[serde(default)]
    pub members: Vec<TeamMember>,
}

/// 协作策略
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum TeamStrategy {
    /// 合成模式：各成员并行回答 → 团长阅读所有回答后综合结论
    #[default]
    Synthesize,
    /// 辩论模式：两轮互相质疑后综合结论
    Debate,
}

/// 团长配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeamCaptain {
    /// 团长模型，默认跟随全局
    pub model: Option<String>,
    /// 团长 system prompt 覆写
    pub system_prompt_override: Option<String>,
}

/// 团队成员
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub name: String,
    pub source: MemberSource,
}

/// 成员来源：内联定义 或 引用已有 Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MemberSource {
    /// 内联定义：直接在团队表单中填写配置
    Inline {
        persona: String,
        model: Option<String>,
        temperature: Option<f32>,
    },
    /// 引用已有 Agent 的 config name
    AgentRef(String),
}
```

### 2.2 前端类型映射

```typescript
interface TeamConfig {
  id: string;
  name: string;
  description: string;
  strategy: 'synthesize' | 'debate';
  captain: { model?: string; system_prompt_override?: string };
  members: TeamMember[];
}

type TeamMember =
  | { name: string; source: { persona: string; model?: string; temperature?: number } }
  | { name: string; source: string }; // AgentRef 为已有 Agent 的 name
```

---

## 3. 编排流程

### 3.1 合成模式（Synthesize）

```
用户消息
  → 团长收到消息（system prompt 中注入成员列表 + 策略指令）
  → 团长调用 spawn 工具并行启动所有成员
  → 各成员独立执行 agent loop（每人带自己的 persona/model）
  → 团长收集所有成员结果
  → 团长输出综合结论
```

### 3.2 辩论模式（Debate）

```
用户消息
  → 团长 spawn 所有成员（同步模式）
  → 第一轮：各成员独立回答
  → 团长打包所有回答，发送给每个成员：
    "以下是其他专家的意见，请审视你的结论，指出同意/不同意的点，修正或坚持观点"
  → 第二轮：各成员修正/坚持
  → 团长收集两轮结果 + 分歧点，输出综合结论
```

### 3.3 通用约束

- 所有成员并行执行（同一轮内）
- 成员之间不可直接通信，仅通过团长中转
- 每轮受 max_iterations / timeout 限制
- 团长自身也可作为成员参与回答（可配置）
- 所有成员事件通过 EventBus 推送到前端

---

## 4. 后端 API

### 4.1 JSON-RPC 方法

| 方法 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| `team.config.list` | — | `TeamConfig[]` | 列出所有团队 |
| `team.config.get` | `{ id: string }` | `TeamConfig` | 获取单个团队 |
| `team.config.create` | `TeamConfig`（不含 id） | `{ id: string }` | 创建团队 |
| `team.config.update` | `TeamConfig` | `{ success: bool }` | 更新团队 |
| `team.config.delete` | `{ id: string }` | `{ success: bool }` | 删除团队 |

### 4.2 chat.send 扩展

```json
{
  "content": "审查这段代码",
  "agent_config_name": "default",
  "team_config_id": "code-review-team"
}
```

当 `team_config_id` 存在时，后端进入团队编排流程而非单 Agent 流程。`agent_config_name` + `team_config_id` 互斥：团队优先。

### 4.3 session.bind_team（新增）

```json
{
  "session_id": "xxx",
  "team_config_id": "code-review-team"
}
```

与会话绑定团队，后续消息自动使用该团队。`bind_agent` 和 `bind_team` 互斥：绑定团队时清空 agent 绑定，反之亦然。

### 4.4 新增后端模块

| 模块 | 位置 | 职责 |
|------|------|------|
| `team_orchestrator.rs` | `loom-core` | 团队编排核心：读配置 → 启动团长 → spawn 成员 → 收集结果 → 综合输出 |
| `team_store.rs` | `loom-memory` | 团队配置 CRUD，复用现有 SQLite |

### 4.5 EventBus 扩展

```rust
pub enum AgentEvent {
    // ... 现有事件 ...

    /// 团队启动
    TeamStarted { team_id: String, team_name: String, members: Vec<AgentId> },
    /// 成员完成一轮
    TeamMemberDone { team_id: String, member_id: AgentId, round: usize },
    /// 一轮辩论/合成完成
    TeamRoundComplete { team_id: String, round: usize },
    /// 团队执行完毕，团长给出综合结论
    TeamCompleted { team_id: String, session_id: String, summary: String },
}
```

---

## 5. 前端

### 5.1 输入区：合并选择器

现有 Agent 选择器替换为一个统一入口。点击弹出 Popover，内含两个 Tab：

```
┌─────────────────────────────┐
│  [Agent]       [专家团]      │  ← Tab 切换
├─────────────────────────────┤
│  default            ✓       │
│  架构师                      │  ← 列表内容随 Tab 变化
│  代码审查助手                 │
├─────────────────────────────┤
│  管理                      │  ← 底部入口跳设置页
└─────────────────────────────┘
```

- 选 Agent 时清空团队选择，选团队时切到默认 Agent（互斥）
- 当前选中项高亮打勾
- 底部点击跳转设置页对应 Tab

### 5.2 设置页：新增专家团 Tab

设置导航"助手"组新增「专家团」项，位于 Agent 之后：

```
┌─ 助手 ──────────────────────┐
│  Agent                      │
│  专家团          ← 新增       │
│  Loom.md                    │
│  模型                        │
│  ...                        │
└─────────────────────────────┘
```

Tab 内容：

- 团队列表（卡片形式）
- 创建/编辑表单：
  - 团队名称、描述
  - 策略选择（合成/辩论）
  - 团长模型选择
  - 成员列表：每个成员支持两种来源 ——
    - "从 Agent 添加"：下拉选择已有 Agent
    - "自定义成员"：内联填写 name + persona + model
- 删除确认

### 5.3 聊天区：团队执行展示

启动团队对话后，消息流展示：

1. 用户消息（正常展示）
2. 团队成员卡片列表（复用 `SubagentCard` 样式），显示：
   - 每个成员的名字
   - 运行状态（running/done/errored）
   - 简要结果摘要
3. 团长汇总结果 — 作为最终 AI 回复呈现

### 5.4 Zustand Store 扩展

```typescript
interface TeamSlice {
  teams: TeamConfig[];
  currentTeamId: string | null;
  sessionTeamBindings: Record<string, string>;
  setTeams: (teams: TeamConfig[]) => void;
  setCurrentTeamId: (id: string | null) => void;
  setSessionTeamBinding: (sessionId: string, teamId: string) => void;
  getSessionTeam: (sessionId: string) => TeamConfig | undefined;
}
```

---

## 6. 涉及文件

### 新增文件

| 文件 | 说明 |
|------|------|
| `backend/crates/loom-core/src/team_orchestrator.rs` | 团队编排核心逻辑 |
| `backend/crates/loom-memory/src/team_store.rs` | 团队配置 CRUD |
| `backend/crates/loom-types/src/team.rs` | TeamConfig / TeamStrategy 等类型定义 |
| `backend/crates/loom-server/src/dispatch/team.rs` | team.config.* RPC handlers |
| `frontend/src/renderer/src/stores/team.ts` | 前端 Team Zustand slice |
| `frontend/src/renderer/src/components/settings/TeamTab.tsx` | 设置页专家团 Tab |
| `frontend/src/renderer/src/components/input/EntitySelector.tsx` | 合并后的 Agent/团队选择器 |
| `frontend/src/renderer/src/components/chat/TeamCard.tsx` | 聊天区团队成员卡片 |

### 修改文件

| 文件 | 变更 |
|------|------|
| `backend/crates/loom-core/src/lib.rs` | 导出 `team_orchestrator` |
| `backend/crates/loom-core/src/event_bus.rs` | 新增 Team* 事件 |
| `backend/crates/loom-memory/src/lib.rs` | 导出 `team_store` |
| `backend/crates/loom-types/src/lib.rs` | 导出 `team` 模块 |
| `backend/crates/loom-server/src/dispatch/mod.rs` | 注册 team 路由 |
| `backend/crates/loom-server/src/dispatch/chat.rs` | `chat.send` 支持 `team_config_id` |
| `backend/crates/loom-server/src/dispatch/session.rs` | 支持 `session.bind_team` |
| `frontend/src/renderer/src/components/input/AgentSelector.tsx` | 替换为 `EntitySelector` |
| `frontend/src/renderer/src/components/settings/SettingsPage.tsx` | 新增专家团 Tab |
| `frontend/src/renderer/src/stores/index.ts` | 注册 team slice |
| `frontend/src/renderer/src/i18n/` | 新增团队相关文案键 |

---

## 7. 边界情况

| 场景 | 处理 |
|------|------|
| 某成员执行失败（超时/报错） | 标记为 errored，团长汇总时标注"该成员未能完成" |
| 团队配置被删除时有活跃会话 | 会话降级为普通 Agent 模式，不阻塞已有消息 |
| 辩论模式下某成员第二轮不配合 | 直接使用其第一轮结果，团长标注无修正 |
| 团长自身执行失败 | 返回错误给用户，团队执行整体失败 |
| 成员之间产生循环依赖 | 架构保证：成员不可直接通信，无此问题 |

---

## 8. 验收标准

1. 设置页可创建/编辑/删除专家团，支持两种成员来源
2. 输入区 Popover 正常切换 Agent Tab 和专家团 Tab
3. 选中团队后启动对话，所有成员并行执行
4. 合成模式：团长输出综合结论
5. 辩论模式：两轮执行后团长输出结论
6. 团队对话可正常停止
7. 成员执行失败不影响其他成员
8. 切换团队/Agent 后互斥生效
9. 团队删除后已绑定会话降级为普通 Agent 模式

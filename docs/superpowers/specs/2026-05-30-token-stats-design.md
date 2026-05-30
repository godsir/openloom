# Token 消耗统计仪表盘 — 设计文档

**日期:** 2026-05-30
**状态:** 待实现

## 概述

在设置页面新增"用量"Tab，展示完整的 Token 消耗统计仪表盘。数据来源为混合方案：实时 WebSocket 推送 + 后端 SQLite 持久化查询。

## 数据流

```
Agent Loop (TokenUsage Event)
  ├─► WebSocket 推送 → 前端实时聚合 (本次会话)
  └─► SQLite 持久化 → JSON-RPC API → 前端历史查询
```

## 后端变更

### 数据库迁移 V10

```sql
CREATE TABLE token_usage (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id         TEXT NOT NULL,
    model              TEXT NOT NULL,
    prompt_tokens      INTEGER NOT NULL DEFAULT 0,
    completion_tokens  INTEGER NOT NULL DEFAULT 0,
    cached_tokens      INTEGER NOT NULL DEFAULT 0,
    latency_ms         INTEGER NOT NULL DEFAULT 0,
    context_window     INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_token_usage_model ON token_usage(model);
CREATE INDEX idx_token_usage_created ON token_usage(created_at);
```

**文件:** `backend/migrations/V10__token_usage.sql`

### 持久化 TokenUsage 事件

在 orchestrator 或 agent_loop 中，每次触发 `TokenUsage` 事件时写入 SQLite。

**文件:** `backend/crates/loom-core/src/orchestrator.rs` (新增 `record_token_usage` 方法)

### 新增 JSON-RPC API

#### `stats.token_summary`

查询时间范围内的汇总 KPI。

```json
// request
{ "from": "2026-05-01", "to": "2026-05-30" }

// response
{
  "total_prompt_tokens": 1234567,
  "total_completion_tokens": 456789,
  "total_cached_tokens": 89012,
  "total_requests": 1234,
  "avg_latency_ms": 2345,
  "cache_hit_rate": 0.72,
  "by_model": [
    {
      "model": "claude-opus-4-7",
      "prompt": 500000,
      "completion": 200000,
      "cached": 50000,
      "requests": 500,
      "avg_latency_ms": 3200,
      "avg_context_utilization": 0.65
    }
  ]
}
```

#### `stats.token_history`

查询时间范围内的时序聚合数据。

```json
// request
{ "from": "2026-05-01", "to": "2026-05-30", "granularity": "day" }

// response
{
  "points": [
    {
      "date": "2026-05-01",
      "prompt": 40000,
      "completion": 15000,
      "cached": 3000,
      "requests": 42,
      "by_model": {
        "claude-opus-4-7": { "prompt": 30000, "completion": 10000, "requests": 30 }
      }
    }
  ]
}
```

Granularity 支持: `"hour"` | `"day"` | `"week"`

**文件:** `backend/crates/loom-server/src/dispatch.rs` (新增两个 handler)

## 前端变更

### 依赖

```bash
npm install echarts
```

### 新文件

| 文件 | 说明 |
|------|------|
| `src/stores/tokenStats.ts` | Zustand slice — 实时聚合 + 历史查询 |
| `src/components/shared/TokenUsagePanel.tsx` | Token 统计主面板组件 |
| `src/components/shared/TokenUsagePanel.module.css` | 面板样式 |

### 修改文件

| 文件 | 变更 |
|------|------|
| `src/components/shared/SettingsModal.tsx` | 新增 `token` Tab + 渲染 TokenUsagePanel |
| `src/stores/index.ts` | 注册 TokenStatsSlice |

### 组件结构

```
TokenUsagePanel
├── KPI 卡片行
│   ├── 总 Token 消耗 (prompt + completion)
│   ├── 请求次数
│   ├── 缓存命中率
│   └── 平均延迟
├── 时间范围选择器 (全部 / 近7天 / 近30天 / 自定义)
├── 日消耗趋势折线图 (ECharts line)
│   └── 支持切换线/柱状图，按模型堆叠
├── 模型分布饼图 (ECharts pie)
├── 模型排名柱状图 (ECharts bar)
│   └── 按请求次数排名，hover 显示详细信息
└── 模型明细表
    ├── 模型名称 / 请求数 / Prompt / Completion / 缓存 / 平均延迟
    └── 按消耗降序排列
```

### Store — tokenStats slice

```ts
interface TokenStatsSlice {
  // 实时会话数据 (当前进程生命周期)
  sessionTotal: { prompt: number; completion: number; cached: number; requests: number }
  sessionByModel: Record<string, { prompt: number; completion: number; cached: number; requests: number }>

  // 历史查询状态
  summary: TokenSummary | null
  history: TokenHistoryPoint[]
  loading: boolean
  timeRange: 'all' | '7d' | '30d'

  // Actions
  recordUsage: (usage: TokenUsageEvent) => void
  loadSummary: (from: string, to: string) => Promise<void>
  loadHistory: (from: string, to: string, granularity: string) => Promise<void>
}
```

### WebSocket 集成

在现有 WS 消息处理中，监听 `chat.token_usage` 通知，调用 `tokenStats.recordUsage()`。

**文件:** `src/services/bootstrap.ts` 或 `src/stores/connection.ts`

### 样式

- 使用 CSS Module，遵循项目现有模式
- KPI 卡片: 半透明背景 + 边框，数字大字体，标签小字 muted
- 图表: 深色主题匹配暗色模式，使用项目 CSS 变量 (--accent, --green, --amber)
- 响应式: 图表宽度 100%，高度固定 300px

## 关于模型定价

初期不包含成本估算。原因是本地模型 (LM Studio / Ollama) 免费，各云端 API 定价各异且频繁变动。后续可作为独立功能添加定价配置。

## 测试计划

### 后端

- `loom-memory`: 单元测试 — token_usage 表 CRUD
- `loom-core`: 集成测试 — TokenUsage 事件写入后可从 SQLite 查询
- `loom-server`: API 测试 — stats.token_summary / stats.token_history 返回正确聚合结果

### 前端

- 手动验证: 发送聊天消息后，切换到"用量"Tab 查看数据更新
- 验证空状态: 无数据时显示占位提示
- 验证时间范围切换: 7天 / 30天 / 全部 数据正确

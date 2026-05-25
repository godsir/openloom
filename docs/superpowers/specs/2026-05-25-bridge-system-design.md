# Bridge 外部平台接入系统设计

**日期:** 2026-05-25  
**状态:** 草案  
**作者:** openLoom 团队

## 1. 概述

Bridge 系统让 openLoom Agent 能作为机器人接入外部消息平台（Telegram、飞书、微信、QQ），实现双向完整交互：Agent 接收外部消息并自动回复，也可主动发送消息。

### 1.1 设计原则

借鉴 OpenClaw 架构的最佳实践：

1. **Channel = Adapter**: 每个平台是一个 Adapter，职责三连——连接平台、标准化入站消息、格式化出站消息
2. **确定性路由**: 回复始终回到消息来源的平台，模型不选择输出通道
3. **Session 隔离**: 每个平台 + 每个对话独立 session
4. **懒加载**: 重量级 Adapter（如微信）仅在启用时加载
5. **安全分层**: DM 配对码 + allowlist + 频率限制

### 1.2 范围

**包含:**
- 4 个平台适配器: Telegram, 飞书, 微信, QQ
- 私聊 (DM) 模式
- 多媒体消息（图片/文件/语音）
- 双向完整交互（接收 + 自动回复 + 主动发送）
- 前端 Bridge 状态展示和消息记录

**不包含:**
- 群聊模式（后续版本）
- 跨平台消息同步（Bridge 转发）
- 外部 Gateway 服务

## 2. 架构

### 2.1 整体架构

```
                    ┌──────────────────────────────┐
                    │   openLoom Engine (Gateway)    │
                    │   BridgeManager (singleton)    │
                    └──────────┬───────────────────┘
                    ┌──────────┼──────────┬──────────┐
               ┌────▼───┐ ┌───▼────┐ ┌───▼────┐ ┌───▼──┐
               │Telegram│ │ 飞书   │ │ 微信   │ │ QQ  │
               │Adapter │ │Adapter │ │Adapter │ │Adapter│
               │(Bot API│ │(WS长连 │ │(iLink  │ │(Bot  │
               │polling)│ │ 接)    │ │桥接)   │ │ API) │
               └────────┘ └────────┘ └────────┘ └──────┘
                    │
        InboundMessage(normalized)
                    │
              MessageRouter
                    │
          ┌─────────┴─────────┐
          │                   │
    Session Lookup     Rate Limiter
    (bridge_sessions)  + Dedup
          │                   │
    Engine.handle_bridge_message()
          │
    Agent 生成回复
          │
    OutboundMessage → 原路返回
```

### 2.2 模块结构

```
crates/engine/src/bridge/
├── mod.rs          # pub mod 声明 + re-exports
├── manager.rs      # BridgeManager: Adapter 生命周期 + 消息分发
├── adapter.rs      # ChannelAdapter trait 定义
├── router.rs       # MessageRouter: 消息路由 + Session 管理
├── media.rs        # MediaService: 媒体下载/上传/格式转换
├── security.rs     # BridgeSecurity: 频率限制 + 配对 + 去重
├── types.rs        # BridgeMessage, Platform, MessageContent 等类型
├── telegram.rs     # TelegramAdapter 实现
├── feishu.rs       # FeishuAdapter 实现
├── wechat.rs       # WechatAdapter 实现 (iLink)
└── qq.rs           # QQAdapter 实现
```

## 3. 核心接口

### 3.1 标准化消息格式

```rust
/// 标准化的内部消息格式，所有平台适配器将平台消息转换为此格式
pub struct BridgeMessage {
    pub platform: Platform,           // Telegram/Feishu/Wechat/QQ
    pub chat_id: String,             // 平台会话 ID
    pub sender_id: String,           // 发送者平台 ID
    pub sender_name: String,
    pub content: MessageContent,     // Text/Media/...
    pub reply_to: Option<String>,    // 引用消息
    pub external_message_id: String, // 平台原始消息 ID（用于去重）
    pub timestamp: DateTime<Utc>,
}

pub enum MessageContent {
    Text(String),
    Image { url: String, caption: Option<String> },
    File { url: String, name: String, size: u64 },
    Audio { url: String, duration_secs: u32 },
}

pub enum Platform {
    Telegram,
    Feishu,
    Wechat,
    QQ,
}
```

### 3.2 ChannelAdapter Trait

```rust
/// 每个平台实现此 trait
#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    /// 平台标识
    fn platform(&self) -> Platform;
    
    /// 连接到平台 API（启动 polling/webhook）
    async fn connect(&mut self) -> Result<()>;
    
    /// 断开连接
    async fn disconnect(&mut self) -> Result<()>;
    
    /// 发送消息到平台
    /// 返回平台分配的 external_message_id
    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String>;
    
    /// 接收消息的 channel（Adapter 内部轮询/webhook 后推送）
    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage>;
    
    /// 健康状态
    fn health(&self) -> AdapterHealth;
}

pub enum AdapterHealth {
    Connected,
    Connecting,
    Disconnected,
    Error(String),
}
```

## 4. 消息处理流程

### 4.1 Inbound 流程（外部 → Agent）

```
1. 用户在 Telegram 发消息 "帮我查下天气"
2. TelegramAdapter: Bot API polling 收到消息
3. → 标准化为 BridgeMessage { 
     platform: Telegram, 
     chat_id: "123456", 
     content: Text("帮我查下天气"),
     external_message_id: "msg_789"
   }
4. → MessageRouter: 
     a. 查找/创建 bridge_session
     b. RateLimiter: 检查该用户的发送频率
     c. Dedup: 检查 external_message_id 去重
5. → Engine.handle_bridge_message():
     a. 构造 system prompt（包含 bridge context）
     b. 调用 Agent 处理（复用现有 handle_message 流程）
6. → Agent 生成回复 "北京今天晴，28°C"
```

### 4.2 Outbound 流程（Agent → 外部）

```
1. Agent 生成回复文本
2. MessageRouter: 根据 session.platform 路由回原 Adapter
3. TelegramAdapter.send("123456", Text("北京今天晴，28°C"))
4. 调用 Telegram Bot API: sendMessage
5. 返回 external_message_id
6. 记录到 bridge_messages 表
```

### 4.3 媒体处理

```
Inbound:
  平台原始 URL → MediaService.download() → 本地临时文件 (data_dir/bridge_media/tmp/) → 传给 Agent

Outbound:
  Agent 生成媒体 → MediaService.upload() → 平台 API → 平台 URL
```

**媒体存储策略:**
- 下载的媒体存储在 `{data_dir}/bridge_media/{platform}/{session_id}/`
- 文件名: `{timestamp}_{external_message_id}.{ext}`
- 自动清理: 保留最近 7 天，超期删除
- 大文件: 超过 20MB 的媒体仅保留 URL，不下载

### 4.4 Engine 集成

```rust
impl Engine {
    /// 处理来自 Bridge 的消息
    pub async fn handle_bridge_message(&self, msg: BridgeMessage) -> Result<String> {
        // 1. 查找或创建 bridge_session
        let session_id = format!("bridge:{}:{}", msg.platform, msg.chat_id);
        
        // 2. 构造 system prompt（包含 bridge context）
        let system = self.bridge_system_instruction(&msg.platform, &msg.sender_name);
        
        // 3. 复用现有 handle_message 流程
        let user_content = self.format_bridge_content(&msg.content);
        let reply = self.complete_with_model_streaming_meta(
            &session_id,
            &user_content,
            model_id,
            provider,
        ).await?;
        
        // 4. 记录消息到 bridge_messages 表
        self.record_bridge_message(&session_id, "inbound", &msg);
        self.record_bridge_message(&session_id, "outbound", &reply);
        
        Ok(reply)
    }
    
    /// Bridge 专用的 system prompt
    fn bridge_system_instruction(&self, platform: &Platform, sender: &str) -> String {
        let base = self.system_instruction();
        format!(
            "{}\n\n## Bridge Context\n\
             You are responding via {} (external messaging platform).\n\
             The user's name on this platform is: {}.\n\
             Keep responses concise and conversational.",
            base,
            platform.name(),
            sender
        )
    }
}
```

### 4.5 Session 生命周期

**创建时机:**
- 首次收到某 `platform:chat_id` 的消息时自动创建

**更新时机:**
- 每次收到消息更新 `last_message_at` 和 `message_count`

**过期清理:**
- 超过 90 天无活动的 session 标记为 `archived`
- 归档后不再接收消息，需手动恢复

**手动管理:**
- 前端可删除 session（同时删除相关消息记录）
- 前端可拉黑 session（设置 `access_state = 'blocked'`）

## 5. 数据模型

### 5.1 DB Schema（迁移 V7）

```sql
CREATE TABLE bridge_sessions (
    id TEXT PRIMARY KEY,                    -- "{platform}:{chat_id}"
    platform TEXT NOT NULL,
    external_chat_id TEXT NOT NULL,
    external_user_id TEXT,
    user_name TEXT,
    user_avatar_url TEXT,
    access_state TEXT DEFAULT 'active',     -- active/blocked/pending
    pairing_code TEXT,
    created_at TEXT NOT NULL,
    last_message_at TEXT,
    message_count INTEGER DEFAULT 0
);

CREATE TABLE bridge_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES bridge_sessions(id),
    direction TEXT NOT NULL,               -- inbound/outbound
    content TEXT,
    media_type TEXT DEFAULT 'text',        -- text/image/file/audio
    media_url TEXT,
    external_message_id TEXT,
    timestamp TEXT NOT NULL
);

CREATE TABLE bridge_known_users (
    platform TEXT NOT NULL,
    user_id TEXT NOT NULL,
    user_name TEXT,
    avatar_url TEXT,
    first_seen TEXT NOT NULL,
    last_seen TEXT,
    PRIMARY KEY (platform, user_id)
);

CREATE INDEX idx_bridge_sessions_platform ON bridge_sessions(platform);
CREATE INDEX idx_bridge_messages_session ON bridge_messages(session_id, timestamp);
```

### 5.2 配置模型

```rust
/// 每个平台的配置（存储在 settings.bridge.{platform}）
pub struct BridgePlatformConfig {
    pub enabled: bool,
    pub credentials: serde_json::Value,  // 平台特定的凭证（bot token, app_id 等）
    pub owner: Option<String>,           // Bridge 拥有者
    pub access_mode: AccessMode,
    pub rate_limit: RateLimitConfig,
}

pub enum AccessMode {
    Pairing,    // 新用户需 8 位配对码，管理员审批
    Allowlist,  // 白名单
    Open,       // 无限制
}

pub struct RateLimitConfig {
    pub max_messages_per_minute: u32,    // 默认 30
    pub max_commands_per_hour: u32,      // 默认 100
}
```

## 6. 安全模型

### 6.1 访问控制

```rust
pub struct BridgeSecurity {
    pub access_mode: AccessMode,
    pub rate_limiter: RateLimiter,
    pub loop_protection: LoopDetector,
    pub known_users: HashMap<(Platform, String), KnownUser>,
}
```

**配对流程:**
1. 新用户首次发消息
2. 系统生成 8 位配对码
3. 前端弹出审批 UI
4. 管理员批准/拒绝
5. 批准后用户可正常交互

**频率限制:**
- 默认: 30 条消息/分钟，100 条命令/小时
- 超限后返回友好提示，不封禁

**Bot 循环保护:**
- 检测 bot-to-bot 对话
- 连续 3 次 bot 互回后自动停止

## 7. 平台适配要点

### 7.1 Telegram

**接入方式:** Bot API + Long Polling  
**凭证:** Bot Token  
**消息格式:** Markdown  
**媒体:** photo/document/voice  

**实现要点:**
- 使用 `reqwest` 轮询 `getUpdates`
- 解析 `message.from`, `message.chat.id`, `message.text`
- 发送时调用 `sendMessage` / `sendPhoto` / `sendDocument`

### 7.2 飞书

**接入方式:** Open Platform WebSocket 长连接  
**凭证:** App ID + App Secret  
**消息格式:** Rich text card  
**媒体:** image/file  

**实现要点:**
- 使用 `tokio-tungstenite` 建立 WebSocket
- 接收 `im.message.receive_v1` 事件
- 发送时调用 `im/v1/messages` API

### 7.3 微信

**接入方式:** iLink 第三方桥接服务  
**凭证:** iLink API Key  
**消息格式:** 纯文本 + 图片  
**媒体:** image/file  

**实现要点:**
- 轮询 iLink API 获取新消息
- 调用 iLink API 发送消息
- 媒体文件需先上传到 iLink

### 7.4 QQ

**接入方式:** QQ 开放平台 Bot API  
**凭证:** App ID + Token  
**消息格式:** Markdown  
**媒体:** image/file  

**实现要点:**
- 使用 WebSocket 接收事件
- 解析 `GROUP_AT_MESSAGE` / `C2C_MESSAGE`
- 发送时调用消息 API

## 8. 前端改动

### 8.1 现有基础设施

Settings 中的 Bridge 面板已存在（`web/src/settings/tabs/bridge/`）：
- 平台配置输入
- 连接测试按钮
- 状态展示

### 8.2 需新增

**连接状态指示:**
- 实时显示每个平台的连接状态（通过 WebSocket 推送 `bridge.status_changed` 事件）
- 状态颜色: 绿=已连接, 黄=连接中, 红=断开/错误

**消息记录:**
- 查看 Agent 在各平台的历史对话
- 按平台筛选
- 显示方向（inbound/outbound）和时间戳

**已知用户管理:**
- 列表展示所有已知联系人
- 支持批准/拒绝配对请求
- 支持拉黑用户

**配对审批:**
- 新用户请求接入时弹出通知
- 显示配对码和用户信息
- 批准/拒绝按钮

## 9. 实现分步

### M1: 核心框架

**内容:**
- `types.rs`: BridgeMessage, Platform, MessageContent 等类型定义
- `adapter.rs`: ChannelAdapter trait
- `manager.rs`: BridgeManager（Adapter 生命周期管理）
- `router.rs`: MessageRouter（消息路由 + Session 管理）
- `security.rs`: BridgeSecurity（频率限制 + 去重）
- DB 迁移 V7: bridge_sessions, bridge_messages, bridge_known_users
- Engine 集成: `handle_bridge_message()` 方法

**验收标准:**
- BridgeManager 能启动/停止 Adapter
- MessageRouter 能创建 session 并路由消息
- 消息能记录到 DB

### M2: Telegram Adapter

**内容:**
- `telegram.rs`: TelegramAdapter 实现
- Bot API long polling
- 文本消息收发
- 图片/文件/语音收发

**验收标准:**
- 能通过 Telegram Bot 与 Agent 对话
- 支持图片发送和接收

### M3: 飞书 Adapter

**内容:**
- `feishu.rs`: FeishuAdapter 实现
- WebSocket 长连接
- 文本消息收发
- 富文本卡片
- 媒体处理

**验收标准:**
- 能通过飞书 Bot 与 Agent 对话
- 支持图片和文件

### M4: 微信 Adapter

**内容:**
- `wechat.rs`: WechatAdapter 实现
- iLink API 集成
- 文本消息收发
- 图片处理

**验收标准:**
- 能通过微信与 Agent 对话
- 支持图片消息

### M5: QQ Adapter

**内容:**
- `qq.rs`: QQAdapter 实现
- QQ Bot API 集成
- 文本消息收发
- 媒体处理

**验收标准:**
- 能通过 QQ Bot 与 Agent 对话

### M6: 前端完善

**内容:**
- 连接状态实时指示（WebSocket 事件）
- 消息记录查看器
- 已知用户管理 UI
- 配对审批流程

**验收标准:**
- 前端能实时显示 Bridge 状态
- 能查看历史消息
- 能管理已知用户和配对请求

## 10. 风险与缓解

### 10.1 平台 API 变更

**风险:** 外部平台 API 可能变更  
**缓解:** 
- Adapter 层隔离，变更只影响单个 Adapter
- 添加 API 版本检查
- 定期更新依赖

### 10.2 微信 iLink 依赖

**风险:** iLink 是第三方服务，可能不稳定  
**缓解:**
- 添加重试和超时
- 提供降级提示
- 后续可考虑官方企业微信 API

### 10.3 消息丢失

**风险:** 网络问题导致消息丢失  
**缓解:**
- 使用 `external_message_id` 去重
- 记录所有消息到 DB
- 支持手动重发

## 11. 测试策略

### 11.1 单元测试

- `types.rs`: 序列化/反序列化
- `router.rs`: Session 创建/查找逻辑
- `security.rs`: 频率限制、去重算法
- 各 Adapter: Mock 平台 API

### 11.2 集成测试

- 端到端消息流: Inbound → Router → Engine → Outbound
- DB 持久化: 消息记录正确性
- 并发: 多平台同时收发消息

### 11.3 E2E 测试

- 使用真实 Telegram Bot 测试完整流程
- 模拟飞书 WebSocket 事件
- 压力测试: 高频消息场景

## 12. 后续扩展

- **群聊模式**: 支持群组消息、@提及解析
- **跨平台同步**: Bridge 转发（Telegram → 飞书）
- **更多平台**: Discord, Slack, Signal
- **高级媒体**: 视频、语音转文字
- **智能路由**: 根据消息内容选择不同 Agent

## 13. 参考

- OpenClaw 架构: https://github.com/openclaw/openclaw
- Telegram Bot API: https://core.telegram.org/bots/api
- 飞书开放平台: https://open.feishu.cn/
- iLink 微信桥接: https://ilink.ai/
- QQ 开放平台: https://q.qq.com/

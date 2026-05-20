# Phase 3B: Productionization — 设计规范

**版本:** 1.0
**日期:** 2026-05-20
**状态:** 设计完成
**前置:** Phase 3A (AI Activation)

---

## 1. 目标

从"能推理的内核"升级为"可发布的桌面产品"。KV Cache 磁盘持久化、安全沙箱、跨平台打包、Engine 拆分、认知审核面板。

**核心交付:**
1. Q4 KV Cache Store — safetensors 块池，前缀 prefill 一次永久复用
2. 安全沙箱 — 声明式权限 + OS 原生强制
3. Engine lib.rs 拆分 — 730 行→多模块
4. 认知审核面板 — 查看/回滚认知图谱
5. 跨平台打包 — macOS .dmg / Windows .msi / Linux .AppImage

**不做（后续迭代）：** 多 Agent 协作、Skill 市场、移动端、一键安装向导（Phase 3B 做打包脚本，完整 installer 后续）

---

## 2. Crate 变更

| Crate | 变更 |
|-------|------|
| `cache` | NoopCache→SafetensorsCache：safetensors 块池读写，per-agent 隔离 |
| `engine` | lib.rs 拆为 core/rate_limiter/session/config/shutdown 子模块 |
| `sandbox` | 新建 crate：声明式权限引擎 + OS 原生适配（Seatbelt/Landlock/Restricted Tokens） |
| `web` | 新增 CognitionAuditPanel（查看+回滚），TokenDashboard 增强 |
| `electron` | 打包脚本 + 跨平台 CI/CD |

---

## 3. 详细设计

### 3.1 Q4 KV Cache Store

**当前状态：** `KvCache` trait + `NoopCache`。Weaver 记录了 `static_prefix_len` 但从未使用。

**设计：** 将 system prompt + persona summary（Weaver 的 static prefix）在首次推理后 quantize 并落盘。后续请求从磁盘恢复 KV 块到注意力层，跳过 prefill。

```
~/.openloom/cache/
  {prefix_hash}/
    block_0000.safetensors   ← Q4 KV cache 块 (1024 tokens/块)
    block_0001.safetensors
    meta.json                 ← token 范围, 创建时间, 命中次数
```

**实现路径：**

```rust
// crates/cache/src/lib.rs
pub struct SafetensorsCache {
    cache_dir: PathBuf,
    max_blocks: usize,
    total_budget_mb: usize,
}

impl KvCache for SafetensorsCache {
    fn lookup(&self, prefix_hash: u64) -> Option<CachedPrefix> {
        // 读取 cache_dir/{hash}/block_*.safetensors → 反序列化 → CachedPrefix
    }
    fn store(&self, prefix_hash: u64, blocks: CachedPrefix) {
        // Q4 quantize KV → safetensors → 写入磁盘
        // LRU eviction if over budget
    }
    fn stats(&self) -> CacheStats {
        // 真实命中率/块数/大小
    }
}
```

**Engine 集成：** `ContextWeaver::assemble()` 返回 `static_prefix_len` 后，Engine 在推理完成时将 KV cache 块写入 `SafetensorsCache`。`SafetensorsCache::lookup()` 在下次相同 prefix 时返回缓存块，推理引擎跳过 prefill。

**注意：** llama-cpp-2 的 KV cache save/restore API 需要通过 `LlamaContext::state_get_size()` / `state_get_data()` / `state_set_data()` 实现——这是标准 API，v0.1.146 支持。

### 3.2 安全沙箱

**当前状态：** `SkillPermissions` 定义在 `models` 中，但权限检查完全跳过。所有 5 个内置 Skill 有完整系统访问权限。

**设计：** 新建 `sandbox` crate。

```
crates/sandbox/src/
├── lib.rs          ← 权限引擎入口
├── manifest.rs     ← 解析 SkillPermissions 声明
├── fs.rs           ← fs_read/fs_write 路径白名单检查
├── network.rs      ← 网络域名白名单检查
├── shell.rs        ← shell 命令白名单 + 参数校验
└── platform.rs     ← OS 原生沙箱适配
    ├── macos.rs    ← Seatbelt (sandbox-exec)
    ├── linux.rs    ← Landlock + Bubblewrap
    └── windows.rs  ← Restricted Tokens
```

**权限强制点：**
- `FileManager` → `fs::read/write` 路径必须匹配 `fs_read`/`fs_write` 白名单
- `WebBrowser` → HTTP 域名必须匹配 `network` 白名单
- `CodeAssistant` → subprocess 为 false 时禁止 `std::process::Command`

**OS 原生沙箱（Phase 3B 做声明式权限，OS 原生可选）：**
- macOS: `sandbox-exec` profile 限制文件/网络/进程
- Linux: Landlock LSM ruleset
- Windows: Restricted Token + Integrity Level

### 3.3 Engine lib.rs 拆分

**当前：** `crates/engine/src/lib.rs` ~730 行，包含 10+ 关注点。

**拆分后：**
```
crates/engine/src/
├── lib.rs              ← Engine struct + new() + public API (~150 lines)
├── rate_limiter.rs     ← RateLimiter struct + check()
├── agent_loop.rs       ← agent_loop() + parse_tool_call() + execute_tool()
├── session.rs          ← SessionCommand + spawn_session_thread()
├── token_store.rs      ← TokenUsageRecord + spawn_token_store_thread()
├── config.rs           ← get_config() + set_config() + load_config_into_engine()
├── shutdown.rs         ← shutdown() + draining + WAL checkpoint
├── persona_watcher.rs  ← spawn_persona_watcher()
├── heartbeat.rs        ← Hub heartbeat loop (Phase 3A 新增)
├── handlers.rs         ← handle_message() + assemble_and_complete()
├── events.rs           ← search_events() + list_events()
└── memory_thread.rs    ← [已有] spawn_memory_thread() + ProcessRequest
```

**API 不变：** `pub use engine::Engine;` 对外接口完全兼容，现有 consumer（server/cli）不需要任何修改。

### 3.4 认知审核面板

**前端新增 CognitionAuditPanel（替代现有 PersonaPanel 的只读展示）：**
```tsx
// web/src/components/CognitionAuditPanel.tsx
// 功能:
// 1. 查看所有 cognitions（已有 memory.cognitions API）
// 2. 展开单条→查看版本历史（调用 snapshots_for）
// 3. 回滚到历史版本（调用新的 memory.cognition_rollback JSON-RPC）
// 4. 手动创建/编辑 cognition（调用新的 memory.cognition_upsert）
```

**新增 JSON-RPC 方法：**
```rust
"memory.cognition_snapshots" → engine.snapshots_for(id)
"memory.cognition_rollback" → engine.rollback_cognition(id, version)
```

### 3.5 跨平台打包

```
electron/
├── build/
│   ├── macos/
│   │   └── build.sh       ← dmg 打包（electron-builder）
│   ├── windows/
│   │   └── build.ps1      ← msi 打包
│   └── linux/
│       └── build.sh        ← AppImage 打包
└── package.json            ← 加 electron-builder 配置
```

**electron-builder 配置（package.json 新增）：**
```json
"build": {
    "appId": "com.openloom.app",
    "productName": "openLoom",
    "files": ["main.js", "preload.js", "../web/dist/**/*"],
    "extraResources": [{ "from": "../target/release/openloom", "to": "engine" }],
    "mac": { "target": "dmg", "category": "public.app-category.productivity" },
    "win": { "target": "msi" },
    "linux": { "target": "AppImage" }
}
```

---

## 4. 文件结构

```
F:/openLoom/
├── crates/
│   ├── cache/src/lib.rs          ← [重写] SafetensorsCache 替代 NoopCache
│   ├── sandbox/                  ← [新建 create]
│   │   ├── Cargo.toml
│   │   └── src/{lib,manifest,fs,network,shell,platform}.rs
│   ├── engine/src/               ← [重构] lib.rs→12 模块
│   │   ├── lib.rs + 11 子模块
│   │   └── memory_thread.rs
│   ├── server/src/dispatch.rs    ← [Modify] +cognition_snapshots +cognition_rollback
│   └── models/src/lib.rs         ← [Modify] +EngineEvent 可能新增
├── electron/
│   ├── build/{macos,windows,linux}/  ← [新建] 打包脚本
│   └── package.json              ← [Modify] +electron-builder config
├── web/src/components/
│   └── CognitionAuditPanel.tsx   ← [新建] 替代 PersonaPanel
└── docs/superpowers/specs/
    └── 2026-05-20-phase3b-design.md  ← 本文件
```

---

## 5. 依赖关系

```
KV Cache (3.1) — 依赖 llama-cpp-2 state_get/set API
安全沙箱 (3.2) — 独立，无内部依赖
Engine 拆分 (3.3) — 独立，纯代码重构
认知审核面板 (3.4) — 依赖 cognition_snapshots 表（Phase 2 补缺已建）
跨平台打包 (3.5) — 依赖 Phase 3A 推理 + Phase 3B KV Cache + 沙箱
```

---

## 6. 错误处理

| 场景 | 策略 |
|------|------|
| KV Cache 磁盘满 | LRU eviction，最旧块先删，tracing::warn! |
| safetensors 损坏 | 校验和检测→丢弃损坏块→标记 cache miss→重新 prefill |
| 权限拒绝 | 返回 ErrorCode::PermissionDenied + 详细的被拒绝操作描述 |
| Engine 模块编译错误 | `pub use engine::*;` 保持向后兼容，consumer 不需要改 import |
| electron-builder 失败 | 各平台独立打包脚本，一个失败不影响其他 |

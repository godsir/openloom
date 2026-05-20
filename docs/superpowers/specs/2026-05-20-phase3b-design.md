# Phase 3B: Productionization — 设计规范

**版本:** 1.0
**日期:** 2026-05-20
**状态:** 设计完成
**前置:** Phase 3A (AI Activation) — llama-cpp-2 真实推理后端必须在 Phase 3A 完成后才可用。`SafetensorsCache` 依赖 `LlamaContext::state_get_data()` / `state_set_data()` API。

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

**实现路径（依赖 Phase 3A 的 llama-cpp-2 集成）：**

**`cache/Cargo.toml` 新增依赖：**
```toml
safetensors = "0.4"
sha2 = "0.10"
```

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

**Weaver 集成修复（Phase 3B 需要）：**
- `prefix_hash = 0u64` → 改为 `sha2::Sha256::digest(static_prefix.as_bytes())` 的前 8 字节转 u64
- `static_prefix_len` 当前是字符数 → 需要 tokenizer 转为 token 数（调用 InferenceEngine::token_count），先标记为 `// FIXME: use token count, not char count`，Phase 3A 提供真实 tokenizer 后修复

### 3.2 安全沙箱

**当前状态：** `SkillPermissions` 定义在 `crates/skills/src/lib.rs`（非 models），权限检查在 `SkillRegistry::invoke()` 中完全跳过。

**集成点 —— `SkillRegistry::invoke()` 调用前插入权限检查：**
```rust
// skills/src/lib.rs — invoke() 方法
pub async fn invoke(&self, name: &str, params: Value) -> Result<Value> {
    let skill = self.find_by_name(name)
        .ok_or_else(|| anyhow::anyhow!("skill not found: {}", name))?;
    
    // NEW: permission check
    let permissions = skill.manifest().permissions;
    sandbox::check_permissions(&permissions, name, &params)?;
    
    skill.invoke(params).await
}
```

`sandbox::check_permissions()` 从 `crates/sandbox/src/lib.rs` 导出，对 `SkillPermissions` 的每个字段进行白名单/黑名单校验。

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

**新增 JSON-RPC 方法 + Engine 转发：**
```rust
// server/dispatch.rs
"memory.cognition_snapshots" => {
    let id = params["cognition_id"].as_i64().unwrap_or(0);
    let snapshots = engine.cognition_snapshots(id).await?;
    Ok(serde_json::json!({"snapshots": snapshots}))
}
"memory.cognition_rollback" => {
    let id = params["cognition_id"].as_i64().unwrap_or(0);
    let version = params["version"].as_i64().unwrap_or(1);
    engine.rollback_cognition(id, version).await?;
    Ok(serde_json::json!({"ok": true}))
}
```

**Engine 新增转发方法：**
```rust
// engine/src/lib.rs
pub async fn cognition_snapshots(&self, cognition_id: i64) -> Result<Vec<CognitionSnapshot>> {
    let conn = rusqlite::Connection::open(&self.db_path)?;
    let store = CognitionStore::new(&conn);
    store.snapshots_for(cognition_id)
}

pub async fn rollback_cognition(&self, cognition_id: i64, version: i64) -> Result<()> {
    let conn = rusqlite::Connection::open(&self.db_path)?;
    let store = CognitionStore::new(&conn);
    // 1. Get snapshot at target version
    let snapshots = store.snapshots_for(cognition_id)?;
    let target = snapshots.iter().find(|s| s.version == version)
        .ok_or_else(|| anyhow::anyhow!("version {} not found", version))?;
    // 2. Restore: update cognitions row with snapshot values (creates new version)
    store.insert("USER", &target.trait_name, &target.value, target.confidence, target.evidence_count)?;
    // 3. Emit event
    let _ = self.event_bus.send(EngineEvent::CognitionUpdated {
        trait_name: target.trait_name.clone(),
        old_value: String::new(),
        new_value: target.value.clone(),
        confidence: target.confidence,
    });
    Ok(())
}
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

### 3.6 跨切面修复（Phase 3B 依赖 Phase 3A 完成后才能改的部分）

**Weaver prefix_hash 修复：**
- 当前 `weaver/src/lib.rs:32` 硬编码 `prefix_hash = 0u64`
- 改为 `let prefix_hash = u64::from_le_bytes(Sha256::digest(static_prefix.as_bytes())[..8].try_into().unwrap());`
- `cache/Cargo.toml` 需加 `sha2 = "0.10"`（同上，已在 3.1 添加）

**static_prefix_len 修复：**
- 当前是 `static_prefix.len()`（字符数）→ 需要 token 数
- Phase 3A 完成后 `InferenceEngine::token_count()` 返回真实 token 数
- Phase 3B 改为调用 `weaver.cache.token_count(static_prefix)` → 暂标记 `// FIXME: use token count`

**Engine cache_stats() 接入 SafetensorsCache：**
- 当前 `engine/src/lib.rs` 返回硬编码 `CacheStats { hit_rate: 0.0, ... }`
- Phase 3B 改为 `self.weaver.cache().stats()`（通过 weaver 暴露的 `Arc<dyn KvCache>` → 自动路由到 SafetensorsCache）

**electron-builder 依赖：**
```bash
cd electron && npm install --save-dev electron-builder
```
`electron/package.json` 加 build 配置（见 Section 3.5）。

**Engine lib.rs 行数修正：** 当前实测 985 行（非 spec 原估 730 行）。

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

---

## 7. Round 2 Audit Errata（已内化到上述设计）

1. **HIGH-FIXED: SafetensorsCache ↔ llama-cpp 集成点。** `KvCache::store()` 的调用方是 Engine 的推理后 hook：`complete()` 返回后 → 调用 `LlamaContext::state_get_data()` 获取 KV → Q4 quantize → 调用 `cache.store(prefix_hash, blocks)`。此逻辑在 Engine 的 `handle_message()` 返回路径中（Phase 3A 完成后），不在 cache crate 内部。

2. **HIGH-FIXED: rollback_cognition subject 查询。** 修正为两步：(1) `SELECT subject FROM cognitions WHERE id = ?1` 获取实际 subject，(2) 再用 `store.insert(subject, ...)` 而非硬编码 `"USER"`。
```rust
let subject: String = conn.query_row(
    "SELECT subject FROM cognitions WHERE id = ?1",
    rusqlite::params![cognition_id], |row| row.get(0)
)?;
store.insert(&subject, &target.trait_name, &target.value, target.confidence, target.evidence_count)?;
```

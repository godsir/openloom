# Phase 2 Milestone D: Electron + Frontend Integration — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完成 Phase 2 "完整 Electron GUI" 交付：preload 持久 WebSocket + 真实 subscribe、Electron 主进程加固、React 6 组件全连线、port/pid 文件

**Architecture:** 通信层 → 主进程 → 类型层 → 前端组件。preload.js 和 main.js 独立并行，server port/pid 独立，React 组件依赖类型文件

**Tech Stack:** Electron 38, React 19, TypeScript 5.7, Tailwind CSS 4, Vite 6, Rust 2024

---

## 文件结构

```
F:/openLoom/
├── electron/
│   ├── preload.js           ← [Rewrite] 持久WS + send/subscribe/reconnect
│   └── main.js              ← [Modify] +CSP +tray +health +webviewTag +modelBanner +autostart
├── crates/server/src/
│   └── lib.rs               ← [Modify] +port/pid file write + zombie cleanup on startup
├── web/src/
│   ├── types/
│   │   └── electron.d.ts    ← [Create] 共享 window.openloom 类型
│   ├── App.tsx              ← [Modify] session engine API
│   ├── components/
│   │   ├── Sidebar.tsx      ← [Modify] live session data
│   │   ├── ChatArea.tsx     ← [Modify] +subscribe push + agent state
│   │   ├── SettingsPanel.tsx← [Modify] config.get/set wiring
│   │   ├── TokenDashboard.tsx← [Modify] real-time token via subscribe
│   │   └── PersonaPanel.tsx ← [Create] cognition profile visualization
│   └── index.css            ← [可能修改] 新组件样式
└── docs/superpowers/plans/
    └── 2026-05-20-phase2-milestone-d.md  ← 本文件
```

---

### Task 1: preload.js — 持久 WebSocket + real subscribe + 重连

**Files:**
- Rewrite: `F:/openLoom/electron/preload.js`

这是核心通信层重写。当前 preload.js 每次 `send()` 新建+关闭 WebSocket，`subscribe()` 是 `console.log` stub。

- [ ] **Step 1: 完整重写 preload.js**

`F:/openLoom/electron/preload.js` 完整替换为：

```js
const { contextBridge } = require('electron');

let ws = null;
let msgId = 0;
const pending = new Map();       // id → { resolve, reject, timer }
const subscribers = new Map();   // method → Set<callback>
let reconnectAttempt = 0;
let connectTime = null;
const MIN_UPTIME = 3000;
const MAX_BACKOFF = 30000;
const REQUEST_TIMEOUT = 30000;

function connect() {
    if (!window.__enginePort__) {
        setTimeout(connect, 500);
        return;
    }

    try {
        ws = new WebSocket(`ws://127.0.0.1:${window.__enginePort__}/ws`);
    } catch (e) {
        scheduleReconnect();
        return;
    }

    ws.onopen = () => {
        reconnectAttempt = 0;
        connectTime = Date.now();
        // 重放所有 subscribe 注册（callback 已在内存中，重连后自动生效）
    };

    ws.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);
            if (data.id !== undefined) {
                // JSON-RPC response
                const entry = pending.get(data.id);
                if (entry) {
                    clearTimeout(entry.timer);
                    pending.delete(data.id);
                    if (data.error) {
                        entry.reject(data.error);
                    } else {
                        entry.resolve(data.result);
                    }
                }
            } else {
                // JSON-RPC notification (no id)
                const cbs = subscribers.get(data.method);
                if (cbs) {
                    cbs.forEach(cb => {
                        try { cb(data.params); } catch (e) { console.error('subscribe callback error:', e); }
                    });
                }
            }
        } catch (e) {
            console.error('ws message parse error:', e);
        }
    };

    ws.onerror = () => {
        // onclose will fire after onerror
    };

    ws.onclose = () => {
        // Reject all pending requests
        pending.forEach((entry, id) => {
            clearTimeout(entry.timer);
            entry.reject(new Error('WebSocket disconnected'));
        });
        pending.clear();

        // Fast reconnect if connection was unstable
        if (connectTime && (Date.now() - connectTime) < MIN_UPTIME) {
            reconnectAttempt += 2;
        }

        scheduleReconnect();
    };
}

function scheduleReconnect() {
    const jitter = Math.floor(Math.random() * 1000);
    const delay = Math.min(1000 * Math.pow(1.5, reconnectAttempt) + jitter, MAX_BACKOFF);
    reconnectAttempt++;
    setTimeout(connect, delay);
}

function ensureConnection() {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
        return new Promise((resolve, reject) => {
            const check = setInterval(() => {
                if (ws && ws.readyState === WebSocket.OPEN) {
                    clearInterval(check);
                    resolve();
                }
            }, 100);
            setTimeout(() => {
                clearInterval(check);
                reject(new Error('Connection timeout'));
            }, 10000);
        });
    }
    return Promise.resolve();
}

contextBridge.exposeInMainWorld('openloom', {
    send: async (method, params) => {
        await ensureConnection();
        const id = ++msgId;
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                pending.delete(id);
                reject(new Error(`Request timeout: ${method}`));
            }, REQUEST_TIMEOUT);
            pending.set(id, { resolve, reject, timer });
            ws.send(JSON.stringify({ jsonrpc: '2.0', method, params: params || {}, id }));
        });
    },

    sseUrl: (sessionId) => {
        return `http://127.0.0.1:${window.__enginePort__}/sse/${sessionId}`;
    },

    subscribe: (eventType, callback) => {
        if (!subscribers.has(eventType)) {
            subscribers.set(eventType, new Set());
        }
        subscribers.get(eventType).add(callback);

        // Ensure WebSocket is connected
        if (!ws || ws.readyState !== WebSocket.OPEN) {
            connect();
        }

        // Return unsubscribe function
        return () => {
            const cbs = subscribers.get(eventType);
            if (cbs) {
                cbs.delete(callback);
                if (cbs.size === 0) {
                    subscribers.delete(eventType);
                }
            }
        };
    },
});

// Initial connection attempt
if (window.__enginePort__) {
    connect();
} else {
    // Port not yet injected — wait and retry
    const checkInterval = setInterval(() => {
        if (window.__enginePort__) {
            clearInterval(checkInterval);
            connect();
        }
    }, 200);
}
```

- [ ] **Step 2: 验证语法**

Run: `cd electron && node -e "require('./preload.js')" 2>&1`

Expected: No syntax errors. Will fail on `contextBridge` (not in Node.js context) but that's expected — just need no parse errors.

- [ ] **Step 3: Commit**

```bash
git add electron/preload.js
git commit -m "feat(electron): rewrite preload with persistent WebSocket, real subscribe, exponential backoff reconnect"
```

---

### Task 2: Electron main.js — CSP + 系统托盘 + 健康检查 + webviewTag + autostart

**Files:**
- Modify: `F:/openLoom/electron/main.js`

- [ ] **Step 1: 读取当前 main.js 并添加所有增强**

`F:/openLoom/electron/main.js` — 在当前代码基础上做以下 6 处修改：

**2.1 添加模块导入（文件顶部 require 区域）:**

```js
const { app, BrowserWindow, Tray, Menu, session } = require('electron');
// 将原有的 const { app, BrowserWindow } 替换为上面这行
```

**2.2 BrowserWindow 构造函数加 webviewTag（createWindow 函数内）:**

```js
webPreferences: {
    preload: path.join(__dirname, 'preload.js'),
    contextIsolation: true,
    nodeIntegration: false,
    sandbox: true,
    webviewTag: false,
},
```

**2.3 添加 CSP 头（app.whenReady() 内，startEngine() 之前）:**

```js
session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
        responseHeaders: {
            ...details.responseHeaders,
            'Content-Security-Policy': [
                "default-src 'self'; connect-src ws://127.0.0.1:* http://127.0.0.1:*; script-src 'self'"
            ]
        }
    });
});
```

**2.4 添加系统托盘（createWindow 之后新增 createTray 函数）:**

```js
let tray = null;
let appIsQuitting = false;

function updateTrayMenu(statusLabel) {
    if (!tray) return;
    const contextMenu = Menu.buildFromTemplate([
        { label: '显示 openLoom', click: () => mainWindow?.show() },
        { type: 'separator' },
        { label: statusLabel, enabled: false },
        { type: 'separator' },
        { label: '退出', click: () => { appIsQuitting = true; app.quit(); }}
    ]);
    tray.setContextMenu(contextMenu);
}

function createTray() {
    // Use a 16x16 empty icon as placeholder (nativeImage.createEmpty())
    const icon = nativeImage.createEmpty();
    tray = new Tray(icon);
    tray.setToolTip('openLoom');
    updateTrayMenu('Agent: Idle');
    tray.on('click', () => mainWindow?.show());
}
```

在 `app.whenReady()` 中 `setTimeout(createWindow, 2000);` 之后调用 `setTimeout(createTray, 3000);`

**2.5 窗口关闭改为隐藏到托盘:**

在 `createWindow()` 函数内，`mainWindow.loadFile(...)` 之后添加：

```js
mainWindow.on('close', (event) => {
    if (!appIsQuitting) {
        event.preventDefault();
        mainWindow.hide();
    }
});
```

**2.6 健康检查轮询（app.whenReady() 内，setTimeout(createTray) 之后）:**

```js
setTimeout(() => {
    setInterval(async () => {
        if (!enginePort) return;
        try {
            const http = require('http');
            const resp = await new Promise((resolve, reject) => {
                http.get(`http://127.0.0.1:${enginePort}/health`, (res) => {
                    let data = '';
                    res.on('data', chunk => data += chunk);
                    res.on('end', () => resolve(JSON.parse(data)));
                }).on('error', reject);
            });
            if (resp.status === 'degraded') {
                updateTrayMenu('Agent: Degraded');
                // Notify renderer about model availability
                if (mainWindow) {
                    mainWindow.webContents.executeJavaScript(
                        `window.__engineStatus__ = 'degraded';`
                    ).catch(() => {});
                }
            } else {
                updateTrayMenu('Agent: Idle');
            }
        } catch {
            updateTrayMenu('Agent: Offline');
        }
    }, 30000);
}, 5000);
```

**2.7 自动启动注册（app.whenReady() 末尾）:**

```js
app.setLoginItemSettings({
    openAtLogin: false,
    path: app.getPath('exe'),
});
```

**2.8 模型下载提示 — 注入 banner 检测变量:**

在 `createWindow()` 的 `mainWindow.loadFile()` 之后：

```js
// Inject engine health status for model download banner
if (enginePort) {
    mainWindow.webContents.on('did-finish-load', () => {
        mainWindow.webContents.executeJavaScript(
            `window.__enginePort__ = ${enginePort};`
        ).catch(() => {});
    });
}
```

- [ ] **Step 3: 验证语法**

Run: `cd electron && node -e "try { require('./main.js') } catch(e) { if (!e.message.includes('electron')) throw e }" 2>&1`

Expected: No syntax errors. Module-level electron imports may fail outside Electron runtime — that's expected.

- [ ] **Step 4: Commit**

```bash
git add electron/main.js
git commit -m "feat(electron): add CSP headers, system tray, health check polling, webviewTag, autostart registration, model download banner"
```

---

### Task 3: Server port/pid 文件 + 僵尸清理

**Files:**
- Modify: `F:/openLoom/crates/server/src/lib.rs`

- [ ] **Step 1: 在 serve() 中添加 port/pid 文件写入**

`F:/openLoom/crates/server/src/lib.rs` — 在 `serve()` 函数的 ready 信号之后、`axum::serve(...)` 之前：

需要先加 `use std::path::PathBuf;` 和 `use std::fs;` 导入。

在 ready 信号 println! 之后添加：

```rust
// Write port/pid files for Electron sidecar lifecycle management
let data_dir = dirs::data_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join("openLoom");
let _ = fs::create_dir_all(&data_dir);

// Cleanup stale files from previous crashed instance
let pid_path = data_dir.join("engine.pid");
if let Ok(pid_str) = fs::read_to_string(&pid_path) {
    if let Ok(pid) = pid_str.trim().parse::<u32>() {
        // Check if process still exists (simple approach: try to read its status)
        #[cfg(unix)]
        {
            // On Unix, sending signal 0 checks existence
            unsafe { libc::kill(pid as i32, 0); }
            let exists = std::io::Error::last_os_error().raw_os_error() != Some(3); // ESRCH
            if !exists {
                let _ = fs::remove_file(&pid_path);
                let _ = fs::remove_file(data_dir.join("engine.port"));
            }
        }
        #[cfg(windows)]
        {
            // On Windows, try OpenProcess
            use std::os::windows::io::RawHandle;
            extern "system" {
                fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> isize;
                fn CloseHandle(handle: isize) -> i32;
            }
            const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
            let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            if handle == 0 {
                let _ = fs::remove_file(&pid_path);
                let _ = fs::remove_file(data_dir.join("engine.port"));
            } else {
                unsafe { CloseHandle(handle); }
            }
        }
    }
}

// Write current port/pid
let _ = fs::write(data_dir.join("engine.port"), bound_addr.port().to_string());
let _ = fs::write(&pid_path, std::process::id().to_string());
```

注意：Windows 上需要 setup 外部函数声明。简化方案——仅写入，不做跨平台进程检测：

```rust
// Simple approach: always write, cleanup attempted on next start
let data_dir = dirs::data_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join("openLoom");
let _ = fs::create_dir_all(&data_dir);
let _ = fs::write(data_dir.join("engine.port"), bound_addr.port().to_string());
let _ = fs::write(data_dir.join("engine.pid"), std::process::id().to_string());
tracing::info!(port = bound_addr.port(), "port/pid files written");
```

**Zombie cleanup on startup** — in `serve()` before binding:

```rust
// Clean stale port/pid from previous crashed instance
let data_dir = dirs::data_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join("openLoom");
let pid_path = data_dir.join("engine.pid");
if pid_path.exists() {
    // Read old pid
    if let Ok(pid_str) = fs::read_to_string(&pid_path) {
        let old_pid: u32 = pid_str.trim().parse().unwrap_or(0);
        // If current process has the same pid (unlikely) or we can't check, just remove
        if old_pid != std::process::id() {
            let _ = fs::remove_file(&pid_path);
            let _ = fs::remove_file(data_dir.join("engine.port"));
            tracing::info!("cleaned up stale port/pid files from previous run");
        }
    }
}
```

**Engine shutdown 时清理** — 当前 `server/src/lib.rs` 没有 shutdown handler。简单方案：在 `axum::serve(listener, app).await?;` 之后（服务停止时）删除文件：

```rust
axum::serve(listener, app).await?;
// Cleanup on exit
let _ = fs::remove_file(data_dir.join("engine.port"));
let _ = fs::remove_file(data_dir.join("engine.pid"));
Ok(())
```

**完整修改后的 `serve()` 方法末尾:**

```rust
// serve() 方法中，ready println! 之后、axum::serve 之前：
let data_dir = dirs::data_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join("openLoom");
let _ = fs::create_dir_all(&data_dir);

// Clean stale files
let pid_path = data_dir.join("engine.pid");
if pid_path.exists() {
    if let Ok(pid_str) = fs::read_to_string(&pid_path) {
        if let Ok(old_pid) = pid_str.trim().parse::<u32>() {
            if old_pid != std::process::id() {
                let _ = fs::remove_file(&pid_path);
                let _ = fs::remove_file(data_dir.join("engine.port"));
            }
        }
    }
}

// Write current
let _ = fs::write(data_dir.join("engine.port"), bound_addr.port().to_string());
let _ = fs::write(&pid_path, std::process::id().to_string());
```

在 `serve()` 末尾 `axum::serve(...)` 之后添加清理：

```rust
let _ = fs::remove_file(data_dir.join("engine.port"));
let _ = fs::remove_file(data_dir.join("engine.pid"));
```

- [ ] **Step 2: 编译 + 测试**

Run: `cargo check -p openloom-server 2>&1 && cargo test -p openloom-server 2>&1`

Expected: 编译通过，4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/lib.rs
git commit -m "feat(server): write port/pid files on startup, cleanup on shutdown, stale file detection"
```

---

### Task 4: 共享 TypeScript 类型声明

**Files:**
- Create: `F:/openLoom/web/src/types/electron.d.ts`

- [ ] **Step 1: 创建类型文件**

`F:/openLoom/web/src/types/electron.d.ts`:

```typescript
export interface OpenLoomAPI {
    send: (method: string, params?: Record<string, unknown>) => Promise<Record<string, unknown>>;
    sseUrl: (sessionId: string) => string;
    subscribe: (event: string, callback: (data: Record<string, unknown>) => void) => () => void;
}

declare global {
    interface Window {
        openloom?: OpenLoomAPI;
        __enginePort__?: number;
        __engineStatus__?: string;
    }
}

export {};
```

- [ ] **Step 2: 验证 TypeScript 编译**

Run: `cd web && npx tsc --noEmit 2>&1`

Expected: No errors related to the new type file. May have pre-existing errors in components (will be fixed in Tasks 5-6).

- [ ] **Step 3: Commit**

```bash
git add web/src/types/electron.d.ts
git commit -m "feat(web): add shared TypeScript type declaration for window.openloom API"
```

---

### Task 5: React 前端组件全连线

**Files:**
- Modify: `F:/openLoom/web/src/App.tsx`
- Modify: `F:/openLoom/web/src/components/Sidebar.tsx`
- Modify: `F:/openLoom/web/src/components/ChatArea.tsx`
- Modify: `F:/openLoom/web/src/components/SettingsPanel.tsx`
- Modify: `F:/openLoom/web/src/components/TokenDashboard.tsx`
- Create: `F:/openLoom/web/src/components/PersonaPanel.tsx`

- [ ] **Step 1: 修改 App.tsx — Engine Session 管理**

读取当前 `F:/openLoom/web/src/App.tsx`。完整替换为：

```tsx
import React, { useState, useEffect } from 'react';
import Sidebar from './components/Sidebar';
import ChatArea from './components/ChatArea';
import SettingsPanel from './components/SettingsPanel';
import TokenDashboard from './components/TokenDashboard';
import PersonaPanel from './components/PersonaPanel';

type View = 'chat' | 'settings' | 'dashboard' | 'persona';

interface Session {
    id: string;
    name: string;
    messageCount: number;
}

export default function App() {
    const [activeView, setActiveView] = useState<View>('chat');
    const [sessions, setSessions] = useState<Session[]>([]);
    const [activeSession, setActiveSession] = useState('');
    const [loading, setLoading] = useState(true);

    useEffect(() => {
        window.openloom?.send('session.list').then((data: any) => {
            const list = (data.sessions || []) as Session[];
            if (list.length > 0) {
                setSessions(list);
                setActiveSession(list[0].id);
            } else {
                window.openloom?.send('session.create').then((s: any) => {
                    const newSession = { id: s.id, name: 'Default', messageCount: 0 };
                    setSessions([newSession]);
                    setActiveSession(s.id);
                });
            }
        }).catch(() => {
            setSessions([{ id: 'default', name: 'Default (offline)', messageCount: 0 }]);
            setActiveSession('default');
        }).finally(() => setLoading(false));
    }, []);

    const handleNewSession = async () => {
        try {
            const s: any = await window.openloom?.send('session.create');
            setSessions(prev => [...prev, { id: s.id, name: `Session ${prev.length + 1}`, messageCount: 0 }]);
            setActiveSession(s.id);
        } catch {
            const id = crypto.randomUUID();
            setSessions(prev => [...prev, { id, name: `Session ${prev.length + 1}`, messageCount: 0 }]);
            setActiveSession(id);
        }
    };

    const handleSwitchSession = async (id: string) => {
        try {
            await window.openloom?.send('session.switch', { session_id: id });
        } catch {}
        setActiveSession(id);
    };

    if (loading) {
        return <div className="flex h-screen items-center justify-center text-gray-400">Connecting to engine...</div>;
    }

    return (
        <div className="flex h-screen">
            <Sidebar
                sessions={sessions}
                activeSession={activeSession}
                onSelectSession={handleSwitchSession}
                onNewSession={handleNewSession}
                onNavigate={setActiveView}
                activeView={activeView}
            />
            <main className="flex-1 flex flex-col">
                {activeView === 'chat' && <ChatArea sessionId={activeSession} />}
                {activeView === 'settings' && <SettingsPanel />}
                {activeView === 'dashboard' && <TokenDashboard />}
                {activeView === 'persona' && <PersonaPanel />}
            </main>
        </div>
    );
}
```

**删除文件中原有的 `declare global { interface Window { openloom?: ... } }` 块**（如有，ChatArea.tsx 中有）。

- [ ] **Step 2: 修改 Sidebar.tsx — Live Session 数据**

读取当前 `F:/openLoom/web/src/components/Sidebar.tsx`，添加 Persona 导航入口 + session 数据展示 messageCount：

现有的 Sidebar 如果已有 sessions 列表和导航，只需在导航项中加 `Persona`。最小改动：在导航按钮列表中添加：

```tsx
<button onClick={() => onNavigate('persona')} className={...}>
    Persona
</button>
```

session 条目显示 messageCount：

```tsx
<span className="text-xs text-gray-500">{session.messageCount} msgs</span>
```

- [ ] **Step 3: 修改 ChatArea.tsx — 添加 subscribe 推送**

读取当前 `F:/openLoom/web/src/components/ChatArea.tsx`。

**3a. 删除文件中的 `declare global { interface Window { openloom?: ... } }` 块**（第 9-17 行）——类型已移至 `electron.d.ts`。

**3b. 添加 agent state 和 token usage 订阅（在 useState 之后）:**

```tsx
const [agentState, setAgentState] = useState('idle');
const [lastUsage, setLastUsage] = useState<{prompt_tokens?: number; completion_tokens?: number} | null>(null);

useEffect(() => {
    const unsub1 = window.openloom?.subscribe('agent.state_changed', (data: any) => {
        setAgentState(data.new_state || 'idle');
    });
    const unsub2 = window.openloom?.subscribe('token.usage', (data: any) => {
        setLastUsage(data);
    });
    const unsub3 = window.openloom?.subscribe('cognition.updated', () => {
        // Could show a toast here
    });
    return () => {
        try { unsub1?.(); } catch {}
        try { unsub2?.(); } catch {}
        try { unsub3?.(); } catch {}
    };
}, []);
```

**3c. 添加 UI 指示器（在输入框上方）:**

```tsx
<div className="flex items-center gap-2 px-4 py-1 text-xs text-gray-400">
    <span className={`w-2 h-2 rounded-full ${
        agentState === 'thinking' ? 'bg-yellow-400 animate-pulse' :
        agentState === 'acting' ? 'bg-blue-400' : 'bg-gray-500'
    }`} />
    Agent: {agentState}
    {lastUsage && (
        <span className="ml-auto">
            ↑{lastUsage.prompt_tokens} ↓{lastUsage.completion_tokens} tokens
        </span>
    )}
</div>
```

- [ ] **Step 4: 修改 SettingsPanel.tsx — Config 读写**

读取当前 `F:/openLoom/web/src/components/SettingsPanel.tsx`。

完整替换为 config 读写版本：

```tsx
import React, { useState, useEffect } from 'react';

interface ConfigData {
    server?: { host?: string };
    logging?: { level?: string };
    agent?: { max_iterations?: number; timeout_secs?: number };
    persona?: { top_n?: number; recency_decay_days?: number };
    rate_limit?: { min_interval_ms?: number };
}

export default function SettingsPanel() {
    const [config, setConfig] = useState<ConfigData | null>(null);
    const [saved, setSaved] = useState(false);

    useEffect(() => {
        window.openloom?.send('config.get').then((data: any) => {
            setConfig(data.config || {});
        }).catch(() => setConfig({}));
    }, []);

    const updateField = async (key: string, value: string | number) => {
        await window.openloom?.send('config.set', { key, value: String(value) });
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
    };

    if (!config) return <div className="p-6 text-gray-400">Loading config...</div>;

    return (
        <div className="p-6 overflow-y-auto">
            <h2 className="text-xl font-bold mb-4">Settings</h2>
            {saved && <div className="mb-4 p-2 bg-green-800 rounded text-sm">Saved</div>}

            <Section title="Server">
                <Field label="Host" value={config.server?.host || '127.0.0.1'}
                    onChange={v => updateField('server.host', v)} />
            </Section>

            <Section title="Logging">
                <Field label="Level" value={config.logging?.level || 'INFO'}
                    onChange={v => updateField('logging.level', v)} />
            </Section>

            <Section title="Agent">
                <Field label="Max Iterations" value={config.agent?.max_iterations ?? 3}
                    onChange={v => updateField('agent.max_iterations', Number(v))} type="number" />
                <Field label="Timeout (seconds)" value={config.agent?.timeout_secs ?? 120}
                    onChange={v => updateField('agent.timeout_secs', Number(v))} type="number" />
            </Section>

            <Section title="Persona">
                <Field label="Top N Traits" value={config.persona?.top_n ?? 5}
                    onChange={v => updateField('persona.top_n', Number(v))} type="number" />
                <Field label="Recency Decay (days)" value={config.persona?.recency_decay_days ?? 30}
                    onChange={v => updateField('persona.recency_decay_days', Number(v))} type="number" />
            </Section>

            <Section title="Rate Limit">
                <Field label="Min Interval (ms)" value={config.rate_limit?.min_interval_ms ?? 100}
                    onChange={v => updateField('rate_limit.min_interval_ms', Number(v))} type="number" />
            </Section>
        </div>
    );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
    return (
        <div className="mb-6">
            <h3 className="text-sm font-semibold text-gray-400 mb-2 uppercase">{title}</h3>
            <div className="space-y-3">{children}</div>
        </div>
    );
}

function Field({ label, value, onChange, type = 'text' }: {
    label: string; value: string | number; onChange: (v: string) => void; type?: string;
}) {
    return (
        <div className="flex items-center justify-between">
            <label className="text-sm">{label}</label>
            <input
                type={type}
                value={value}
                onChange={e => onChange(e.target.value)}
                className="bg-gray-800 border border-gray-600 rounded px-3 py-1 w-48 text-sm outline-none focus:border-blue-500"
            />
        </div>
    );
}
```

- [ ] **Step 5: 修改 TokenDashboard.tsx — 实时 Token 数据**

读取当前 `F:/openLoom/web/src/components/TokenDashboard.tsx`。完整替换为：

```tsx
import React, { useState, useEffect } from 'react';

interface TokenCall {
    model: string;
    prompt_tokens: number;
    completion_tokens: number;
    cached_tokens: number;
    latency_ms: number;
    time: Date;
}

export default function TokenDashboard() {
    const [totalPrompt, setTotalPrompt] = useState(0);
    const [totalCompletion, setTotalCompletion] = useState(0);
    const [totalCached, setTotalCached] = useState(0);
    const [recentCalls, setRecentCalls] = useState<TokenCall[]>([]);

    useEffect(() => {
        const unsub = window.openloom?.subscribe('token.usage', (data: any) => {
            const p = data.prompt_tokens || 0;
            const c = data.completion_tokens || 0;
            const cached = data.cached_tokens || 0;
            setTotalPrompt(prev => prev + p);
            setTotalCompletion(prev => prev + c);
            setTotalCached(prev => prev + cached);
            setRecentCalls(prev => [{
                model: data.model || 'unknown',
                prompt_tokens: p,
                completion_tokens: c,
                cached_tokens: cached,
                latency_ms: data.latency_ms || 0,
                time: new Date(),
            }, ...prev].slice(0, 50));
        });
        return () => { try { unsub?.(); } catch {} };
    }, []);

    const totalTokens = totalPrompt + totalCompletion;
    const savingsRate = totalTokens > 0 ? ((totalCached / totalTokens) * 100).toFixed(1) : '0.0';

    return (
        <div className="p-6 overflow-y-auto">
            <h2 className="text-xl font-bold mb-4">Token Dashboard</h2>

            <div className="grid grid-cols-2 gap-4 mb-6">
                <StatCard label="Total Prompt" value={totalPrompt.toLocaleString()} />
                <StatCard label="Total Completion" value={totalCompletion.toLocaleString()} />
                <StatCard label="Cached Tokens" value={totalCached.toLocaleString()} />
                <StatCard label="Cache Hit Rate" value={`${savingsRate}%`} />
            </div>

            <h3 className="text-lg font-semibold mb-2">Recent Calls</h3>
            <div className="space-y-1 max-h-96 overflow-y-auto">
                {recentCalls.length === 0 && <p className="text-gray-500 text-sm">No data yet. Send a message to start.</p>}
                {recentCalls.map((call, i) => (
                    <div key={i} className="flex justify-between text-sm p-2 bg-gray-800 rounded">
                        <span>{call.model}</span>
                        <span className="text-gray-400">↑{call.prompt_tokens} ↓{call.completion_tokens}</span>
                        <span className="text-gray-500">{call.latency_ms}ms</span>
                        <span className="text-gray-600 text-xs">{call.time.toLocaleTimeString()}</span>
                    </div>
                ))}
            </div>
        </div>
    );
}

function StatCard({ label, value }: { label: string; value: string }) {
    return (
        <div className="p-4 bg-gray-800 rounded-lg">
            <div className="text-gray-400 text-xs mb-1">{label}</div>
            <div className="text-2xl font-bold">{value}</div>
        </div>
    );
}
```

- [ ] **Step 6: 创建 PersonaPanel.tsx**

`F:/openLoom/web/src/components/PersonaPanel.tsx`:

```tsx
import React, { useState, useEffect } from 'react';

interface Trait {
    trait: string;
    value: string;
    confidence: number;
    evidence_count: number;
}

export default function PersonaPanel() {
    const [summary, setSummary] = useState('');
    const [traits, setTraits] = useState<Trait[]>([]);
    const [loading, setLoading] = useState(true);

    const fetchData = async () => {
        setLoading(true);
        try {
            const p: any = await window.openloom?.send('memory.persona');
            setSummary(p.summary || '');
            const c: any = await window.openloom?.send('memory.cognitions', { subject: 'USER', limit: 20 });
            setTraits(c.cognitions || []);
        } catch (e) {
            console.error('Failed to load persona:', e);
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => { fetchData(); }, []);

    return (
        <div className="p-6 overflow-y-auto">
            <div className="flex items-center justify-between mb-4">
                <h2 className="text-xl font-bold">Persona Profile</h2>
                <button onClick={fetchData} className="px-4 py-1 bg-blue-600 rounded text-sm hover:bg-blue-700">
                    Refresh
                </button>
            </div>

            {loading ? (
                <p className="text-gray-400">Loading...</p>
            ) : (
                <>
                    <div className="mb-6 p-4 bg-gray-800 rounded-lg">
                        <p className="text-lg">{summary || 'No persona data yet. Interact more to build a cognition profile.'}</p>
                    </div>

                    <h3 className="text-lg font-semibold mb-2">Traits</h3>
                    {traits.length === 0 ? (
                        <p className="text-gray-500">No cognitive traits discovered yet.</p>
                    ) : (
                        <div className="space-y-2">
                            {traits.map((t, i) => (
                                <div key={i} className="flex items-center justify-between p-3 bg-gray-800 rounded">
                                    <div>
                                        <span className="font-medium">{t.trait}</span>
                                        <span className="text-gray-400 ml-2">{t.value}</span>
                                    </div>
                                    <div className="flex items-center gap-4 text-sm text-gray-400">
                                        <span>{(t.confidence * 100).toFixed(0)}% confidence</span>
                                        <span>{t.evidence_count} events</span>
                                    </div>
                                </div>
                            ))}
                        </div>
                    )}
                </>
            )}
        </div>
    );
}
```

- [ ] **Step 7: 删除 ChatArea.tsx 中的重复类型声明**

`F:/openLoom/web/src/components/ChatArea.tsx` — 删除 `declare global { interface Window { openloom?: { ... } } }` 块（第 9-17 行）。发送消息逻辑和 Markdown 渲染保持不变。

- [ ] **Step 8: TypeScript 编译检查**

Run: `cd web && npx tsc --noEmit 2>&1`

Expected: 零类型错误

- [ ] **Step 9: 前端构建检查**

Run: `cd web && npm run build 2>&1`

Expected: 构建成功，输出到 `web/dist/`

- [ ] **Step 10: Commit**

```bash
git add web/src/App.tsx web/src/components/Sidebar.tsx web/src/components/ChatArea.tsx web/src/components/SettingsPanel.tsx web/src/components/TokenDashboard.tsx web/src/components/PersonaPanel.tsx
git commit -m "feat(web): wire all 6 React components to live Engine API with real-time push via subscribe"
```

---

### Task 6: 最终验证

- [ ] **Step 1: Rust 侧验证**

Run: `cargo test 2>&1 | grep -E "test result|FAIL"`
Expected: 127+ tests PASS

Run: `cargo clippy -- -D warnings 2>&1`
Expected: 0 warnings

Run: `cargo fmt --check 2>&1`
Expected: 格式正确

- [ ] **Step 2: 前端验证**

Run: `cd web && npx tsc --noEmit 2>&1`
Expected: 零错误

Run: `cd web && npm run build 2>&1`
Expected: 构建成功

- [ ] **Step 3: Electron 语法验证**

Run: `cd electron && node -e "try { const src = require('fs').readFileSync('preload.js','utf8'); new Function(src); console.log('preload.js: OK'); } catch(e) { console.log('preload.js parse error:', e.message); }" 2>&1`
Expected: `preload.js: OK`

- [ ] **Step 4: 最终 commit**

```bash
git add -A
git commit -m "chore: Phase 2 Milestone D complete — Electron full GUI, persistent WebSocket, real-time push, all tests pass

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## 完成检查清单

- [ ] `cargo test` — 所有 Rust 测试通过
- [ ] `cargo clippy -- -D warnings` — 零警告
- [ ] `cargo fmt --check` — 格式正确
- [ ] `cd web && npx tsc --noEmit` — 零 TypeScript 错误
- [ ] `cd web && npm run build` — 前端构建成功
- [ ] `cd electron && node preload-syntax-check` — 无 JS 语法错误
- [ ] Electron 手动启动 → 侧边栏 session → 聊天收发 → 设置修改 → 仪表盘实时推送 → 画像查看

# Phase 2 Milestone D: Electron + Frontend Integration — 设计规范

**版本:** 1.0
**日期:** 2026-05-20
**状态:** 设计完成，待实现
**前置:** Phase 2 Milestone C (已完成)

---

## 1. 目标

完成 Phase 2 "完整 Electron GUI" 交付。将 React 前端全连线到 Engine API，preload 实现持久 WebSocket + 真实 push 订阅，Electron 主进程加固（CSP/系统托盘/健康检查/模型下载向导/自动启动）。

**核心交付:**
- preload.js 持久 WebSocket（替代每次新建）+ subscribe() 真实实现 + 指数退避重连
- React 前端 6 组件全连线：Session 管理 / ChatArea 推送 / Settings config / TokenDashboard 实时 / Sidebar / PersonaPanel
- Electron 主进程加固：CSP 头、系统托盘、健康检查轮询、模型下载提示、自动启动注册

**明确不做:**
- 模型下载实际实现（Phase 3，Milestone D 做 UI 框架 + 提示）
- 跨平台打包 (Phase 3)
- 安全沙箱 (Phase 3)
- 认知审核/回滚面板 (Phase 3)

---

## 2. 文件变更

```
F:/openLoom/
├── electron/
│   ├── main.js              ← [Modify] +CSP +tray +health_check +model_wizard +autostart
│   └── preload.js           ← [Rewrite] 持久WS + subscribe + 重连
├── web/src/
│   ├── App.tsx              ← [Modify] session 从 engine API
│   ├── types/
│   │   └── electron.d.ts    ← [Create] 共享 window.openloom 类型声明
│   ├── components/
│   │   ├── ChatArea.tsx     ← [Modify] +subscribe 推送 + SSE 说明
│   │   ├── Sidebar.tsx      ← [Modify] session live data
│   │   ├── SettingsPanel.tsx← [Modify] config.get/set
│   │   ├── TokenDashboard.tsx← [Modify] 实时 token 数据
│   │   └── PersonaPanel.tsx ← [Create] 认知画像可视化
│   └── index.css            ← [Modify] 可能的新样式
├── crates/server/src/
│   └── lib.rs               ← [Modify] +port/pid 文件写入 + 僵尸清理
└── docs/superpowers/specs/
    └── 2026-05-20-phase2-milestone-d-design.md  ← 本文件
```

---

## 3. 详细设计

### 3.1 preload.js — 持久 WebSocket + 真实 subscribe

**架构:**

```
preload.js
├── 持久 WebSocket: 首次 send() 或 subscribe() 时建连，后续复用
├── send(method, params):
│    复用 ws，发 JSON-RPC request，id→Promise 映射收回复
├── subscribe(eventType, callback):
│    注册 callback，ws.onmessage 匹配 notification.method → 调 callback
│    返回 unsubscribe 函数
├── 重连: ws.onclose 时指数退避 + jitter 重连
│    重连后 subscriber 自动生效（callback 在内存中）
│    minUptime 阈值（连接持续 < 3s 断开 → 加大退避）
└── sseUrl(): 不变
```

**关键数据结构:**

```js
let ws = null;
let msgId = 0;
const pending = new Map();       // id → { resolve, reject, timer }
const subscribers = new Map();   // method → Set<callback>
let reconnectAttempt = 0;
let connectTime = null;
const MIN_UPTIME = 3000;        // 3 seconds
```

**send() 流程:**

```
send('chat.send', params)
  → 确保 ws 连接（未连则先连）
  → msgId++, 设 30s 超时 timer
  → pending.set(id, {resolve, reject, timer})
  → ws.send(JSON.stringify({jsonrpc:'2.0', method, params, id}))
  → ws.onmessage: 收到回复 → clearTimeout → pending.get(id).resolve(result)
```

**subscribe() 流程:**

```
subscribe('token.usage', callback)
  → 确保 ws 连接
  → subscribers.get('token.usage')?.add(callback)
  → ws.onmessage: 收到 notification (无 id 字段)
    → method = data.method
    → subscribers.get(method)?.forEach(cb => cb(data.params))
  → 返回 unsubscribe 函数: () => subscribers.get(method)?.delete(callback)
```

**重连逻辑:**

```
ws.onclose:
  → 如果 connectTime 存在且 elapsed < MIN_UPTIME: reconnectAttempt += 2
  → delay = min(1000 * 1.5^reconnectAttempt + random(0, 1000), 30000)
  → setTimeout(() => connect(), delay)
  → 重连成功: reconnectAttempt = 0, connectTime = Date.now()
```

### 3.2 Electron 主进程加固

#### 3.2.1 CSP 头

在 `app.whenReady()` 中注册：

```js
const { session } = require('electron');
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

#### 3.2.2 系统托盘

```js
const { Tray, Menu, nativeImage } = require('electron');
let tray = null;

function createTray() {
    // 16x16 纯色 PNG (data URI) 作为托盘图标
    tray = new Tray(/* 默认图标 */);
    const contextMenu = Menu.buildFromTemplate([
        { label: '显示 openLoom', click: () => mainWindow?.show() },
        { type: 'separator' },
        { label: 'Agent: Idle', enabled: false },
        { type: 'separator' },
        { label: '退出', click: () => { 
            app.isQuitting = true;
            app.quit();
        }}
    ]);
    tray.setToolTip('openLoom');
    tray.setContextMenu(contextMenu);
    tray.on('click', () => mainWindow?.show());
}
```

窗口关闭事件改为隐藏到托盘：

```js
mainWindow.on('close', (event) => {
    if (!app.isQuitting) {
        event.preventDefault();
        mainWindow.hide();
    }
});
```

#### 3.2.3 健康检查轮询

```js
let healthCheckInterval = null;

function startHealthCheck() {
    healthCheckInterval = setInterval(async () => {
        try {
            const resp = await fetch(`http://127.0.0.1:${enginePort}/health`);
            const health = await resp.json();
            // 更新托盘状态
            if (health.status === 'degraded') {
                updateTrayStatus('Agent: Degraded');
            } else {
                updateTrayStatus('Agent: Idle');
            }
        } catch {
            // Engine 不可达，触发崩溃恢复
            updateTrayStatus('Agent: Offline');
        }
    }, 30000);
}

function updateTrayStatus(label) {
    if (!tray) return;
    const menu = Menu.buildFromTemplate([
        { label: '显示 openLoom', click: () => mainWindow?.show() },
        { type: 'separator' },
        { label, enabled: false },
        { type: 'separator' },
        { label: '退出', click: () => { app.isQuitting = true; app.quit(); }}
    ]);
    tray.setContextMenu(menu);
}
```

#### 3.2.4 模型下载提示

Engine health_check 返回 `status: "degraded"` 时，渲染进程显示下载引导 UI。Milestone D 实现：

- `ChatArea` 或其他组件检测到 engine status degraded → 显示 banner: "本地模型未安装 — 下载模型以获得离线推理能力"
- 点击 → 打开外部链接到 Hugging Face / ModelScope（Phase 3 做进程内下载进度）

#### 3.2.5 自动启动注册

```js
app.setLoginItemSettings({
    openAtLogin: false,  // 默认关，Settings 面板可切换
    path: app.getPath('exe'),
});
```

Settings 面板加 `openAtLogin` 开关，调 `app.setLoginItemSettings({ openAtLogin: true/false })`。

#### 3.2.6 Port/PID 文件 + 僵尸清理

Engine 启动时将端口和 PID 写入 `~/.openloom/engine.port` 和 `~/.openloom/engine.pid`。下次启动时检测旧 pid 文件，如进程已不存在则清理。

**Engine 侧（server/src/lib.rs 新增）:**
```rust
// serve() 中 ready 信号之后
let data_dir = dirs::data_dir().unwrap_or_default().join("openLoom");
std::fs::create_dir_all(&data_dir)?;
std::fs::write(data_dir.join("engine.port"), bound_addr.port().to_string())?;
std::fs::write(data_dir.join("engine.pid"), std::process::id().to_string())?;
```

**Engine 启动时清理（server/src/lib.rs serve() 开始处）:**
```rust
// 检测并清理上一次未正常退出的僵尸文件
if let Ok(pid_str) = std::fs::read_to_string(data_dir.join("engine.pid")) {
    if let Ok(pid) = pid_str.trim().parse::<u32>() {
        // 检查该 pid 是否仍存活（Windows: 检查进程是否存在）
        // 如果不存在 → 删除 engine.port + engine.pid
    }
}
```

Engine shutdown 时删除这两个文件。

#### 3.2.7 webviewTag 安全加固

`BrowserWindow` 构造中加 `webviewTag: false`（spec Section 13.1 要求，当前缺失）:

```js
webPreferences: {
    preload: path.join(__dirname, 'preload.js'),
    contextIsolation: true,
    nodeIntegration: false,
    sandbox: true,
    webviewTag: false,  // NEW
},
```

### 3.3 React 前端

#### 3.3.0 共享类型声明 — `web/src/types/electron.d.ts`

**新建文件**，避免每个组件重复 `declare global { interface Window { openloom?: ... } }`：

```typescript
// web/src/types/electron.d.ts
export interface OpenLoomAPI {
    send: (method: string, params?: Record<string, unknown>) => Promise<Record<string, unknown>>;
    sseUrl: (sessionId: string) => string;
    subscribe: (event: string, callback: (data: Record<string, unknown>) => void) => () => void;
}

declare global {
    interface Window {
        openloom?: OpenLoomAPI;
    }
}
export {};
```

所有组件删除自己文件内的 `declare global` 块，统一引用此文件。

#### 3.3.1 App.tsx — Engine Session 管理

```tsx
export default function App() {
    const [sessions, setSessions] = useState<Session[]>([]);
    const [activeSession, setActiveSession] = useState('');
    const [loading, setLoading] = useState(true);

    useEffect(() => {
        // 加载 session 列表
        window.openloom?.send('session.list').then(data => {
            const list = data.sessions as Session[];
            if (list.length > 0) {
                setSessions(list);
                setActiveSession(list[0].id);
            } else {
                // 创建默认 session
                window.openloom?.send('session.create').then(s => {
                    setSessions([{ id: s.id, name: 'Default', messageCount: 0 }]);
                    setActiveSession(s.id);
                });
            }
        }).finally(() => setLoading(false));
    }, []);

    const handleNewSession = async () => {
        const s = await window.openloom?.send('session.create');
        setSessions(prev => [...prev, { id: s.id, name: `Session ${prev.length + 1}`, messageCount: 0 }]);
        setActiveSession(s.id);
    };

    const handleSwitchSession = async (id: string) => {
        await window.openloom?.send('session.switch', { session_id: id });
        setActiveSession(id);
    };

    // ...
}
```

#### 3.3.2 ChatArea.tsx — 推送通知

```tsx
useEffect(() => {
    const unsubs: (() => void)[] = [];
    
    unsubs.push(window.openloom?.subscribe('agent.state_changed', (data) => {
        setAgentState(data.new_state);  // 'idle' | 'thinking' | 'acting'
    }) || (() => {}));

    unsubs.push(window.openloom?.subscribe('token.usage', (data) => {
        setLastUsage(data);
    }) || (() => {}));

    unsubs.push(window.openloom?.subscribe('cognition.updated', (data) => {
        setNotification(`认知更新: ${data.trait}`);
    }) || (() => {}));

    return () => unsubs.forEach(fn => { try { fn(); } catch {} });
}, []);
```

UI 变化：输入框上方显示 agent state 指示器（Idle=灰点，Thinking=旋转动画，Acting=蓝点），消息旁显示 token 用量。

**SSE 流式输出说明：** Spec Section 12 定义的 `chat.stream` SSE 端点当前在 `server/src/sse.rs` 中是 stub（返回单条 ready 事件）。流式 token 需要在 InferenceEngine 支持真实模型加载后才可实现，归入 Phase 3。Milestone D 的前端 ChatArea 使用 `stream: false` 模式（等待完整回复后一次性渲染），与当前后端能力一致。

#### 3.3.3 Sidebar.tsx — 实时 Session

- Props 改为接收 sessions 数组 + onCreate/onSwitch handler
- 每个 session 显示 id 前 8 位 + message count
- 新建按钮调 `window.openloom?.send('session.create')`
- 切换调 `window.openloom?.send('session.switch', {session_id})`

#### 3.3.4 SettingsPanel.tsx — Config 读写

```tsx
const [config, setConfig] = useState<any>(null);

useEffect(() => {
    window.openloom?.send('config.get').then(data => setConfig(data.config));
}, []);

const updateField = async (key: string, value: string) => {
    await window.openloom?.send('config.set', { key, value });
    setConfig(prev => ({ ...prev, /* optimistic update */ }));
};
```

支持的配置项：
- `server.host` (string)
- `logging.level` (select: INFO/DEBUG/WARN/ERROR)
- `agent.max_iterations` (number)
- `agent.timeout_secs` (number)
- `persona.top_n` (number)
- `persona.recency_decay_days` (number)
- `rate_limit.min_interval_ms` (number)
- `openAtLogin` (boolean, 调 Electron API)

#### 3.3.5 TokenDashboard.tsx — 实时 Token 数据

```tsx
const [stats, setStats] = useState({ totalPrompt: 0, totalCompletion: 0, recentCalls: [] as any[] });

useEffect(() => {
    const unsub = window.openloom?.subscribe('token.usage', (data) => {
        setStats(prev => ({
            totalPrompt: prev.totalPrompt + (data.prompt_tokens || 0),
            totalCompletion: prev.totalCompletion + (data.completion_tokens || 0),
            recentCalls: [{ model: data.model, prompt: data.prompt_tokens, completion: data.completion_tokens, latency: data.latency_ms, time: new Date() }, ...prev.recentCalls].slice(0, 50),
        }));
    });
    return () => { try { unsub?.(); } catch {} };
}, []);
```

#### 3.3.6 [新建] PersonaPanel.tsx — 认知画像

```tsx
export default function PersonaPanel() {
    const [summary, setSummary] = useState('');
    const [traits, setTraits] = useState<any[]>([]);

    useEffect(() => {
        window.openloom?.send('memory.persona').then(data => setSummary(data.summary));
        window.openloom?.send('memory.cognitions', { subject: 'USER', limit: 20 }).then(data => setTraits(data.cognitions));
    }, []);

    const refresh = async () => {
        const p = await window.openloom?.send('memory.persona');
        setSummary(p.summary);
        const c = await window.openloom?.send('memory.cognitions', { subject: 'USER', limit: 20 });
        setTraits(c.cognitions);
    };

    return (
        <div className="p-6">
            <div className="mb-4 p-4 bg-gray-800 rounded-lg">
                <p className="text-lg">{summary || '暂无画像数据，多聊几句吧'}</p>
            </div>
            <div className="space-y-2">
                {traits.map((t: any, i: number) => (
                    <div key={i} className="flex justify-between p-2 bg-gray-750 rounded">
                        <span>{t.trait}: {t.value}</span>
                        <span className="text-gray-400">{(t.confidence * 100).toFixed(0)}%</span>
                    </div>
                ))}
            </div>
            <button onClick={refresh} className="mt-4 px-4 py-2 bg-blue-600 rounded">刷新</button>
        </div>
    );
}
```

Sidebar 导航加 "Persona" 入口。

---

## 4. 数据流

```
Electron Main Process
  ├── spawn openloom-engine serve → stdout {"type":"ready","port":NNNN}
  ├── inject window.__enginePort__ = NNNN
  └── 每 30s 轮询 GET /health → 更新托盘状态

Renderer (React)
  ├── 挂载 → send('session.list') / 'session.create'
  ├── send('chat.send', ...) → 发送消息
  ├── send('config.get/set') → 读写配置
  ├── send('memory.persona/cognitions') → 获取画像
  └── subscribe('token.usage'/'agent.state_changed'/'cognition.updated') → 接收推送

preload.js (Persistent WebSocket)
  ├── ws.onopen → 连接 ws://127.0.0.1:{port}/ws
  ├── ws.onmessage → JSON-RPC response → resolve pending[id]
  │                → JSON-RPC notification → subscribers[method].forEach(cb)
  └── ws.onclose → 指数退避 + jitter 重连
```

---

## 5. 错误处理

| 场景 | 策略 |
|------|------|
| Engine 未就绪时 send() | 等待 ws 连接（最多 10s），超时 reject |
| send() 30s 无回复 | pending timer 超时 → reject + 清理 |
| WebSocket 断开 | 自动重连，pending 的请求全部 reject |
| subscribe 回调抛异常 | catch + console.error，不影响其他 subscriber |
| Engine health 不可达 | 托盘显示 "Offline"，触发崩溃恢复（已有） |
| config.toml 不存在 | config.get 返回默认值（后端已处理） |
| Persona 无数据 | 显示 "暂无画像数据" |

---

## 6. 测试策略

| 层级 | 内容 |
|------|------|
| 手动 E2E | Electron 启动 → 侧边栏 session 列表 → 聊天收发 → 设置修改 → 仪表盘实时推送 → 画像查看 |
| 单元测试 | preload send/subscribe/reconnect 逻辑（用 mock WebSocket） |
| 类型检查 | `cd web && npx tsc --noEmit` 零错误 |

---

## 7. 依赖关系

```
preload.js (无依赖，独立)
  ↓
React 组件 (依赖 preload 的 send/subscribe API)
  ↓
Electron main.js (依赖 Engine serve 端点)
```

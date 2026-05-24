/**
 * app-init.ts — Loom 应用初始化
 *
 * 连接 Loom 引擎 WebSocket → 加载 agents → 加载 sessions → 监听 WS 事件
 */

import { useStore } from './stores';
import { loomRpc, loomSubscribe } from './adapter';
import { loadSessions } from './stores/session-actions';
import { loadAvatars } from './stores/agent-actions';
import { handleServerMessage } from './services/ws-message-handler';
import { loadModels } from './utils/ui-helpers';

let _initialized = false;

function hasPreload(): boolean {
  return !!(window as any).openloom;
}

/** 等待 __enginePort__ 被主进程注入（最多 maxWaitMs） */
async function waitForEnginePort(maxWaitMs = 5000): Promise<number> {
  const deadline = Date.now() + maxWaitMs;
  return new Promise((resolve, reject) => {
    function check() {
      const port = (window as any).__enginePort__;
      if (port && port > 0) {
        resolve(port as number);
        return;
      }
      if (Date.now() > deadline) {
        reject(new Error('Timeout waiting for __enginePort__'));
        return;
      }
      setTimeout(check, 300);
    }
    check();
  });
}

export async function initApp(): Promise<void> {
  if (_initialized) return;
  _initialized = true;

  // Browser mode (no Electron preload): skip backend init, show welcome
  if (!hasPreload()) {
    console.log('[init] Browser mode — no Electron preload, backend unavailable');
    useStore.setState({
      connected: false,
      wsState: 'disconnected',
      statusKey: 'status.browserMode',
      welcomeVisible: true,
    });
    return;
  }

  // Electron mode: wait for engine port from main process
  let port: number;
  try {
    port = await waitForEnginePort(5000);
    console.log('[init] Engine port:', port);
    useStore.setState({
      connected: false,
      wsState: 'reconnecting',
      statusKey: 'status.connecting',
    });
  } catch (err) {
    console.warn('[init] Engine port not injected within 5s');
    useStore.setState({ connected: false, wsState: 'disconnected', statusKey: 'status.disconnected' });
    return;
  }

  await bootApp();
}

async function bootApp(): Promise<void> {
  // ── 2. 拉 system.health（带重试，WS 刚建立可能需要稍等） ──
  let healthOk = false;
  for (let attempt = 0; attempt < 5; attempt++) {
    try {
      const health = await loomRpc('system.health');
      console.log('[init] system.health ok:', health);
      if (health) {
        const s = useStore.getState();
        useStore.setState({
          connected: true,
          wsState: 'connected',
          statusKey: 'status.connected',
          currentAgentId: health.agentId ?? s.currentAgentId,
          agentName: health.agentName ?? s.agentName ?? 'Loom',
          agentYuan: health.yuan ?? s.agentYuan ?? 'loom',
        });
        if (health.avatars) loadAvatars(health.avatars);
        if (health.agents) useStore.setState({ agents: health.agents });
      } else {
        useStore.setState({ connected: true, wsState: 'connected', statusKey: 'status.connected' });
      }
      healthOk = true;
      break;
    } catch (err) {
      console.warn(`[init] system.health attempt ${attempt + 1} failed:`, err);
      await new Promise(r => setTimeout(r, 1000));
    }
  }

  if (!healthOk) {
    console.warn('[init] system.health failed after 5 attempts, continuing anyway');
    useStore.setState({ connected: false, wsState: 'disconnected', statusKey: 'status.disconnected' });
  }

  // ── 2.5. 加载用户名 ──
  try {
    const config = await loomRpc('config.get', { key: 'settings' });
    const userName = config?.config?.user?.name || config?.user?.name;
    if (userName) {
      useStore.getState().setUserName(userName);
    }
  } catch { /* config not available yet */ }

  // ── 3. 加载 session 列表 ──
  try {
    await loadSessions();
  } catch (err) {
    console.warn('[init] loadSessions failed:', err);
    useStore.setState({ welcomeVisible: true });
  }

  // ── 3.5. 加载模型列表 ──
  try {
    await loadModels();
  } catch (err) {
    console.warn('[init] loadModels failed:', err);
  }

  // ── 4. 订阅 WS 推送事件 ──
  // Electron 环境：通过 preload IPC 桥接收后端事件（loomSubscribe '*'）
  // 直连 WS 环境：通过 websocket.ts 接收（ws-message-handler 已在 websocket.ts 内部调用）
  // 两者不能同时启用，否则每条消息会被 handleServerMessage 处理两次导致重复渲染
  if (hasPreload()) {
    loomSubscribe('*', (data: any) => {
      try { handleServerMessage(data); } catch (e) { console.error('[ws] handler error:', e); }
    });
  }
  // 非 Electron 环境下 websocket.ts 的 connectWebSocket 会自行处理消息

  // 通知 Electron 主进程渲染器就绪
  try { await (window as any).hana?.appReady?.(); } catch {}
}

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

/** 等待 WebSocket 连接就绪（轮询 store.wsState） */
async function waitForConnection(maxWaitMs = 15_000): Promise<boolean> {
  const deadline = Date.now() + maxWaitMs;
  while (Date.now() < deadline) {
    if (useStore.getState().wsState === 'connected') return true;
    await new Promise(r => setTimeout(r, 300));
  }
  return false;
}

async function bootApp(): Promise<void> {
  // Wait for WebSocket to actually connect before sending any RPC
  const connected = await waitForConnection(15_000);
  if (!connected) {
    console.warn('[init] WebSocket did not connect within 15s, continuing anyway');
    useStore.setState({ connected: false, wsState: 'disconnected', statusKey: 'status.disconnected' });
  }

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

  // ── 2.5. 加载用户名 + workspace + agents ──
  try {
    const config = await loomRpc('config.get', { key: 'settings' });
    const cfg = config?.config || config || {};
    const userName = cfg?.user?.name;
    if (userName) {
      useStore.getState().setUserName(userName);
    }
    // Load agents first to determine current agent
    const agentsData = await loomRpc('agent.list');
    const agents = agentsData?.agents || [];
    if (agents.length > 0) {
      useStore.setState({ agents });
      // Determine current agent: read from saved currentAgentId or use primary
      const savedAgentId = cfg?.currentAgentId;
      const primary = agents.find((a: any) => a.isPrimary) || agents[0];
      const currentId = savedAgentId || primary?.id || 'default';
      const currentAgent = agents.find((a: any) => a.id === currentId) || primary || agents[0];
      if (currentAgent && !useStore.getState().currentAgentId) {
        useStore.setState({
          currentAgentId: currentAgent.id,
          agentName: currentAgent.name || 'Loom',
          agentYuan: currentAgent.yuan || 'loom',
        });
      }
    }
    // Load workspace folder: try current agent's config, then cwd_history, then default agent
    const s = useStore.getState();
    let homeFolder: string | null = null;
    const currentId = s.currentAgentId || 'default';
    // Try current agent's home_folder
    const agentHome = cfg?.agent?.[currentId]?.config?.desk?.home_folder;
    if (agentHome) homeFolder = agentHome;
    // Try cwd_history (last used workspace)
    if (!homeFolder && Array.isArray(cfg?.cwd_history) && cfg.cwd_history.length > 0) {
      homeFolder = cfg.cwd_history[0];
    }
    // Fallback to default agent's home_folder
    if (!homeFolder) {
      const defaultHome = cfg?.agent?.default?.config?.desk?.home_folder;
      if (defaultHome) homeFolder = defaultHome;
    }
    if (homeFolder) {
      useStore.setState({
        homeFolder,
        selectedFolder: homeFolder,
        deskBasePath: homeFolder,
      });
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

  // ── 3.6. 恢复工作台 ──
  const workspacedir = useStore.getState().selectedFolder || useStore.getState().homeFolder;
  if (workspacedir) {
    try {
      const { activateWorkspaceDesk } = await import('./stores/desk-actions');
      await activateWorkspaceDesk(workspacedir);
    } catch (err) {
      console.warn('[init] restore workspace failed:', err);
    }
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

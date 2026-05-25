import { getEnginePort, loomRpc } from '../adapter';

const DEFAULT_TIMEOUT = 30_000;

/**
 * 构建 Loom 本地引擎 HTTP URL
 */
export function hanaUrl(path: string): string {
  const port = getEnginePort();
  return `http://127.0.0.1:${port}${path}`;
}

/**
 * hanaFetch — 优先走 loomRpc 桥接（无 CORS 问题），不匹配的 fallback 到 HTTP
 *
 * 将 openhanako 的 REST 路径映射到 openLoom 的 loomRpc 调用。
 * 匹配不到的路径 fallback 到 HTTP fetch（需后端 CORS 支持）。
 */
export async function hanaFetch(
  path: string,
  opts: RequestInit & { timeout?: number; throwOnHttpError?: boolean } = {},
): Promise<Response> {
  // Try loomRpc bridge first
  const rpcResult = await tryRpcBridge(path, opts);
  if (rpcResult !== null) return rpcResult;

  // Fallback to HTTP
  const {
    timeout = DEFAULT_TIMEOUT,
    signal: callerSignal,
    throwOnHttpError = true,
    ...fetchOpts
  } = opts;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);
  if (callerSignal) {
    if (callerSignal.aborted) controller.abort();
    else callerSignal.addEventListener('abort', () => controller.abort(), { once: true });
  }

  try {
    const res = await fetch(hanaUrl(path), {
      ...fetchOpts,
      signal: controller.signal,
    });
    if (throwOnHttpError && !res.ok) {
      throw new Error(`hanaFetch ${path}: ${res.status} ${res.statusText}`);
    }
    return res;
  } finally {
    clearTimeout(timer);
  }
}

function json(data: any, status = 200): Response {
  return new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });
}

function parseBody(opts: RequestInit): any {
  if (!opts.body) return {};
  try { return JSON.parse(String(opts.body)); } catch { return {}; }
}

/**
 * Try to map a REST path to a loomRpc call. Returns Response if mapped, null if not.
 */
async function tryRpcBridge(path: string, opts: RequestInit): Promise<Response | null> {
  const method = opts.method?.toUpperCase() || 'GET';
  const body = method !== 'GET' ? parseBody(opts) : {};

  // Only try RPC if the bridge is available
  if (typeof window === 'undefined' || !window.openloom) return null;

  try {
    // ── File upload ──
    if (path === '/api/upload-blob' && method === 'POST') {
      const r = await loomRpc('file.upload_blob', {
        name: body.name,
        base64Data: body.base64Data,
        mimeType: body.mimeType,
        sessionPath: body.sessionPath || '',
      });
      return json(r);
    }

    // ── Models ──
    if (path === '/api/models') {
      const r = await loomRpc('model.list');
      return json(r);
    }
    if (path === '/api/models/set' && method === 'POST') {
      const r = await loomRpc('model.switch', { model_id: body.modelId || body.id, provider: body.provider });
      return json(r);
    }
    if (path === '/api/models/switch' && method === 'POST') {
      const r = await loomRpc('model.switch', { model_id: body.modelId || body.id, provider: body.provider, session_path: body.sessionPath });
      return json({ ok: true, model: { id: body.modelId || body.id, provider: body.provider } });
    }

    // ── Skills ──
    if (path.startsWith('/api/skills')) {
      const agentId = new URLSearchParams(path.split('?')[1] || '').get('agentId') || 'default';
      const r = await loomRpc('skill.list');
      return json(r);
    }

    // ── Browser session states (stub) ──
    if (path === '/api/browser/session-states') {
      return json({});
    }

    // ── Permission mode ──
    if (path === '/api/session-permission-mode') {
      if (method === 'POST') {
        const r = await loomRpc('session.set_permission_mode', {
          mode: body.mode || 'ask',
          session_id: body.sessionPath || '',
          pending_new_session: body.pendingNewSession === true,
        });
        return json(r);
      }
      const r = await loomRpc('session.permission_mode', {
        session_id: body.sessionPath || '',
      });
      return json(r);
    }

    // ── Context usage ──
    if (path === '/api/context-usage') {
      const r = await loomRpc('context_usage', {});
      return json(r || { used: 0, total: 128000 });
    }

    // ── Sessions ──
    if (path === '/api/sessions/latest-user-message/replay' && method === 'POST') {
      const r = await loomRpc('chat.replay', { session_id: body.path || body.session_id });
      return json(r);
    }

    // ── Context usage ──
    // (handled via direct loomRpc call in session-actions.ts, not hanaFetch)

    // ── Plugins (stubs) ──
    if (path.startsWith('/api/plugins/pages')) return json({ pages: [] });
    if (path.startsWith('/api/plugins/widgets')) return json({ widgets: [] });
    if (path.startsWith('/api/plugins/ui-host-capabilities')) return json({ capabilities: {} });

    // ── Preferences ──
    if (path.startsWith('/api/preferences/workspace-ui-state')) {
      if (method === 'PUT') {
        try {
          const existing = await loomRpc('config.get', { key: 'settings.workspace-ui-state' });
          const all = existing?.config ?? {};
          const ws = body.workspace || '';
          const surface = body.surface || 'electron';
          if (!all[ws]) all[ws] = {};
          all[ws][surface] = body.state ?? {};
          await loomRpc('config.set', { key: 'settings.workspace-ui-state', value: all });
        } catch {
          const ws = body.workspace || '';
          const surface = body.surface || 'electron';
          await loomRpc('config.set', {
            key: 'settings.workspace-ui-state',
            value: { [ws]: { [surface]: body.state ?? {} } },
          });
        }
        return json({ ok: true });
      }
      const qs = new URLSearchParams(path.split('?')[1] || '');
      const workspace = qs.get('workspace') || '';
      const surface = qs.get('surface') || 'electron';
      try {
        const r = await loomRpc('config.get', { key: 'settings.workspace-ui-state' });
        const all = r?.config ?? r;
        const state = all?.[workspace]?.[surface] ?? null;
        return json({ state });
      } catch {
        return json({ state: null });
      }
    }
    if (path === '/api/preferences/plugin-ui') {
      if (method === 'PUT') {
        await loomRpc('config.set', { key: 'settings.plugin-ui', value: body });
        return json({ ok: true });
      }
      return json({});
    }

    // ── Channels (stubs) ──
    if (path.startsWith('/api/channels') || path.startsWith('/api/dm')) {
      return json({});
    }

    // ── Access/Security (stubs) ──
    if (path.startsWith('/api/access')) return json({});
    if (path.startsWith('/api/checkpoints')) return json({ checkpoints: [] });

    // ── Search verify ──
    if (path.startsWith('/api/search')) {
      const hana = (window as any).hana;
      if (hana?.searchVerify) {
        const result = await hana.searchVerify(body);
        return json(result);
      }
      return json({ ok: false, error: 'Desktop bridge not available' });
    }

    // ── Thinking level ──
    if (path === '/api/session-thinking-level' && method === 'POST') {
      await loomRpc('session.thinking_level.set', { session_id: body.sessionPath, level: body.level });
      return json({ ok: true, thinkingLevel: body.level });
    }
    if (path.startsWith('/api/thinking-level')) return json({ ok: true });

    // ── Desk / Workspace file listing ──
    if (path.startsWith('/api/desk/files') && method === 'GET') {
      const qs = new URLSearchParams(path.split('?')[1] || '');
      const dir = qs.get('dir') || '';
      const subdir = qs.get('subdir') || '';
      const r = await loomRpc('desk.list', { dir, subdir });
      return json(r);
    }
    if (path === '/api/desk/files' && method === 'POST') {
      const actionMap: Record<string, string> = {
        create: 'desk.create_file',
        mkdir: 'desk.create_dir',
        rename: 'desk.rename',
        move: 'desk.move',
        safeDelete: 'desk.delete_item',
      };
      const rpcMethod = actionMap[body.action] || 'desk.update_item';
      const r = await loomRpc(rpcMethod, body);
      return json(r);
    }
    if (path.startsWith('/api/desk/jian')) {
      if (method === 'POST') {
        const r = await loomRpc('desk.update_note', { content: body.content });
        return json(r);
      }
      const r = await loomRpc('config.get', { key: 'settings.desk.jian' });
      return json({ content: r?.config || null });
    }
    if (path.startsWith('/api/desk/search-files')) {
      const qs = new URLSearchParams(path.split('?')[1] || '');
      const r = await loomRpc('desk.search', { dir: qs.get('dir') || '', q: qs.get('q') || '' });
      return json(r);
    }
    if (path.startsWith('/api/desk/skills')) {
      // stub — skills are loaded by the engine
      return json({ skills: [] });
    }

    // ── Config / workspace history ──
    if (path === '/api/config/workspaces/recent' && method === 'POST') {
      await loomRpc('config.set', { key: 'settings.cwd_history', value: body.path ? [body.path] : [] });
      return json({ cwd_history: body.path ? [body.path] : [] });
    }

    // Not mapped — let HTTP fallback handle it
    return null;
  } catch (err: any) {
    console.warn(`[hanaFetch] RPC bridge failed for ${path}:`, err);
    return null;
  }
}

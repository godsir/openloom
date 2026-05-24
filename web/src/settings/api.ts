/**
 * Settings API utilities — openLoom adapter
 * 将 openhanako 的 REST 风格 hanaFetch 调用桥接到 openLoom 的 loomRpc
 * 所有请求走 IPC，无 CORS 问题
 */
import { loomRpc } from '../adapter';
import { useSettingsStore } from './store';
import { yuanFallbackAvatarUrl } from '../utils/yuan-avatar-map';

/** Deep merge overlay into base. Arrays are replaced, objects are recursively merged. */
function deepMerge(base: any, overlay: any): any {
  if (!base || typeof base !== 'object' || Array.isArray(base)) return overlay;
  if (!overlay || typeof overlay !== 'object' || Array.isArray(overlay)) return overlay;
  const result = { ...base };
  for (const key of Object.keys(overlay)) {
    if (key in result && typeof result[key] === 'object' && result[key] !== null && !Array.isArray(result[key])
      && typeof overlay[key] === 'object' && overlay[key] !== null && !Array.isArray(overlay[key])) {
      result[key] = deepMerge(result[key], overlay[key]);
    } else {
      result[key] = overlay[key];
    }
  }
  return result;
}

function json(data: any, status = 200): Response {
  return new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });
}

function parseBody(opts: RequestInit): any {
  if (!opts.body) return {};
  try { return JSON.parse(String(opts.body)); } catch { return {}; }
}

/** 匹配 /api/providers/{id}/models/{modelId} 动态路径 */
function matchProviderModel(path: string): { providerId: string; modelId: string } | null {
  const m = path.match(/^\/api\/providers\/([^/]+)\/models\/(.+)$/);
  return m ? { providerId: m[1], modelId: m[2] } : null;
}

/** 匹配 /api/providers/{id}/discovered-models 动态路径 */
function matchProviderDiscoveredModels(path: string): string | null {
  const m = path.match(/^\/api\/providers\/([^/]+)\/discovered-models$/);
  return m ? m[1] : null;
}

/** 匹配 /api/agents/{id}/subpath 动态路径 */
function matchAgentSub(path: string): { agentId: string; sub: string } | null {
  const m = path.match(/^\/api\/agents\/([^/]+)\/(.+)$/);
  return m ? { agentId: m[1], sub: m[2] } : null;
}

/**
 * hanaFetch — 全量 loomRpc 桥接
 */
export async function hanaFetch(
  path: string,
  opts: RequestInit & { timeout?: number } = {},
): Promise<Response> {
  const method = opts.method?.toUpperCase() || 'GET';
  const body = method !== 'GET' ? parseBody(opts) : {};

  try {
    // ── /api/providers/{id}/models/{modelId} ──
    const providerModel = matchProviderModel(path);
    if (providerModel) {
      if (method === 'PUT') {
        // Save model capabilities into the provider's models array in settings
        const { providerId, modelId } = providerModel;
        // Read current provider config to get models list
        const cfgRes = await loomRpc('config.get', { key: 'settings' });
        const settings = cfgRes.config || {};
        const providers = settings.providers || settings.general?.providers || {};
        const prov = providers[providerId] || {};
        const currentModels = Array.isArray(prov.models) ? prov.models : [];
        // Update or add the model entry with capabilities
        const updatedModels = currentModels.map((m: any) => {
          if ((typeof m === 'string' && m === modelId) || (typeof m === 'object' && m?.id === modelId)) {
            return { id: modelId, ...(typeof m === 'object' ? m : {}), ...body };
          }
          return m;
        });
        const exists = updatedModels.some((m: any) =>
          (typeof m === 'string' && m === modelId) || (typeof m === 'object' && m?.id === modelId)
        );
        if (!exists) {
          updatedModels.push({ id: modelId, ...body });
        }
        const r = await loomRpc('config.set', { key: 'general', value: { providers: { [providerId]: { models: updatedModels } } } });
        return json(r);
      }
      return json({});
    }

    // ── /api/providers/{id}/discovered-models ──
    const discoveredProvider = matchProviderDiscoveredModels(path);
    if (discoveredProvider) {
      const r = await loomRpc('providers.fetch_models', { name: discoveredProvider });
      return json(r);
    }

    // ── /api/agents/{id}/subpath ──
    const agentSub = matchAgentSub(path);
    if (agentSub) {
      return await handleAgentSub(agentSub.agentId, agentSub.sub, method, body);
    }

    // ── 静态路径映射 ──
    switch (path) {
      // ── 核心数据 ──
      case '/api/config': {
        if (method === 'PUT') {
          const r = await loomRpc('config.set', { key: 'general', value: body });
          return json(r);
        }
        const r = await loomRpc('config.get', {});
        const cfg = r.config || {};
        // Merge typed config fields with free-form settings for round-trip
        const settings = cfg.settings || {};
        const general = settings.general || {};
        // Flatten: settings.general.* → top-level, then settings.* → top-level
        return json({ ...cfg, ...settings, ...general });
      }

      case '/api/agents': {
        if (method === 'POST' || method === 'PUT') {
          // agents/order or agents/switch etc — shouldn't hit here but handle gracefully
          return json({ ok: true });
        }
        const r = await loomRpc('agent.list');
        return json(r);
      }

      case '/api/agents/switch': {
        const r = await loomRpc('agent.switch', { agent_id: body.id || 'default' });
        return json(r);
      }

      case '/api/agents/primary': {
        return json({ ok: true });
      }

      case '/api/agents/order': {
        return json({ ok: true });
      }

      case '/api/health': {
        const r = await loomRpc('system.health');
        return json(r);
      }

      case '/api/models': {
        const r = await loomRpc('model.list');
        return json(r);
      }

      case '/api/models/set': {
        const r = await loomRpc('model.switch', { model_id: body.modelId || body.id, provider: body.provider });
        return json(r);
      }

      case '/api/models/switch': {
        // Per-session model switch — also delegates to model.switch
        const r = await loomRpc('model.switch', { model_id: body.modelId || body.id, provider: body.provider, session_path: body.sessionPath });
        // Return in the format the frontend expects
        return json({ ok: true, model: { id: body.modelId || body.id, provider: body.provider } });
      }

      case '/api/models/health': {
        return json({ ok: false });
      }

      case '/api/user-profile': {
        if (method === 'PUT') {
          const r = await loomRpc('config.set', { key: 'settings.user-profile', value: body });
          return json(r);
        }
        const r = await loomRpc('config.get', { key: 'settings.user-profile' });
        return json(r.config || { content: '' });
      }

      case '/api/preferences/models': {
        if (method === 'PUT') {
          // Deep merge with existing settings.models to avoid overwriting other fields
          const existing = await loomRpc('config.get', { key: 'settings.models' });
          const current = existing.config || {};
          const merged = deepMerge(current, body);
          const r = await loomRpc('config.set', { key: 'settings.models', value: merged });
          return json(r);
        }
        const r = await loomRpc('config.get', { key: 'settings.models' });
        return json(r.config || {});
      }

      case '/api/preferences/computer-use': {
        if (method === 'PUT') {
          const r = await loomRpc('config.set', { key: 'settings.computer-use', value: body });
          return json(r);
        }
        return json({});
      }

      case '/api/preferences/computer-use/request-permissions': {
        return json({ ok: true });
      }

      // ── Providers ──
      case '/api/providers/summary': {
        const r = await loomRpc('providers.summary');
        return json(r);
      }

      case '/api/providers/fetch-models': {
        const r = await loomRpc('providers.fetch_models', body);
        return json(r);
      }

      case '/api/providers/test': {
        const r = await loomRpc('providers.test', body);
        return json(r);
      }

      // ── Bridge ──
      case '/api/bridge/settings': {
        if (method === 'PUT') {
          return json({ ok: true });
        }
        return json({});
      }

      case '/api/bridge/test': {
        return json({ ok: false });
      }

      case '/api/bridge/wechat/qrcode': {
        return json({ qr_url: '' });
      }

      case '/api/bridge/wechat/qrcode-status': {
        return json({ status: 'disconnected' });
      }

      // ── Auth / OAuth ──
      case '/api/auth/oauth/start': {
        return json({ url: '' });
      }

      case '/api/auth/oauth/callback': {
        return json({ ok: false });
      }

      case '/api/auth/oauth/logout': {
        return json({ ok: true });
      }

      // ── Character cards ──
      case '/api/character-cards/plan': {
        return json({ fields: {}, avatarAction: 'keep' });
      }

      case '/api/character-cards/import': {
        return json({ ok: true });
      }

      case '/api/character-cards/export/preview': {
        return json({});
      }

      case '/api/character-cards/export': {
        return json({});
      }

      // ── Checkpoints ──
      case '/api/checkpoints': {
        return json({ checkpoints: [] });
      }

      // ── Plugins ──
      case '/api/plugins/settings': {
        return json({ allow_full_access: false, plugin_dev_tools_enabled: false, plugins_dir: '' });
      }

      case '/api/plugins/settings-tabs': {
        return json([]);
      }

      case '/api/plugins/marketplace': {
        return json({ plugins: [] });
      }

      case '/api/plugins/install': {
        return json({ ok: true });
      }

      case '/api/plugins/diagnostics': {
        return json({ diagnostics: [] });
      }

      case '/api/plugins/image-gen/providers': {
        return json({ providers: [] });
      }

      case '/api/plugins/image-gen/config': {
        if (method === 'PUT') return json({ ok: true });
        return json({});
      }

      case '/api/plugins/mcp/connectors': {
        if (method === 'PUT') return json({ ok: true });
        return json({ connectors: [] });
      }

      case '/api/plugins/mcp/settings/enabled': {
        if (method === 'PUT') return json({ ok: true });
        return json({ enabled: {} });
      }

      // ── Skills ──
      case '/api/skills/external-paths': {
        return json({ paths: [] });
      }

      case '/api/skills/install': {
        return json({ ok: true });
      }

      case '/api/skills/translate': {
        return json({ ok: true });
      }

      // ── Search ──
      case '/api/search/verify': {
        return json({ ok: false });
      }

      // ── Access ──
      case '/api/access/summary': {
        return json({ desktop: false, mobile: false, network: { enabled: false } });
      }

      case '/api/access/network': {
        if (method === 'PUT') return json({ ok: true });
        return json({ enabled: false });
      }

      case '/api/access/account/password': {
        return json({ ok: true });
      }

      case '/api/access/account/profile': {
        return json({ ok: true });
      }

      case '/api/access/desktop-credentials': {
        return json({ has_password: false });
      }

      case '/api/access/mobile-credentials': {
        return json({ has_password: false });
      }
    }

    // ── 带查询参数的 plugins 路径 ──
    if (path.startsWith('/api/plugins') || path.startsWith('/api/skills')) {
      return json({});
    }

    // ── Avatar ──
    if (path.startsWith('/api/avatar')) {
      if (method === 'POST') {
        const body = parseBody(opts);
        const role = path.split('/').pop() || 'user';
        const result = await loomRpc('avatar.upload', { role, data: body.data });
        return json(result);
      }
      // GET: return null data so caller falls back
      const role = path.split('/').pop() || 'user';
      const match = path.match(/^\/api\/avatar\/([^/?]+)/);
      const resolvedRole = match ? match[1] : role;
      const result = await loomRpc('avatar.get', { role: resolvedRole });
      return json(result);
    }

    // ── 兜底：尝试 loomRpc config.get ──
    console.warn(`[settings/api] unmapped path: ${path} ${method}, returning empty`);
    return json({});
  } catch (err: any) {
    console.error(`[settings/api] loomRpc error for ${path}:`, err);
    return json({ error: err.message || 'rpc error' }, 500);
  }
}

/** 处理 /api/agents/{id}/subpath 请求 */
async function handleAgentSub(agentId: string, sub: string, method: string, body: any): Promise<Response> {
  const settingsKey = (field: string) => `settings.agent.${agentId}.${field}`;
  switch (sub) {
    case 'config': {
      if (method === 'PUT') {
        const r = await loomRpc('config.set', { key: settingsKey('config'), value: body });
        return json(r);
      }
      const r = await loomRpc('config.get', { key: settingsKey('config') });
      return json(r.config || {});
    }

    case 'identity': {
      if (method === 'PUT') {
        const r = await loomRpc('config.set', { key: settingsKey('identity'), value: body });
        return json(r);
      }
      const r = await loomRpc('config.get', { key: settingsKey('identity') });
      return json(r.config || { content: '' });
    }

    case 'ishiki': {
      if (method === 'PUT') {
        const r = await loomRpc('config.set', { key: settingsKey('ishiki'), value: body });
        return json(r);
      }
      const r = await loomRpc('config.get', { key: settingsKey('ishiki') });
      return json(r.config || { content: '' });
    }

    case 'public-ishiki': {
      if (method === 'PUT') {
        const r = await loomRpc('config.set', { key: settingsKey('public-ishiki'), value: body });
        return json(r);
      }
      const r = await loomRpc('config.get', { key: settingsKey('public-ishiki') });
      return json(r.config || { content: '' });
    }

    case 'pinned': {
      if (method === 'PUT') {
        const r = await loomRpc('config.set', { key: settingsKey('pinned'), value: body.pins || [] });
        return json(r);
      }
      const r = await loomRpc('config.get', { key: settingsKey('pinned') });
      return json(r.config || { pins: [] });
    }

    case 'avatar': {
      return json({ ok: true });
    }

    default: {
      console.warn(`[settings/api] unmapped agent sub: /api/agents/${agentId}/${sub}`);
      return json({});
    }
  }
}

/** Build URL for static assets (avatars etc.) */
export function hanaUrl(path: string): string {
  const port = (window as any).__enginePort__ ?? 0;
  return `http://127.0.0.1:${port}${path}`;
}

/** 根据 yuan 类型返回 fallback 头像路径 */
export function yuanFallbackAvatar(yuan?: string): string {
  return yuanFallbackAvatarUrl(yuan);
}

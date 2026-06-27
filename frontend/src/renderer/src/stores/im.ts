import { create } from 'zustand';

// Re-define types locally for the renderer process
// (renderer can't import from main process — mirrors main/im/types.ts).
// Keep these in sync with frontend/src/main/im/types.ts.
export type Platform = 'telegram' | 'feishu' | 'wechat' | 'wecom' | 'dingtalk' | 'qq' | 'discord' | 'popo';

export type AccessMode = 'open' | 'pairing' | 'allowlist' | 'disabled';

export interface InstanceConfig {
  id: string;
  platform: Platform;
  instanceId: string;
  instanceName: string;
  enabled: boolean;
  configJson: Record<string, unknown>;
  dmPolicy: AccessMode;
  allowFrom: string[];
  groupPolicy: 'open' | 'allowlist' | 'disabled';
  groupAllowFrom: string[];
  agentId?: string;
  createdAt: number;
  updatedAt: number;
}

export interface IMSettings {
  globalEnabled: boolean;
  defaultDmPolicy: AccessMode;
  skillsEnabled: boolean;
  defaultAgentId: string;
}

export interface ChannelStatus {
  connected: boolean;
  startedAt?: number | null;
  lastError?: string | null;
  lastInboundAt?: number | null;
  lastOutboundAt?: number | null;
  accountId?: string | null;
  botUsername?: string | null;
}

export interface ChannelStatusEvent {
  platform: Platform;
  instanceId: string;
  connected: boolean;
  accountId?: string;
  error?: string;
}

export interface ConnectivityCheck {
  code: string;
  level: 'pass' | 'info' | 'warn' | 'fail';
  message: string;
  suggestion?: string;
}

export interface ConnectivityResult {
  platform: Platform;
  testedAt: number;
  verdict: 'pass' | 'warn' | 'fail';
  checks: ConnectivityCheck[];
}

// Backend's IMGatewayStatus is nested by platform; we flatten it to a per-key map.
interface IMGatewayStatusInstance {
  instanceId: string;
  instanceName: string;
  connected: boolean;
  startedAt: number | null;
  lastError: string | null;
  lastInboundAt: number | null;
  lastOutboundAt: number | null;
  botUsername?: string | null;
  accountId?: string | null;
}
interface IMGatewayStatus {
  [platform: string]: { instances: IMGatewayStatusInstance[] };
}

export const PLATFORM_LABELS: Record<Platform, string> = {
  telegram: 'Telegram',
  feishu: 'Feishu',
  wechat: '微信',
  wecom: '企业微信',
  dingtalk: '钉钉',
  qq: 'QQ',
  discord: 'Discord',
  popo: 'POPO',
};

export const PLATFORM_ORDER: Platform[] = [
  'wechat', 'feishu', 'telegram', 'wecom', 'dingtalk', 'qq', 'discord', 'popo',
];

/** Platforms that have a real backend implementation in the Electron layer. */
export const IMPLEMENTED_PLATFORMS: Platform[] = ['wechat', 'telegram', 'discord', 'qq', 'feishu', 'wecom', 'dingtalk', 'popo'];

export function statusKey(platform: Platform, instanceId: string): string {
  return `${platform}:${instanceId}`;
}

/** Flatten the backend's nested IMGatewayStatus into a per-key ChannelStatus map. */
function flattenStatus(gw: IMGatewayStatus): Record<string, ChannelStatus> {
  const out: Record<string, ChannelStatus> = {};
  for (const [platform, group] of Object.entries(gw || {})) {
    for (const inst of group.instances) {
      out[statusKey(platform as Platform, inst.instanceId)] = {
        connected: inst.connected,
        startedAt: inst.startedAt,
        lastError: inst.lastError,
        lastInboundAt: inst.lastInboundAt,
        lastOutboundAt: inst.lastOutboundAt,
        accountId: inst.accountId,
        botUsername: inst.botUsername,
      };
    }
  }
  return out;
}

const DEFAULT_SETTINGS: IMSettings = {
  globalEnabled: true,
  defaultDmPolicy: 'pairing',
  skillsEnabled: true,
  defaultAgentId: 'main',
};

interface IMState {
  instances: InstanceConfig[];
  settings: IMSettings;
  statuses: Record<string, ChannelStatus>;
  selectedPlatform: Platform;
  loading: boolean;
  connectivityResults: Record<string, ConnectivityResult>;
  imSessionSources: Record<string, { platform: Platform; conversationId: string }>;

  loadConfigs: () => Promise<void>;
  loadSessionBindings: () => Promise<void>;
  loadSettings: () => Promise<void>;
  saveSettings: (settings: Partial<IMSettings>) => Promise<void>;
  refreshStatus: () => Promise<void>;
  saveConfig: (config: InstanceConfig) => Promise<void>;
  deleteConfig: (platform: Platform, instanceId: string) => Promise<void>;
  startChannel: (platform: Platform, instanceId: string) => Promise<{ ok: boolean; error?: string }>;
  stopChannel: (platform: Platform, instanceId: string) => Promise<{ ok: boolean; error?: string }>;
  testConnectivity: (platform: Platform, instanceId: string) => Promise<ConnectivityResult>;
  sendHelp: (platform: Platform, instanceId: string) => Promise<{ ok: boolean; error?: string }>;
  wechatQrStart: (instanceId: string) => Promise<{ qrDataUrl: string; qrContent: string; sessionKey: string }>;
  wechatQrWait: (instanceId: string, sessionKey: string) => Promise<{ connected: boolean; accountId?: string; message?: string }>;
  popoQrStart: (instanceId: string) => Promise<{ qrUrl: string; taskToken: string; timeoutMs: number }>;
  popoQrPoll: (taskToken: string) => Promise<{ success: boolean; appKey?: string; appSecret?: string; aesKey?: string; message: string }>;
  telegramLogin: (platform: Platform, instanceId: string, token: string) => Promise<{ ok: boolean; error?: string }>;
  discordLogin: (platform: Platform, instanceId: string, token: string) => Promise<{ ok: boolean; error?: string }>;
  qqLogin: (platform: Platform, instanceId: string, appId: string, clientSecret: string) => Promise<{ ok: boolean; error?: string }>;
  feishuLogin: (platform: Platform, instanceId: string, appId: string, appSecret: string) => Promise<{ ok: boolean; error?: string }>;
  wecomLogin: (platform: Platform, instanceId: string, corpId: string, secret: string, agentId: string) => Promise<{ ok: boolean; error?: string }>;
  dingtalkLogin: (platform: Platform, instanceId: string, appKey: string, appSecret: string) => Promise<{ ok: boolean; error?: string }>;
  setSelectedPlatform: (p: Platform) => void;
  /** Subscribe to backend channel-status/message events. Returns an unsubscribe. */
  subscribeEvents: () => () => void;
}

export const useIMStore = create<IMState>((set, get) => ({
  instances: [],
  settings: DEFAULT_SETTINGS,
  statuses: {},
  selectedPlatform: 'wechat',
  loading: false,
  connectivityResults: {},
  imSessionSources: {},

  loadConfigs: async () => {
    set({ loading: true });
    try {
      const [instances, gwStatus] = await Promise.all([
        (window as any).loom.imListConfigs(),
        (window as any).loom.imGetStatus().catch(() => ({})),
      ]);
      set({
        instances,
        statuses: flattenStatus(gwStatus as IMGatewayStatus),
        loading: false,
      });
    } catch (err) {
      console.error('[IMStore] loadConfigs failed:', err);
      set({ loading: false });
    }
  },

  loadSettings: async () => {
    try {
      const settings = await (window as any).loom.imGetSettings();
      set({ settings: { ...DEFAULT_SETTINGS, ...settings } });
    } catch (err) {
      console.error('[IMStore] loadSettings failed:', err);
    }
  },

  saveSettings: async (settings) => {
    const prev = get().settings;
    set({ settings: { ...prev, ...settings } });
    try {
      await (window as any).loom.imSetSettings(settings);
    } catch (err) {
      console.error('[IMStore] saveSettings failed:', err);
      set({ settings: prev });
    }
  },

  refreshStatus: async () => {
    try {
      const gwStatus = await (window as any).loom.imGetStatus();
      set({ statuses: flattenStatus(gwStatus as IMGatewayStatus) });
    } catch (err) {
      console.error('[IMStore] refreshStatus failed:', err);
    }
  },

  saveConfig: async (config) => {
    await (window as any).loom.imSetConfig(config);
    await get().loadConfigs();
  },

  deleteConfig: async (platform, instanceId) => {
    await (window as any).loom.imDeleteConfig(platform, instanceId);
    await get().loadConfigs();
  },

  startChannel: async (platform, instanceId) => {
    const res = await (window as any).loom.imStartChannel(platform, instanceId);
    if (res?.ok) {
      get().refreshStatus();
      return { ok: true };
    }
    return { ok: false, error: res?.error || 'Failed to start channel' };
  },

  stopChannel: async (platform, instanceId) => {
    const res = await (window as any).loom.imStopChannel(platform, instanceId);
    if (res?.ok) {
      get().refreshStatus();
      return { ok: true };
    }
    return { ok: false, error: res?.error || 'Failed to stop channel' };
  },

  testConnectivity: async (platform, instanceId) => {
    const result = await (window as any).loom.imTestConnectivity(platform, instanceId);
    set((s) => ({
      connectivityResults: { ...s.connectivityResults, [statusKey(platform, instanceId)]: result },
    }));
    return result;
  },

  sendHelp: async (platform, instanceId) => {
    return (window as any).loom.imSendHelp(platform, instanceId);
  },

  wechatQrStart: async (instanceId) => {
    return (window as any).loom.imWechatQrStart(instanceId);
  },

  wechatQrWait: async (instanceId, sessionKey) => {
    const result = await (window as any).loom.imWechatQrWait(instanceId, sessionKey);
    if (result?.connected) {
      await get().loadConfigs();
      get().refreshStatus();
    }
    return result;
  },

  popoQrStart: async (instanceId) => {
    return (window as any).loom.imPopoQrStart(instanceId);
  },

  popoQrPoll: async (taskToken) => {
    return (window as any).loom.imPopoQrPoll(taskToken);
  },

  telegramLogin: async (platform, instanceId, token) => {
    const result = await (window as any).loom.imTelegramLogin(platform, instanceId, token);
    if (result?.ok) {
      await get().loadConfigs();
      get().refreshStatus();
    }
    return result;
  },

  discordLogin: async (platform, instanceId, token) => { const r = await (window as any).loom.imDiscordLogin(platform, instanceId, token); if (r?.ok) { await get().loadConfigs(); get().refreshStatus(); } return r; },
  qqLogin: async (platform, instanceId, appId, clientSecret) => { const r = await (window as any).loom.imQqLogin(platform, instanceId, appId, clientSecret); if (r?.ok) { await get().loadConfigs(); get().refreshStatus(); } return r; },
  feishuLogin: async (platform, instanceId, appId, appSecret) => { const r = await (window as any).loom.imFeishuLogin(platform, instanceId, appId, appSecret); if (r?.ok) { await get().loadConfigs(); get().refreshStatus(); } return r; },
  wecomLogin: async (platform, instanceId, corpId, secret, agentId) => { const r = await (window as any).loom.imWecomLogin(platform, instanceId, corpId, secret, agentId); if (r?.ok) { await get().loadConfigs(); get().refreshStatus(); } return r; },
  dingtalkLogin: async (platform, instanceId, appKey, appSecret) => { const r = await (window as any).loom.imDingtalkLogin(platform, instanceId, appKey, appSecret); if (r?.ok) { await get().loadConfigs(); get().refreshStatus(); } return r; },

  setSelectedPlatform: (p) => set({ selectedPlatform: p }),

  loadSessionBindings: async () => {
    try {
      const bindings: Array<{ sessionId: string; platform: Platform; instanceId: string; conversationId: string }> =
        await (window as any).loom.imListSessionBindings();
      const map: Record<string, { platform: Platform; conversationId: string }> = {};
      for (const b of bindings) {
        if (b.sessionId) {
          map[b.sessionId] = { platform: b.platform, conversationId: b.conversationId };
        }
      }
      set({ imSessionSources: map });
    } catch (e) {
      console.warn('[IM store] loadSessionBindings failed:', e);
    }
  },

  subscribeEvents: () => {
    const loom = (window as any).loom;
    // Channel-status events fire on connect/disconnect/error; refresh full
    // status (carries lastInboundAt/lastError) from the backend on each event.
    const unsubStatus = loom?.onIMChannelStatus?.(() => {
      get().refreshStatus();
    });
    // Inbound messages also refresh status so "last received" stays fresh.
    const unsubMsg = loom?.onIMMessage?.(() => {
      get().refreshStatus();
    });
    return () => {
      unsubStatus?.();
      unsubMsg?.();
    };
  },
}));

import { create } from 'zustand';

// Re-define Platform and types locally for the renderer process
// (renderer can't import from main process -- creates a mirror)
export type Platform = 'telegram' | 'feishu' | 'wechat' | 'wecom' | 'dingtalk' | 'qq' | 'discord' | 'popo';

export interface InstanceConfig {
  id: string;
  platform: Platform;
  instanceId: string;
  instanceName: string;
  enabled: boolean;
  configJson: Record<string, unknown>;
  dmPolicy: 'open' | 'pairing' | 'allowlist' | 'disabled';
  allowFrom: string[];
  groupPolicy: 'open' | 'allowlist' | 'disabled';
  groupAllowFrom: string[];
  agentId?: string;
  createdAt: number;
  updatedAt: number;
}

export interface IMSettings {
  defaultDmPolicy: 'open' | 'pairing' | 'allowlist' | 'disabled';
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

interface IMState {
  instances: InstanceConfig[];
  settings: IMSettings;
  statuses: Record<string, ChannelStatus>;
  selectedPlatform: Platform;
  loading: boolean;
  connectivityResults: Record<string, ConnectivityResult>;

  loadConfigs: () => Promise<void>;
  saveConfig: (config: InstanceConfig) => Promise<void>;
  deleteConfig: (platform: Platform, instanceId: string) => Promise<void>;
  startChannel: (platform: Platform, instanceId: string) => Promise<void>;
  stopChannel: (platform: Platform, instanceId: string) => Promise<void>;
  testConnectivity: (platform: Platform, instanceId: string) => Promise<ConnectivityResult>;
  wechatQrStart: (instanceId: string) => Promise<{ qrDataUrl: string; sessionKey: string }>;
  wechatQrWait: (instanceId: string, sessionKey: string) => Promise<{ connected: boolean; accountId?: string; message?: string }>;
  popoQrStart: () => Promise<{ qrUrl: string; taskToken: string; timeoutMs: number }>;
  popoQrPoll: (taskToken: string) => Promise<{ success: boolean; appKey?: string; appSecret?: string; aesKey?: string; message: string }>;
  setSelectedPlatform: (p: Platform) => void;
  updateChannelStatus: (platform: Platform, instanceId: string, status: Partial<ChannelStatus>) => void;
}

export const useIMStore = create<IMState>((set, get) => ({
  instances: [],
  settings: { defaultDmPolicy: 'pairing', skillsEnabled: true, defaultAgentId: 'main' },
  statuses: {},
  selectedPlatform: 'wechat',
  loading: false,
  connectivityResults: {},

  loadConfigs: async () => {
    set({ loading: true });
    try {
      const instances = await (window as any).loom.imListConfigs();
      set({ instances, loading: false });
    } catch (err) {
      console.error('[IMStore] loadConfigs failed:', err);
      set({ loading: false });
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
    await (window as any).loom.imStartChannel(platform, instanceId);
    get().updateChannelStatus(platform, instanceId, { connected: true });
  },

  stopChannel: async (platform, instanceId) => {
    await (window as any).loom.imStopChannel(platform, instanceId);
    get().updateChannelStatus(platform, instanceId, { connected: false });
  },

  testConnectivity: async (platform, instanceId) => {
    const result = await (window as any).loom.imTestConnectivity(platform, instanceId);
    set((s) => ({
      connectivityResults: { ...s.connectivityResults, [`${platform}:${instanceId}`]: result },
    }));
    return result;
  },

  wechatQrStart: async (instanceId) => {
    return (window as any).loom.imWechatQrStart(instanceId);
  },

  wechatQrWait: async (instanceId, sessionKey) => {
    return (window as any).loom.imWechatQrWait(instanceId, sessionKey);
  },

  popoQrStart: async () => {
    return (window as any).loom.imPopoQrStart();
  },

  popoQrPoll: async (taskToken) => {
    return (window as any).loom.imPopoQrPoll(taskToken);
  },

  setSelectedPlatform: (p) => set({ selectedPlatform: p }),

  updateChannelStatus: (platform, instanceId, status) => {
    const key = `${platform}:${instanceId}`;
    set((s) => ({
      statuses: { ...s.statuses, [key]: { ...s.statuses[key], ...status } },
    }));
  },
}));

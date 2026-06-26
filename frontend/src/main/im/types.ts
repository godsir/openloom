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

export interface IMGatewayConfig {
  instances: InstanceConfig[];
  settings: IMSettings;
}

export interface IMMessage {
  platform: Platform;
  messageId: string;
  conversationId: string;
  senderId: string;
  senderName?: string;
  groupName?: string;
  content: string;
  chatType: 'direct' | 'group';
  timestamp: number;
}

export interface IMGatewayStatus {
  [platform: string]: {
    instances: Array<{
      instanceId: string;
      instanceName: string;
      connected: boolean;
      startedAt: number | null;
      lastError: string | null;
      lastInboundAt: number | null;
      lastOutboundAt: number | null;
      botUsername?: string | null;
      accountId?: string | null;
    }>;
  };
}

export const DEFAULT_IM_SETTINGS: IMSettings = {
  globalEnabled: true,
  defaultDmPolicy: 'pairing',
  skillsEnabled: true,
  defaultAgentId: 'main',
};

export const MAX_INSTANCES = 20;

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

export interface ChannelStatusEvent {
  platform: Platform;
  instanceId: string;
  connected: boolean;
  accountId?: string;
  error?: string;
}

export interface ConnectivityResult {
  platform: Platform;
  testedAt: number;
  verdict: 'pass' | 'warn' | 'fail';
  checks: ConnectivityCheck[];
}

export interface ConnectivityCheck {
  code: string;
  level: 'pass' | 'info' | 'warn' | 'fail';
  message: string;
  suggestion?: string;
}

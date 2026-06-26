import { EventEmitter } from 'events';
import { IMStore } from './imStore';
import { WechatChannel } from './wechatChannel';
import type { Platform, InstanceConfig, IMMessage, IMGatewayStatus } from './types';

export interface IMGatewayManagerOptions {
  imStore: IMStore;
  /** Called when a channel receives a message — forwards to renderer → Rust */
  onMessage?: (message: IMMessage) => void;
}

export class IMGatewayManager extends EventEmitter {
  private imStore: IMStore;
  private channels: Map<string, WechatChannel> = new Map();
  private onMessage?: (message: IMMessage) => void;

  constructor(options: IMGatewayManagerOptions) {
    super();
    this.imStore = options.imStore;
    this.onMessage = options.onMessage;
  }

  private channelKey(platform: Platform, instanceId: string): string {
    return `${platform}:${instanceId}`;
  }

  /**
   * Start a channel for the given config.
   * Currently only WeChat is implemented in Electron layer.
   */
  async startChannel(config: InstanceConfig): Promise<void> {
    const key = this.channelKey(config.platform, config.instanceId);

    if (this.channels.has(key)) {
      console.log(`[IMGatewayManager] Channel ${key} already running`);
      return;
    }

    if (config.platform === 'wechat') {
      const ch = new WechatChannel({
        instanceId: config.instanceId,
        instanceName: config.instanceName,
      });

      ch.on('message', (msg) => {
        console.log(`[IMGatewayManager] WeChat message from ${msg.senderId}`);
        if (this.onMessage) {
          this.onMessage({
            platform: 'wechat',
            messageId: msg.messageId,
            conversationId: msg.conversationId,
            senderId: msg.senderId,
            senderName: msg.senderName,
            groupName: msg.groupName,
            content: msg.content,
            chatType: msg.chatType,
            timestamp: msg.timestamp,
          });
        }
        this.emit('im-message', {
          platform: 'wechat',
          messageId: msg.messageId,
          conversationId: msg.conversationId,
          senderId: msg.senderId,
          senderName: msg.senderName,
          groupName: msg.groupName,
          content: msg.content,
          chatType: msg.chatType,
          timestamp: msg.timestamp,
        });
      });

      ch.on('connected', (info) => {
        console.log(`[IMGatewayManager] WeChat connected, accountId: ${info.accountId}`);
        // Update config with accountId
        const updated: InstanceConfig = {
          ...config,
          configJson: { ...config.configJson, accountId: info.accountId },
          enabled: true,
          updatedAt: Date.now(),
        };
        this.imStore.upsertInstance(updated);
        this.emit('channel-status', {
          platform: 'wechat' as Platform,
          instanceId: config.instanceId,
          connected: true,
          accountId: info.accountId,
        });
      });

      ch.on('error', (err) => {
        console.error(`[IMGatewayManager] WeChat error:`, err);
        this.emit('channel-status', {
          platform: 'wechat' as Platform,
          instanceId: config.instanceId,
          connected: false,
          error: err.message,
        });
      });

      this.channels.set(key, ch);

      // If we already have an accountId from previous session, restore
      const accountId = config.configJson?.accountId as string | undefined;
      const token = config.configJson?.token as string | undefined;
      const baseUrl = config.configJson?.baseUrl as string | undefined;
      if (accountId && token && baseUrl) {
        await ch.restoreConnection(accountId, token, baseUrl);
      }
      // Otherwise, the renderer will call startLogin() to get a QR code
    }
  }

  async stopChannel(platform: Platform, instanceId: string): Promise<void> {
    const key = this.channelKey(platform, instanceId);
    const ch = this.channels.get(key);
    if (ch) {
      await ch.disconnect();
      this.channels.delete(key);
      console.log(`[IMGatewayManager] Stopped channel ${key}`);
    }
  }

  async startAllEnabled(): Promise<void> {
    const instances = this.imStore.listInstances().filter(i => i.enabled);
    console.log(`[IMGatewayManager] Starting ${instances.length} enabled channels`);
    for (const inst of instances) {
      try {
        await this.startChannel(inst);
      } catch (err: any) {
        console.error(`[IMGatewayManager] Failed to start ${inst.platform}:${inst.instanceId}:`, err.message);
      }
    }
  }

  stopAll(): void {
    for (const [key, ch] of this.channels) {
      ch.disconnect().catch(() => {});
    }
    this.channels.clear();
  }

  getStatus(): IMGatewayStatus {
    const status: IMGatewayStatus = {};
    for (const [key, ch] of this.channels) {
      const [platform] = key.split(':') as [Platform, string];
      if (!status[platform]) {
        status[platform] = { instances: [] };
      }
      status[platform].instances.push({
        instanceId: ch.currentAccountId || key.split(':')[1],
        instanceName: key,
        connected: ch.isConnected,
        startedAt: null,
        lastError: null,
        lastInboundAt: null,
        lastOutboundAt: null,
        accountId: ch.currentAccountId,
      });
    }
    return status;
  }

  // WeChat QR flow

  async wechatQrStart(instanceId: string): Promise<{ qrDataUrl: string; sessionKey: string }> {
    const key = this.channelKey('wechat', instanceId);
    let ch = this.channels.get(key);
    if (!ch) {
      // Create channel on demand if not started yet
      const config = this.imStore.listInstances().find(i => i.platform === 'wechat' && i.instanceId === instanceId);
      if (!config) throw new Error(`No WeChat config found for instance ${instanceId}`);
      await this.startChannel(config);
      ch = this.channels.get(key);
    }
    if (!ch) throw new Error('WeChat channel not found after start');
    return ch.startLogin();
  }

  async wechatQrWait(instanceId: string, sessionKey: string): Promise<{ connected: boolean; accountId?: string; message?: string }> {
    const key = this.channelKey('wechat', instanceId);
    const ch = this.channels.get(key);
    if (!ch) throw new Error('WeChat channel not found');
    const result = await ch.waitForScan(sessionKey);

    // If connected, store the account info for future restores
    if (result.connected && result.accountId) {
      const config = this.imStore.listInstances().find(i => i.platform === 'wechat' && i.instanceId === instanceId);
      if (config) {
        this.imStore.upsertInstance({
          ...config,
          configJson: {
            ...config.configJson,
            accountId: result.accountId,
          },
          enabled: true,
          updatedAt: Date.now(),
        });
      }
    }

    return result;
  }

  // POPO QR flow (stub for now)

  popoQrStart(): { qrUrl: string; taskToken: string; timeoutMs: number } {
    console.log('[IMGatewayManager] POPO QR start (stub)');
    const taskToken = `popo_${Date.now()}`;
    return {
      qrUrl: '',
      taskToken,
      timeoutMs: 600_000,
    };
  }

  async popoQrPoll(taskToken: string): Promise<{ success: boolean; appKey?: string; appSecret?: string; aesKey?: string; message: string }> {
    console.log(`[IMGatewayManager] POPO QR poll ${taskToken} (stub)`);
    return { success: false, message: 'POPO not yet implemented' };
  }
}

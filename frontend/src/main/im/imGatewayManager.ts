import { EventEmitter } from 'events';
import { IMStore } from './imStore';
import { WechatChannel } from './wechatChannel';
import { TelegramChannel } from './telegramChannel';
import { PopoChannel } from './popoChannel';
import { DiscordChannel } from './discordChannel';
import { QQChannel } from './qqChannel';
import { FeishuChannel } from './feishuChannel';
import { WecomChannel } from './wecomChannel';
import { DingTalkChannel } from './dingtalkChannel';
import type { IChannel } from './IChannel';
import type { Platform, InstanceConfig, IMMessage, IMGatewayStatus } from './types';
import type { ImBridge } from './imBridge';
import { HELP_MESSAGE } from './imBridge';

export interface IMGatewayManagerOptions {
  imStore: IMStore;
  /** Called when a channel receives a message — forwards to renderer → Rust */
  onMessage?: (message: IMMessage) => void;
}

interface ChannelStatusMeta {
  startedAt: number | null;
  lastError: string | null;
  lastInboundAt: number | null;
  lastOutboundAt: number | null;
}

export class IMGatewayManager extends EventEmitter {
  private imStore: IMStore;
  channels: Map<string, IChannel> = new Map();
  private onMessage?: (message: IMMessage) => void;
  private statusMeta: Map<string, ChannelStatusMeta> = new Map();
  private imBridge?: ImBridge;

  constructor(options: IMGatewayManagerOptions) {
    super();
    this.imStore = options.imStore;
    this.onMessage = options.onMessage;
  }

  /** Attach the IM↔engine bridge so incoming messages are routed to the agent. */
  setBridge(bridge: ImBridge): void {
    this.imBridge = bridge;
  }

  private channelKey(platform: Platform, instanceId: string): string {
    return `${platform}:${instanceId}`;
  }

  private getStatusMeta(key: string): ChannelStatusMeta {
    let m = this.statusMeta.get(key);
    if (!m) {
      m = { startedAt: null, lastError: null, lastInboundAt: null, lastOutboundAt: null };
      this.statusMeta.set(key, m);
    }
    return m;
  }

  /**
   * Register standard IChannel event handlers and wire them to IM status,
   * internal message pipeline, and engine bridge routing.
   */
  private registerChannelHandlers(
    ch: IChannel,
    config: InstanceConfig,
    platform: Platform,
    instanceId: string,
    meta: ChannelStatusMeta,
  ): void {
    ch.on('message', (msg) => {
      console.log(`[IMGatewayManager] ${platform} message from ${msg.senderId}`);
      meta.lastInboundAt = Date.now();
      const imMsg: IMMessage = {
        platform,
        messageId: msg.messageId,
        conversationId: msg.conversationId,
        senderId: msg.senderId,
        senderName: msg.senderName,
        groupName: msg.groupName,
        content: msg.content,
        chatType: msg.chatType,
        timestamp: msg.timestamp,
      };
      if (this.onMessage) this.onMessage(imMsg);
      this.emit('im-message', imMsg);
      if (this.imBridge) {
        this.imBridge
          .handleMessage(imMsg, config, async (text) => {
            const ok = await ch.sendMessage(imMsg.conversationId, text);
            if (ok) {
              meta.lastOutboundAt = Date.now();
              this.emit('channel-status', { platform, instanceId, connected: true });
            }
          })
          .catch((e) => console.error('[IMGatewayManager] bridge error:', e?.message ?? e));
      }
    });

    ch.on('connected', (info) => {
      console.log(`[IMGatewayManager] ${platform} connected, accountId: ${info.accountId}`);
      this.emit('channel-status', {
        platform,
        instanceId,
        connected: true,
        accountId: info.accountId,
      });
    });

    ch.on('error', (err) => {
      console.error(`[IMGatewayManager] ${platform} error:`, err);
      meta.lastError = err.message;
      this.emit('channel-status', {
        platform,
        instanceId,
        connected: false,
        error: err.message,
      });
    });

    ch.on('disconnected', () => {
      console.log(`[IMGatewayManager] ${platform} disconnected: ${this.channelKey(platform, instanceId)}`);
      this.emit('channel-status', {
        platform,
        instanceId,
        connected: false,
      });
    });
  }

  /**
   * Start a channel for the given config.
   * Only WeChat is implemented in the Electron layer for now; other platforms
   * throw so the caller surfaces "not yet supported" instead of faking success.
   */
  async startChannel(config: InstanceConfig): Promise<void> {
    const key = this.channelKey(config.platform, config.instanceId);

    if (this.channels.has(key)) {
      console.log(`[IMGatewayManager] Channel ${key} already running`);
      return;
    }

    let ch: IChannel;
    switch (config.platform) {
      case 'wechat':
        ch = new WechatChannel({
          instanceId: config.instanceId,
          instanceName: config.instanceName,
        });
        break;
      case 'telegram':
        ch = new TelegramChannel({
          instanceId: config.instanceId,
          instanceName: config.instanceName,
        });
        break;
      case 'popo':
        ch = new PopoChannel({
          instanceId: config.instanceId,
          instanceName: config.instanceName,
        });
        break;
      case 'discord':
        ch = new DiscordChannel({
          instanceId: config.instanceId,
          instanceName: config.instanceName,
        });
        break;
      case 'qq':
        ch = new QQChannel({
          instanceId: config.instanceId,
          instanceName: config.instanceName,
        });
        break;
      case 'feishu':
        ch = new FeishuChannel({
          instanceId: config.instanceId,
          instanceName: config.instanceName,
        });
        break;
      case 'wecom':
        ch = new WecomChannel({
          instanceId: config.instanceId,
          instanceName: config.instanceName,
        });
        break;
      case 'dingtalk':
        ch = new DingTalkChannel({
          instanceId: config.instanceId,
          instanceName: config.instanceName,
        });
        break;
      default:
        throw new Error(`${config.platform} 接入尚未实现`);
    }

    const meta = this.getStatusMeta(key);
    meta.startedAt = Date.now();

    this.registerChannelHandlers(ch, config, config.platform, config.instanceId, meta);

    this.channels.set(key, ch);

    // If we already have credentials from a previous session, restore + poll.
    const creds = config.configJson as Record<string, unknown>;
    if (config.platform === 'wechat') {
      const accountId = creds.accountId as string | undefined;
      const token = creds.token as string | undefined;
      const baseUrl = creds.baseUrl as string | undefined;
      if (accountId && token && baseUrl) {
        ch.restoreConnection({ accountId, token, baseUrl });
        ch.startPolling();
      }
    } else if (config.platform === 'telegram') {
      const token = creds.token as string | undefined;
      if (token) {
        try {
          const tgCh = ch as TelegramChannel;
          const verifyResult = await tgCh.verifyToken(token);
          if (!verifyResult.ok) {
            console.warn(`[IMGatewayManager] Telegram token invalid for ${key}, skipping restore`);
            this.channels.delete(key);
            this.statusMeta.delete(key);
            return;
          }
          // Update stored bot info in case it changed
          if (verifyResult.accountId) {
            const updatedConfig = { ...config, configJson: { ...config.configJson, accountId: verifyResult.accountId, botUsername: verifyResult.botUsername }, updatedAt: Date.now() };
            this.imStore.upsertInstance(updatedConfig);
          }
        } catch (e) {
          console.warn(`[IMGatewayManager] Telegram verifyToken failed for ${key}, will attempt polling anyway`);
        }
        ch.restoreConnection(creds);
        ch.startPolling();
      }
      const aesKey = creds.aesKey as string | undefined;
      if (appKey && appSecret && aesKey) {
        ch.restoreConnection(creds);
        ch.startPolling();
      }
    } else if (config.platform === 'discord') {
      const token = creds.token as string | undefined;
      if (token) {
        ch.restoreConnection(creds);
        ch.startPolling();
      }
    } else if (config.platform === 'qq') {
      const appId = creds.appId as string | undefined;
      const clientSecret = creds.clientSecret as string | undefined;
      if (appId && clientSecret) {
        ch.restoreConnection(creds);
        ch.startPolling();
      }
    } else if (config.platform === 'feishu') {
      const appId = creds.appId as string | undefined;
      const appSecret = creds.appSecret as string | undefined;
      if (appId && appSecret) {
        ch.restoreConnection(creds);
        ch.startPolling();
      }
    } else if (config.platform === 'wecom') {
      const corpId = creds.corpId as string | undefined;
      const secret = creds.secret as string | undefined;
      const agentId = creds.agentId as string | undefined;
      if (corpId && secret && agentId) {
        ch.restoreConnection(creds);
        ch.startPolling();
      }
    } else if (config.platform === 'dingtalk') {
      const appKey = creds.appKey as string | undefined;
      const appSecret = creds.appSecret as string | undefined;
      if (appKey && appSecret) {
        ch.restoreConnection(creds);
        ch.startPolling();
      }
    }
    // Otherwise, the renderer will trigger the platform-specific login flow.
  }

  async stopChannel(platform: Platform, instanceId: string): Promise<void> {
    const key = this.channelKey(platform, instanceId);
    const ch = this.channels.get(key);
    if (ch) {
      // Persist runtime state before disconnecting (e.g. Telegram lastUpdateId)
      if (platform === 'telegram' && typeof (ch as any).getPersistState === 'function') {
        const state = (ch as any).getPersistState() as Record<string, unknown>;
        const config = this.imStore
          .listInstances()
          .find(i => i.platform === platform && i.instanceId === instanceId);
        if (config) {
          this.imStore.upsertInstance({
            ...config,
            configJson: { ...config.configJson, ...state },
            updatedAt: Date.now(),
          });
        }
      }
      await ch.disconnect();
      this.channels.delete(key);
      this.statusMeta.delete(key);
      console.log(`[IMGatewayManager] Stopped channel ${key}`);
    }
  }

  /**
   * Send the help message to the real WeChat user who scanned the QR code to
   * log in (the bot owner). Used by the "test connection" button: a successful
   * send both verifies connectivity and shows the help intro in WeChat.
   *
   * Falls back to the most recently active bound conversation if the owner
   * user id isn't available (e.g. credentials were saved by an older build).
   */
  async sendHelpMessage(platform: Platform, instanceId: string): Promise<{ ok: boolean; error?: string }> {
    const key = this.channelKey(platform, instanceId);
    const ch = this.channels.get(key);
    if (!ch) return { ok: false, error: 'Channel not running' };

    // Prefer the WeChat user who scanned the QR code — that's the real user
    // this bot belongs to. Fall back to the last active bound conversation.
    const config = this.imStore
      .listInstances()
      .find(i => i.platform === platform && i.instanceId === instanceId);
    const ownerUserId = config?.configJson?.ownerUserId as string | undefined;
    let toUserId = ownerUserId || '';
    if (!toUserId) {
      const convs = this.imStore.listConversations(instanceId);
      if (convs.length === 0) {
        return { ok: false, error: '尚未识别到登录用户，请重新扫码连接' };
      }
      toUserId = convs[0].conversationId;
    }

    const ok = await ch.sendMessage(toUserId, HELP_MESSAGE);
    if (ok) {
      this.getStatusMeta(key).lastOutboundAt = Date.now();
      this.emit('channel-status', { platform, instanceId, connected: true });
    }
    return { ok };
  }

  async startAllEnabled(): Promise<void> {
    const settings = this.imStore.getSettings();
    if (!settings.globalEnabled) {
      console.log(`[IMGatewayManager] IM globally disabled, skipping auto-start`);
      return;
    }
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
      const [platform, instanceId] = key.split(':') as [Platform, string];
      const meta = this.getStatusMeta(key);
      if (!status[platform]) {
        status[platform] = { instances: [] };
      }
      status[platform].instances.push({
        instanceId,
        instanceName: key,
        connected: ch.isConnected,
        startedAt: meta.startedAt,
        lastError: meta.lastError,
        lastInboundAt: meta.lastInboundAt,
        lastOutboundAt: meta.lastOutboundAt,
        accountId: ch.currentAccountId,
      });
    }
    return status;
  }

  // WeChat QR flow

  async wechatQrStart(instanceId: string): Promise<{ qrDataUrl: string; qrContent: string; sessionKey: string }> {
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
    return (ch as WechatChannel).startLogin();
  }

  async wechatQrWait(instanceId: string, sessionKey: string): Promise<{ connected: boolean; accountId?: string; message?: string }> {
    const key = this.channelKey('wechat', instanceId);
    const ch = this.channels.get(key);
    if (!ch) throw new Error('WeChat channel not found');
    const result = await (ch as WechatChannel).waitForScan(sessionKey);

    // Persist credentials so the channel can be restored on restart, then
    // begin long-polling for incoming messages.
    if (result.connected && result.accountId) {
      const config = this.imStore.listInstances().find(i => i.platform === 'wechat' && i.instanceId === instanceId);
      if (config) {
        this.imStore.upsertInstance({
          ...config,
          configJson: {
            ...config.configJson,
            accountId: result.accountId,
            token: result.botToken,
            baseUrl: result.baseUrl,
            ownerUserId: result.userId,
          },
          enabled: true,
          updatedAt: Date.now(),
        });
      }
      ch.startPolling();
    }

    return result;
  }

  // POPO QR flow

  async popoQrStart(instanceId: string): Promise<{ qrUrl: string; taskToken: string; timeoutMs: number }> {
    const key = this.channelKey('popo', instanceId);
    let ch = this.channels.get(key);
    if (!ch) {
      const config = this.imStore.listInstances().find(i => i.platform === 'popo' && i.instanceId === instanceId);
      if (!config) throw new Error(`No POPO config found for instance ${instanceId}`);
      await this.startChannel(config);
      ch = this.channels.get(key);
    }
    if (!ch) throw new Error('POPO channel not found after start');

    console.log('[IMGatewayManager] POPO QR start');
    // 生成唯一 taskToken，扫码后用于轮询换取凭据
    const taskToken = `${instanceId}_${Date.now()}`;
    const timeoutMs = 600_000; // 10 分钟
    // 构建 POPO 扫码 H5 页面 URL（用户扫码后确认授权）
    const qrUrl = `https://f2e.popo.netease.com/polymers/lobster-bot-h5/?pp_htb=1&pp_back_type=cross&taskToken=${encodeURIComponent(taskToken)}&timeout=${Date.now() + timeoutMs}`;

    return {
      qrUrl,
      taskToken,
      timeoutMs,
    };
  }

  async popoQrPoll(taskToken: string): Promise<{ success: boolean; appKey?: string; appSecret?: string; aesKey?: string; message: string }> {
    console.log(`[IMGatewayManager] POPO QR poll ${taskToken}`);

    // 从 taskToken 提取 instanceId（格式: instanceId_timestamp）
    const lastUnderscore = taskToken.lastIndexOf('_');
    const instanceId = lastUnderscore > 0 ? taskToken.slice(0, lastUnderscore) : taskToken;
    const key = this.channelKey('popo', instanceId);
    const ch = this.channels.get(key);

    if (!ch) {
      return { success: false, message: 'POPO channel not found' };
    }

    try {
      // 轮询 POPO Open API，检查用户是否已扫码确认
      const url = `https://open.popo.netease.com/open-apis/no-auth/openclaw/v1/polling?taskToken=${encodeURIComponent(taskToken)}`;
      const res = await fetch(url, {
        method: 'GET',
        headers: { 'Accept': 'application/json' },
        signal: AbortSignal.timeout(10_000),
      });
      const body = await res.json();
      const data = (body as any)?.data;

      if (data?.status === 'CREATED' && data?.result) {
        const { appKey, appSecret, aesKey } = data.result;
        // 持久化凭据
        const config = this.imStore.listInstances().find(i => i.platform === 'popo' && i.instanceId === instanceId);
        if (config) {
          this.imStore.upsertInstance({
            ...config,
            configJson: { ...config.configJson, appKey, appSecret, aesKey },
            enabled: true,
            updatedAt: Date.now(),
          });
        }
        // 恢复连接并启动轮询
        ch.restoreConnection({ appKey, appSecret, aesKey });
        ch.startPolling();
        this.emit('channel-status', { platform: 'popo' as Platform, instanceId, connected: true, accountId: appKey });

        // 通知服务端扫码已完成（best-effort）
        fetch(`https://open.popo.netease.com/open-apis/no-auth/openclaw/v1/completed?taskToken=${encodeURIComponent(taskToken)}`, { method: 'GET' })
          .catch(() => { /* ignore */ });

        return { success: true, appKey, appSecret, aesKey, message: 'connected' };
      }

      return { success: false, message: 'waiting' };
    } catch {
      return { success: false, message: 'waiting' };
    }
  }

  // ── Telegram Token 登录 ──

  async telegramLogin(platform: Platform, instanceId: string, token: string): Promise<{ ok: boolean; error?: string }> {
    const key = this.channelKey(platform, instanceId);

    // 如果已有 channel 在运行，先停掉
    const existing = this.channels.get(key);
    if (existing) {
      await existing.disconnect();
      this.channels.delete(key);
    }

    // 创建新 channel 并验证 token
    const ch = new TelegramChannel({
      instanceId,
      instanceName: instanceId,
    });

    const verifyResult = await ch.verifyToken(token);
    if (!verifyResult.ok) {
      return { ok: false, error: verifyResult.error };
    }

    // 持久化凭据
    const config = this.imStore.listInstances().find(
      (i) => i.platform === platform && i.instanceId === instanceId
    );
    if (config) {
      this.imStore.upsertInstance({
        ...config,
        configJson: {
          ...config.configJson,
          token,
          accountId: verifyResult.accountId,
          botUsername: verifyResult.botUsername,
        },
        enabled: true,
        updatedAt: Date.now(),
      });
    }

    // 注册事件
    const meta = this.getStatusMeta(key);
    meta.startedAt = Date.now();

    this.registerChannelHandlers(ch, config!, platform, instanceId, meta);

    // 启动 channel
    this.channels.set(key, ch);
    ch.restoreConnection({
      token,
      accountId: verifyResult.accountId,
      botUsername: verifyResult.botUsername,
    });
    ch.startPolling();

    this.emit('channel-status', {
      platform: 'telegram' as Platform,
      instanceId,
      connected: true,
      accountId: verifyResult.accountId,
    });

    return { ok: true };
  }

  // ── Discord Token 登录 ──

  async discordLogin(platform: Platform, instanceId: string, token: string): Promise<{ ok: boolean; error?: string }> {
    const key = this.channelKey(platform, instanceId);

    const existing = this.channels.get(key);
    if (existing) {
      await existing.disconnect();
      this.channels.delete(key);
    }

    const ch = new DiscordChannel({
      instanceId,
      instanceName: instanceId,
    });

    const verifyResult = await ch.verifyToken(token);
    if (!verifyResult.ok) {
      return { ok: false, error: verifyResult.error };
    }

    const config = this.imStore.listInstances().find(
      (i) => i.platform === platform && i.instanceId === instanceId
    );
    if (config) {
      this.imStore.upsertInstance({
        ...config,
        configJson: {
          ...config.configJson,
          token,
          accountId: verifyResult.accountId,
          botUserId: verifyResult.botUserId,
        },
        enabled: true,
        updatedAt: Date.now(),
      });
    }

    const meta = this.getStatusMeta(key);
    meta.startedAt = Date.now();

    this.registerChannelHandlers(ch, config!, platform, instanceId, meta);

    this.channels.set(key, ch);
    ch.restoreConnection({
      token,
      accountId: verifyResult.accountId,
      botUserId: verifyResult.botUserId,
    });
    ch.startPolling();

    this.emit('channel-status', {
      platform: 'discord' as Platform,
      instanceId,
      connected: true,
      accountId: verifyResult.accountId,
    });

    return { ok: true };
  }

  // ── QQ 登录 ──

  async qqLogin(platform: Platform, instanceId: string, appId: string, clientSecret: string): Promise<{ ok: boolean; error?: string }> {
    const key = this.channelKey(platform, instanceId);

    const existing = this.channels.get(key);
    if (existing) {
      await existing.disconnect();
      this.channels.delete(key);
    }

    const ch = new QQChannel({
      instanceId,
      instanceName: instanceId,
    });

    const verifyResult = await ch.verifyCredentials(appId, clientSecret);
    if (!verifyResult.ok) {
      return { ok: false, error: verifyResult.error };
    }

    const config = this.imStore.listInstances().find(
      (i) => i.platform === platform && i.instanceId === instanceId
    );
    if (config) {
      this.imStore.upsertInstance({
        ...config,
        configJson: {
          ...config.configJson,
          appId,
          clientSecret,
          accountId: verifyResult.accountId,
        },
        enabled: true,
        updatedAt: Date.now(),
      });
    }

    const meta = this.getStatusMeta(key);
    meta.startedAt = Date.now();

    this.registerChannelHandlers(ch, config!, platform, instanceId, meta);

    this.channels.set(key, ch);
    ch.restoreConnection({
      appId,
      clientSecret,
      accountId: verifyResult.accountId,
    });
    ch.startPolling();

    this.emit('channel-status', {
      platform: 'qq' as Platform,
      instanceId,
      connected: true,
      accountId: verifyResult.accountId,
    });

    return { ok: true };
  }

  // ── 飞书登录 ──

  async feishuLogin(platform: Platform, instanceId: string, appId: string, appSecret: string): Promise<{ ok: boolean; error?: string }> {
    const key = this.channelKey(platform, instanceId);

    const existing = this.channels.get(key);
    if (existing) {
      await existing.disconnect();
      this.channels.delete(key);
    }

    const ch = new FeishuChannel({
      instanceId,
      instanceName: instanceId,
    });

    const verifyResult = await ch.verifyApp(appId, appSecret);
    if (!verifyResult.ok) {
      return { ok: false, error: verifyResult.error };
    }

    const config = this.imStore.listInstances().find(
      (i) => i.platform === platform && i.instanceId === instanceId
    );
    if (config) {
      this.imStore.upsertInstance({
        ...config,
        configJson: {
          ...config.configJson,
          appId,
          appSecret,
          accountId: verifyResult.accountId,
          botOpenId: verifyResult.botOpenId,
        },
        enabled: true,
        updatedAt: Date.now(),
      });
    }

    const meta = this.getStatusMeta(key);
    meta.startedAt = Date.now();

    this.registerChannelHandlers(ch, config!, platform, instanceId, meta);

    this.channels.set(key, ch);
    ch.restoreConnection({
      appId,
      appSecret,
      accountId: verifyResult.accountId,
      botOpenId: verifyResult.botOpenId,
    });
    ch.startPolling();

    this.emit('channel-status', {
      platform: 'feishu' as Platform,
      instanceId,
      connected: true,
      accountId: verifyResult.accountId,
    });

    return { ok: true };
  }

  // ── 企业微信登录 ──

  async wecomLogin(platform: Platform, instanceId: string, corpId: string, secret: string, agentId?: string): Promise<{ ok: boolean; error?: string }> {
    const key = this.channelKey(platform, instanceId);

    const existing = this.channels.get(key);
    if (existing) {
      await existing.disconnect();
      this.channels.delete(key);
    }

    const ch = new WecomChannel({
      instanceId,
      instanceName: instanceId,
    });

    const verifyResult = await ch.verifyCredentials(corpId, secret);
    if (!verifyResult.ok) {
      return { ok: false, error: verifyResult.error };
    }

    const config = this.imStore.listInstances().find(
      (i) => i.platform === platform && i.instanceId === instanceId
    );
    if (config) {
      this.imStore.upsertInstance({
        ...config,
        configJson: {
          ...config.configJson,
          corpId,
          secret,
          agentId,
        },
        enabled: true,
        updatedAt: Date.now(),
      });
    }

    const meta = this.getStatusMeta(key);
    meta.startedAt = Date.now();

    this.registerChannelHandlers(ch, config!, platform, instanceId, meta);

    this.channels.set(key, ch);
    ch.restoreConnection({
      corpId,
      secret,
      agentId,
    });
    ch.startPolling();

    this.emit('channel-status', {
      platform: 'wecom' as Platform,
      instanceId,
      connected: true,
      accountId: verifyResult.accountId,
    });

    return { ok: true };
  }

  // ── 钉钉登录 ──

  async dingtalkLogin(platform: Platform, instanceId: string, appKey: string, appSecret: string): Promise<{ ok: boolean; error?: string }> {
    const key = this.channelKey(platform, instanceId);

    const existing = this.channels.get(key);
    if (existing) {
      await existing.disconnect();
      this.channels.delete(key);
    }

    const ch = new DingTalkChannel({
      instanceId,
      instanceName: instanceId,
    });

    const verifyResult = await ch.verifyCredentials(appKey, appSecret);
    if (!verifyResult.ok) {
      return { ok: false, error: verifyResult.error };
    }

    const config = this.imStore.listInstances().find(
      (i) => i.platform === platform && i.instanceId === instanceId
    );
    if (config) {
      this.imStore.upsertInstance({
        ...config,
        configJson: {
          ...config.configJson,
          appKey,
          appSecret,
          accountId: verifyResult.accountId,
        },
        enabled: true,
        updatedAt: Date.now(),
      });
    }

    const meta = this.getStatusMeta(key);
    meta.startedAt = Date.now();

    this.registerChannelHandlers(ch, config!, platform, instanceId, meta);

    this.channels.set(key, ch);
    ch.restoreConnection({
      appKey,
      appSecret,
      accountId: verifyResult.accountId,
    });
    ch.startPolling();

    this.emit('channel-status', {
      platform: 'dingtalk' as Platform,
      instanceId,
      connected: true,
      accountId: verifyResult.accountId,
    });

    return { ok: true };
  }
}

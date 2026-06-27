// frontend/src/main/im/dingtalkChannel.ts
import { EventEmitter } from 'events';
import { randomUUID } from 'crypto';
import type { IChannel, ChannelMessage, ConnectedInfo, ChannelOptions } from './IChannel';

// ── 钉钉 Open API 类型 ──

interface DTAccessTokenResp {
  accessToken: string;
  expireIn: number;
}

interface DTConnectionOpenReq {
  clientId: string;
  subscriptions: Array<{
    type: string;
    topic: string;
  }>;
}

interface DTConnectionOpenResp {
  endpoint: string;
  ticket: string;
}

interface DTEvent {
  type?: string;
  headers?: Record<string, string>;
  data?: string;
}

interface DTRobotMessageEventContent {
  conversationId?: string;
  conversationType?: string; // '1' = direct, '2' = group
  msgId?: string;
  senderId?: string;
  senderNick?: string;
  senderStaffId?: string;
  createAt?: number;
  text?: {
    content?: string;
  };
}

interface DTBatchSendReq {
  robotCode: string;
  userIds: string[];
  msgKey: string;
  msgParam: string;
}

interface DTBatchSendResp {
  processQueryKey?: string;
}

// ── 常量 ──

const DT_API_BASE = 'https://api.dingtalk.com';
const DEFAULT_API_TIMEOUT_MS = 15_000;
const HEARTBEAT_INTERVAL_MS = 30_000;
const RECONNECT_DELAY_MS = 3_000;

// ── DingTalkChannel ──

/**
 * DingTalkChannel — 基于钉钉 Open API v1.0 (WebSocket stream mode) 实现 IChannel。
 *
 * - OAuth2 accessToken 认证 (appKey + appSecret)
 * - WebSocket 长连接接收消息
 * - batchSend API 发送文本
 * - 支持 app 重启时凭据恢复
 */
export class DingTalkChannel extends EventEmitter implements IChannel {
  private instanceId: string;
  private instanceName: string;
  private connected: boolean = false;
  private accountId: string | null = null;
  private appKey: string | null = null;
  private appSecret: string | null = null;
  private accessToken: string | null = null;
  private abortController: AbortController | null = null;
  private pollPromise: Promise<void> | null = null;
  private ws: any = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private botUserId: string | null = null;
  private clientId: string;

  constructor(options: ChannelOptions) {
    super();
    this.instanceId = options.instanceId;
    this.instanceName = options.instanceName;
    this.clientId = `robot_${this.instanceId}_${randomUUID()}`;
  }

  // ── 属性 ──

  get isConnected(): boolean {
    return this.connected;
  }

  get currentAccountId(): string | null {
    return this.accountId;
  }

  // ── 凭据恢复 ──

  restoreConnection(credentials: Record<string, unknown>): void {
    const appKey = credentials.appKey as string | undefined;
    const appSecret = credentials.appSecret as string | undefined;
    if (!appKey || !appSecret) {
      console.error(`[DingTalkChannel:${this.instanceId}] restoreConnection: missing appKey/appSecret`);
      return;
    }
    this.appKey = appKey;
    this.appSecret = appSecret;
    if (credentials.accessToken) {
      this.accessToken = credentials.accessToken as string;
    }
    if (credentials.accountId) {
      this.accountId = credentials.accountId as string;
    }
    if (credentials.clientId) {
      this.clientId = credentials.clientId as string;
    }
    this.connected = true;
    console.log(`[DingTalkChannel:${this.instanceId}] Connection restored`);
  }

  // ── 认证 ──

  /**
   * 获取/刷新 accessToken。
   * 使用 appKey + appSecret 换取短期 token（通常 2 小时有效）。
   */
  async getAccessToken(): Promise<string> {
    if (!this.appKey || !this.appSecret) {
      throw new Error('appKey/appSecret not set');
    }

    const url = `${DT_API_BASE}/v1.0/oauth2/accessToken`;
    const body = JSON.stringify({
      appKey: this.appKey,
      appSecret: this.appSecret,
    });

    const rawText = await this.apiPost(url, body, undefined, DEFAULT_API_TIMEOUT_MS);
    const data: DTAccessTokenResp = JSON.parse(rawText);

    if (!data.accessToken) {
      throw new Error(`Failed to get accessToken: ${rawText.slice(0, 200)}`);
    }

    this.accessToken = data.accessToken;
    this.accountId = this.appKey;
    console.log(`[DingTalkChannel:${this.instanceId}] Access token obtained, expires in ${data.expireIn}s`);
    return this.accessToken;
  }

  // ── 凭据验证 ──

  /**
   * 验证 appKey 和 appSecret 有效性。
   * 由 IMGatewayManager 在钉钉登录时调用。
   */
  async verifyCredentials(appKey: string, appSecret: string): Promise<{ ok: boolean; accountId?: string; error?: string }> {
    try {
      const url = `${DT_API_BASE}/v1.0/oauth2/accessToken`;
      const body = JSON.stringify({ appKey, appSecret });
      const rawText = await this.apiPost(url, body, undefined, DEFAULT_API_TIMEOUT_MS);
      const data: DTAccessTokenResp = JSON.parse(rawText);
      if (!data.accessToken) {
        return { ok: false, error: '获取 accessToken 失败，请检查 appKey 和 appSecret' };
      }
      this.appKey = appKey;
      this.appSecret = appSecret;
      this.accessToken = data.accessToken;
      this.accountId = appKey;
      return { ok: true, accountId: appKey };
    } catch (e: any) {
      return { ok: false, error: e?.message || String(e) };
    }
  }

  // ── 消息接收 ──

  startPolling(): void {
    if (!this.connected) {
      console.error(`[DingTalkChannel:${this.instanceId}] Cannot start polling: not connected`);
      return;
    }
    if (!this.appKey || !this.appSecret) {
      console.error(`[DingTalkChannel:${this.instanceId}] Cannot start polling: missing credentials`);
      return;
    }
    if (this.pollPromise) {
      console.warn(`[DingTalkChannel:${this.instanceId}] Polling already active`);
      return;
    }

    this.abortController = new AbortController();
    this.pollPromise = this.pollLoop(this.abortController.signal);
  }

  private async pollLoop(abortSignal: AbortSignal): Promise<void> {
    console.log(`[DingTalkChannel:${this.instanceId}] Starting DingTalk WebSocket loop`);

    while (!abortSignal.aborted) {
      try {
        // 1. 确保有 accessToken
        if (!this.accessToken) {
          await this.getAccessToken();
        }

        // 2. 获取 WebSocket endpoint + ticket
        const openUrl = `${DT_API_BASE}/v1.0/gateway/connections/open`;
        const openBody: DTConnectionOpenReq = {
          clientId: this.clientId,
          subscriptions: [
            { type: 'EVENT', topic: '*' },
          ],
        };
        const rawOpen = await this.apiPost(openUrl, JSON.stringify(openBody), abortSignal, DEFAULT_API_TIMEOUT_MS);
        const connInfo: DTConnectionOpenResp = JSON.parse(rawOpen);

        if (!connInfo.endpoint || !connInfo.ticket) {
          throw new Error(`Invalid connection response: ${rawOpen.slice(0, 200)}`);
        }

        // 3. 连接 WebSocket → 发送 ticket → 接收事件
        await this.connectWebSocket(connInfo.endpoint, connInfo.ticket, abortSignal);
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          break;
        }
        console.warn(`[DingTalkChannel:${this.instanceId}] Poll error (will retry): ${err instanceof Error ? err.message : String(err)}`);
        // 等待后重试
        await new Promise((r) => setTimeout(r, RECONNECT_DELAY_MS));
      }
    }

    console.log(`[DingTalkChannel:${this.instanceId}] Poll loop exited`);
  }

  private async connectWebSocket(
    endpoint: string,
    ticket: string,
    abortSignal: AbortSignal,
  ): Promise<void> {
    return new Promise((resolve, reject) => {
      // 动态 import ws 模块（Electron 环境）
      // 使用 require 避免类型检查问题
      const WebSocket = require('ws');

      const socket = new WebSocket(endpoint);
      this.ws = socket;
      let ticketSent = false;
      let resolved = false;

      const cleanup = (): void => {
        if (this.ws === socket) this.ws = null;
        this.stopHeartbeat();
        resolved = true;
      };

      socket.on('open', () => {
        console.log(`[DingTalkChannel:${this.instanceId}] WebSocket connected, sending ticket`);
        // 发送 ticket 作为首条消息
        socket.send(JSON.stringify({ ticket }));
        ticketSent = true;

        // 启动心跳
        this.startHeartbeat(socket);

        // 发送 connected 事件
        if (!this.connected) {
          this.connected = true;
        }
        this.emit('connected', {
          accountId: this.appKey || '',
        } as ConnectedInfo);

        resolve();
      });

      socket.on('message', (data: any) => {
        try {
          const raw = typeof data === 'string' ? data : data.toString();
          const event: DTEvent = JSON.parse(raw);

          // 解析钉钉推送的事件
          this.handleDTEvent(event);
        } catch (err) {
          console.warn(`[DingTalkChannel:${this.instanceId}] Failed to parse WS message:`, err);
        }
      });

      socket.on('close', (code: number, reason: Buffer) => {
        console.log(`[DingTalkChannel:${this.instanceId}] WebSocket closed: code=${code} reason=${reason?.toString() || ''}`);
        if (!resolved) {
          cleanup();
          reject(new Error(`WebSocket closed before ready: code=${code}`));
        } else {
          cleanup();
        }
      });

      socket.on('error', (err: Error) => {
        console.error(`[DingTalkChannel:${this.instanceId}] WebSocket error:`, err.message);
        if (!resolved) {
          cleanup();
          reject(err);
        } else {
          this.emit('error', err);
        }
      });

      // 监听外部 abort
      const onAbort = (): void => {
        socket.close(1000, 'Client abort');
        if (!resolved) {
          cleanup();
          reject(new DOMException('Aborted', 'AbortError'));
        }
      };
      if (abortSignal.aborted) {
        onAbort();
        return;
      }
      abortSignal.addEventListener('abort', onAbort, { once: true });
    });
  }

  private handleDTEvent(event: DTEvent): void {
    // 事件体在 data 字段，可能是 JSON 字符串
    if (!event.data) return;

    try {
      const payload: DTRobotMessageEventContent = JSON.parse(event.data);
      const msg = this.convertEvent(payload);
      if (msg) {
        this.emit('message', msg);
      }
    } catch (err) {
      console.warn(`[DingTalkChannel:${this.instanceId}] Failed to parse event data:`, err);
    }
  }

  private convertEvent(event: DTRobotMessageEventContent): ChannelMessage | null {
    // 必须要有 senderId 和内容
    if (!event.senderId) return null;

    // 过滤 bot 自身的消息，避免死循环
    if (event.senderId === this.botUserId) return null;

    const content = event.text?.content || '[非文本消息]';

    const chatType: 'direct' | 'group' =
      event.conversationType === '2' ? 'group' : 'direct';

    const conversationId = event.conversationId || event.senderId;
    const messageId = event.msgId || randomUUID();

    return {
      messageId,
      conversationId,
      senderId: event.senderId,
      senderName: event.senderNick,
      groupName: undefined, // 批量回调不会返回群名，可通过聊天历史补齐
      content,
      chatType,
      timestamp: event.createAt || Date.now(),
    };
  }

  // ── 心跳 ──

  private startHeartbeat(socket: any): void {
    this.stopHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      if (socket.readyState === 1 /* OPEN */) {
        socket.ping();
      }
    }, HEARTBEAT_INTERVAL_MS);
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer !== null) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  // ── 消息发送 ──

  async sendMessage(conversationId: string, text: string): Promise<boolean> {
    if (!this.connected) {
      console.error(`[DingTalkChannel:${this.instanceId}] Cannot send: not connected`);
      return false;
    }

    if (!this.accessToken) {
      try {
        await this.getAccessToken();
      } catch (e: any) {
        console.error(`[DingTalkChannel:${this.instanceId}] Cannot get token for send:`, e?.message);
        return false;
      }
    }

    try {
      const url = `${DT_API_BASE}/v1.0/robot/oToMessages/batchSend`;
      const body: DTBatchSendReq = {
        robotCode: this.appKey!,
        userIds: [conversationId],
        msgKey: 'sampleText',
        msgParam: JSON.stringify({ content: text }),
      };

      const rawText = await this.apiPost(url, JSON.stringify(body), undefined, DEFAULT_API_TIMEOUT_MS);
      const resp: DTBatchSendResp = JSON.parse(rawText);

      if (!resp.processQueryKey) {
        console.error(`[DingTalkChannel:${this.instanceId}] batchSend returned no processQueryKey`);
        this.emit('error', new Error(`batchSend failed: no processQueryKey`));
        return false;
      }

      console.log(`[DingTalkChannel:${this.instanceId}] Message sent to ${conversationId}, processQueryKey: ${resp.processQueryKey}`);
      return true;
    } catch (e: any) {
      console.error(`[DingTalkChannel:${this.instanceId}] sendMessage failed:`, e?.message);
      this.emit('error', new Error(`sendMessage failed: ${e?.message || String(e)}`));
      return false;
    }
  }

  // ── 连接管理 ──

  async disconnect(): Promise<void> {
    try {
      // 通过 abortController 停止 pollLoop
      if (this.abortController) {
        this.abortController.abort();
        this.abortController = null;
      }

      // 等待 pollLoop 结束
      if (this.pollPromise) {
        try { await this.pollPromise; } catch { /* ignore */ }
        this.pollPromise = null;
      }

      // 关闭 WebSocket
      this.stopHeartbeat();
      if (this.ws) {
        try { this.ws.close(1000, 'Client disconnect'); } catch { /* ignore */ }
        this.ws = null;
      }

      this.connected = false;
      this.accountId = null;
      this.appKey = null;
      this.appSecret = null;
      this.accessToken = null;
      this.botUserId = null;

      this.emit('disconnected');
      console.log(`[DingTalkChannel:${this.instanceId}] Disconnected`);
    } catch (e: any) {
      console.error(`[DingTalkChannel:${this.instanceId}] disconnect error:`, e?.message);
    }
  }

  // ── HTTP helpers ──

  private async apiGet(url: string, timeoutMs?: number, abortSignal?: AbortSignal): Promise<string> {
    const controller = timeoutMs ? new AbortController() : undefined;
    const timer = controller && timeoutMs
      ? setTimeout(() => controller!.abort(), timeoutMs)
      : undefined;

    let signal: AbortSignal | undefined = controller?.signal;
    let cleanup = (): void => {};
    if (abortSignal) {
      if (abortSignal.aborted) {
        controller?.abort();
      } else {
        const onAbort = (): void => controller?.abort();
        abortSignal.addEventListener('abort', onAbort, { once: true });
        cleanup = (): void => abortSignal.removeEventListener('abort', onAbort);
      }
    }

    try {
      const headers: Record<string, string> = {};
      if (this.accessToken) {
        headers['x-acs-dingtalk-access-token'] = this.accessToken;
      }

      const res = await fetch(url, {
        method: 'GET',
        headers,
        ...(signal ? { signal } : {}),
      });
      if (timer) clearTimeout(timer);
      const text = await res.text();
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${text.slice(0, 200)}`);
      }
      return text;
    } catch (err) {
      if (timer) clearTimeout(timer);
      throw err;
    } finally {
      cleanup();
    }
  }

  private async apiPost(
    url: string,
    body: string,
    abortSignal?: AbortSignal,
    timeoutMs?: number,
  ): Promise<string> {
    const controller = timeoutMs ? new AbortController() : undefined;
    const timer = controller && timeoutMs
      ? setTimeout(() => controller!.abort(), timeoutMs)
      : undefined;

    let signal: AbortSignal | undefined = controller?.signal;
    let cleanup = (): void => {};
    if (abortSignal) {
      if (abortSignal.aborted) {
        controller?.abort();
      } else {
        const onAbort = (): void => controller?.abort();
        abortSignal.addEventListener('abort', onAbort, { once: true });
        cleanup = (): void => abortSignal.removeEventListener('abort', onAbort);
      }
    }

    try {
      const headers: Record<string, string> = {
        'Content-Type': 'application/json',
      };
      if (this.accessToken) {
        headers['x-acs-dingtalk-access-token'] = this.accessToken;
      }

      const res = await fetch(url, {
        method: 'POST',
        headers,
        body,
        ...(signal ? { signal } : {}),
      });
      if (timer) clearTimeout(timer);
      const text = await res.text();
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${text.slice(0, 200)}`);
      }
      return text;
    } catch (err) {
      if (timer) clearTimeout(timer);
      throw err;
    } finally {
      cleanup();
    }
  }
}

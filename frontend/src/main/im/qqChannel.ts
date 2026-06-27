// frontend/src/main/im/qqChannel.ts
import { EventEmitter } from 'events';
import type { IChannel, ChannelMessage, ConnectedInfo, ChannelOptions } from './IChannel';

// ── QQ Bot API 类型 ──

interface QQAccessTokenResp {
  access_token: string;
  expires_in: number;
}

interface QQGatewayResp {
  url: string;
}

interface QQWebSocketPayload {
  op: number;
  d?: any;
  s?: number;
  t?: string;
}

interface QQAuthor {
  id: string;
  username?: string;
  avatar?: string;
  bot?: boolean;
}

interface QQMessageData {
  id: string;
  author: QQAuthor;
  content: string;
  channel_id?: string;
  guild_id?: string;
  group_openid?: string;
  timestamp?: string;
}

// ── 常量 ──

const QQ_AUTH_URL = 'https://bots.qq.com/app/getAppAccessToken';
const QQ_API_BASE = 'https://api.sgroup.qq.com';
const DEFAULT_API_TIMEOUT_MS = 15_000;
const TOKEN_REFRESH_MARGIN_MS = 300_000; // 提前 5 分钟刷新（默认 expires_in = 7200s）

// ── QQChannel ──

/**
 * QQChannel — 基于 QQ Bot WebSocket API 实现 IChannel。
 *
 * - appId + clientSecret → accessToken
 * - WebSocket 长连接接收消息（AT_MESSAGE_CREATE / DIRECT_MESSAGE_CREATE）
 * - HTTP API 发送消息（群聊 / 私聊）
 * - 支持 token 过期自动刷新（7200s）
 * - 支持 app 重启时凭据恢复
 */
export class QQChannel extends EventEmitter implements IChannel {
  private instanceId: string;
  private instanceName: string;
  private connected: boolean = false;
  private accountId: string | null = null;
  private appId: string | null = null;
  private clientSecret: string | null = null;
  private accessToken: string | null = null;
  private botId: string | null = null;
  private abortController: AbortController | null = null;
  private pollPromise: Promise<void> | null = null;
  private ws: WebSocket | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private tokenRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  private lastSequence: number | null = null;
  /** 记录已知群聊 openid，用于 sendMessage 时选择正确的 API 端点 */
  private groupIds: Set<string> = new Set();

  constructor(options: ChannelOptions) {
    super();
    this.instanceId = options.instanceId;
    this.instanceName = options.instanceName;
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
    const appId = credentials.appId as string | undefined;
    const clientSecret = credentials.clientSecret as string | undefined;
    if (!appId || !clientSecret) {
      console.error(`[QQChannel:${this.instanceId}] restoreConnection: missing appId or clientSecret`);
      return;
    }
    this.appId = appId;
    this.clientSecret = clientSecret;
    if (credentials.accountId) {
      this.accountId = credentials.accountId as string;
    }
    if (credentials.botId) {
      this.botId = credentials.botId as string;
    }
    this.connected = true;
    console.log(`[QQChannel:${this.instanceId}] Connection restored`);
  }

  // ── Access Token ──

  /**
   * 使用 appId + clientSecret 换取 access_token。
   * 自动设置定时器提前刷新 token。
   */
  private async getAccessToken(): Promise<string> {
    if (!this.appId || !this.clientSecret) {
      throw new Error('appId or clientSecret not configured');
    }

    try {
      const rawText = await this.apiPost(
        QQ_AUTH_URL,
        JSON.stringify({ appId: this.appId, clientSecret: this.clientSecret }),
        false, // token 接口不需要 Authorization header
      );
      const data: QQAccessTokenResp = JSON.parse(rawText);
      if (!data.access_token) {
        throw new Error('Failed to get access token: no access_token in response');
      }
      this.accessToken = data.access_token;

      // 提前刷新 token（7200s 过期，默认提前 5 分钟刷新）
      const expiresInMs = (data.expires_in || 7200) * 1000;
      const refreshMs = Math.max(expiresInMs - TOKEN_REFRESH_MARGIN_MS, 60_000);
      if (this.tokenRefreshTimer) clearTimeout(this.tokenRefreshTimer);
      this.tokenRefreshTimer = setTimeout(() => {
        this.getAccessToken().catch((e) =>
          console.warn(`[QQChannel:${this.instanceId}] Token refresh failed:`, e?.message),
        );
      }, refreshMs);

      console.log(`[QQChannel:${this.instanceId}] Access token obtained, expires in ${data.expires_in || 7200}s`);
      return this.accessToken;
    } catch (e: any) {
      console.error(`[QQChannel:${this.instanceId}] getAccessToken failed:`, e?.message);
      throw e;
    }
  }

  // ── 凭据验证 ──

  /**
   * 验证 appId 和 clientSecret 有效性。
   * 由 IMGatewayManager 在 QQ 登录时调用。
   */
  async verifyCredentials(appId: string, clientSecret: string): Promise<{ ok: boolean; accountId?: string; error?: string }> {
    try {
      const rawText = await this.apiPost(
        QQ_AUTH_URL,
        JSON.stringify({ appId, clientSecret }),
        false,
      );
      const data: QQAccessTokenResp = JSON.parse(rawText);
      if (!data.access_token) {
        return { ok: false, error: '获取 access_token 失败，请检查 appId 和 clientSecret' };
      }
      this.appId = appId;
      this.clientSecret = clientSecret;
      this.accessToken = data.access_token;
      return { ok: true, accountId: appId };
    } catch (e: any) {
      return { ok: false, error: e?.message || String(e) };
    }
  }

  // ── 消息接收（WebSocket 长连接） ──

  startPolling(): void {
    if (!this.connected || !this.appId || !this.clientSecret) {
      console.error(`[QQChannel:${this.instanceId}] Cannot start polling: not connected or missing credentials`);
      return;
    }
    if (this.pollPromise) {
      console.warn(`[QQChannel:${this.instanceId}] Polling already active`);
      return;
    }

    this.abortController = new AbortController();
    this.pollPromise = this.wsLoop(this.abortController.signal);
  }

  private async wsLoop(abortSignal: AbortSignal): Promise<void> {
    console.log(`[QQChannel:${this.instanceId}] Starting QQ Bot WebSocket loop`);

    while (!abortSignal.aborted) {
      try {
        // 1. 获取 access token
        const token = await this.getAccessToken();

        // 2. 获取 WebSocket 网关地址
        const gwRaw = await this.apiPost(
          `${QQ_API_BASE}/gateway/bot`,
          '{}',
          true,
        );
        const gwResp: QQGatewayResp = JSON.parse(gwRaw);
        if (!gwResp.url) {
          throw new Error('No gateway URL returned from /gateway/bot');
        }

        console.log(`[QQChannel:${this.instanceId}] Gateway URL obtained`);

        // 3. 连接 WebSocket（阻塞直到连接断开）
        await this.connectWebSocket(gwResp.url, token, abortSignal);
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          break; // 主动断开
        }
        console.warn(
          `[QQChannel:${this.instanceId}] WS loop error (will retry in 5s): ${
            err instanceof Error ? err.message : String(err)
          }`,
        );
        await new Promise((r) => setTimeout(r, 5000));
      }
    }

    console.log(`[QQChannel:${this.instanceId}] WS loop exited`);
  }

  /**
   * 建立 WebSocket 连接并处理生命周期。
   * 返回的 Promise 在连接关闭时 resolve（外部循环负责重试）。
   */
  private connectWebSocket(
    url: string,
    token: string,
    abortSignal: AbortSignal,
  ): Promise<void> {
    return new Promise<void>((resolve) => {
      // 清理旧连接
      this.clearHeartbeat();
      if (this.ws) {
        try { this.ws.close(); } catch { /* ignore */ }
        this.ws = null;
      }

      const ws = new WebSocket(url);
      this.ws = ws;
      let settled = false;

      const finish = (): void => {
        if (settled) return;
        settled = true;
        abortSignal.removeEventListener('abort', onAbort);
        resolve();
      };

      const onAbort = (): void => {
        try { ws.close(); } catch { /* ignore */ }
      };

      if (abortSignal.aborted) {
        try { ws.close(); } catch { /* ignore */ }
        return;
      }
      abortSignal.addEventListener('abort', onAbort, { once: true });

      ws.onopen = (): void => {
        console.log(`[QQChannel:${this.instanceId}] WebSocket connected, sending IDENTIFY`);
        const identify: QQWebSocketPayload = {
          op: 2,
          d: {
            token: `QQBot ${token}`,
            intents: 1 | 512, // GUILDS | GUILD_MESSAGES
            shard: [0, 1],
          },
        };
        ws.send(JSON.stringify(identify));
      };

      ws.onmessage = (event): void => {
        try {
          const payload: QQWebSocketPayload = JSON.parse(event.data as string);
          // 记录最新的 sequence 用于心跳
          if (payload.s !== undefined) {
            this.lastSequence = payload.s;
          }
          this.handleWSPayload(payload, ws);
        } catch (e) {
          console.warn(`[QQChannel:${this.instanceId}] Failed to parse WS message`);
        }
      };

      ws.onerror = (): void => {
        console.error(`[QQChannel:${this.instanceId}] WebSocket error`);
        this.emit('error', new Error('QQ WebSocket connection error'));
      };

      ws.onclose = (event): void => {
        console.log(
          `[QQChannel:${this.instanceId}] WebSocket closed: code=${event.code} reason=${event.reason}`,
        );
        this.clearHeartbeat();
        this.ws = null;

        if (this.connected) {
          this.connected = false;
          this.emit('disconnected');
        }

        finish();
      };
    });
  }

  private handleWSPayload(payload: QQWebSocketPayload, ws: WebSocket): void {
    switch (payload.op) {
      case 10: {
        // HELLO — 开始心跳
        const heartbeatInterval: number = payload.d?.heartbeat_interval || 41250;
        console.log(`[QQChannel:${this.instanceId}] HELLO received, heartbeat_interval=${heartbeatInterval}ms`);
        this.startHeartbeat(ws, heartbeatInterval);
        break;
      }
      case 11: {
        // HEARTBEAT_ACK — 无需处理
        break;
      }
      case 0: {
        // DISPATCH — 事件分发
        this.handleDispatch(payload);
        break;
      }
      case 7: {
        // RECONNECT — 服务端要求重连
        console.log(`[QQChannel:${this.instanceId}] Server requested reconnect`);
        try { ws.close(4000, 'server reconnect request'); } catch { /* ignore */ }
        break;
      }
      case 9: {
        // INVALID_SESSION — 会话失效，需重新 IDENTIFY
        console.warn(`[QQChannel:${this.instanceId}] Invalid session, reconnecting...`);
        try { ws.close(4000, 'invalid session'); } catch { /* ignore */ }
        break;
      }
      default:
        console.log(`[QQChannel:${this.instanceId}] Unhandled op=${payload.op}`);
    }
  }

  private handleDispatch(payload: QQWebSocketPayload): void {
    const eventType = payload.t;
    const data = payload.d as QQMessageData | undefined;

    switch (eventType) {
      case 'READY': {
        const readyData = payload.d as any;
        this.botId = readyData?.user?.id || null;
        if (this.botId) {
          this.accountId = this.botId;
        }
        if (!this.connected) {
          this.connected = true;
          this.emit('connected', { accountId: this.accountId || '' });
        }
        console.log(`[QQChannel:${this.instanceId}] READY, botId=${this.botId}`);
        break;
      }
      case 'RESUMED': {
        if (!this.connected) {
          this.connected = true;
          this.emit('connected', { accountId: this.accountId || '' });
        }
        console.log(`[QQChannel:${this.instanceId}] Session resumed`);
        break;
      }
      case 'AT_MESSAGE_CREATE':
      case 'DIRECT_MESSAGE_CREATE': {
        if (!data) return;
        const msg = this.convertMessage(data);
        if (msg) {
          this.emit('message', msg);
        }
        break;
      }
      default:
        // 静默忽略其他事件类型
        break;
    }
  }

  /**
   * 将 QQ Bot 下行消息数据转换为统一的 ChannelMessage 格式。
   * 过滤掉机器人自身的消息，避免死循环。
   */
  private convertMessage(data: QQMessageData): ChannelMessage | null {
    // 过滤 bot 自身消息
    if (this.botId && data.author.id === this.botId) {
      return null;
    }

    // 判断群聊还是私聊：存在 group_openid 为群聊
    const chatType: 'direct' | 'group' = data.group_openid ? 'group' : 'direct';

    let conversationId: string;
    if (chatType === 'group') {
      conversationId = data.group_openid!;
      this.groupIds.add(data.group_openid!);
    } else {
      conversationId = data.author.id;
    }

    // QQ 时间戳可能是秒级（10位）或毫秒级（13位）
    let timestamp: number;
    if (data.timestamp) {
      const ts = Number(data.timestamp);
      timestamp = ts > 1e12 ? ts : ts * 1000;
    } else {
      timestamp = Date.now();
    }

    return {
      messageId: data.id,
      conversationId,
      senderId: data.author.id,
      senderName: data.author.username,
      content: data.content || '[非文本消息]',
      chatType,
      timestamp,
    };
  }

  // ── 消息发送 ──

  /**
   * 发送文本消息到指定会话。
   * 根据 conversationId 是否已知群聊自动选择群聊或私聊 API。
   * 遇到 401 时自动刷新 access_token 并重试一次。
   */
  async sendMessage(conversationId: string, text: string): Promise<boolean> {
    if (!this.connected || !this.accessToken) {
      console.error(`[QQChannel:${this.instanceId}] Cannot send: not connected`);
      return false;
    }

    try {
      return await this.doSendMessage(conversationId, text);
    } catch (e: any) {
      console.error(`[QQChannel:${this.instanceId}] sendMessage failed:`, e?.message);
      this.emit('error', new Error(`sendMessage failed: ${e?.message || String(e)}`));
      return false;
    }
  }

  private async doSendMessage(conversationId: string, text: string): Promise<boolean> {
    // 判断群聊或私聊端点
    const isGroup = this.groupIds.has(conversationId);
    const url = isGroup
      ? `${QQ_API_BASE}/v2/groups/${encodeURIComponent(conversationId)}/messages`
      : `${QQ_API_BASE}/v2/users/${encodeURIComponent(conversationId)}/messages`;

    const body = JSON.stringify({
      content: text,
      msg_type: 0, // 文本消息
    });

    const rawText = await this.apiPost(url, body, true, DEFAULT_API_TIMEOUT_MS);
    const resp = JSON.parse(rawText);

    // 检查错误码
    if (resp.code !== undefined && resp.code !== 0) {
      // Token 过期或无效，刷新后重试一次
      if (resp.code === 401 || resp.code === 40001) {
        console.log(`[QQChannel:${this.instanceId}] Token expired (code=${resp.code}), refreshing...`);
        try {
          this.accessToken = null;
          await this.getAccessToken();
          // 重试（不检查 groupIds 以避免无限递归，直接使用相同端点）
          const retryRaw = await this.apiPost(url, body, true, DEFAULT_API_TIMEOUT_MS);
          const retryResp = JSON.parse(retryRaw);
          if (retryResp.code !== undefined && retryResp.code !== 0) {
            console.error(
              `[QQChannel:${this.instanceId}] sendMessage retry failed: code=${retryResp.code} message=${retryResp.message || 'unknown'}`,
            );
            this.emit('error', new Error(`sendMessage failed after token refresh: ${retryResp.message || 'unknown'}`));
            return false;
          }
          console.log(`[QQChannel:${this.instanceId}] Message sent (after token refresh) to ${conversationId}`);
          return true;
        } catch (refreshErr: any) {
          console.error(`[QQChannel:${this.instanceId}] Token refresh failed:`, refreshErr?.message);
          return false;
        }
      }

      console.error(
        `[QQChannel:${this.instanceId}] sendMessage failed: code=${resp.code} message=${resp.message || 'unknown'}`,
      );
      this.emit('error', new Error(`sendMessage failed: ${resp.message || 'unknown'}`));
      return false;
    }

    console.log(`[QQChannel:${this.instanceId}] Message sent to ${conversationId}`);
    return true;
  }

  // ── 心跳 ──

  private startHeartbeat(ws: WebSocket, interval: number): void {
    this.clearHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      if (ws.readyState === WebSocket.OPEN) {
        const d = this.lastSequence ?? 0;
        ws.send(JSON.stringify({ op: 1, d }));
      }
    }, interval);
  }

  private clearHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  // ── 连接管理 ──

  async disconnect(): Promise<void> {
    try {
      // 终止轮询循环
      if (this.abortController) {
        this.abortController.abort();
        this.abortController = null;
      }

      // 清理所有定时器
      this.clearHeartbeat();
      if (this.tokenRefreshTimer) {
        clearTimeout(this.tokenRefreshTimer);
        this.tokenRefreshTimer = null;
      }

      // 关闭 WebSocket
      if (this.ws) {
        try { this.ws.close(); } catch { /* ignore */ }
        this.ws = null;
      }

      // 等待轮询循环退出
      if (this.pollPromise) {
        try { await this.pollPromise; } catch { /* ignore */ }
        this.pollPromise = null;
      }

      this.connected = false;
      this.accountId = null;
      this.appId = null;
      this.clientSecret = null;
      this.accessToken = null;
      this.botId = null;
      this.lastSequence = null;
      this.groupIds.clear();

      this.emit('disconnected');
      console.log(`[QQChannel:${this.instanceId}] Disconnected`);
    } catch (e: any) {
      console.error(`[QQChannel:${this.instanceId}] disconnect error:`, e?.message);
    }
  }

  // ── HTTP helpers ──

  /**
   * HTTP GET 请求。
   * useAuth=true 时自动添加 QQBot {accessToken} Authorization header。
   */
  private async apiGet(
    url: string,
    useAuth: boolean = true,
    timeoutMs?: number,
  ): Promise<string> {
    const headers: Record<string, string> = {};
    if (useAuth && this.accessToken) {
      headers.Authorization = `QQBot ${this.accessToken}`;
    }

    const controller = timeoutMs ? new AbortController() : undefined;
    const timer = controller && timeoutMs
      ? setTimeout(() => controller.abort(), timeoutMs)
      : undefined;

    try {
      const res = await fetch(url, {
        method: 'GET',
        headers,
        ...(controller?.signal ? { signal: controller.signal } : {}),
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
    }
  }

  /**
   * HTTP POST 请求。
   * useAuth=true 时自动添加 QQBot {accessToken} Authorization header。
   */
  private async apiPost(
    url: string,
    body: string,
    useAuth: boolean = true,
    timeoutMs?: number,
    abortSignal?: AbortSignal,
  ): Promise<string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };
    if (useAuth && this.accessToken) {
      headers.Authorization = `QQBot ${this.accessToken}`;
    }

    const controller = timeoutMs ? new AbortController() : undefined;
    const timer = controller && timeoutMs
      ? setTimeout(() => controller.abort(), timeoutMs)
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

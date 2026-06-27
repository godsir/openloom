// frontend/src/main/im/feishuChannel.ts
import { EventEmitter } from 'events';
import type { IChannel, ChannelMessage, ConnectedInfo, ChannelOptions } from './IChannel';

// ── Feishu Open API 类型 ──

interface FeishuTenantTokenResp {
  code: number;
  msg?: string;
  tenant_access_token?: string;
  expire?: number;
}

interface FeishuBotInfoResp {
  code: number;
  msg?: string;
  bot?: {
    app_id: string;
    open_id: string;
  };
}

interface FeishuWsResp {
  code: number;
  msg?: string;
  url?: string;
}

interface FeishuSendMessageResp {
  code: number;
  msg?: string;
  data?: {
    message_id: string;
  };
}

// Feishu WebSocket 下行消息
interface FeishuWsEvent {
  type: string;
  data?: any;
  event?: FeishuWsMessagePayload;
}

interface FeishuWsMessagePayload {
  type?: string;
  event_type?: string;
  event?: {
    sender?: {
      sender_id?: { open_id?: string };
      sender_type?: string;
    };
    message?: {
      message_id: string;
      chat_id: string;
      chat_type: string;
      content: string; // JSON 编码的文本
      create_time: string;
    };
  };
}

// ── 常量 ──

const FEISHU_API_BASE = 'https://open.feishu.cn/open-apis';
const DEFAULT_API_TIMEOUT_MS = 15_000;
const WS_PING_INTERVAL_MS = 30_000;
const WS_RECONNECT_DELAY_MS = 5_000;
// token 默认有效期 2 小时，提前 5 分钟刷新
const TOKEN_REFRESH_BEFORE_EXPIRE_MS = 300_000;

// ── FeishuChannel ──

/**
 * FeishuChannel — 基于飞书/Lark Open API (WebSocket) 实现 IChannel。
 *
 * - 使用 appId + appSecret 获取 tenant_access_token
 * - 通过 WebSocket 接收消息（订阅 im.message.receive_v1）
 * - sendMessage 通过 HTTP POST 发送文本
 * - 支持 app 重启时凭据恢复
 */
export class FeishuChannel extends EventEmitter implements IChannel {
  private instanceId: string;
  private instanceName: string;
  private connected: boolean = false;
  private accountId: string | null = null;
  private appId: string | null = null;
  private appSecret: string | null = null;
  private tenantAccessToken: string | null = null;
  private tokenExpireAt: number = 0;
  private abortController: AbortController | null = null;
  private pollPromise: Promise<void> | null = null;
  private ws: WebSocket | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private botOpenId: string | null = null;

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
    const appSecret = credentials.appSecret as string | undefined;
    if (!appId || !appSecret) {
      console.error(`[FeishuChannel:${this.instanceId}] restoreConnection: 缺少 appId/appSecret`);
      return;
    }
    this.appId = appId;
    this.appSecret = appSecret;
    if (credentials.tenantAccessToken) {
      this.tenantAccessToken = credentials.tenantAccessToken as string;
    }
    if (credentials.tokenExpireAt) {
      this.tokenExpireAt = credentials.tokenExpireAt as number;
    }
    if (credentials.accountId) {
      this.accountId = credentials.accountId as string;
    }
    if (credentials.botOpenId) {
      this.botOpenId = credentials.botOpenId as string;
    }
    this.connected = true;
    console.log(`[FeishuChannel:${this.instanceId}] 连接已恢复`);
  }

  // ── Token 管理 ──

  /**
   * 获取 tenant_access_token，自动处理缓存和刷新。
   */
  async getTenantAccessToken(): Promise<string> {
    // 缓存未过期时直接返回
    if (this.tenantAccessToken && Date.now() < this.tokenExpireAt - TOKEN_REFRESH_BEFORE_EXPIRE_MS) {
      return this.tenantAccessToken;
    }

    if (!this.appId || !this.appSecret) {
      throw new Error('缺少 appId 或 appSecret');
    }

    const resp = await this.apiPost(
      `${FEISHU_API_BASE}/auth/v3/tenant_access_token/internal`,
      JSON.stringify({ app_id: this.appId, app_secret: this.appSecret }),
    );
    const data: FeishuTenantTokenResp = JSON.parse(resp);

    if (data.code !== 0 || !data.tenant_access_token) {
      throw new Error(`获取 tenant_access_token 失败: ${data.msg || '未知错误'} (code: ${data.code})`);
    }

    this.tenantAccessToken = data.tenant_access_token;
    // expire 单位为秒，默认 2 小时
    this.tokenExpireAt = Date.now() + (data.expire || 7200) * 1000;

    console.log(`[FeishuChannel:${this.instanceId}] 已获取 tenant_access_token，过期时间: ${new Date(this.tokenExpireAt).toISOString()}`);
    return this.tenantAccessToken;
  }

  // ── 认证验证 ──

  /**
   * 验证 App ID / App Secret 有效性并获取 bot 信息。
   * 由 IMGatewayManager 在 startChannel 之前调用。
   */
  async verifyApp(
    appId: string,
    appSecret: string,
  ): Promise<{ ok: boolean; accountId?: string; botOpenId?: string; error?: string }> {
    try {
      // 第一步：获取 tenant_access_token
      const resp1 = await this.apiPost(
        `${FEISHU_API_BASE}/auth/v3/tenant_access_token/internal`,
        JSON.stringify({ app_id: appId, app_secret: appSecret }),
      );
      const tokenData: FeishuTenantTokenResp = JSON.parse(resp1);
      if (tokenData.code !== 0 || !tokenData.tenant_access_token) {
        return { ok: false, error: tokenData.msg || '获取 tenant_access_token 失败' };
      }

      const token = tokenData.tenant_access_token;

      // 第二步：获取 bot 信息以验证凭证有效
      const resp2 = await this.apiGet(
        `${FEISHU_API_BASE}/bot/v3/info`,
        { Authorization: `Bearer ${token}` },
      );
      const botData: FeishuBotInfoResp = JSON.parse(resp2);
      if (botData.code !== 0) {
        return { ok: false, error: botData.msg || '获取应用信息失败，请检查应用是否已发布' };
      }

      const botOpenId = botData.bot?.open_id || '';
      const accountId = botOpenId;

      return { ok: true, accountId, botOpenId };
    } catch (e: any) {
      return { ok: false, error: e?.message || String(e) };
    }
  }

  // ── 消息接收 (WebSocket) ──

  startPolling(): void {
    if (!this.connected || !this.appId || !this.appSecret) {
      console.error(`[FeishuChannel:${this.instanceId}] 无法启动轮询: 未连接`);
      return;
    }
    if (this.pollPromise) {
      console.warn(`[FeishuChannel:${this.instanceId}] WebSocket 已在运行`);
      return;
    }

    this.abortController = new AbortController();
    this.pollPromise = this.wsLoop(this.abortController.signal);
  }

  private async wsLoop(abortSignal: AbortSignal): Promise<void> {
    console.log(`[FeishuChannel:${this.instanceId}] 启动飞书 WebSocket 循环`);

    while (!abortSignal.aborted) {
      try {
        // 先获取/刷新 token
        const token = await this.getTenantAccessToken();

        // 获取 WebSocket 连接地址
        const wsResp = await this.apiPost(
          `${FEISHU_API_BASE}/ws`,
          '',
          { Authorization: `Bearer ${token}` },
        );
        const wsData: FeishuWsResp = JSON.parse(wsResp);
        if (wsData.code !== 0 || !wsData.url) {
          throw new Error(`获取 WebSocket URL 失败: ${wsData.msg || '未知'} (code: ${wsData.code})`);
        }

        console.log(`[FeishuChannel:${this.instanceId}] 获取到 WebSocket URL: ${wsData.url.slice(0, 60)}...`);

        // 连接 WebSocket 并等待连接关闭
        await this.connectWebSocket(wsData.url, abortSignal);
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          break;
        }
        console.warn(`[FeishuChannel:${this.instanceId}] WebSocket 异常 (${WS_RECONNECT_DELAY_MS / 1000}s 后重连): ${err instanceof Error ? err.message : String(err)}`);
        await new Promise((r) => setTimeout(r, WS_RECONNECT_DELAY_MS));
      }
    }

    console.log(`[FeishuChannel:${this.instanceId}] WebSocket 循环已退出`);
  }

  private connectWebSocket(url: string, abortSignal: AbortSignal): Promise<void> {
    return new Promise<void>((resolve) => {
      // 清理旧连接
      if (this.ws) {
        try { this.ws.close(); } catch { /* ignore */ }
        this.ws = null;
      }

      const ws = new WebSocket(url);
      this.ws = ws;

      const onAbort = (): void => {
        ws.close();
      };
      abortSignal.addEventListener('abort', onAbort, { once: true });

      ws.onopen = (): void => {
        console.log(`[FeishuChannel:${this.instanceId}] WebSocket 已连接`);
        // 发送事件订阅
        const subMsg = JSON.stringify({
          type: 'subscription',
          data: {
            subscription_type: 'event_v2',
            event_type: 'im.message.receive_v1',
          },
        });
        ws.send(subMsg);
        console.log(`[FeishuChannel:${this.instanceId}] 已发送事件订阅: im.message.receive_v1`);
      };

      ws.onmessage = (event: MessageEvent): void => {
        try {
          const data: FeishuWsEvent = JSON.parse(event.data as string);
          this.handleWsMessage(data);
        } catch (e) {
          console.warn(`[FeishuChannel:${this.instanceId}] WebSocket 消息解析失败:`, e);
        }
      };

      ws.onerror = (): void => {
        console.error(`[FeishuChannel:${this.instanceId}] WebSocket 连接错误`);
      };

      ws.onclose = (event: CloseEvent): void => {
        console.log(`[FeishuChannel:${this.instanceId}] WebSocket 已关闭: code=${event.code} reason=${event.reason}`);
        abortSignal.removeEventListener('abort', onAbort);

        if (this.heartbeatTimer) {
          clearInterval(this.heartbeatTimer);
          this.heartbeatTimer = null;
        }

        resolve();
      };
    });
  }

  private handleWsMessage(data: FeishuWsEvent): void {
    switch (data.type) {
      case 'session': {
        // 服务器下发的会话信息，包含心跳间隔
        const heartbeatInterval: number =
          data.data?.heartbeat_interval ? data.data.heartbeat_interval * 1000 : WS_PING_INTERVAL_MS;

        if (this.heartbeatTimer) clearInterval(this.heartbeatTimer);
        if (!this.ws) break;

        const ws = this.ws;
        const sig = this.abortController?.signal;
        this.heartbeatTimer = setInterval(() => {
          if (sig?.aborted || ws.readyState !== WebSocket.OPEN) return;
          try { ws.send(JSON.stringify({ type: 'ping' })); } catch { /* ignore */ }
        }, heartbeatInterval);

        console.log(`[FeishuChannel:${this.instanceId}] WebSocket 会话建立, heartbeat_interval=${heartbeatInterval}ms`);
        break;
      }
      case 'event':
        this.handleEvent(data);
        break;
      case 'pong':
        // 心跳回包，无需处理
        break;
      default:
        // 未知消息类型，静默忽略
        break;
    }
  }

  private handleEvent(data: FeishuWsEvent): void {
    const payload = data.event;
    if (!payload || !payload.event_type) return;

    if (payload.event_type === 'im.message.receive_v1' && payload.event) {
      const msgEvent = payload.event;

      // 过滤 Bot 自身发出的消息，避免死循环
      if (msgEvent.sender?.sender_type === 'bot') return;

      const msg = this.convertMessage(msgEvent);
      if (msg) {
        this.emit('message', msg);
      }
    }
  }

  private convertMessage(
    event: NonNullable<FeishuWsMessagePayload['event']>,
  ): ChannelMessage | null {
    const message = event.message;
    const sender = event.sender;
    if (!message || !sender) return null;

    // 飞书消息内容为 JSON 编码字符串，需解析后提取 text
    const rawContent = message.content;
    let content = '[非文本消息]';
    try {
      const parsed = JSON.parse(rawContent);
      if (parsed.text) {
        content = parsed.text;
      }
    } catch {
      // 如果不是 JSON 则直接使用原始内容
      content = rawContent || '[非文本消息]';
    }

    const chatType: 'direct' | 'group' =
      message.chat_type === 'group' ? 'group' : 'direct';

    const senderId = sender.sender_id?.open_id || '';
    const conversationId = chatType === 'direct' ? senderId : message.chat_id;

    return {
      messageId: message.message_id,
      conversationId,
      senderId,
      content,
      chatType,
      timestamp: parseInt(message.create_time, 10) * 1000, // 飞书使用毫秒级时间戳字符串
    };
  }

  // ── 消息发送 ──

  async sendMessage(conversationId: string, text: string): Promise<boolean> {
    if (!this.connected) {
      console.error(`[FeishuChannel:${this.instanceId}] 无法发送: 未连接`);
      return false;
    }

    try {
      const token = await this.getTenantAccessToken();
      const url = `${FEISHU_API_BASE}/im/v1/messages?receive_id_type=chat_id`;
      const body = JSON.stringify({
        receive_id: conversationId,
        msg_type: 'text',
        content: JSON.stringify({ text }),
      });

      const rawText = await this.apiPost(url, body, { Authorization: `Bearer ${token}` });
      const resp: FeishuSendMessageResp = JSON.parse(rawText);

      if (resp.code !== 0) {
        console.error(`[FeishuChannel:${this.instanceId}] sendMessage 失败: ${resp.msg || '未知'} (code: ${resp.code})`);
        this.emit('error', new Error(`sendMessage 失败: ${resp.msg || '未知'}`));
        return false;
      }

      console.log(`[FeishuChannel:${this.instanceId}] 消息已发送至 ${conversationId}`);
      return true;
    } catch (e: any) {
      console.error(`[FeishuChannel:${this.instanceId}] sendMessage 异常:`, e?.message);
      this.emit('error', new Error(`sendMessage 异常: ${e?.message || String(e)}`));
      return false;
    }
  }

  // ── 连接管理 ──

  async disconnect(): Promise<void> {
    try {
      if (this.abortController) {
        this.abortController.abort();
        this.abortController = null;
      }
      if (this.heartbeatTimer) {
        clearInterval(this.heartbeatTimer);
        this.heartbeatTimer = null;
      }
      if (this.ws) {
        try { this.ws.close(); } catch { /* ignore */ }
        this.ws = null;
      }
      if (this.pollPromise) {
        try { await this.pollPromise; } catch { /* ignore */ }
        this.pollPromise = null;
      }
      this.connected = false;
      this.accountId = null;
      this.tenantAccessToken = null;
      this.tokenExpireAt = 0;
      this.appId = null;
      this.appSecret = null;
      this.botOpenId = null;

      this.emit('disconnected');
      console.log(`[FeishuChannel:${this.instanceId}] 已断开连接`);
    } catch (e: any) {
      console.error(`[FeishuChannel:${this.instanceId}] disconnect 异常:`, e?.message);
    }
  }

  // ── HTTP 辅助方法 ──

  private async apiGet(
    url: string,
    extraHeaders?: Record<string, string>,
    timeoutMs?: number,
    abortSignal?: AbortSignal,
  ): Promise<string> {
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
      const headers: Record<string, string> = {
        ...(extraHeaders || {}),
      };
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
    extraHeaders?: Record<string, string>,
    abortSignal?: AbortSignal,
    timeoutMs?: number,
  ): Promise<string> {
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
      const headers: Record<string, string> = {
        'Content-Type': 'application/json',
        ...(extraHeaders || {}),
      };
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

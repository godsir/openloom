// frontend/src/main/im/discordChannel.ts
import { EventEmitter } from 'events';
import type { IChannel, ChannelMessage, ConnectedInfo, ChannelOptions } from './IChannel';

// ── Discord API 类型 ──

interface DiscordUser {
  id: string;
  username: string;
  discriminator: string;
  avatar?: string;
  bot?: boolean;
}

interface DiscordGetMeResp {
  id: string;
  username: string;
  discriminator: string;
  avatar?: string;
  bot?: boolean;
}

interface DiscordGatewayHello {
  op: 10;
  d: {
    heartbeat_interval: number;
  };
  s: null;
  t: null;
}

interface DiscordGatewayPayload {
  op: number;
  d: unknown;
  s: number | null;
  t: string | null;
}

interface DiscordMessageData {
  id: string;
  channel_id: string;
  guild_id?: string;
  author: DiscordUser;
  content: string;
  timestamp: string;
  attachments: unknown[];
  embeds?: unknown[];
}

interface DiscordReadyData {
  v: number;
  user: DiscordUser;
  guilds: unknown[];
  session_id: string;
  resume_gateway_url: string;
  shard?: [number, number];
  application: { id: string; flags: number };
}

// ── 常量 ──

const DISCORD_API_BASE = 'https://discord.com/api/v10';
const DISCORD_GATEWAY_URL = 'wss://gateway.discord.gg/?v=10&encoding=json';
const DEFAULT_API_TIMEOUT_MS = 15_000;

// 33281 = GUILDS(1) | GUILD_MESSAGES(512) | MESSAGE_CONTENT(32768)
const GATEWAY_INTENTS = 33281;

// ── DiscordChannel ──

/**
 * DiscordChannel — 基于 Discord Bot API + Gateway WebSocket 实现 IChannel。
 *
 * - WebSocket 长连接接收消息（MESSAGE_CREATE 事件）
 * - REST API 发送消息与验证 token
 * - 支持 Gateway 心跳、重连、INVALID_SESSION
 * - 支持 app 重启时凭据恢复
 */
export class DiscordChannel extends EventEmitter implements IChannel {
  private instanceId: string;
  private instanceName: string;
  private connected: boolean = false;
  private accountId: string | null = null;
  private token: string | null = null;
  private abortController: AbortController | null = null;
  private pollPromise: Promise<void> | null = null;
  private ws: WebSocket | null = null;
  private lastSequence: number | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private botUserId: string | null = null;

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
    const token = credentials.token as string | undefined;
    if (!token) {
      console.error(`[DiscordChannel:${this.instanceId}] restoreConnection: missing token`);
      return;
    }
    this.token = token;
    if (credentials.accountId) {
      this.accountId = credentials.accountId as string;
    }
    if (credentials.botUserId) {
      this.botUserId = credentials.botUserId as string;
    }
    this.connected = true;
    console.log(`[DiscordChannel:${this.instanceId}] Connection restored`);
  }

  // ── 认证验证 ──

  /**
   * 验证 Bot Token 有效性并获取 bot 信息。
   * 由 IMGatewayManager 在 startPolling 之前调用。
   */
  async verifyToken(token: string): Promise<{
    ok: boolean;
    username?: string;
    accountId?: string;
    botUserId?: string;
    error?: string;
  }> {
    try {
      const resp = await this.apiGet(
        `${DISCORD_API_BASE}/users/@me`,
        DEFAULT_API_TIMEOUT_MS,
        undefined,
        { Authorization: `Bot ${token}` },
      );
      const data: DiscordGetMeResp = JSON.parse(resp);
      const username = `${data.username}#${data.discriminator}`;
      return {
        ok: true,
        username,
        accountId: data.id,
        botUserId: data.id,
      };
    } catch (e: any) {
      return { ok: false, error: e?.message || String(e) };
    }
  }

  // ── 消息接收（Gateway WebSocket） ──

  startPolling(): void {
    if (!this.connected || !this.token) {
      console.error(`[DiscordChannel:${this.instanceId}] Cannot start: not connected`);
      return;
    }
    if (this.pollPromise) {
      console.warn(`[DiscordChannel:${this.instanceId}] Already active`);
      return;
    }

    this.abortController = new AbortController();
    this.pollPromise = this.wsLoop(this.abortController.signal);
  }

  private async wsLoop(abortSignal: AbortSignal): Promise<void> {
    const token = this.token!;
    console.log(`[DiscordChannel:${this.instanceId}] Starting Discord Gateway WebSocket loop`);

    while (!abortSignal.aborted) {
      try {
        await this.connectAndHandle(token, abortSignal);
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          break; // 正常关闭
        }
        console.warn(
          `[DiscordChannel:${this.instanceId}] WS error (will reconnect in 5s): ${
            err instanceof Error ? err.message : String(err)
          }`,
        );
        await new Promise((r) => setTimeout(r, 5000));
      }
    }

    console.log(`[DiscordChannel:${this.instanceId}] WS loop exited`);
  }

  /**
   * 建立 Gateway WebSocket 连接，处理整个生命周期。
   * - op:10 HELLO → 启动心跳 + 发送 IDENTIFY
   * - op:0 DISPATCH → READY 触发 connected / MESSAGE_CREATE 转换为 ChannelMessage
   * - op:7 RECONNECT → 关闭当前连接让循环重连
   * - op:9 INVALID_SESSION → 关闭连接让循环重新 IDENTIFY
   */
  private connectAndHandle(token: string, abortSignal: AbortSignal): Promise<void> {
    return new Promise((resolve, reject) => {
      let settled = false;

      const finish = (err?: Error): void => {
        if (settled) return;
        settled = true;
        this.clearHeartbeat();
        try {
          this.ws?.close();
        } catch {
          /* ignore */
        }
        this.ws = null;
        if (err) reject(err);
        else resolve();
      };

      if (abortSignal.aborted) {
        finish(new DOMException('Aborted', 'AbortError'));
        return;
      }

      const onAbort = (): void => finish(new DOMException('Aborted', 'AbortError'));
      abortSignal.addEventListener('abort', onAbort, { once: true });

      let ws: WebSocket;
      try {
        ws = new WebSocket(DISCORD_GATEWAY_URL);
        this.ws = ws;
      } catch (e: any) {
        abortSignal.removeEventListener('abort', onAbort);
        finish(new Error(`WebSocket creation failed: ${e?.message || e}`));
        return;
      }

      ws.addEventListener('open', () => {
        console.log(`[DiscordChannel:${this.instanceId}] WS connected, waiting for HELLO`);
      });

      ws.addEventListener('message', (event) => {
        let payload: DiscordGatewayPayload;
        try {
          payload = JSON.parse(event.data as string);
        } catch {
          return;
        }

        // 跟踪最新 sequence
        if (payload.s != null) {
          this.lastSequence = payload.s;
        }

        switch (payload.op) {
          case 10: {
            // HELLO — 启动心跳并发送 IDENTIFY
            const hello = payload as unknown as DiscordGatewayHello;
            this.startHeartbeat(hello.d.heartbeat_interval);
            this.sendGateway(ws, {
              op: 2,
              d: {
                token,
                intents: GATEWAY_INTENTS,
                properties: {
                  os: 'windows',
                  browser: 'openloom',
                  device: 'openloom',
                },
              },
            });
            break;
          }

          case 0: {
            // DISPATCH
            if (payload.t === 'READY') {
              const ready = payload.d as DiscordReadyData;
              this.botUserId = ready.user.id;
              this.accountId = ready.user.id;
              console.log(
                `[DiscordChannel:${this.instanceId}] READY — bot: ${ready.user.username}#${ready.user.discriminator}`,
              );
              this.emit('connected', {
                accountId: ready.user.id,
              } as ConnectedInfo);
            } else if (payload.t === 'MESSAGE_CREATE') {
              const msg = this.convertDiscordMessage(payload.d as DiscordMessageData);
              if (msg) {
                this.emit('message', msg);
              }
            }
            break;
          }

          case 7: {
            // RECONNECT — 服务器要求重连
            console.log(`[DiscordChannel:${this.instanceId}] Server requested RECONNECT`);
            finish();
            break;
          }

          case 9: {
            // INVALID_SESSION — 需要重新 IDENTIFY
            const canResume = payload.d as boolean;
            console.log(
              `[DiscordChannel:${this.instanceId}] Invalid session (canResume=${canResume}), reconnecting`,
            );
            // 关闭当前连接，由 wsLoop 重新连接并发送 IDENTIFY
            finish(
              new Error(`Invalid session (canResume=${canResume})`),
            );
            break;
          }

          case 11: {
            // HEARTBEAT_ACK — 无需处理
            break;
          }

          default:
            console.log(
              `[DiscordChannel:${this.instanceId}] Unhandled op: ${payload.op} t: ${payload.t}`,
            );
        }
      });

      ws.addEventListener('close', (event) => {
        console.log(
          `[DiscordChannel:${this.instanceId}] WS closed: code=${event.code} reason=${event.reason}`,
        );
        abortSignal.removeEventListener('abort', onAbort);
        if (!settled) {
          finish(new Error(`WebSocket closed unexpectedly: code=${event.code}`));
        }
      });

      ws.addEventListener('error', (e) => {
        console.error(`[DiscordChannel:${this.instanceId}] WS error:`, e);
        // close 事件会在 error 之后触发，清理逻辑在 close handler 中处理
      });
    });
  }

  // ── 消息映射 ──

  private convertDiscordMessage(d: DiscordMessageData): ChannelMessage | null {
    // 过滤 bot 自身消息，避免死循环
    if (this.botUserId && d.author.id === this.botUserId) {
      return null;
    }

    const content = d.content || '';
    const chatType: 'direct' | 'group' = d.guild_id ? 'group' : 'direct';

    // 始终使用 channel_id 作为 conversationId（DM 和 guild 都适用）
    const conversationId = d.channel_id;

    return {
      messageId: d.id,
      conversationId,
      senderId: d.author.id,
      senderName: d.author.username,
      content: content || '[非文本消息]',
      chatType,
      timestamp: new Date(d.timestamp).getTime(),
    };
  }

  // ── 消息发送 ──

  async sendMessage(conversationId: string, text: string): Promise<boolean> {
    if (!this.connected || !this.token) {
      console.error(`[DiscordChannel:${this.instanceId}] Cannot send: not connected`);
      return false;
    }

    try {
      const url = `${DISCORD_API_BASE}/channels/${conversationId}/messages`;
      const body = JSON.stringify({ content: text });

      await this.apiPost(url, body, DEFAULT_API_TIMEOUT_MS, {
        Authorization: `Bot ${this.token}`,
      });

      console.log(`[DiscordChannel:${this.instanceId}] Message sent to ${conversationId}`);
      return true;
    } catch (e: any) {
      console.error(`[DiscordChannel:${this.instanceId}] sendMessage failed:`, e?.message);
      this.emit('error', new Error(`sendMessage failed: ${e?.message || String(e)}`));
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
      this.clearHeartbeat();
      if (this.ws) {
        this.ws.close();
        this.ws = null;
      }
      if (this.pollPromise) {
        try {
          await this.pollPromise;
        } catch {
          /* ignore */
        }
        this.pollPromise = null;
      }
      this.connected = false;
      this.accountId = null;
      this.token = null;
      this.botUserId = null;
      this.lastSequence = null;

      this.emit('disconnected');
      console.log(`[DiscordChannel:${this.instanceId}] Disconnected`);
    } catch (e: any) {
      console.error(`[DiscordChannel:${this.instanceId}] disconnect error:`, e?.message);
    }
  }

  // ── Heartbeat ──

  private startHeartbeat(intervalMs: number): void {
    this.clearHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        this.sendGateway(this.ws, {
          op: 1,
          d: this.lastSequence,
        });
      }
    }, intervalMs);
  }

  private clearHeartbeat(): void {
    if (this.heartbeatTimer != null) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  // ── Gateway helpers ──

  private sendGateway(ws: WebSocket, payload: Record<string, unknown>): void {
    try {
      ws.send(JSON.stringify(payload));
    } catch (e: any) {
      console.error(`[DiscordChannel:${this.instanceId}] sendGateway failed:`, e?.message);
    }
  }

  // ── HTTP helpers ──

  private async apiGet(
    url: string,
    timeoutMs?: number,
    abortSignal?: AbortSignal,
    headers?: Record<string, string>,
  ): Promise<string> {
    const controller = timeoutMs ? new AbortController() : undefined;
    const timer =
      controller && timeoutMs ? setTimeout(() => controller.abort(), timeoutMs) : undefined;

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
        method: 'GET',
        headers: { 'Content-Type': 'application/json', ...(headers || {}) },
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
    timeoutMs?: number,
    headers?: Record<string, string>,
  ): Promise<string> {
    const controller = timeoutMs ? new AbortController() : undefined;
    const timer =
      controller && timeoutMs ? setTimeout(() => controller.abort(), timeoutMs) : undefined;

    const signal: AbortSignal | undefined = controller?.signal;

    try {
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...(headers || {}) },
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
    }
  }
}

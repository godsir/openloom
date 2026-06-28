// frontend/src/main/im/popoChannel.ts
import { EventEmitter } from 'events';
import { createHash } from 'crypto';
import type { IChannel, ChannelMessage, ConnectedInfo, ChannelOptions } from './IChannel';

// ── POPO API 类型 ──

interface PopoWsMessage {
  type?: string;
  data?: {
    msgId?: string;
    chatId?: string;
    senderId?: string;
    senderName?: string;
    groupName?: string;
    content?: string;
    chatType?: string;
    timestamp?: number;
  };
}

interface PopoSendResponse {
  code?: number;
  message?: string;
}

// ── 常量 ──

const POPO_BASE_URL = 'https://open.popo.netease.com';
const POPO_WS_URL = 'wss://open.popo.netease.com/open-apis/ws';
const DEFAULT_API_TIMEOUT_MS = 15_000;
const RECONNECT_DELAY_MS = 5_000;

// ── PopoChannel ──

/**
 * PopoChannel — 基于 POPO Open API WebSocket 实现 IChannel。
 *
 * - 凭据: appKey / appSecret / aesKey
 * - WebSocket 长连接收发消息
 * - 签名认证: SHA1(appSecret + nonce + curTime)
 */
export class PopoChannel extends EventEmitter implements IChannel {
  private instanceId: string;
  private instanceName: string;
  private connected: boolean = false;
  private accountId: string | null = null;
  private appKey: string | null = null;
  private appSecret: string | null = null;
  private aesKey: string | null = null;
  private abortController: AbortController | null = null;
  private wsPromise: Promise<void> | null = null;
  private ws: WebSocket | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;

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
    const appKey = credentials.appKey as string | undefined;
    const appSecret = credentials.appSecret as string | undefined;
    const aesKey = credentials.aesKey as string | undefined;

    if (!appKey || !appSecret || !aesKey) {
      console.error(
        `[PopoChannel:${this.instanceId}] restoreConnection: missing credentials`,
      );
      return;
    }

    this.appKey = appKey;
    this.appSecret = appSecret;
    this.aesKey = aesKey;
    if (credentials.accountId) {
      this.accountId = credentials.accountId as string;
    }
    this.connected = true;
    console.log(
      `[PopoChannel:${this.instanceId}] Connection restored, accountId=${this.accountId}`,
    );
  }

  // ── 凭据验证 ──

  verifyCredentials(appKey: string, appSecret: string, aesKey: string): { ok: boolean; error?: string } {
    if (!appKey) {
      return { ok: false, error: '缺少 appKey' };
    }
    if (!appSecret) {
      return { ok: false, error: '缺少 appSecret' };
    }
    if (!aesKey) {
      return { ok: false, error: '缺少 aesKey' };
    }
    return { ok: true };
  }

  // ── 消息接收（WebSocket） ──

  startPolling(): void {
    if (!this.connected || !this.appKey || !this.appSecret) {
      console.error(
        `[PopoChannel:${this.instanceId}] Cannot start WS: not connected`,
      );
      return;
    }

    if (this.wsPromise) {
      console.warn(`[PopoChannel:${this.instanceId}] WS already active`);
      return;
    }

    this.abortController = new AbortController();
    this.wsPromise = this.wsLoop(this.abortController.signal);
  }

  private buildAuthQuery(): string {
    const nonce = Math.random().toString(36).substring(2);
    const curTime = String(Math.floor(Date.now() / 1000));
    const checkSum = createHash('sha1')
      .update(this.appSecret! + nonce + curTime, 'utf8')
      .digest('hex');

    const params = new URLSearchParams();
    params.set('appKey', this.appKey!);
    params.set('nonce', nonce);
    params.set('curTime', curTime);
    params.set('checkSum', checkSum);
    return params.toString();
  }

  private async wsLoop(abortSignal: AbortSignal): Promise<void> {
    console.log(
      `[PopoChannel:${this.instanceId}] Starting POPO WebSocket loop`,
    );

    while (!abortSignal.aborted) {
      try {
        await this.connectWs(abortSignal);
        // 连接成功后保持，直到断开
        if (!abortSignal.aborted) {
          console.log(
            `[PopoChannel:${this.instanceId}] WS disconnected, reconnecting in ${RECONNECT_DELAY_MS}ms...`,
          );
          await this.delay(RECONNECT_DELAY_MS, abortSignal);
        }
      } catch (err) {
        if (abortSignal.aborted) break;
        console.warn(
          `[PopoChannel:${this.instanceId}] WS error, will reconnect: ${err instanceof Error ? err.message : String(err)}`,
        );
        await this.delay(RECONNECT_DELAY_MS, abortSignal);
      }
    }

    console.log(`[PopoChannel:${this.instanceId}] WS loop exited`);
  }

  private connectWs(abortSignal: AbortSignal): Promise<void> {
    return new Promise<void>((resolve) => {
      const query = this.buildAuthQuery();
      const url = `${POPO_WS_URL}?${query}`;

      let ws: WebSocket;
      try {
        ws = new WebSocket(url);
      } catch (e) {
        resolve();
        return;
      }
      this.ws = ws;

      const cleanup = (): void => {
        if (this.heartbeatTimer) {
          clearInterval(this.heartbeatTimer);
          this.heartbeatTimer = null;
        }
        this.ws = null;
      };

      ws.addEventListener('open', () => {
        console.log(
          `[PopoChannel:${this.instanceId}] WS opened`,
        );

        // 启动心跳
        this.heartbeatTimer = setInterval(() => {
          if (ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ type: 'ping' }));
          }
        }, 30_000);

        this.connected = true;
        this.emit('connected', {
          accountId: this.accountId || this.appKey || '',
        });
      });

      ws.addEventListener('message', (event) => {
        try {
          const msg: PopoWsMessage = JSON.parse(event.data as string);
          if (msg.type === 'pong') return;

          const channelMsg = this.convertMessage(msg);
          if (channelMsg) {
            this.emit('message', channelMsg);
          }
        } catch {
          // 忽略解析失败的消息
        }
      });

      ws.addEventListener('close', () => {
        console.log(`[PopoChannel:${this.instanceId}] WS closed`);
        cleanup();
        resolve();
      });

      ws.addEventListener('error', (err) => {
        console.error(`[PopoChannel:${this.instanceId}] WS error`);
        cleanup();
        resolve();
      });

      // 外部 abort 时主动关闭
      const onAbort = (): void => {
        ws.close();
      };
      abortSignal.addEventListener('abort', onAbort, { once: true });
    });
  }

  private convertMessage(msg: PopoWsMessage): ChannelMessage | null {
    const d = msg.data;
    if (!d) return null;

    const content = d.content || '';
    if (!content) return null;

    const senderId = d.senderId || '';
    if (!senderId) return null;

    const chatType: 'direct' | 'group' =
      d.chatType === 'group' ? 'group' : 'direct';

    return {
      messageId: d.msgId || `${senderId}-${d.timestamp || Date.now()}`,
      conversationId: d.chatId || (chatType === 'direct' ? senderId : d.chatId || senderId),
      senderId,
      senderName: d.senderName,
      groupName: d.groupName,
      content,
      chatType,
      timestamp: d.timestamp || Date.now(),
    };
  }

  // ── 消息发送 ──

  async sendMessage(conversationId: string, text: string): Promise<boolean> {
    if (!this.appKey || !this.appSecret) {
      console.error(
        `[PopoChannel:${this.instanceId}] Cannot send: not connected`,
      );
      return false;
    }

    // 优先通过 WebSocket 发送
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      try {
        this.ws.send(
          JSON.stringify({
            type: 'message',
            data: {
              chatId: conversationId,
              content: text,
              msgType: 'text',
            },
          }),
        );
        console.log(
          `[PopoChannel:${this.instanceId}] Message sent via WS to ${conversationId}`,
        );
        return true;
      } catch (e: any) {
        console.error(
          `[PopoChannel:${this.instanceId}] WS send failed:`,
          e?.message,
        );
        // 回退到 HTTP POST
      }
    }

    // HTTP POST 后备
    try {
      const nonce = Math.random().toString(36).substring(2);
      const curTime = String(Math.floor(Date.now() / 1000));
      const checkSum = createHash('sha1')
        .update(this.appSecret! + nonce + curTime, 'utf8')
        .digest('hex');

      const body = JSON.stringify({
        receiver_id: conversationId,
        msg_type: 'text',
        content: JSON.stringify({ text }),
      });

      const res = await fetch(`${POPO_BASE_URL}/open-apis/im/v1/messages`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          AppKey: this.appKey!,
          Nonce: nonce,
          CurTime: curTime,
          CheckSum: checkSum,
        },
        body,
        signal: AbortSignal.timeout(DEFAULT_API_TIMEOUT_MS),
      });

      const rawText = await res.text();
      if (!res.ok) {
        console.error(
          `[PopoChannel:${this.instanceId}] HTTP send failed: HTTP ${res.status}`,
        );
        return false;
      }

      const resp: PopoSendResponse = JSON.parse(rawText);
      if (resp.code !== 0) {
        console.error(
          `[PopoChannel:${this.instanceId}] sendMessage failed: ${resp.message || 'unknown'}`,
        );
        return false;
      }

      console.log(
        `[PopoChannel:${this.instanceId}] Message sent via HTTP to ${conversationId}`,
      );
      return true;
    } catch (e: any) {
      console.error(
        `[PopoChannel:${this.instanceId}] sendMessage failed:`,
        e?.message,
      );
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
        this.ws.close();
        this.ws = null;
      }

      if (this.wsPromise) {
        try {
          await this.wsPromise;
        } catch {
          /* ignore shutdown errors */
        }
        this.wsPromise = null;
      }

      this.connected = false;
      this.accountId = null;
      this.appKey = null;
      this.appSecret = null;
      this.aesKey = null;

      this.emit('disconnected');
      console.log(`[PopoChannel:${this.instanceId}] Disconnected`);
    } catch (e: any) {
      console.error(
        `[PopoChannel:${this.instanceId}] disconnect error:`,
        e?.message,
      );
    }
  }

  // ── 工具方法 ──

  private delay(ms: number, abortSignal: AbortSignal): Promise<void> {
    return new Promise<void>((resolve) => {
      const timer = setTimeout(resolve, ms);
      const onAbort = (): void => {
        clearTimeout(timer);
        resolve();
      };
      abortSignal.addEventListener('abort', onAbort, { once: true });
    });
  }
}

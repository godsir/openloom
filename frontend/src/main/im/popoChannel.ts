// frontend/src/main/im/popoChannel.ts
import { EventEmitter } from 'events';
import { createHash } from 'crypto';
import type { IChannel, ChannelMessage, ConnectedInfo, ChannelOptions } from './IChannel';

// ── POPO API 类型 ──

interface PopoMessageItem {
  msgId?: string;
  message_id?: string;
  chat_id?: string;
  conversation_id?: string;
  sender_id?: string;
  from_user_id?: string;
  content?: string;
  text?: string;
  chat_type?: string;
  timestamp?: number;
}

interface PopoPollResponse {
  code?: number;
  data?: {
    messages?: PopoMessageItem[];
    has_more?: boolean;
  };
}

interface PopoSendResponse {
  code?: number;
  message?: string;
}

// ── 常量 ──

const POPO_BASE_URL = 'https://open.popo.netease.com';
const POLL_INTERVAL_MS = 10_000;
const DEFAULT_API_TIMEOUT_MS = 15_000;

// ── PopoChannel ──

/**
 * PopoChannel — 基于 POPO Open API (HTTP 轮询) 实现 IChannel。
 *
 * - QR 码登录由 IMGatewayManager.popoQrStart/popoQrPoll 处理
 * - 凭据恢复使用 appKey / appSecret / aesKey
 * - HTTP 轮询接收消息（每 10s）
 * - REST POST 发送消息
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
  private pollPromise: Promise<void> | null = null;

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

  /**
   * 验证凭据完整性 — 检查 appKey、appSecret、aesKey 是否全部存在。
   * 由 IMGatewayManager 在恢复连接时调用。
   */
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

  // ── 消息接收（HTTP 轮询） ──

  startPolling(): void {
    if (!this.connected || !this.appKey || !this.appSecret) {
      console.error(
        `[PopoChannel:${this.instanceId}] Cannot start polling: not connected`,
      );
      return;
    }

    if (this.pollPromise) {
      console.warn(`[PopoChannel:${this.instanceId}] Polling already active`);
      return;
    }

    this.abortController = new AbortController();
    this.pollPromise = this.pollLoop(this.abortController.signal);

    // Emit connected now that polling is active
    this.emit('connected', { accountId: this.accountId || '' });
  }

  private async pollLoop(abortSignal: AbortSignal): Promise<void> {
    console.log(
      `[PopoChannel:${this.instanceId}] Starting POPO polling loop (interval=${POLL_INTERVAL_MS}ms)`,
    );

    while (!abortSignal.aborted) {
      try {
        const rawText = await this.apiGet(
          `${POPO_BASE_URL}/open-apis/im/v1/messages`,
          abortSignal,
        );

        const resp: PopoPollResponse = JSON.parse(rawText);

        if (resp.code === 0 && resp.data?.messages) {
          for (const msg of resp.data.messages) {
            const channelMsg = this.convertMessage(msg);
            if (channelMsg) {
              this.emit('message', channelMsg);
            }
          }
        }
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          break;
        }
        console.warn(
          `[PopoChannel:${this.instanceId}] Poll error (will retry): ${err instanceof Error ? err.message : String(err)}`,
        );
      }

      // 等待轮询间隔
      await new Promise<void>((resolve) => {
        const timer = setTimeout(resolve, POLL_INTERVAL_MS);
        const onAbort = (): void => {
          clearTimeout(timer);
          resolve();
        };
        abortSignal.addEventListener('abort', onAbort, { once: true });
      });
    }

    console.log(`[PopoChannel:${this.instanceId}] Poll loop exited`);
  }

  private convertMessage(msg: PopoMessageItem): ChannelMessage | null {
    const content = msg.content || msg.text || '';
    if (!content) return null;

    const senderId = msg.sender_id || msg.from_user_id || '';
    if (!senderId) return null;

    const chatType: 'direct' | 'group' =
      msg.chat_type === 'group' ? 'group' : 'direct';

    const conversationId = msg.chat_id || msg.conversation_id || senderId;
    const messageId =
      msg.msgId || msg.message_id || `${senderId}-${msg.timestamp || Date.now()}`;

    return {
      messageId,
      conversationId,
      senderId,
      content,
      chatType,
      timestamp: msg.timestamp || Date.now(),
    };
  }

  // ── 消息发送 ──

  async sendMessage(conversationId: string, text: string): Promise<boolean> {
    if (!this.connected || !this.appKey || !this.appSecret) {
      console.error(
        `[PopoChannel:${this.instanceId}] Cannot send: not connected`,
      );
      return false;
    }

    try {
      const body = JSON.stringify({
        receiver_id: conversationId,
        msg_type: 'text',
        content: JSON.stringify({ text }),
      });

      const rawText = await this.apiPost(
        `${POPO_BASE_URL}/open-apis/im/v1/messages`,
        body,
      );

      const resp: PopoSendResponse = JSON.parse(rawText);

      if (resp.code !== 0) {
        console.error(
          `[PopoChannel:${this.instanceId}] sendMessage failed: ${resp.message || 'unknown'}`,
        );
        this.emit(
          'error',
          new Error(`sendMessage failed: ${resp.message || 'unknown'}`),
        );
        return false;
      }

      console.log(
        `[PopoChannel:${this.instanceId}] Message sent to ${conversationId}`,
      );
      return true;
    } catch (e: any) {
      console.error(
        `[PopoChannel:${this.instanceId}] sendMessage failed:`,
        e?.message,
      );
      this.emit(
        'error',
        new Error(`sendMessage failed: ${e?.message || String(e)}`),
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

      if (this.pollPromise) {
        try {
          await this.pollPromise;
        } catch {
          /* ignore shutdown errors */
        }
        this.pollPromise = null;
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

  // ── HTTP helpers ──

  /**
   * 构建 POPO Open API 签名认证请求头。
   * 采用 NetEase Open API 通用签名规范：SHA1(appSecret + nonce + curTime)
   */
  private buildAuthHeaders(): Record<string, string> {
    const nonce = Math.random().toString(36).substring(2);
    const curTime = String(Math.floor(Date.now() / 1000));
    const checkSum = createHash('sha1')
      .update(this.appSecret! + nonce + curTime, 'utf8')
      .digest('hex');

    return {
      'Content-Type': 'application/json',
      AppKey: this.appKey!,
      Nonce: nonce,
      CurTime: curTime,
      CheckSum: checkSum,
    };
  }

  private async apiGet(
    url: string,
    abortSignal?: AbortSignal,
    timeoutMs: number = DEFAULT_API_TIMEOUT_MS,
  ): Promise<string> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);

    let cleanup = (): void => {};
    if (abortSignal) {
      if (abortSignal.aborted) {
        controller.abort();
      } else {
        const onAbort = (): void => controller.abort();
        abortSignal.addEventListener('abort', onAbort, { once: true });
        cleanup = (): void =>
          abortSignal.removeEventListener('abort', onAbort);
      }
    }

    try {
      const headers = this.buildAuthHeaders();
      // GET 请求不需要 Content-Type，仅保留认证头
      const getHeaders: Record<string, string> = {};
      for (const key of Object.keys(headers)) {
        if (key !== 'Content-Type') {
          getHeaders[key] = headers[key as keyof typeof headers];
        }
      }

      const res = await fetch(url, {
        method: 'GET',
        headers: getHeaders,
        signal: controller.signal,
      });
      clearTimeout(timer);
      const text = await res.text();
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${text.slice(0, 200)}`);
      }
      return text;
    } catch (err) {
      clearTimeout(timer);
      throw err;
    } finally {
      cleanup();
    }
  }

  private async apiPost(
    url: string,
    body: string,
    timeoutMs: number = DEFAULT_API_TIMEOUT_MS,
    abortSignal?: AbortSignal,
  ): Promise<string> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);

    let cleanup = (): void => {};
    if (abortSignal) {
      if (abortSignal.aborted) {
        controller.abort();
      } else {
        const onAbort = (): void => controller.abort();
        abortSignal.addEventListener('abort', onAbort, { once: true });
        cleanup = (): void =>
          abortSignal.removeEventListener('abort', onAbort);
      }
    }

    try {
      const headers = this.buildAuthHeaders();

      const res = await fetch(url, {
        method: 'POST',
        headers,
        body,
        signal: controller.signal,
      });
      clearTimeout(timer);
      const text = await res.text();
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${text.slice(0, 200)}`);
      }
      return text;
    } catch (err) {
      clearTimeout(timer);
      throw err;
    } finally {
      cleanup();
    }
  }
}

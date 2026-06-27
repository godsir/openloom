// frontend/src/main/im/telegramChannel.ts
import { EventEmitter } from 'events';
import type { IChannel, ChannelMessage, ConnectedInfo, ChannelOptions } from './IChannel';

// ── Telegram Bot API 类型 ──

interface TgUser {
  id: number;
  is_bot: boolean;
  first_name: string;
  username?: string;
}

interface TgChat {
  id: number;
  type: 'private' | 'group' | 'supergroup' | 'channel';
  title?: string;
  username?: string;
}

interface TgMessage {
  message_id: number;
  from?: TgUser;
  chat: TgChat;
  date: number;
  text?: string;
  caption?: string;
}

interface TgUpdate {
  update_id: number;
  message?: TgMessage;
  edited_message?: TgMessage;
}

interface TgGetUpdatesResp {
  ok: boolean;
  result?: TgUpdate[];
  description?: string;
}

interface TgSendMessageResp {
  ok: boolean;
  result?: TgMessage;
  description?: string;
}

interface TgGetMeResp {
  ok: boolean;
  result?: TgUser;
  description?: string;
}

// ── 常量 ──

const TG_BASE_URL = 'https://api.telegram.org';
const DEFAULT_LONG_POLL_TIMEOUT_S = 30;
const DEFAULT_API_TIMEOUT_MS = 15_000;

// ── TelegramChannel ──

/**
 * TelegramChannel — 基于 Telegram Bot API (HTTP long polling) 实现 IChannel。
 *
 * - getUpdates 长轮询接收消息
 * - sendMessage 发送文本
 * - getMe 验证 token
 * - 支持 app 重启时凭据恢复
 */
export class TelegramChannel extends EventEmitter implements IChannel {
  private instanceId: string;
  private instanceName: string;
  private connected: boolean = false;
  private accountId: string | null = null;
  private token: string | null = null;
  private abortController: AbortController | null = null;
  private pollPromise: Promise<void> | null = null;
  private lastUpdateId: number = 0;
  private botUsername: string | null = null;

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
      console.error(`[TelegramChannel:${this.instanceId}] restoreConnection: missing token`);
      return;
    }
    this.token = token;
    if (credentials.accountId) {
      this.accountId = credentials.accountId as string;
    }
    if (credentials.botUsername) {
      this.botUsername = credentials.botUsername as string;
    }
    if (credentials.lastUpdateId) {
      this.lastUpdateId = credentials.lastUpdateId as number;
    }
    this.connected = true;
    console.log(`[TelegramChannel:${this.instanceId}] Connection restored`);
  }

  // ── 认证验证 ──

  /**
   * 验证 Bot Token 有效性并获取 bot 信息。
   * 由 IMGatewayManager.telegramLogin() 在 startPolling 之前调用。
   */
  async verifyToken(token: string): Promise<{ ok: boolean; botUsername?: string; accountId?: string; error?: string }> {
    try {
      const resp = await this.apiGet(`${TG_BASE_URL}/bot${token}/getMe`);
      const data: TgGetMeResp = JSON.parse(resp);
      if (!data.ok || !data.result) {
        return { ok: false, error: data.description || 'Token 验证失败' };
      }
      const username = data.result.username || `bot_${data.result.id}`;
      return { ok: true, botUsername: username, accountId: String(data.result.id) };
    } catch (e: any) {
      return { ok: false, error: e?.message || String(e) };
    }
  }

  // ── 消息接收 ──

  startPolling(): void {
    if (!this.connected || !this.token) {
      console.error(`[TelegramChannel:${this.instanceId}] Cannot start polling: not connected`);
      return;
    }
    if (this.pollPromise) {
      console.warn(`[TelegramChannel:${this.instanceId}] Polling already active`);
      return;
    }

    this.abortController = new AbortController();
    this.pollPromise = this.pollLoop(this.abortController.signal);
  }

  private async pollLoop(abortSignal: AbortSignal): Promise<void> {
    const token = this.token!;
    console.log(`[TelegramChannel:${this.instanceId}] Starting Telegram long-poll loop`);

    while (!abortSignal.aborted) {
      try {
        const url = `${TG_BASE_URL}/bot${token}/getUpdates`;
        const body = JSON.stringify({
          offset: this.lastUpdateId + 1,
          timeout: DEFAULT_LONG_POLL_TIMEOUT_S,
          allowed_updates: ['message', 'edited_message'],
        });

        const rawText = await this.apiPost(url, body, abortSignal);
        const resp: TgGetUpdatesResp = JSON.parse(rawText);

        if (resp.ok && resp.result) {
          for (const update of resp.result) {
            this.lastUpdateId = Math.max(this.lastUpdateId, update.update_id);
            const msg = this.convertUpdate(update);
            if (msg) {
              this.emit('message', msg);
            }
          }
        }
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          break; // 正常关闭
        }
        console.warn(`[TelegramChannel:${this.instanceId}] Poll error (will retry): ${err instanceof Error ? err.message : String(err)}`);
        await new Promise((r) => setTimeout(r, 1000));
      }
    }

    console.log(`[TelegramChannel:${this.instanceId}] Poll loop exited`);
  }

  private convertUpdate(update: TgUpdate): ChannelMessage | null {
    const tgMsg = update.message || update.edited_message;
    if (!tgMsg || !tgMsg.from) return null;

    // 忽略 bot 自身消息，避免死循环
    if (tgMsg.from.is_bot) return null;

    const content = tgMsg.text || tgMsg.caption || '';

    const chatType: 'direct' | 'group' =
      tgMsg.chat.type === 'private' ? 'direct' : 'group';

    const senderId = String(tgMsg.from.id);
    const conversationId = chatType === 'direct' ? senderId : String(tgMsg.chat.id);

    return {
      messageId: String(tgMsg.message_id),
      conversationId,
      senderId,
      senderName: tgMsg.from.username || tgMsg.from.first_name,
      groupName: tgMsg.chat.title,
      content: content || '[非文本消息]',
      chatType,
      timestamp: tgMsg.date * 1000, // Telegram 使用秒级时间戳
    };
  }

  // ── 消息发送 ──

  async sendMessage(conversationId: string, text: string): Promise<boolean> {
    if (!this.connected || !this.token) {
      console.error(`[TelegramChannel:${this.instanceId}] Cannot send: not connected`);
      return false;
    }

    try {
      const url = `${TG_BASE_URL}/bot${this.token}/sendMessage`;
      const body = JSON.stringify({
        chat_id: conversationId,
        text,
      });

      const rawText = await this.apiPost(url, body, undefined, DEFAULT_API_TIMEOUT_MS);
      const resp: TgSendMessageResp = JSON.parse(rawText);

      if (!resp.ok) {
        console.error(`[TelegramChannel:${this.instanceId}] sendMessage failed: ${resp.description || 'unknown'}`);
        this.emit('error', new Error(`sendMessage failed: ${resp.description || 'unknown'}`));
        return false;
      }

      console.log(`[TelegramChannel:${this.instanceId}] Message sent to ${conversationId}`);
      return true;
    } catch (e: any) {
      console.error(`[TelegramChannel:${this.instanceId}] sendMessage failed:`, e?.message);
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
      if (this.pollPromise) {
        try { await this.pollPromise; } catch { /* ignore */ }
        this.pollPromise = null;
      }
      this.connected = false;
      this.accountId = null;
      this.token = null;
      this.botUsername = null;
      this.lastUpdateId = 0;

      this.emit('disconnected');
      console.log(`[TelegramChannel:${this.instanceId}] Disconnected`);
    } catch (e: any) {
      console.error(`[TelegramChannel:${this.instanceId}] disconnect error:`, e?.message);
    }
  }

  // ── HTTP helpers ──

  private async apiGet(url: string, timeoutMs?: number, abortSignal?: AbortSignal): Promise<string> {
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
        method: 'GET',
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
        headers: { 'Content-Type': 'application/json' },
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

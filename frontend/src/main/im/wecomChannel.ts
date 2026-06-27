import { EventEmitter } from 'events';
import type { IChannel, ChannelMessage, ConnectedInfo, ChannelOptions } from './IChannel';

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const WECOM_API_BASE = 'https://qyapi.weixin.qq.com';
const POLL_INTERVAL_MS = 5_000;

// ---------------------------------------------------------------------------
// WecomChannel
// ---------------------------------------------------------------------------

/**
 * WecomChannel — 企业微信 HTTP polling channel.
 *
 * Uses corpId + secret + agentId for authentication.
 * Polls /cgi-bin/getmsg every 5 seconds to receive messages.
 * Sends messages via /cgi-bin/message/send.
 *
 * Unlike WechatChannel which uses QR-code login, WecomChannel uses
 * a simple access-token flow (corpId + secret → access_token).
 */
export class WecomChannel extends EventEmitter implements IChannel {
  private instanceId: string;
  private instanceName: string;
  private corpId: string | null = null;
  private secret: string | null = null;
  private agentId: string | null = null;
  private accessToken: string | null = null;
  private connected: boolean = false;
  private accountId: string | null = null;
  private abortController: AbortController | null = null;
  private pollPromise: Promise<void> | null = null;
  private cursor: string | null = null;

  constructor(options: ChannelOptions) {
    super();
    this.instanceId = options.instanceId;
    this.instanceName = options.instanceName;
  }

  // -----------------------------------------------------------------------
  // IChannel props
  // -----------------------------------------------------------------------

  get isConnected(): boolean {
    return this.connected;
  }

  get currentAccountId(): string | null {
    return this.accountId;
  }

  // -----------------------------------------------------------------------
  // Access Token
  // -----------------------------------------------------------------------

  /**
   * Fetch a new access token from 企业微信.
   * GET https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={corpId}&corpsecret={secret}
   */
  async getAccessToken(): Promise<string> {
    if (!this.corpId || !this.secret) {
      throw new Error('corpId and secret are required');
    }

    const url =
      `${WECOM_API_BASE}/cgi-bin/gettoken?corpid=${encodeURIComponent(this.corpId)}&corpsecret=${encodeURIComponent(this.secret)}`;

    let res: Response;
    try {
      res = await fetch(url, { method: 'GET' });
    } catch (err) {
      throw new Error(`gettoken network error: ${err instanceof Error ? err.message : String(err)}`);
    }

    const data = (await res.json()) as { errcode: number; errmsg: string; access_token: string; expires_in: number };

    if (data.errcode !== 0 || !data.access_token) {
      throw new Error(`gettoken failed: ${data.errmsg} (errcode=${data.errcode})`);
    }

    this.accessToken = data.access_token;
    console.log(
      `[WecomChannel:${this.instanceId}] Access token obtained, expires in ${data.expires_in}s`,
    );
    return this.accessToken;
  }

  // -----------------------------------------------------------------------
  // Verify credentials
  // -----------------------------------------------------------------------

  /**
   * Verify that corpId + secret are valid by calling gettoken.
   * Does not modify connection state.
   */
  async verifyCredentials(corpId: string, secret: string): Promise<{ ok: boolean; accountId?: string; error?: string }> {
    // Temporarily set corpId/secret so getAccessToken can use them
    const prevCorpId = this.corpId;
    const prevSecret = this.secret;
    try {
      this.corpId = corpId;
      this.secret = secret;
      await this.getAccessToken();
      return { ok: true, accountId: corpId };
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : String(error);
      return { ok: false, error: `验证失败: ${msg}` };
    } finally {
      // Restore previous values — verifyCredentials is read-only
      this.corpId = prevCorpId;
      this.secret = prevSecret;
    }
  }

  /**
   * Connect using the provided credentials.
   * Gets an access token and emits 'connected'.
   */
  async connect(corpId: string, secret: string, agentId: string): Promise<void> {
    this.corpId = corpId;
    this.secret = secret;
    this.agentId = agentId;

    await this.getAccessToken();

    this.connected = true;
    this.accountId = corpId;

    console.log(
      `[WecomChannel:${this.instanceId}] Connected: corpId=${corpId}, agentId=${agentId}`,
    );

    this.emit('connected', { accountId: corpId });
  }

  // -----------------------------------------------------------------------
  // Polling
  // -----------------------------------------------------------------------

  /**
   * Start the HTTP polling loop that calls /cgi-bin/getmsg every 5 seconds.
   * Call after a successful connect() or restoreConnection().
   */
  startPolling(): void {
    if (!this.connected || !this.accessToken) {
      console.error(
        `[WecomChannel:${this.instanceId}] Cannot start polling: not connected`,
      );
      return;
    }

    if (this.pollPromise) {
      console.warn(`[WecomChannel:${this.instanceId}] Polling already active`);
      return;
    }

    this.abortController = new AbortController();
    this.pollPromise = this.pollLoop(this.abortController.signal);
  }

  private async pollLoop(signal: AbortSignal): Promise<void> {
    console.log(`[WecomChannel:${this.instanceId}] Starting getmsg poll loop`);

    while (!signal.aborted) {
      try {
        await this.pollOnce(signal);
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          break;
        }
        console.warn(
          `[WecomChannel:${this.instanceId}] Poll error (will retry): ` +
            `${err instanceof Error ? err.message : String(err)}`,
        );
      }

      // Wait POLL_INTERVAL_MS before the next poll
      if (!signal.aborted) {
        await new Promise<void>((resolve) => {
          const timer = setTimeout(resolve, POLL_INTERVAL_MS);
          const onAbort = (): void => {
            clearTimeout(timer);
            resolve();
          };
          signal.addEventListener('abort', onAbort, { once: true });
        });
      }
    }

    console.log(`[WecomChannel:${this.instanceId}] Poll loop exited`);
  }

  private async pollOnce(signal: AbortSignal): Promise<void> {
    if (!this.accessToken) return;

    const url =
      `${WECOM_API_BASE}/cgi-bin/getmsg?access_token=${encodeURIComponent(this.accessToken)}`;

    const body: Record<string, unknown> = {
      cursor: this.cursor ?? '',
      token: '',
      limit: 100,
    };

    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal,
    });

    const data = (await res.json()) as WecomGetmsgResponse;

    if (data.errcode !== 0) {
      // Token expired — refresh and return (next poll cycle will retry)
      if (data.errcode === 42001 || data.errcode === 40014) {
        console.log(
          `[WecomChannel:${this.instanceId}] Token expired during polling, refreshing...`,
        );
        await this.getAccessToken();
        return;
      }

      throw new Error(`getmsg failed: ${data.errmsg} (errcode=${data.errcode})`);
    }

    // Update cursor for next poll
    if (data.next_cursor) {
      this.cursor = data.next_cursor;
    }

    // Process incoming messages
    if (data.msgs && data.msgs.length > 0) {
      for (const msg of data.msgs) {
        const channelMsg = this.convertMessage(msg);
        if (channelMsg) {
          this.emit('message', channelMsg);
        }
      }
    }
  }

  // -----------------------------------------------------------------------
  // Message conversion
  // -----------------------------------------------------------------------

  private convertMessage(
    msg: WecomMessageWire,
  ): ChannelMessage | null {
    if (!msg.msgid || !msg.from_userid) {
      return null;
    }

    // chat_type is 'group' for group chats, 'single' or absent for direct
    const chatType: 'direct' | 'group' =
      msg.chat_type === 'group' ? 'group' : 'direct';

    return {
      messageId: msg.msgid,
      conversationId: msg.from_userid,
      senderId: msg.from_userid,
      content: msg.content || '',
      chatType,
      timestamp: msg.create_time * 1000,
    };
  }

  // -----------------------------------------------------------------------
  // Send message
  // -----------------------------------------------------------------------

  /**
   * Send a text message to a 企业微信 user or group.
   *
   * POST https://qyapi.weixin.qq.com/cgi-bin/message/send?access_token={token}
   * Body: { touser, msgtype: "text", agentid, text: { content } }
   */
  async sendMessage(conversationId: string, text: string): Promise<boolean> {
    if (!this.connected || !this.accessToken || !this.agentId) {
      console.error(
        `[WecomChannel:${this.instanceId}] Cannot send: not connected`,
      );
      return false;
    }

    try {
      const body = {
        touser: conversationId,
        msgtype: 'text' as const,
        agentid: this.agentId,
        text: { content: text },
      };

      const success = await this.sendMessageInternal(body);

      // If token expired, refresh and retry once
      if (!success) {
        // Check if it was a token error by attempting refresh
        try {
          await this.getAccessToken();
          return await this.sendMessageInternal(body);
        } catch {
          return false;
        }
      }

      return success;
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : String(error);
      console.error(
        `[WecomChannel:${this.instanceId}] sendMessage failed:`,
        msg,
      );
      this.emit('error', new Error(`sendMessage failed: ${msg}`));
      return false;
    }
  }

  private async sendMessageInternal(body: {
    touser: string;
    msgtype: string;
    agentid: string;
    text: { content: string };
  }): Promise<boolean> {
    const url =
      `${WECOM_API_BASE}/cgi-bin/message/send?access_token=${encodeURIComponent(this.accessToken!)}`;

    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });

    const data = (await res.json()) as WecomSendResponse;

    if (data.errcode !== 0) {
      console.error(
        `[WecomChannel:${this.instanceId}] sendMessage failed: errcode=${data.errcode} errmsg=${data.errmsg}`,
      );
      this.emit(
        'error',
        new Error(`sendMessage failed: ${data.errmsg} (errcode=${data.errcode})`),
      );
      return false;
    }

    console.log(
      `[WecomChannel:${this.instanceId}] Message sent to ${body.touser}`,
    );
    return true;
  }

  // -----------------------------------------------------------------------
  // Connection management
  // -----------------------------------------------------------------------

  /**
   * Restore connection from previously saved credentials.
   * Used when the app starts and credentials are already stored.
   */
  restoreConnection(credentials: Record<string, unknown>): void {
    const corpId = credentials.corpId as string;
    const secret = credentials.secret as string;
    const agentId = credentials.agentId as string;
    const accessToken = credentials.accessToken as string | undefined;
    const cursor = credentials.cursor as string | undefined;

    if (!corpId || !secret || !agentId) {
      console.error(
        `[WecomChannel:${this.instanceId}] restoreConnection: missing credentials`,
      );
      return;
    }

    this.corpId = corpId;
    this.secret = secret;
    this.agentId = agentId;
    this.accountId = corpId;

    if (accessToken) {
      this.accessToken = accessToken;
    }

    if (cursor) {
      this.cursor = cursor;
    }

    this.connected = true;

    console.log(
      `[WecomChannel:${this.instanceId}] Connection restored for corpId=${corpId}`,
    );
  }

  /**
   * Get current credentials for persistence.
   * Called by the gateway to save state between app restarts.
   */
  getCredentials(): Record<string, unknown> {
    return {
      corpId: this.corpId,
      secret: this.secret,
      agentId: this.agentId,
      accessToken: this.accessToken,
      cursor: this.cursor,
    };
  }

  /**
   * Disconnect the polling loop and clean up all state.
   */
  async disconnect(): Promise<void> {
    try {
      // Abort the poll loop
      if (this.abortController) {
        this.abortController.abort();
        this.abortController = null;
      }

      // Wait for the poll loop to exit
      if (this.pollPromise) {
        try {
          await this.pollPromise;
        } catch {
          // Ignore errors during shutdown
        }
        this.pollPromise = null;
      }

      this.connected = false;
      this.accountId = null;
      this.accessToken = null;
      this.cursor = null;

      this.emit('disconnected');
      console.log(`[WecomChannel:${this.instanceId}] Disconnected`);
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : String(error);
      console.error(
        `[WecomChannel:${this.instanceId}] disconnect error:`,
        msg,
      );
    }
  }
}

// ---------------------------------------------------------------------------
// Internal wire types
// ---------------------------------------------------------------------------

interface WecomMessageWire {
  msgid: string;
  from_userid: string;
  chat_type?: string;
  content: string;
  create_time: number;
}

interface WecomGetmsgResponse {
  errcode: number;
  errmsg: string;
  next_cursor?: string;
  msgs?: WecomMessageWire[];
}

interface WecomSendResponse {
  errcode: number;
  errmsg: string;
}

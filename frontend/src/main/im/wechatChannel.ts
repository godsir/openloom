import { EventEmitter } from 'events';
import { randomUUID } from 'crypto';

// ---------------------------------------------------------------------------
// Public interfaces (also exported for consumers)
// ---------------------------------------------------------------------------

export interface WechatChannelOptions {
  instanceId: string;
  instanceName: string;
}

export interface WechatQrStartResult {
  /** Base64 PNG data URL of the QR code (empty when qrContent is used). */
  qrDataUrl: string;
  /** Value to encode into a QR (rendered client-side); scanning it confirms login. */
  qrContent: string;
  /** Opaque session key that must be passed to waitForScan(). */
  sessionKey: string;
}

export interface WechatQrWaitResult {
  connected: boolean;
  accountId?: string;
  /** Bot token used for subsequent API calls (getUpdates, sendMessage, etc.). */
  botToken?: string;
  /** API base URL for subsequent API calls. */
  baseUrl?: string;
  /** The iLink user ID of the person who scanned the QR code. */
  userId?: string;
  message?: string;
}

export interface WechatMessage {
  messageId: string;
  conversationId: string;
  senderId: string;
  senderName?: string;
  groupName?: string;
  content: string;
  chatType: 'direct' | 'group';
  timestamp: number;
}

// ---------------------------------------------------------------------------
// Internal types (mirrors the package's iLink API wire format)
// ---------------------------------------------------------------------------

interface QrCodeResponse {
  qrcode: string;
  qrcode_img_content: string;
}

interface QrStatusResponse {
  status: 'wait' | 'scaned' | 'confirmed' | 'expired' | 'scaned_but_redirect' | 'need_verifycode' | 'verify_code_blocked' | 'binded_redirect';
  bot_token?: string;
  ilink_bot_id?: string;
  baseurl?: string;
  ilink_user_id?: string;
  redirect_host?: string;
}

interface GetUpdatesReqBody {
  get_updates_buf?: string;
  base_info?: { channel_version?: string; bot_agent?: string };
}

interface WeixinMessageItem {
  type?: number;
  text_item?: { text?: string };
  image_item?: unknown;
  voice_item?: unknown;
  file_item?: unknown;
  video_item?: unknown;
}

interface WeixinMessageWire {
  seq?: number;
  message_id?: number;
  from_user_id?: string;
  to_user_id?: string;
  client_id?: string;
  create_time_ms?: number;
  session_id?: string;
  group_id?: string;
  message_type?: number;
  message_state?: number;
  item_list?: WeixinMessageItem[];
  context_token?: string;
  run_id?: string;
}

interface GetUpdatesResp {
  ret?: number;
  errcode?: number;
  errmsg?: string;
  msgs?: WeixinMessageWire[];
  get_updates_buf?: string;
  longpolling_timeout_ms?: number;
}

interface SendMessageResp {
  ret?: number;
  errmsg?: string;
}

// ---------------------------------------------------------------------------
// Module-level state for active QR login sessions
// ---------------------------------------------------------------------------

interface ActiveLogin {
  sessionKey: string;
  qrcode: string;
  qrcodeUrl: string;
  startedAt: number;
  currentApiBaseUrl: string;
}

const ACTIVE_LOGIN_TTL_MS = 5 * 60_000;
const FIXED_BASE_URL = 'https://ilinkai.weixin.qq.com';
const QR_LONG_POLL_TIMEOUT_MS = 35_000;
const LOGIN_TIMEOUT_MS = 480_000;
const DEFAULT_BOT_TYPE = '3';
const DEFAULT_LONG_POLL_TIMEOUT_MS = 35_000;
const DEFAULT_API_TIMEOUT_MS = 15_000;
const BOT_AGENT = 'OpenLoom';

const activeLogins = new Map<string, ActiveLogin>();

function purgeExpiredLogins(): void {
  for (const [id, login] of activeLogins) {
    if (Date.now() - login.startedAt >= ACTIVE_LOGIN_TTL_MS) {
      activeLogins.delete(id);
    }
  }
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

function ensureTrailingSlash(url: string): string {
  return url.endsWith('/') ? url : `${url}/`;
}

function buildHeaders(token?: string): Record<string, string> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    AuthorizationType: 'ilink_bot_token',
    'iLink-App-Id': '',
    'iLink-App-ClientVersion': '0',
  };
  if (token?.trim()) {
    headers.Authorization = `Bearer ${token.trim()}`;
  }
  return headers;
}

async function apiPost(
  baseUrl: string,
  endpoint: string,
  body: string,
  token?: string,
  timeoutMs?: number,
  abortSignal?: AbortSignal,
): Promise<string> {
  const base = ensureTrailingSlash(baseUrl);
  const url = new URL(endpoint, base);
  const headers = buildHeaders(token);

  const controller = timeoutMs != null ? new AbortController() : undefined;
  const timer = controller != null && timeoutMs != null
    ? setTimeout(() => controller.abort(), timeoutMs)
    : undefined;

  // Combine internal timeout with external abort signal
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
    const res = await fetch(url.toString(), {
      method: 'POST',
      headers,
      body,
      ...(signal ? { signal } : {}),
    });
    if (timer !== undefined) clearTimeout(timer);
    const rawText = await res.text();
    if (!res.ok) {
      throw new Error(`${endpoint} HTTP ${res.status}: ${rawText.slice(0, 200)}`);
    }
    return rawText;
  } catch (err) {
    if (timer !== undefined) clearTimeout(timer);
    throw err;
  } finally {
    cleanup();
  }
}

async function apiGet(
  baseUrl: string,
  endpoint: string,
  timeoutMs?: number,
): Promise<string> {
  const base = ensureTrailingSlash(baseUrl);
  const url = new URL(endpoint, base);

  const controller = timeoutMs != null && timeoutMs > 0 ? new AbortController() : undefined;
  const timer = controller != null && timeoutMs != null
    ? setTimeout(() => controller.abort(), timeoutMs)
    : undefined;

  try {
    const res = await fetch(url.toString(), { method: 'GET' });
    if (timer !== undefined) clearTimeout(timer);
    const rawText = await res.text();
    if (!res.ok) {
      throw new Error(`GET ${endpoint} HTTP ${res.status}: ${rawText.slice(0, 200)}`);
    }
    return rawText;
  } catch (err) {
    if (timer !== undefined) clearTimeout(timer);
    throw err;
  }
}

// ---------------------------------------------------------------------------
// QR login helpers
// ---------------------------------------------------------------------------

async function fetchQrCode(): Promise<QrCodeResponse> {
  const rawText = await apiPost(
    FIXED_BASE_URL,
    `ilink/bot/get_bot_qrcode?bot_type=${encodeURIComponent(DEFAULT_BOT_TYPE)}`,
    JSON.stringify({ local_token_list: [] }),
  );
  return JSON.parse(rawText) as QrCodeResponse;
}

async function pollQrStatus(apiBaseUrl: string, qrcode: string): Promise<QrStatusResponse> {
  try {
    const rawText = await apiGet(
      apiBaseUrl,
      `ilink/bot/get_qrcode_status?qrcode=${encodeURIComponent(qrcode)}`,
      QR_LONG_POLL_TIMEOUT_MS,
    );
    return JSON.parse(rawText) as QrStatusResponse;
  } catch (err) {
    // Network errors / timeouts => treat as "wait" so the poll loop continues
    if (err instanceof Error && err.name === 'AbortError') {
      return { status: 'wait' };
    }
    return { status: 'wait' };
  }
}

// ---------------------------------------------------------------------------
// WechatChannel
// ---------------------------------------------------------------------------

/**
 * WechatChannel — wraps the WeChat iLink HTTP API for QR-code login and
 * long-poll messaging.
 *
 * Uses the same iLink endpoints as the @tencent-weixin/openclaw-weixin
 * package, but implemented directly on Node.js fetch so it works in
 * Electron without the OpenClaw plugin SDK.
 */
export class WechatChannel extends EventEmitter {
  private instanceId: string;
  private instanceName: string;
  private connected: boolean = false;
  private accountId: string | null = null;
  private token: string | null = null;
  private baseUrl: string | null = null;
  private abortController: AbortController | null = null;
  private pollPromise: Promise<void> | null = null;
  private pollingBuf: string | null = null;

  constructor(options: WechatChannelOptions) {
    super();
    this.instanceId = options.instanceId;
    this.instanceName = options.instanceName;
  }

  get isConnected(): boolean {
    return this.connected;
  }

  get currentAccountId(): string | null {
    return this.accountId;
  }

  // -----------------------------------------------------------------------
  // QR Code Login
  // -----------------------------------------------------------------------

  /**
   * Start the QR code login flow.
   * Returns a QR code URL (HTTPS image link) and a session key for waitForScan().
   */
  async startLogin(): Promise<WechatQrStartResult> {
    try {
      purgeExpiredLogins();

      const qrResponse = await fetchQrCode();
      const sessionKey = this.instanceId || randomUUID();

      const login: ActiveLogin = {
        sessionKey,
        qrcode: qrResponse.qrcode,
        qrcodeUrl: qrResponse.qrcode_img_content,
        startedAt: Date.now(),
        currentApiBaseUrl: FIXED_BASE_URL,
      };

      activeLogins.set(sessionKey, login);

      // qrcode_img_content is a JS-rendered SPA landing page (not an image),
      // so fetching it yields HTML. Return the URL as qrContent and let the
      // renderer render the QR via qrcode.react — scanning this URL triggers
      // the WeChat confirm flow on the phone.
      console.log(`[WechatChannel:${this.instanceId}] QR code generated, sessionKey=${sessionKey.slice(0, 8)}...`);
      return {
        qrDataUrl: '',
        qrContent: qrResponse.qrcode_img_content,
        sessionKey,
      };
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : String(error);
      console.error(`[WechatChannel:${this.instanceId}] startLogin failed:`, msg);
      throw error;
    }
  }

  /**
   * Poll/wait for the user to scan the QR code and confirm the login.
   * Blocks until confirmed, expired, or timeout.
   */
  async waitForScan(sessionKey: string): Promise<WechatQrWaitResult> {
    try {
      const activeLogin = activeLogins.get(sessionKey);

      if (!activeLogin) {
        return {
          connected: false,
          message: '当前没有进行中的登录，请先发起登录。',
        };
      }

      if (Date.now() - activeLogin.startedAt >= ACTIVE_LOGIN_TTL_MS) {
        activeLogins.delete(sessionKey);
        return {
          connected: false,
          message: '二维码已过期，请重新生成。',
        };
      }

      const deadline = Date.now() + LOGIN_TIMEOUT_MS;

      console.log(`[WechatChannel:${this.instanceId}] Waiting for QR scan (timeout: ${LOGIN_TIMEOUT_MS}ms)...`);

      while (Date.now() < deadline) {
        const statusResp = await pollQrStatus(
          activeLogin.currentApiBaseUrl,
          activeLogin.qrcode,
        );

        switch (statusResp.status) {
          case 'wait':
            break;

          case 'scaned':
            console.log(`[WechatChannel:${this.instanceId}] QR code scanned, waiting for confirmation...`);
            break;

          case 'expired':
            activeLogins.delete(sessionKey);
            return {
              connected: false,
              message: '二维码已过期，请重新获取。',
            };

          case 'binded_redirect':
            activeLogins.delete(sessionKey);
            return {
              connected: false,
              message: '已连接过此设备，无需重复连接。',
            };

          case 'scaned_but_redirect': {
            const redirectHost = statusResp.redirect_host;
            if (redirectHost) {
              activeLogin.currentApiBaseUrl = `https://${redirectHost}`;
              console.log(`[WechatChannel:${this.instanceId}] IDC redirect to ${redirectHost}`);
            }
            break;
          }

          case 'need_verifycode':
            // Verification code is required — not supported in the initial
            // Electron UI flow. The user will need to retry from CLI first.
            console.warn(`[WechatChannel:${this.instanceId}] Verification code required (not supported in UI flow)`);
            break;

          case 'verify_code_blocked':
            activeLogins.delete(sessionKey);
            return {
              connected: false,
              message: '多次输入错误，请稍后再试。',
            };

          case 'confirmed': {
            if (!statusResp.ilink_bot_id) {
              activeLogins.delete(sessionKey);
              return {
                connected: false,
                message: '登录失败：服务器未返回 bot ID。',
              };
            }

            activeLogins.delete(sessionKey);

            const botToken = statusResp.bot_token || '';
            const accountId = statusResp.ilink_bot_id;
            const baseUrl = statusResp.baseurl || FIXED_BASE_URL;

            // Store credentials
            this.token = botToken;
            this.baseUrl = baseUrl;
            this.accountId = accountId;
            this.connected = true;

            console.log(`[WechatChannel:${this.instanceId}] Login confirmed! accountId=${accountId}`);

            this.emit('connected', { accountId, baseUrl });

            return {
              connected: true,
              accountId,
              botToken,
              baseUrl,
              userId: statusResp.ilink_user_id,
              message: '已连接到微信。',
            };
          }
        }

        // Wait 1 second between polls
        await new Promise((r) => setTimeout(r, 1000));
      }

      // Timeout
      activeLogins.delete(sessionKey);
      return {
        connected: false,
        message: '登录超时，请重试。',
      };
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : String(error);
      console.error(`[WechatChannel:${this.instanceId}] waitForScan failed:`, msg);
      activeLogins.delete(sessionKey);
      return {
        connected: false,
        message: `登录失败: ${msg}`,
      };
    }
  }

  // -----------------------------------------------------------------------
  // Messaging long-poll loop
  // -----------------------------------------------------------------------

  /**
   * Start the long-poll getUpdates loop. Call after a successful login.
   * Incoming messages are emitted as 'message' events.
   */
  startPolling(): void {
    if (!this.connected || !this.token || !this.baseUrl) {
      console.error(`[WechatChannel:${this.instanceId}] Cannot start polling: not connected`);
      return;
    }

    if (this.pollPromise) {
      console.warn(`[WechatChannel:${this.instanceId}] Polling already active`);
      return;
    }

    this.abortController = new AbortController();
    this.pollingBuf = null;

    this.pollPromise = this.pollLoop(this.abortController.signal);
  }

  private async pollLoop(abortSignal: AbortSignal): Promise<void> {
    const baseUrl = this.baseUrl!;
    const token = this.token!;

    console.log(`[WechatChannel:${this.instanceId}] Starting long-poll loop at ${baseUrl}`);

    while (!abortSignal.aborted) {
      try {
        const body: GetUpdatesReqBody = {
          get_updates_buf: this.pollingBuf ?? '',
          base_info: { bot_agent: BOT_AGENT },
        };

        const rawText = await apiPost(
          baseUrl,
          'ilink/bot/getupdates',
          JSON.stringify(body),
          token,
          DEFAULT_LONG_POLL_TIMEOUT_MS,
          abortSignal,
        );

        const resp: GetUpdatesResp = JSON.parse(rawText);

        // Update polling buf for next request
        if (resp.get_updates_buf) {
          this.pollingBuf = resp.get_updates_buf;
        }

        // Check for session errors
        if (resp.errcode !== undefined && resp.errcode !== 0 && resp.errcode === -14) {
          console.warn(`[WechatChannel:${this.instanceId}] Session expired (errcode=-14), disconnecting`);
          this.emit('error', new Error('Session expired. Please re-login.'));
          await this.disconnect();
          return;
        }

        // Process incoming messages
        if (resp.msgs && resp.msgs.length > 0) {
          for (const wireMsg of resp.msgs) {
            const msg = this.convertMessage(wireMsg);
            if (msg) {
              this.emit('message', msg);
            }
          }
        }
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          // Normal shutdown
          break;
        }
        // Network errors are expected during long-poll; log and retry
        console.warn(`[WechatChannel:${this.instanceId}] Poll error (will retry): ${err instanceof Error ? err.message : String(err)}`);
        // Brief delay before retry
        await new Promise((r) => setTimeout(r, 1000));
      }
    }

    console.log(`[WechatChannel:${this.instanceId}] Poll loop exited`);
  }

  private convertMessage(wire: WeixinMessageWire): WechatMessage | null {
    // Extract text content from item_list
    let content = '';
    if (wire.item_list) {
      for (const item of wire.item_list) {
        if (item.text_item?.text) {
          content += item.text_item.text;
        }
      }
    }

    if (!content && wire.message_type === 1) {
      // User message with no text items — could be an image/file-only message
      content = '[非文本消息]';
    }

    if (!content && wire.message_type !== 1) {
      // Not a user message or empty — skip
      return null;
    }

    const senderId = wire.from_user_id || '';
    const chatType: 'direct' | 'group' = wire.group_id ? 'group' : 'direct';

    return {
      messageId: wire.client_id || wire.message_id?.toString() || randomUUID(),
      conversationId: chatType === 'group' ? wire.group_id! : senderId,
      senderId,
      senderName: undefined,
      groupName: undefined,
      content,
      chatType,
      timestamp: wire.create_time_ms || Date.now(),
    };
  }

  // -----------------------------------------------------------------------
  // Send message
  // -----------------------------------------------------------------------

  /**
   * Send a text message to a WeChat conversation.
   */
  async sendMessage(conversationId: string, text: string): Promise<boolean> {
    if (!this.connected || !this.token || !this.baseUrl) {
      console.error(`[WechatChannel:${this.instanceId}] Cannot send: not connected`);
      return false;
    }

    try {
      const clientId = randomUUID();
      const body = {
        msg: {
          from_user_id: '',
          to_user_id: conversationId,
          client_id: clientId,
          message_type: 2, // BOT
          message_state: 2, // FINISH
          item_list: text ? [{ type: 1, text_item: { text } }] : [],
          context_token: undefined as string | undefined,
        },
        base_info: { bot_agent: BOT_AGENT },
      };

      const rawText = await apiPost(
        this.baseUrl,
        'ilink/bot/sendmessage',
        JSON.stringify(body),
        this.token,
        DEFAULT_API_TIMEOUT_MS,
      );

      const resp: SendMessageResp = JSON.parse(rawText);
      if (resp.ret && resp.ret !== 0) {
        console.error(`[WechatChannel:${this.instanceId}] sendMessage failed: ret=${resp.ret} errmsg=${resp.errmsg || 'unknown'}`);
        this.emit('error', new Error(`sendMessage failed: ${resp.errmsg || 'unknown'}`));
        return false;
      }

      console.log(`[WechatChannel:${this.instanceId}] Message sent to ${conversationId}`);
      return true;
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : String(error);
      console.error(`[WechatChannel:${this.instanceId}] sendMessage failed:`, msg);
      this.emit('error', new Error(`sendMessage failed: ${msg}`));
      return false;
    }
  }

  // -----------------------------------------------------------------------
  // Connection management
  // -----------------------------------------------------------------------

  /**
   * Restore connection from previously saved credentials (no QR flow).
   * Used when the app starts and credentials are already stored.
   */
  restoreConnection(accountId: string, token: string, baseUrl: string): void {
    this.accountId = accountId;
    this.token = token;
    this.baseUrl = baseUrl;
    this.connected = true;
    console.log(`[WechatChannel:${this.instanceId}] Connection restored for accountId=${accountId}`);
  }

  /**
   * Disconnect the long-poll and clean up.
   */
  async disconnect(): Promise<void> {
    try {
      // Abort the poll loop
      if (this.abortController) {
        this.abortController.abort();
        this.abortController = null;
      }

      // Wait for poll loop to exit
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
      this.token = null;
      this.baseUrl = null;
      this.pollingBuf = null;

      this.emit('disconnected');
      console.log(`[WechatChannel:${this.instanceId}] Disconnected`);
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : String(error);
      console.error(`[WechatChannel:${this.instanceId}] disconnect error:`, msg);
    }
  }
}

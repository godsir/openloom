// frontend/src/main/im/popoChannel.ts
import { EventEmitter } from 'events';
import { createDecipheriv } from 'crypto';
import type { IChannel, ChannelMessage, ConnectedInfo, ChannelOptions } from './IChannel';

// ── POPO Open API 协议（从 moltbot-popo v2.1.13 逆向） ──
//
// 与之前自编的 SHA1 签名 / `/open-apis/im/v1/messages` 端点不同，真实协议为：
//   - 认证: appKey+appSecret 换 accessToken，用 `Open-Access-Token` 头调用其他接口
//   - 接收: STOMP over WebSocket（wss://nws.popo.netease.com:11012/stomp）
//   - 发送: HTTP POST /open-apis/robots/v1/im/send-msg
//   - 消息体加密: ROBOT_EVENT 的 data.encrypt 用 aesKey 做 AES-128-CBC 解密

// ── 常量 ──

const POPO_BASE_URL = 'https://open.popo.netease.com';
const POPO_API_BASE = `${POPO_BASE_URL}/open-apis/robots/v1`;
const POPO_STOMP_URL = 'wss://nws.popo.netease.com:11012/stomp';
const DEFAULT_API_TIMEOUT_MS = 15_000;
const RECONNECT_DELAY_MS = 5_000;
const HEARTBEAT_INTERVAL_MS = 10_000;
const STOMP_DESTINATION_BASE = '/robots/msg/OpenClaw';
const TOKEN_REFRESH_BUFFER_MS = 5 * 60_000; // 提前 5 分钟刷新

// ── 类型 ──

interface AccessTokenCache {
  accessToken: string;
  accessExpiredAt: number; // ms 时间戳
}

interface OnceTokenResult {
  onceToken: string;
  robotUid: string;
}

interface PopoApiResponse {
  errcode?: number;
  errmsg?: string;
  data?: any;
}

/** Mercury 下行消息（STOMP MESSAGE 帧 body 反序列化） */
interface MercuryMessage {
  messageId?: string;
  messageType?: string; // ROBOT_EVENT | MANAGEMENT | ...
  timestamp?: number;
  data?: any;
}

/** ROBOT_EVENT 解密后的事件 */
interface RobotEvent {
  eventType?: string;
  eventData?: any;
  meta?: any;
}

interface StompFrame {
  command: string;
  headers: Record<string, string>;
  body: string;
}

// ── STOMP 帧编解码 ──

function buildStompFrame(command: string, headers: Record<string, string> = {}, body = ''): string {
  let frame = `${command}\n`;
  for (const [k, v] of Object.entries(headers)) {
    frame += `${k}:${v}\n`;
  }
  frame += `\n${body}\0`;
  return frame;
}

function parseStompFrames(data: string): StompFrame[] {
  const frames: StompFrame[] = [];
  // STOMP 帧以 \0 分隔
  for (const part of data.split('\0')) {
    if (!part.trim()) continue;
    const lines = part.split('\n');
    const command = lines[0]?.trim() || '';
    const headers: Record<string, string> = {};
    let i = 1;
    for (; i < lines.length; i++) {
      const line = lines[i] || '';
      if (line === '') {
        i++;
        break;
      }
      const idx = line.indexOf(':');
      if (idx > 0) {
        headers[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
      }
    }
    const body = lines.slice(i).join('\n');
    frames.push({ command, headers, body });
  }
  return frames;
}

// ── AES-128-CBC 解密（与 moltbot-popo crypto.ts 一致） ──
// aesKey 必须 32 字符：前 16 位作 key，后 16 位作 iv

function decryptAes(encryptedText: string, aesKey: string): string {
  if (aesKey.length !== 32) {
    throw new Error(`aesKey must be exactly 32 characters (got ${aesKey.length})`);
  }
  const key = aesKey.substring(0, 16);
  const iv = aesKey.substring(16, 32);
  const decipher = createDecipheriv(
    'aes-128-cbc',
    Buffer.from(key, 'utf8'),
    Buffer.from(iv, 'utf8'),
  );
  let decrypted = decipher.update(encryptedText, 'base64', 'utf8');
  decrypted += decipher.final('utf8');
  return decrypted;
}

// ── PopoChannel ──

/**
 * PopoChannel — 基于 POPO Open API 实现 IChannel。
 *
 * - 凭据: appKey / appSecret / aesKey
 * - 接收: STOMP over WebSocket（Mercury 长连接）
 * - 发送: HTTP POST /im/send-msg
 * - 认证: appKey+appSecret 换 accessToken，`Open-Access-Token` 头
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

  private tokenCache: AccessTokenCache | null = null;
  private tokenPromise: Promise<string> | null = null;
  private robotUid = '';
  private subscriptionId = '';

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
    if (aesKey.length !== 32) {
      return { ok: false, error: `aesKey 必须为 32 字符（当前 ${aesKey.length}）` };
    }
    return { ok: true };
  }

  // ── 消息接收（STOMP over WebSocket） ──

  startPolling(): void {
    if (!this.connected || !this.appKey || !this.appSecret || !this.aesKey) {
      console.error(
        `[PopoChannel:${this.instanceId}] Cannot start: not connected`,
      );
      return;
    }

    if (this.wsPromise) {
      console.warn(`[PopoChannel:${this.instanceId}] STOMP loop already active`);
      return;
    }

    this.abortController = new AbortController();
    this.wsPromise = this.stompLoop(this.abortController.signal);
  }

  private async stompLoop(abortSignal: AbortSignal): Promise<void> {
    console.log(`[PopoChannel:${this.instanceId}] Starting POPO STOMP loop`);

    while (!abortSignal.aborted) {
      try {
        await this.connectStomp(abortSignal);
        if (!abortSignal.aborted) {
          console.log(
            `[PopoChannel:${this.instanceId}] STOMP disconnected, reconnecting in ${RECONNECT_DELAY_MS}ms...`,
          );
          await this.delay(RECONNECT_DELAY_MS, abortSignal);
        }
      } catch (err) {
        if (abortSignal.aborted) break;
        console.warn(
          `[PopoChannel:${this.instanceId}] STOMP error, will reconnect: ${err instanceof Error ? err.message : String(err)}`,
        );
        await this.delay(RECONNECT_DELAY_MS, abortSignal);
      }
    }

    console.log(`[PopoChannel:${this.instanceId}] STOMP loop exited`);
  }

  private async connectStomp(abortSignal: AbortSignal): Promise<void> {
    // 1. 获取 onceToken + robotUid
    const { onceToken, robotUid } = await this.getOnceToken();
    this.robotUid = robotUid;
    console.log(
      `[PopoChannel:${this.instanceId}] Got onceToken, robotUid="${robotUid || '(empty)'}"`,
    );

    // 2. 构建 STOMP WebSocket URL
    const url = new URL(POPO_STOMP_URL);
    url.searchParams.set('auth_type', 'ROBOT_ONCE_TOKEN');
    url.searchParams.set('auth_token', onceToken);
    url.searchParams.set('app_id', 'popo');

    return new Promise<void>((resolve, reject) => {
      if (abortSignal.aborted) {
        resolve();
        return;
      }

      let ws: WebSocket;
      try {
        // STOMP 子协议 v11/v12
        ws = new WebSocket(url.toString(), ['v11.stomp', 'v12.stomp']);
      } catch (e) {
        reject(e instanceof Error ? e : new Error(String(e)));
        return;
      }
      // Electron/undici 全局 WebSocket 默认 binaryType='blob'，会把 STOMP 文本帧当成
      // Blob 投递。设为 'arraybuffer' 后二进制帧以 ArrayBuffer 投递，便于同步转 string。
      try {
        (ws as any).binaryType = 'arraybuffer';
      } catch {
        /* 某些实现只读，忽略 */
      }
      this.ws = ws;

      const cleanup = (): void => {
        if (this.heartbeatTimer) {
          clearInterval(this.heartbeatTimer);
          this.heartbeatTimer = null;
        }
        if (connectDeadlineTimer) {
          clearTimeout(connectDeadlineTimer);
          connectDeadlineTimer = null;
        }
        this.ws = null;
      };

      let connectDeadlineTimer: ReturnType<typeof setTimeout> | null = null;

      ws.addEventListener('open', () => {
        console.log(`[PopoChannel:${this.instanceId}] STOMP transport opened (readyState=${ws.readyState}), sending CONNECT`);
        const connectFrame = buildStompFrame('CONNECT', {
          'accept-version': '1.0,1.1,1.2',
          host: 'mercury',
          'heart-beat': `${HEARTBEAT_INTERVAL_MS},${HEARTBEAT_INTERVAL_MS}`,
        });
        try {
          ws.send(connectFrame);
          console.log(`[PopoChannel:${this.instanceId}] CONNECT frame sent (${connectFrame.length} bytes)`);
        } catch (e) {
          console.error(`[PopoChannel:${this.instanceId}] Failed to send CONNECT frame:`, e instanceof Error ? e.message : String(e));
        }
        // 5 秒内未收到 CONNECTED 则告警（诊断用）
        connectDeadlineTimer = setTimeout(() => {
          if (!this.connected) {
            console.warn(
              `[PopoChannel:${this.instanceId}] No CONNECTED frame within 5s (readyState=${ws.readyState})`,
            );
          }
        }, 5_000);
      });

      ws.addEventListener('message', async (event: MessageEvent) => {
        // event.data 可能是 string / ArrayBuffer / Buffer / Blob，统一转 string
        const raw = (event as any).data;
        let dataStr = '';
        if (typeof raw === 'string') {
          dataStr = raw;
        } else if (raw instanceof ArrayBuffer) {
          dataStr = Buffer.from(raw).toString('utf8');
        } else if (Buffer.isBuffer(raw)) {
          dataStr = raw.toString('utf8');
        } else if (raw && typeof raw.arrayBuffer === 'function') {
          // Blob (binaryType 未生效时的兜底)，异步读
          try {
            const ab = await raw.arrayBuffer();
            dataStr = Buffer.from(ab).toString('utf8');
          } catch (e) {
            console.error(`[PopoChannel:${this.instanceId}] STOMP Blob read failed:`, e instanceof Error ? e.message : String(e));
            return;
          }
        }
        if (!dataStr) {
          console.log(`[PopoChannel:${this.instanceId}] STOMP message: empty data (type=${typeof raw})`);
          return;
        }
        // 诊断：打印原始帧首行
        const firstLine = dataStr.split('\n')[0] || '';
        console.log(
          `[PopoChannel:${this.instanceId}] STOMP raw frame: ${firstLine.slice(0, 80)} (${dataStr.length} bytes)`,
        );
        const frames = parseStompFrames(dataStr);
        for (const frame of frames) {
          this.handleStompFrame(frame, ws);
        }
      });

      ws.addEventListener('close', (evt: CloseEvent) => {
        console.log(
          `[PopoChannel:${this.instanceId}] STOMP closed (code=${evt.code}, reason="${evt.reason || ''}", wasClean=${evt.wasClean})`,
        );
        cleanup();
        resolve();
      });

      ws.addEventListener('error', (err: Event) => {
        const state = ws?.readyState;
        const stateDesc =
          state === 1 ? 'OPEN' : state === 2 ? 'CLOSING' : state === 3 ? 'CLOSED' : 'CONNECTING';
        const msg = (err as ErrorEvent)?.message || '';
        console.error(
          `[PopoChannel:${this.instanceId}] STOMP error (readyState=${state} ${stateDesc})${msg ? ': ' + msg : ''}`,
        );
        // error 后通常会紧跟 close，由 close 触发 resolve；这里不主动 reject 避免重复
      });

      const onAbort = (): void => {
        try { ws.close(); } catch { /* ignore */ }
      };
      abortSignal.addEventListener('abort', onAbort, { once: true });
    });
  }

  private handleStompFrame(frame: StompFrame, ws: WebSocket): void {
    switch (frame.command) {
      case 'CONNECTED':
        this.connected = true;
        console.log(
          `[PopoChannel:${this.instanceId}] STOMP session established (version: ${frame.headers['version'] ?? 'unknown'})`,
        );
        this.subscribeStomp(ws);
        this.startHeartbeat(ws);
        this.emit('connected', { accountId: this.accountId || this.appKey || '' });
        break;
      case 'MESSAGE':
        this.handleMercuryMessage(frame.body);
        break;
      case 'ERROR':
        console.error(
          `[PopoChannel:${this.instanceId}] STOMP ERROR frame: ${frame.headers['message'] || ''} body=${frame.body.slice(0, 200)}`,
        );
        this.emit('error', new Error(`STOMP Error: ${frame.headers['message'] || 'unknown'}`));
        try { ws.close(); } catch { /* ignore */ }
        break;
      case 'RECEIPT':
      case 'PONG':
        // 心跳/回执，忽略
        break;
      default:
        // 未知帧（包括心跳 \n），忽略
        break;
    }
  }

  private subscribeStomp(ws: WebSocket): void {
    const destination = this.robotUid
      ? `${STOMP_DESTINATION_BASE}/${this.robotUid}`
      : STOMP_DESTINATION_BASE;
    this.subscriptionId = `sub-${Math.random().toString(36).substring(2, 10)}`;
    const subFrame = buildStompFrame('SUBSCRIBE', {
      id: this.subscriptionId,
      destination,
      ack: 'auto',
    });
    ws.send(subFrame);
    console.log(`[PopoChannel:${this.instanceId}] Subscribed to ${destination}`);
  }

  private startHeartbeat(ws: WebSocket): void {
    if (this.heartbeatTimer) clearInterval(this.heartbeatTimer);
    this.heartbeatTimer = setInterval(() => {
      if (ws.readyState === WebSocket.OPEN) {
        try {
          ws.send(buildStompFrame('SEND', { destination: '/heart-beat' }));
          ws.send('\n');
        } catch {
          /* ignore heartbeat send error */
        }
      }
    }, HEARTBEAT_INTERVAL_MS);
  }

  private handleMercuryMessage(body: string): void {
    try {
      const mercury: MercuryMessage = JSON.parse(body);
      const msgType = mercury.messageType;
      console.log(
        `[PopoChannel:${this.instanceId}] Mercury message: type=${msgType || '(none)'}, hasData=${!!mercury.data}`,
      );

      if (msgType === 'MANAGEMENT') {
        return; // 管理消息，忽略
      }
      if (msgType !== 'ROBOT_EVENT') {
        return;
      }

      // data 可能是加密字符串，或 { encrypt: "..." }，或明文对象
      let payload: any = mercury.data;
      if (typeof payload === 'string') {
        try {
          payload = JSON.parse(payload);
        } catch {
          /* 保持字符串，下面走加密分支 */
        }
      }

      let evt: RobotEvent;
      if (typeof payload === 'string' && this.aesKey) {
        const decryptedJson = decryptAes(payload, this.aesKey);
        evt = JSON.parse(decryptedJson);
      } else if (payload?.encrypt && this.aesKey) {
        const decryptedJson = decryptAes(payload.encrypt, this.aesKey);
        evt = JSON.parse(decryptedJson);
      } else if (payload?.eventType) {
        evt = payload;
      } else {
        console.warn(
          `[PopoChannel:${this.instanceId}] Unrecognized ROBOT_EVENT payload format, keys=${typeof payload === 'object' ? Object.keys(payload || {}).join(',') : typeof payload}`,
        );
        return;
      }

      const { eventType, eventData } = evt;
      console.log(
        `[PopoChannel:${this.instanceId}] Decrypted event: type=${eventType || '(none)'}, hasEventData=${!!eventData}, msgType=${eventData?.msgType}`,
      );
      if (!eventData) return;

      // 只处理单聊/群聊消息事件
      if (
        eventType !== 'IM_P2P_TO_ROBOT_MSG' &&
        eventType !== 'IM_CHAT_TO_ROBOT_AT_MSG'
      ) {
        return;
      }

      const channelMsg = this.convertEvent(eventType, eventData, mercury.timestamp);
      if (channelMsg) {
        console.log(
          `[PopoChannel:${this.instanceId}] Emitting message: conv=${channelMsg.conversationId}, sender=${channelMsg.senderId}, chatType=${channelMsg.chatType}`,
        );
        this.emit('message', channelMsg);
      } else {
        console.warn(
          `[PopoChannel:${this.instanceId}] convertEvent returned null: msgType=${eventData.msgType}, notify=${JSON.stringify(eventData.notify)?.slice(0, 60)}, from=${eventData.from}`,
        );
      }
    } catch (err) {
      console.error(
        `[PopoChannel:${this.instanceId}] Failed to process STOMP message:`,
        err instanceof Error ? err.message : String(err),
      );
    }
  }

  private convertEvent(
    eventType: string,
    eventData: any,
    mercuryTimestamp?: number,
  ): ChannelMessage | null {
    // msgType=1 为文本；其他类型暂不处理
    const msgType = eventData.msgType;
    if (msgType !== 1) return null;

    const content = eventData.notify || '';
    if (!content) return null;

    const senderId = eventData.from || '';
    if (!senderId) return null;

    const isGroup = eventType === 'IM_CHAT_TO_ROBOT_AT_MSG' || eventData.sessionType === 3;
    const chatType: 'direct' | 'group' = isGroup ? 'group' : 'direct';

    // 单聊 conversationId = 发送者；群聊 = sessionId(群ID)
    const conversationId = isGroup
      ? (eventData.sessionId || senderId)
      : senderId;

    const messageId = eventData.uuid || `${senderId}-${mercuryTimestamp || Date.now()}`;
    const timestamp = this.parseAddtime(eventData.addtime) || mercuryTimestamp || Date.now();

    return {
      messageId,
      conversationId,
      senderId,
      senderName: eventData.fromUserName || eventData.from,
      groupName: isGroup ? eventData.sessionName : undefined,
      content,
      chatType,
      timestamp,
    };
  }

  private parseAddtime(addtime: any): number | null {
    if (!addtime) return null;
    if (typeof addtime === 'number') return addtime;
    const s = String(addtime);
    // 支持 ISO 或 "YYYY-MM-DD HH:mm:ss"
    const ms = Date.parse(s.includes('T') ? s : s.replace(' ', 'T'));
    return Number.isNaN(ms) ? null : ms;
  }

  // ── 消息发送（HTTP POST /im/send-msg） ──

  async sendMessage(conversationId: string, text: string): Promise<boolean> {
    if (!this.appKey || !this.appSecret) {
      console.error(`[PopoChannel:${this.instanceId}] Cannot send: not connected`);
      return false;
    }

    try {
      const ok = await this.callSendMsg(conversationId, text, true);
      if (ok) {
        console.log(`[PopoChannel:${this.instanceId}] Message sent to ${conversationId}`);
      }
      return ok;
    } catch (e: any) {
      console.error(`[PopoChannel:${this.instanceId}] sendMessage failed:`, e?.message);
      this.emit('error', new Error(`sendMessage failed: ${e?.message || String(e)}`));
      return false;
    }
  }

  private async callSendMsg(receiver: string, content: string, retry: boolean): Promise<boolean> {
    const token = await this.getAccessToken();
    const body = JSON.stringify({
      receiver,
      msgType: 'text',
      message: { content },
    });

    const res = await fetch(`${POPO_API_BASE}/im/send-msg`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Open-Access-Token': token,
      },
      body,
      signal: AbortSignal.timeout(DEFAULT_API_TIMEOUT_MS),
    });

    const rawText = await res.text();
    if (!res.ok) {
      console.error(
        `[PopoChannel:${this.instanceId}] send-msg HTTP ${res.status} — ${rawText.slice(0, 500)}`,
      );
      return false;
    }

    const resp: PopoApiResponse = JSON.parse(rawText);
    // token 过期/失效时清缓存重试一次
    if (resp.errcode !== 0 && retry && this.isTokenError(resp.errcode)) {
      this.tokenCache = null;
      return this.callSendMsg(receiver, content, false);
    }
    if (resp.errcode !== 0) {
      console.error(
        `[PopoChannel:${this.instanceId}] send-msg failed: errcode=${resp.errcode} ${resp.errmsg || ''}`,
      );
      return false;
    }
    return true;
  }

  private isTokenError(errcode: number | undefined): boolean {
    // moltbot-popo 中 TOKEN_EXPIRED / TOKEN_INVALID 触发刷新；具体码值未知，
    // 用一个保守的区间判断（非 0 且非业务错误时尝试刷新）。
    return errcode !== undefined && errcode !== 0 && (errcode === 401 || errcode === 40001 || errcode === 40014);
  }

  // ── accessToken / onceToken ──

  private async getAccessToken(): Promise<string> {
    if (this.tokenCache && Date.now() < this.tokenCache.accessExpiredAt - TOKEN_REFRESH_BUFFER_MS) {
      return this.tokenCache.accessToken;
    }
    if (this.tokenPromise) {
      return this.tokenPromise;
    }
    this.tokenPromise = this.refreshAccessToken();
    try {
      return await this.tokenPromise;
    } finally {
      this.tokenPromise = null;
    }
  }

  private async refreshAccessToken(): Promise<string> {
    const res = await fetch(`${POPO_API_BASE}/token`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ appKey: this.appKey, appSecret: this.appSecret }),
      signal: AbortSignal.timeout(DEFAULT_API_TIMEOUT_MS),
    });
    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw new Error(`fetch token HTTP ${res.status}: ${text.slice(0, 200)}`);
    }
    const resp: PopoApiResponse = await res.json();
    if (resp.errcode !== 0 || !resp.data?.accessToken) {
      throw new Error(`fetch token failed: errcode=${resp.errcode} ${resp.errmsg || ''}`);
    }
    this.tokenCache = {
      accessToken: resp.data.accessToken,
      accessExpiredAt: resp.data.accessExpiredAt || (Date.now() + 7200_000),
    };
    return this.tokenCache.accessToken;
  }

  private async getOnceToken(): Promise<OnceTokenResult> {
    const token = await this.getAccessToken();
    const res = await fetch(`${POPO_API_BASE}/im/onceToken/get`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Open-Access-Token': token,
      },
      body: JSON.stringify({}),
      signal: AbortSignal.timeout(DEFAULT_API_TIMEOUT_MS),
    });
    if (!res.ok) {
      // token 过期则清缓存，下次重试
      if (res.status === 401) this.tokenCache = null;
      const text = await res.text().catch(() => '');
      throw new Error(`get onceToken HTTP ${res.status}: ${text.slice(0, 200)}`);
    }
    const resp: PopoApiResponse = await res.json();
    if (resp.errcode !== 0 || !resp.data?.onceToken) {
      if (this.isTokenError(resp.errcode)) {
        this.tokenCache = null;
      }
      throw new Error(`get onceToken failed: errcode=${resp.errcode} ${resp.errmsg || ''}`);
    }
    return {
      onceToken: resp.data.onceToken,
      robotUid: resp.data.robotUid ?? '',
    };
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

      if (this.wsPromise) {
        try {
          await this.wsPromise;
        } catch {
          /* ignore shutdown errors */
        }
        this.wsPromise = null;
      }

      this.connected = false;
      this.tokenCache = null;
      this.tokenPromise = null;
      this.accountId = null;
      this.appKey = null;
      this.appSecret = null;
      this.aesKey = null;
      this.robotUid = '';
      this.subscriptionId = '';

      this.emit('disconnected');
      console.log(`[PopoChannel:${this.instanceId}] Disconnected`);
    } catch (e: any) {
      console.error(`[PopoChannel:${this.instanceId}] disconnect error:`, e?.message);
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

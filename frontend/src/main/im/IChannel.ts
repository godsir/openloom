// frontend/src/main/im/IChannel.ts

export interface ChannelOptions {
  instanceId: string;
  instanceName: string;
}

export interface ChannelMessage {
  messageId: string;
  conversationId: string;
  senderId: string;
  senderName?: string;
  groupName?: string;
  content: string;
  chatType: 'direct' | 'group';
  timestamp: number;
}

export interface ConnectedInfo {
  accountId: string;
  baseUrl?: string;
}

/**
 * IChannel — 所有 IM 平台 Channel 的统一契约。
 *
 * 每个 Channel 实现负责该平台的连接、消息收发、凭据恢复。
 * 平台特有的认证流程（QR / OAuth / Token）不放入接口，
 * 而是作为命名方法放在 IMGatewayManager 上。
 */
export interface IChannel {
  readonly isConnected: boolean;
  readonly currentAccountId: string | null;

  // 事件（实现类继承 EventEmitter）
  on(event: 'message', handler: (msg: ChannelMessage) => void): this;
  on(event: 'connected', handler: (info: ConnectedInfo) => void): this;
  on(event: 'error', handler: (err: Error) => void): this;
  on(event: 'disconnected', handler: () => void): this;

  // 消息收发
  sendMessage(conversationId: string, text: string): Promise<boolean>;
  startPolling(): void;
  disconnect(): Promise<void>;

  // 凭据恢复（app 重启时，格式由各 Channel 自行解析）
  restoreConnection(credentials: Record<string, unknown>): void;
}

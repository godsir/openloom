export interface BridgeSession {
  id: string;
  platform: string;
  chatId: string;
  userName: string | null;
  accessState: string;
  createdAt: string;
  lastMessageAt: string | null;
  messageCount: number;
}

export interface BridgeMessage {
  id: number;
  direction: 'inbound' | 'outbound';
  content: string | null;
  mediaType: string;
  mediaUrl: string | null;
  timestamp: string;
}

export interface BridgeIncomingMessage {
  platform: string;
  sessionKey: string;
  direction: string;
  sender: string;
  text: string;
  isGroup: boolean;
  ts: number;
  agentId?: string;
}

export interface BridgeSlice {
  /** 最新收到的 bridge 消息（ws-message-handler 写入，BridgePanel 订阅） */
  bridgeLatestMessage: BridgeIncomingMessage | null;
  /** 递增计数器，每次 bridge_status 事件 +1，代替 loadStatus 回调 */
  bridgeStatusTrigger: number;
  /** 写入一条 bridge 消息 */
  addBridgeMessage: (msg: BridgeIncomingMessage) => void;
  /** 触发 bridge 状态重载 */
  triggerBridgeReload: () => void;
  /** 各平台连接状态 */
  bridgeStatus: Record<string, string>;
  /** Bridge 会话列表 */
  bridgeSessions: BridgeSession[];
  /** Bridge 消息列表 */
  bridgeMessages: BridgeMessage[];
  /** 当前活跃的 bridge 会话 ID */
  bridgeActiveSession: string | null;
  /** 设置平台连接状态 */
  setBridgeStatus: (platform: string, status: string) => void;
  /** 设置 bridge 会话列表 */
  setBridgeSessions: (sessions: BridgeSession[]) => void;
  /** 设置 bridge 消息列表 */
  setBridgeMessages: (messages: BridgeMessage[]) => void;
  /** 设置当前活跃的 bridge 会话 */
  setBridgeActiveSession: (sessionId: string | null) => void;
}

export const createBridgeSlice = (
  set: (partial: Partial<BridgeSlice> | ((s: BridgeSlice) => Partial<BridgeSlice>)) => void,
): BridgeSlice => ({
  bridgeLatestMessage: null,
  bridgeStatusTrigger: 0,
  addBridgeMessage: (msg) => set({ bridgeLatestMessage: msg }),
  triggerBridgeReload: () =>
    set((s) => ({ bridgeStatusTrigger: s.bridgeStatusTrigger + 1 })),
  bridgeStatus: {},
  bridgeSessions: [],
  bridgeMessages: [],
  bridgeActiveSession: null,
  setBridgeStatus: (platform, status) =>
    set((s) => ({ bridgeStatus: { ...s.bridgeStatus, [platform]: status } })),
  setBridgeSessions: (sessions) => set({ bridgeSessions: sessions }),
  setBridgeMessages: (messages) => set({ bridgeMessages: messages }),
  setBridgeActiveSession: (sessionId) => set({ bridgeActiveSession: sessionId }),
});

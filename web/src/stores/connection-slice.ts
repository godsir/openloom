/**
 * connection-slice.ts — Loom 本地引擎连接状态
 *
 * 简化为单引擎跟踪。保留向后兼容字段。
 */

import { getEnginePort } from '../adapter';

export interface ConnectionSlice {
  enginePort: number;
  serverPort: string | null;      // backward compat: string form of enginePort
  serverToken: string | null;     // backward compat: unused by Loom
  serverConnections: Record<string, any>;  // backward compat: always empty
  activeServerConnectionId: string | null; // backward compat
  activeServerConnection: any;    // backward compat
  connected: boolean;
  statusKey: string;
  statusVars: Record<string, string | number>;
  bridgeDotConnected: boolean;
  wsState: 'connected' | 'reconnecting' | 'disconnected';
  wsReconnectAttempt: number;
  oauthSessionId: string | null;
  setServerPort: (port: string | number | null) => void;
  setServerToken: (token: string | null) => void;
  setActiveServerConnection: (connection: any) => void;
  setLocalServerConnection: (port: string | number | null, token: string | null) => void;
  upsertServerConnection: (connection: any) => void;
  selectServerConnection: (connectionId: string) => void;
  setConnected: (connected: boolean) => void;
  setOauthSessionId: (id: string | null) => void;
}

export const createConnectionSlice = (
  set: (partial: Partial<ConnectionSlice>) => void,
  get?: () => Pick<ConnectionSlice, 'serverPort' | 'serverToken' | 'serverConnections' | 'activeServerConnectionId' | 'activeServerConnection'>,
): ConnectionSlice => ({
  enginePort: getEnginePort(),
  serverPort: null,
  serverToken: null,
  serverConnections: {},
  activeServerConnectionId: null,
  activeServerConnection: null,
  connected: false,
  statusKey: 'status.connecting',
  statusVars: {},
  bridgeDotConnected: false,
  wsState: 'disconnected',
  wsReconnectAttempt: 0,
  oauthSessionId: null,

  setServerPort: (port) => {
    const serverPort = port === null || port === undefined ? null : String(port);
    set({ serverPort, connected: !!serverPort });
  },

  setServerToken: (_token) => {
    // Loom 本地引擎不使用 token 认证；保留以兼容
  },

  setActiveServerConnection: (connection) => set(
    connection
      ? {
          activeServerConnectionId: connection.connectionId,
          activeServerConnection: connection,
        }
      : {
          activeServerConnectionId: null,
          activeServerConnection: null,
        },
  ),

  setLocalServerConnection: (port, _token) => {
    const serverPort = port === null || port === undefined ? null : String(port);
    set({ serverPort, connected: !!serverPort });
  },

  upsertServerConnection: (_connection) => {
    // Loom 仅有单个本地引擎，无需注册表
  },

  selectServerConnection: (_connectionId) => {
    // Loom 仅有单个本地引擎
  },

  setConnected: (connected) => set({ connected }),

  setOauthSessionId: (id) => set({ oauthSessionId: id }),
});

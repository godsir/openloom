export interface ServerConnection { connectionId?: string; host: string; port: number; tls: boolean; token?: string; }

export function resolveServerConnection(_state?: any, _sessionPath?: string): ServerConnection {
  return { host: '127.0.0.1', port: (window as any).__enginePort__ ?? 0, tls: false };
}

export function buildConnectionUrl(conn: ServerConnection, path: string): string {
  return `http${conn.tls ? 's' : ''}://${conn.host}:${conn.port}${path}`;
}

export function buildConnectionWsUrl(conn: ServerConnection, path: string): string {
  return `ws${conn.tls ? 's' : ''}://${conn.host}:${conn.port}${path}`;
}

export function createLocalServerConnection(opts?: { serverPort?: string | null; serverToken?: string | null }): ServerConnection {
  const port = parseInt(opts?.serverPort || String((window as any).__enginePort__ ?? 0), 10);
  return { connectionId: LOCAL_CONNECTION_ID, host: '127.0.0.1', port, tls: false, token: opts?.serverToken ?? undefined };
}

// Loom local-only stubs (no auth, no multi-device)
export function requireServerConnection(source: any, _errorMessage: string): ServerConnection {
  return resolveServerConnection(source);
}

export function appendConnectionAuth(_conn: ServerConnection, headers?: Record<string, string>): Record<string, string> {
  return headers || {};
}

export async function connectDeviceServerConnection(_opts: any): Promise<ServerConnection> {
  return resolveServerConnection();
}

export function persistServerConnectionSelection(connection: ServerConnection, _storage?: any): any {
  return { serverPort: String(connection.port), serverToken: connection.token || null };
}

// Additional stubs for Hanako compatibility
export const LOCAL_CONNECTION_ID = 'local';
export type ServerConnectionRegistry = Record<string, ServerConnection>;

export function readPersistedServerConnectionState(_storage?: any): any {
  return { activeServerConnectionId: LOCAL_CONNECTION_ID, serverConnections: {} };
}

export function refreshLocalServerConnection(opts?: { existingConnection?: ServerConnection; serverPort?: number; serverToken?: string | null }): ServerConnection {
  if (opts?.serverPort) {
    return createLocalServerConnection({ serverPort: String(opts.serverPort), serverToken: opts.serverToken });
  }
  return createLocalServerConnection();
}

export function upsertServerConnection(_connection: any, _registry?: any): any {
  return {};
}

export function hasServerConnection(_source?: any): boolean {
  return true;
}

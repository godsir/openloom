// adapter.ts — Loom JSON-RPC bridge
// Only new file. Everything else is Hanako original.

export async function loomRpc<T = any>(method: string, params?: Record<string, unknown>): Promise<T> {
  if (!window.openloom) throw new Error('Engine not ready');
  // window.openloom.send() already unwraps data.result from the JSON-RPC envelope
  const result = await window.openloom.send(method, params ?? {});
  return result as T;
}

export function loomSubscribe(event: string, handler: (data: any) => void): () => void {
  return window.openloom?.subscribe(event, handler) ?? (() => {});
}

export function getEnginePort(): number {
  return window.__enginePort__ ?? 0;
}

export function isEngineReady(): boolean {
  return !!window.openloom;
}

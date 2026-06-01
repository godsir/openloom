import { handleWsMessage } from './jsonrpc'

type ReconnectCallback = () => void

let ws: WebSocket | null = null
let retryDelay = 1000
let retryCount = 0
const MAX_RETRY_DELAY = 30000
const MAX_RETRIES = 20
let onReconnect: ReconnectCallback | null = null
const onOpenCallbacks: Array<() => void> = []

export function onWsConnected(cb: () => void): void {
  onOpenCallbacks.push(cb)
  // If already connected, fire immediately
  if (ws && ws.readyState === WebSocket.OPEN) cb()
}

// Module-level state for connection tracking
let setWsStateFn: ((state: 'connected' | 'reconnecting' | 'disconnected') => void) | null = null
let setReconnectAttemptFn: ((n: number) => void) | null = null

export function registerConnectionSetters(
  setWsState: (state: 'connected' | 'reconnecting' | 'disconnected') => void,
  setReconnectAttempt: (n: number) => void,
): void {
  setWsStateFn = setWsState
  setReconnectAttemptFn = setReconnectAttempt
}

// All pending resolvers for the in-flight connection. Multiple concurrent
// callers (e.g. React StrictMode double-mount) share the same socket — when
// it opens, every awaiter must resolve so their bootstrap can complete and
// clean up. Without this, the first caller's promise was silently dropped
// when the second caller rebuilt the socket, leaving its bootstrap hung and
// leaking a duplicate `loomSubscribe` handler (→ doubled stream deltas).
let connectResolvers: Array<() => void> = []

function resolveAllPending(): void {
  const resolvers = connectResolvers
  connectResolvers = []
  for (const r of resolvers) r()
}

export function connectWebSocket(port: number): Promise<void> {
  // If already open, resolve immediately
  if (ws && ws.readyState === WebSocket.OPEN) return Promise.resolve()

  // If a socket is already connecting, piggy-back on it instead of tearing
  // it down — tearing down would orphan any awaiter on the prior promise.
  if (ws && ws.readyState === WebSocket.CONNECTING) {
    return new Promise<void>((resolve) => { connectResolvers.push(resolve) })
  }

  // CLOSING / CLOSED — drop the stale reference and build a new socket.
  if (ws) {
    ws.onopen = null
    ws.onmessage = null
    ws.onclose = null
    ws.onerror = null
    try { ws.close() } catch { /* ignore */ }
    ws = null
  }

  const url = `ws://127.0.0.1:${port}/ws`
  ws = new WebSocket(url)

  return new Promise<void>((resolve) => {
    connectResolvers.push(resolve)

    ws!.onopen = () => {
      retryDelay = 1000
      retryCount = 0
      setWsStateFn?.('connected')
      setReconnectAttemptFn?.(0)
      // Flush queued RPC sends first
      for (const cb of onOpenCallbacks) cb()
      onReconnect?.()
      resolveAllPending()
    }

    ws!.onmessage = (event) => {
      handleWsMessage(event.data as string)
    }

    ws!.onclose = () => {
      retryCount++
      if (retryCount <= MAX_RETRIES) {
        setWsStateFn?.('reconnecting')
        setReconnectAttemptFn?.(retryCount)
        setTimeout(() => connectWebSocket(port), retryDelay)
        retryDelay = Math.min(retryDelay * 2, MAX_RETRY_DELAY)
      } else {
        setWsStateFn?.('disconnected')
        // Unblock any awaiters so their bootstrap can finish and clean up.
        resolveAllPending()
      }
    }

    ws!.onerror = () => {
      // onclose fires after onerror
    }
  })
}

export function onWsReconnect(cb: ReconnectCallback): void {
  onReconnect = cb
}

export function getWs(): WebSocket | null {
  return ws
}

export function disconnectWebSocket(): void {
  if (ws) {
    ws.onclose = null
    ws.close()
    ws = null
  }
}

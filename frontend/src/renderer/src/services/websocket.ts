import { handleWsMessage } from './jsonrpc'

type ReconnectCallback = () => void

let lastSeq = 0

export function setLastSeq(seq: number): void {
  lastSeq = seq
}

let ws: WebSocket | null = null
let retryCount = 0
let reconnectTimer: ReturnType<typeof setTimeout> | null = null

// Fast reconnect: 200ms base with 30% jitter, max 5s between attempts.
// After ~100 retries (~5 min), resolve pending awaiters so the UI doesn't hang forever.
const INITIAL_DELAY = 200
const MAX_DELAY = 5000
const JITTER = 0.3
const RETRY_GIVE_UP = 100  // ~5 min of retries → stop and let user manually reconnect

function reconnectDelay(retryCount: number): number {
  const base = Math.min(INITIAL_DELAY * Math.pow(2, retryCount), MAX_DELAY)
  const jitter = base * JITTER * (Math.random() * 2 - 1)
  return Math.max(100, base + jitter)
}

const onReconnectCallbacks: Array<ReconnectCallback> = []
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
  if (reconnectTimer) {
    clearTimeout(reconnectTimer)
    reconnectTimer = null
  }
  if (ws) {
    ws.onopen = null
    ws.onmessage = null
    ws.onclose = null
    ws.onerror = null
    try { ws.close() } catch { /* ignore */ }
    ws = null
  }

  let url = `ws://127.0.0.1:${port}/ws`
  if (lastSeq > 0) {
    url += `?seq=${lastSeq}`
  }
  ws = new WebSocket(url)

  return new Promise<void>((resolve) => {
    connectResolvers.push(resolve)

    ws!.onopen = () => {
      retryCount = 0
      setWsStateFn?.('connected')
      setReconnectAttemptFn?.(0)
      // Flush queued RPC sends first
      for (const cb of onOpenCallbacks) cb()
      for (const cb of [...onReconnectCallbacks]) cb()
      resolveAllPending()
    }

    ws!.onmessage = (event) => {
      handleWsMessage(event.data as string)
    }

    ws!.onclose = () => {
      retryCount++
      if (retryCount <= RETRY_GIVE_UP) {
        setWsStateFn?.('reconnecting')
        setReconnectAttemptFn?.(retryCount)
        const delay = reconnectDelay(retryCount)
        reconnectTimer = setTimeout(() => {
          reconnectTimer = null
          connectWebSocket(port)
        }, delay)
      } else {
        setWsStateFn?.('disconnected')
        resolveAllPending()
      }
    }

    ws!.onerror = () => {
      // onclose fires after onerror
    }
  })
}

// Register a reconnect callback. Fired on initial connect AND on every
// reconnect. Returns an unsubscribe so callers (e.g. bootstrap) that may
// re-register on retry don't leave stale subscribers accumulating.
export function onWsReconnect(cb: ReconnectCallback): () => void {
  onReconnectCallbacks.push(cb)
  return () => {
    const idx = onReconnectCallbacks.indexOf(cb)
    if (idx >= 0) onReconnectCallbacks.splice(idx, 1)
  }
}

export function getWs(): WebSocket | null {
  return ws
}

export function disconnectWebSocket(): void {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer)
    reconnectTimer = null
  }
  if (ws) {
    ws.onclose = null
    ws.close()
    ws = null
  }
}

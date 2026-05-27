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

// Resolve held by connectWebSocket's returned Promise
let connectResolve: (() => void) | null = null

export function connectWebSocket(port: number): Promise<void> {
  // If already open, resolve immediately
  if (ws && ws.readyState === WebSocket.OPEN) return Promise.resolve()

  const url = `ws://127.0.0.1:${port}/ws`
  ws = new WebSocket(url)

  return new Promise<void>((resolve) => {
    connectResolve = resolve

    ws!.onopen = () => {
      retryDelay = 1000
      retryCount = 0
      setWsStateFn?.('connected')
      setReconnectAttemptFn?.(0)
      // Flush queued RPC sends first
      for (const cb of onOpenCallbacks) cb()
      onReconnect?.()
      resolve()
      connectResolve = null
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
    ws.close()
    ws = null
  }
}

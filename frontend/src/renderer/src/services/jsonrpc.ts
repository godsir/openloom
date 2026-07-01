import { getWs, onWsConnected, setLastSeq, updateLastMessageTime } from './websocket'
import type { JsonRpcRequest, JsonRpcResponse } from '../types/bindings'

let nextId = 1
const pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>()

// Pending sends — queued until WS is open
let sendQueue: Array<() => void> = []
let flushScheduled = false

function flushQueue(): void {
  const queue = sendQueue
  sendQueue = []
  flushScheduled = false
  for (const fn of queue) fn()
}

// Listen for WS open events to flush queued sends
onWsConnected(() => {
  flushQueue()
})

function doSend<T>(method: string, params: Record<string, unknown>): Promise<T> {
  const socket = getWs()
  const id = nextId++
  const request: JsonRpcRequest = {
    jsonrpc: '2.0',
    method,
    params,
    id,
  }

  return new Promise((resolve, reject) => {
    // chat.send can run for many minutes (agent loops up to 100 iterations).
    // Use a very long timeout; keep the pending entry so late responses still resolve.
    const timeout = method === 'chat.send' ? 1_800_000 : 30_000 // 30 min
    const timer = setTimeout(() => {
      // Don't delete from pending — if the response eventually arrives, deliver it
      const entry = pending.get(id)
      if (entry) {
        pending.delete(id)
        entry.reject(new Error(`RPC timeout: ${method}`))
      }
    }, timeout)

    pending.set(id, {
      resolve: (v: unknown) => { clearTimeout(timer); resolve(v as T) },
      reject: (e: Error) => { clearTimeout(timer); reject(e) },
    })

    try {
      socket!.send(JSON.stringify(request))
    } catch {
      clearTimeout(timer)
      pending.delete(id)
      reject(new Error(`RPC failed: WebSocket send error [${method}]`))
    }
  })
}

export function loomRpc<T = unknown>(method: string, params?: Record<string, unknown>): Promise<T> {
  const socket = getWs()

  // If socket is ready for sending, go ahead
  if (socket && socket.readyState === WebSocket.OPEN) {
    return doSend<T>(method, params ?? {})
  }

  // If it's connecting, queue the send
  if (socket && socket.readyState === WebSocket.CONNECTING) {
    return new Promise<T>((resolve, reject) => {
      sendQueue.push(() => {
        doSend<T>(method, params ?? {}).then(resolve, reject)
      })
      if (!flushScheduled) {
        flushScheduled = true
        // Also flush after a short timeout as safety
        setTimeout(flushQueue, 1000)
      }
    })
  }

  // Closed or null — reject
  return Promise.reject(
    new Error(`RPC failed: WebSocket not connected [${method}] (state: ${socket?.readyState ?? 'null'})`),
  )
}

// Notification subscriptions
type NotificationHandler = (method: string, params: unknown) => void
const subscribers = new Set<NotificationHandler>()

export function loomSubscribe(handler: NotificationHandler): () => void {
  subscribers.add(handler)
  return () => { subscribers.delete(handler) }
}

// Called by websocket.ts onmessage
export function handleWsMessage(data: string): void {
  try {
    const msg = JSON.parse(data)

    // Track sequence number for reconnection
    if (typeof msg.seq === 'number') {
      setLastSeq(msg.seq)
    }
    updateLastMessageTime()

    if ('id' in msg && msg.id != null) {
      const entry = pending.get(msg.id)
      if (entry) {
        pending.delete(msg.id)
        if (msg.error) {
          entry.reject(new Error(msg.error.message ?? 'RPC error'))
        } else {
          entry.resolve(msg.result)
        }
      }
    } else if ('method' in msg && msg.method) {
      if (msg.method.startsWith('chat.')) {
        console.warn('[ws:in]', msg.method, (msg.params as any)?.delta?.slice(0, 60) || JSON.stringify(msg.params).slice(0, 80))
      }
      for (const handler of subscribers) {
        handler(msg.method, msg.params)
      }
    }
  } catch {
    // ignore parse errors
  }
}

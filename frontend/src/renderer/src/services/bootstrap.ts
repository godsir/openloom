import {
  connectWebSocket,
  onWsReconnect,
  registerConnectionSetters,
} from './websocket'
import { loomSubscribe, loomRpc } from './jsonrpc'
import { streamBufferManager } from './stream-buffer'
import { useStore } from '../stores'

async function waitForPort(maxWait = 10000): Promise<number> {
  const start = Date.now()
  while (Date.now() - start < maxWait) {
    const port = (window as any).__enginePort__
    if (port) return port
    await new Promise((r) => setTimeout(r, 100))
  }
  throw new Error('Timeout waiting for engine port')
}

export async function bootstrapApp(): Promise<void> {
  const port = await waitForPort()
  useStore.getState().setPort(port)

  registerConnectionSetters(
    (state) => useStore.getState().setWsState(state),
    (n) => useStore.getState().setReconnectAttempt(n),
  )

  loomSubscribe((method, params) => {
    const p = params as Record<string, unknown> | undefined
    const sessionId =
      (p?.session_id as string) ||
      useStore.getState().currentSessionId ||
      'default'

    switch (method) {
      case 'chat.stream_delta':
        streamBufferManager.handleStreamDelta(sessionId, (p?.delta as string) || '')
        break
      case 'chat.stream_end':
        streamBufferManager.handleStreamEnd(sessionId)
        break
      case 'chat.token_usage':
        if (p) useStore.getState().setTokenUsage({
          prompt: (p.prompt_tokens as number) || 0,
          completion: (p.completion_tokens as number) || 0,
        })
        break
      case 'tool.started':
        streamBufferManager.handleToolStarted(sessionId, p as any)
        break
      case 'tool.completed':
        streamBufferManager.handleToolCompleted(
          sessionId, (p?.id as string) || '', p?.result as string | undefined)
        break
      case 'agent.state_changed':
        loomRpc('agent.list').then((r: any) =>
          useStore.getState().setAgents(r.agents || [])
        ).catch(() => {})
        break
    }
  })

  // onWsReconnect fires on initial connect AND on every reconnect
  onWsReconnect(async () => {
    await useStore.getState().loadSessions()
    try {
      const agents = await loomRpc<{ agents: unknown[] }>('agent.list')
      useStore.getState().setAgents(agents.agents as any[] || [])
    } catch { /* non-critical */ }
  })

  // Connect — onopen triggers onReconnect which loads data
  await connectWebSocket(port)
}

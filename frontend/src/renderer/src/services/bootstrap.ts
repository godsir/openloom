import {
  connectWebSocket,
  onWsReconnect,
  registerConnectionSetters,
} from './websocket'
import { loomSubscribe, loomRpc } from './jsonrpc'
import { streamBufferManager } from './stream-buffer'
import { useStore } from '../stores'
import { t } from '../i18n'

async function waitForPort(maxWait = 10000): Promise<number> {
  const start = Date.now()
  while (Date.now() - start < maxWait) {
    const port = (window as any).__enginePort__
    if (port) return port
    await new Promise((r) => setTimeout(r, 100))
  }
  throw new Error('Timeout waiting for engine port')
}

let _streaming = false

export async function bootstrapApp(): Promise<() => void> {
  const port = await waitForPort()
  useStore.getState().setPort(port)

  registerConnectionSetters(
    (state) => useStore.getState().setWsState(state),
    (n) => useStore.getState().setReconnectAttempt(n),
  )

  const unsub = loomSubscribe((method, params) => {
    const p = params as Record<string, unknown> | undefined
    const sessionId =
      (p?.session_id as string) ||
      useStore.getState().currentSessionId ||
      'default'

    switch (method) {
      case 'chat.stream_delta': {
        streamBufferManager.handleStreamDelta(sessionId, (p?.delta as string) || '')
        // First delta = AI started replying
        if (!_streaming) {
          _streaming = true
          import('./pet-sync').then(m => m.sendPetState('wave'))
        }
        break
      }
      case 'chat.stream_end':
        streamBufferManager.handleStreamEnd(sessionId)
        _streaming = false
        import('./pet-sync').then(m => m.sendPetState('jump'))
        setTimeout(() => import('./pet-sync').then(m => m.sendPetState('runLeft')), 1500)
        setTimeout(() => import('./pet-sync').then(m => m.sendPetState('idle')), 3000)
        // Native OS notification on task complete (if enabled in settings)
        window.loom.getPreference<boolean>('taskCompleteNotification', false).then((enabled) => {
          if (enabled) {
            const title = 'openLoom'
            const body = t('chat.aiReplied')
            window.loom.showNotification(title, body)
          }
        })
        break
      case 'chat.token_usage':
        if (p) {
          const usage = {
            prompt: (p.prompt_tokens as number) || 0,
            completion: (p.completion_tokens as number) || 0,
            model: (p.model as string) || '',
            contextWindow: (p.context_window as number) || 0,
            cached: (p.cached_tokens as number) || 0,
            cacheRead: (p.cache_read_tokens as number) || 0,
            cacheWrite: (p.cache_write_tokens as number) || 0,
          }
          useStore.getState().setSessionUsage(sessionId, usage)
          // Accumulate into session cumulative for the context ring
          useStore.getState().accumulateSessionUsage(sessionId, usage)
          // Also aggregate into token stats
          useStore.getState().recordUsage({
            session_id: sessionId,
            model: usage.model,
            prompt: usage.prompt,
            completion: usage.completion,
            cached: (p.cached_tokens as number) || 0,
            latency_ms: (p.latency_ms as number) || 0,
            context_window: usage.contextWindow,
          })
          // Also stash on the streaming assistant message so closing/reopening
          // the session can rehydrate the ring from history.
          const buf = streamBufferManager.snapshot(sessionId)
          if (buf?.messageId) {
            useStore.getState().setMessageUsage(sessionId, buf.messageId, usage)
          }
        }
        break
      case 'tool.started':
        streamBufferManager.handleToolStarted(sessionId, p as any)
        import('./pet-sync').then(m => m.sendPetState('dash'))
        break
      case 'tool.completed':
        streamBufferManager.handleToolCompleted(
          sessionId,
          (p?.id as string) || '',
          p?.result as string | undefined,
          p?.name as string | undefined,
          p?.structured_content as Record<string, unknown> | undefined,
        )
        import('./pet-sync').then(m => m.sendPetState('inspect'))
        break
      case 'agent.subagent_spawned':
        import('./pet-sync').then(m => m.sendPetState('dash'))
        break
      case 'agent.subagent_completed':
        import('./pet-sync').then(m => m.sendPetState('inspect'))
        break
      case 'agent.subagent_errored':
        import('./pet-sync').then(m => m.sendPetState('failed'))
        setTimeout(() => import('./pet-sync').then(m => m.sendPetState('idle')), 3000)
        break
      case 'tool.permission_request': {
        // Show permission confirmation dialog with three options
        const callId = p?.call_id as string
        const toolName = p?.tool_name as string
        const risk = p?.risk as string
        const toolArgs = p?.args as Record<string, unknown> | undefined
        if (callId && toolName) {
          const store = useStore.getState()
          // Format risk label with color indicator
          const riskLabel = risk === 'High'
            ? t('permissions.highRisk')
            : risk === 'Medium'
            ? t('permissions.mediumRisk')
            : ''
          // Extract key detail for context
          const detail = toolArgs
            ? String(toolArgs.path || toolArgs.command || toolArgs.file_path || toolArgs.url || '')
            : ''
          // Build structured message with line breaks
          const parts: string[] = []
          if (riskLabel) parts.push(riskLabel)
          if (detail) parts.push(t('permissions.targetPath', { path: detail }))
          parts.push(t('permissions.confirmPrompt'))
          const msg = parts.join('\n')
          store.showPermissionConfirm(
            t('permissions.toolConfirm'),
            msg,
            toolName,
            risk === 'High',
          ).then((choice) => {
            const approved = choice !== 'deny'
            const remember = choice === 'approve_always'
            loomRpc('tool.respond', { call_id: callId, approved, remember }).catch((e) => {
              console.error('[perm] tool.respond failed:', e)
            })
          }).catch(() => {
            loomRpc('tool.respond', { call_id: callId, approved: false, remember: false }).catch((e) => {
              console.error('[perm] tool.respond fallback failed:', e)
            })
          })
        }
        break
      }
      case 'agent.state_changed':
        loomRpc('agent.config.list').then((r: any) =>
          useStore.getState().setAgents(r.configs || [])
        ).catch(() => {})
        break
      case 'todo.list_replaced':
        useStore.getState().handleTodoReplaced((p?.todos as any[]) || [])
        break
      case 'plan.created':
      case 'plan.updated':
        // Plan content changed — reload todos so the panel reflects
        // checkbox changes synced by the backend.
        useStore.getState().loadTodos(sessionId).catch(() => {})
        break
    }
  })

  // onWsReconnect fires on initial connect AND on every reconnect
  const unsubReconnect = onWsReconnect(async () => {
    await useStore.getState().loadSessions()
    try {
      const configs = await loomRpc<{ configs: unknown[] }>('agent.config.list')
      useStore.getState().setAgents(configs.configs as any[] || [])
    } catch { /* non-critical */ }
  })

  // Connect — onopen triggers onReconnect which loads data
  await connectWebSocket(port)
  useStore.getState().setEngineState('running')

  return () => {
    unsub()
    unsubReconnect()
  }
}


import {
  connectWebSocket,
  onWsReconnect,
  registerConnectionSetters,
} from './websocket'
import { loomSubscribe, loomRpc } from './jsonrpc'
import { streamBufferManager } from './stream-buffer'
import { useStore } from '../stores'
import { useIMStore } from '../stores/im'
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

/**
 * 处理 chat.token_usage 通知：累计 session usage、记录 token 统计、
 * 并把 token 消耗写到当前流式 assistant 消息上（消息底部显示）。
 * 普通 ws 与 IM stream-event 共用此逻辑。
 */
function handleTokenUsage(sessionId: string, p: any) {
  if (!p) return
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

export async function bootstrapApp(): Promise<() => void> {
  const port = await waitForPort()
  useStore.getState().setPort(port)

  registerConnectionSetters(
    (state) => useStore.getState().setWsState(state),
    (n) => useStore.getState().setReconnectAttempt(n),
  )

  const unsub = loomSubscribe((method, params) => {
    const p = params as Record<string, unknown> | undefined
    // Process/monitor events are emitted from background sessions
    // (e.g. persistent game stream). The session_id in the params
    // tells us which session they belong to. For all other events,
    // fall back to currentSessionId.
    const sessionId =
      (p?.session_id as string) ||
      useStore.getState().currentSessionId || 'default'

    switch (method) {
      case 'chat.stream_delta': {
        const childName = (p as any)?.child_name as string | undefined
        if (childName) {
          // Team subagent delta — route to SubagentCard block, not captain text
          const store = useStore.getState()
          const sid = (p?.session_id as string) || ''
          const msgs = store.messagesBySession.get(sid) || []
          const lastAsst = [...msgs].reverse().find((m: any) => m.role === 'assistant')
          if (lastAsst) {
            const block = lastAsst.blocks.find((b: any) => b.type === 'subagent' && b.id === 'sub_' + childName)
            const prevBody = (block?.body as string) || ''
            store.upsertBlock(sid, lastAsst.id, {
              type: 'subagent',
              id: 'sub_' + childName,
              name: childName,
              streamStatus: 'running',
              body: prevBody + ((p as any)?.delta || ''),
            })
          }
        } else {
          streamBufferManager.handleStreamDelta(sessionId, (p?.delta as string) || '')
        }
        if (!_streaming) { _streaming = true; import('./pet-sync').then(m => m.sendPetState('wave')) }
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
        handleTokenUsage(sessionId, p)
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
      case 'agent.subagent_spawned': {
        import('./pet-sync').then(m => m.sendPetState('dash'))
        const store = useStore.getState()
        const sid = [...store.streamingSessionIds][0]
        if (!sid) break
        const msgs = store.messagesBySession.get(sid)
        const lastAsst = [...(msgs || [])].reverse().find((m: any) => m.role === 'assistant')
        if (!lastAsst) break
        const childName = (p as any)?.child_name || 'subagent'
        // Create a subagent block in the chat for this member
        store.upsertBlock(sid, lastAsst.id, {
          type: 'subagent',
          id: 'sub_' + childName,
          name: childName,
          streamStatus: 'running',
          body: '',
        })
        // Update team card members list
        if (store.sessionTeamBindings[sid]) {
          const teamId = store.sessionTeamBindings[sid]
          const teamConfig = store.teams.find((t: any) => t.id === teamId)
          const teamBlock = lastAsst.blocks.find((b: any) => b.type === 'team') as any
          const members = (teamBlock?.members || []) as any[]
          members.push({ name: childName, status: 'running' })
          store.upsertBlock(sid, lastAsst.id, { type: 'team', teamName: teamConfig?.name || '专家团', members })
        }
        break
      }
      case 'agent.subagent_completed': {
        import('./pet-sync').then(m => m.sendPetState('inspect'))
        const store2 = useStore.getState()
        const sid2 = [...store2.streamingSessionIds][0]
        if (!sid2) break
        const msgs2 = store2.messagesBySession.get(sid2)
        const lastAsst2 = [...(msgs2 || [])].reverse().find((m: any) => m.role === 'assistant')
        if (!lastAsst2) break
        const childName2 = (p as any)?.child_name || ''
        const result2 = String((p as any)?.result || '')
        // Mark subagent block done
        const subBlock = lastAsst2.blocks.find((b: any) => b.type === 'subagent' && b.id === 'sub_' + childName2)
        store2.upsertBlock(sid2, lastAsst2.id, {
          type: 'subagent',
          id: 'sub_' + childName2,
          name: childName2,
          streamStatus: 'done',
          body: subBlock?.body || '',
          summary: result2.slice(0, 120),
        })
        // Update team card
        if (store2.sessionTeamBindings[sid2]) {
          const teamId2 = store2.sessionTeamBindings[sid2]
          const teamConfig2 = store2.teams.find((t: any) => t.id === teamId2)
          const teamBlock2 = lastAsst2.blocks.find((b: any) => b.type === 'team') as any
          const members2 = ((teamBlock2?.members || []) as any[]).map((m: any) =>
            m.name === childName2 ? { ...m, status: 'done' as const, summary: result2.slice(0, 120) } : m
          )
          store2.upsertBlock(sid2, lastAsst2.id, { type: 'team', teamName: teamConfig2?.name || '专家团', members: members2 })
        }
        break
      }
      case 'team.member_started': {
        const store4 = useStore.getState()
        const sid4 = ((p as any)?.session_id as string) || [...store4.streamingSessionIds][0] || ''
        if (!sid4) break
        const msgs4 = store4.messagesBySession.get(sid4) || []
        const last4 = [...msgs4].reverse().find((m: any) => m.role === 'assistant')
        if (!last4) break
        const mName = (p as any)?.member_name || ''
        store4.upsertBlock(sid4, last4.id, {
          type: 'subagent', id: 'sub_' + mName, name: mName,
          streamStatus: 'running', body: '',
        })
        // Also update team card
        if (store4.sessionTeamBindings[sid4]) {
          const teamId4 = store4.sessionTeamBindings[sid4]
          const teamConfig4 = store4.teams.find((t: any) => t.id === teamId4)
          const teamBlock4 = last4.blocks.find((b: any) => b.type === 'team') as any
          const members4 = (teamBlock4?.members || []) as any[]
          if (!members4.find((m: any) => m.name === mName)) {
            members4.push({ name: mName, status: 'running' })
          }
          store4.upsertBlock(sid4, last4.id, { type: 'team', teamName: teamConfig4?.name || '专家团', members: members4 })
        }
        break
      }
      case 'team.member_delta': {
        const store3 = useStore.getState()
        const sid3 = ((p as any)?.session_id as string) || [...store3.streamingSessionIds][0] || ''
        if (!sid3) break
        const msgs3 = store3.messagesBySession.get(sid3) || []
        const last3 = [...msgs3].reverse().find((m: any) => m.role === 'assistant')
        if (!last3) break
        const memberName = (p as any)?.member_name || ''
        const block = last3.blocks.find((b: any) => b.type === 'subagent' && b.id === 'sub_' + memberName)
        const prev = (block?.body as string) || ''
        store3.upsertBlock(sid3, last3.id, {
          type: 'subagent', id: 'sub_' + memberName, name: memberName,
          streamStatus: 'running', body: prev + ((p as any)?.delta || ''),
        })
        break
      }
      case 'agent.subagent_errored':
        import('./pet-sync').then(m => m.sendPetState('failed'))
        setTimeout(() => import('./pet-sync').then(m => m.sendPetState('idle')), 3000)
        break
      case 'team.started': {
        const store = useStore.getState()
        const sid = [...store.streamingSessionIds][0] || ''
        if (sid) store.setStreamingActivity(sid, { phase: 'team', detail: (p?.team_name as string) || '' })
        import('./pet-sync').then(m => m.sendPetState('dash'))
        break
      }
      case 'team.member_done': {
        const store = useStore.getState()
        const sid = [...store.streamingSessionIds][0] || ''
        if (sid) store.setStreamingActivity(sid, { phase: 'team', detail: `${p?.member_name || ''}: 第${p?.round || ''}轮完成` })
        break
      }
      case 'team.round_complete': {
        const store = useStore.getState()
        const sid = [...store.streamingSessionIds][0] || ''
        if (sid) store.setStreamingActivity(sid, { phase: 'team', detail: `第${p?.round || ''}轮完成` })
        break
      }
      case 'team.completed': {
        const sid = (p?.session_id as string) || [...useStore.getState().streamingSessionIds][0] || ''
        if (sid) useStore.getState().setStreamingActivity(sid, { phase: 'generating', detail: '' })
        break
      }
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
      case 'process.output':
        streamBufferManager.handleProcessOutput(
          sessionId,
          (p?.pid as string) || '',
          (p?.data as string) || '',
          (p?.stream as string) || 'stdout',
        )
        break
      case 'process.exited':
        streamBufferManager.handleProcessExited(
          sessionId,
          (p?.pid as string) || '',
          (p?.exit_code as number) ?? -1,
        )
        break
      case 'monitor.started':
        // Monitor started — mark session as streaming so the Dynamic Island reacts
        if (sessionId) {
          useStore.getState().addStreamingSession(sessionId)
          useStore.getState().setStreamingActivity(sessionId, { phase: 'tool', detail: 'monitor' })
        }
        break
      case 'monitor.output':
        streamBufferManager.handleProcessOutput(
          sessionId,
          (p?.monitor_id as string) || '',
          (p?.data as string) || '',
          (p?.stream as string) || 'stdout',
        )
        break
      case 'monitor.exited':
        streamBufferManager.handleProcessExited(
          sessionId,
          (p?.monitor_id as string) || '',
          (p?.exit_code as number) ?? -1,
        )
        break
      case 'monitor.error': {
        // Monitor error — show inline error notification in the chat
        const errMsg = (p?.error as string) || 'Monitor error'
        const mid = (p?.monitor_id as string) || ''
        streamBufferManager.handleProcessOutput(
          sessionId,
          mid,
          `[Monitor 错误] ${errMsg}`,
          'stderr',
        )
        streamBufferManager.handleProcessExited(sessionId, mid, -1)
        break
      }
      case 'ws.replay_done':
        console.log('[ws] event replay complete:', p)
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
    useIMStore.getState().loadSessionBindings()
  })

  // IM bridge created a session → refresh the sidebar + IM session bindings
  const unsubImSession = window.loom.onIMSessionChanged(() => {
    useStore.getState().loadSessions()
    useIMStore.getState().loadSessionBindings()
  })

  // IM bridge forwards engine streaming events → Dynamic Island reacts
  const unsubImStream = window.loom.onIMStreamEvent((data: { method: string; params: Record<string, unknown> }) => {
    const p = data.params
    const sessionId = (p?.session_id as string) || ''
    if (!sessionId) return

    switch (data.method) {
      case 'chat.stream_delta':
        // Only handle IM-originated sessions; regular chats arrive via loomSubscribe.
        streamBufferManager.handleStreamDeltaIM(sessionId, (p?.delta as string) || '')
        break
      case 'chat.stream_end':
        streamBufferManager.handleStreamEnd(sessionId)
        break
      case 'chat.token_usage':
        handleTokenUsage(sessionId, p)
        break
      case 'tool.started':
        streamBufferManager.handleToolStarted(sessionId, p as any)
        break
      case 'tool.completed':
        streamBufferManager.handleToolCompleted(
          sessionId,
          (p?.id as string) || '',
          p?.result as string | undefined,
          p?.name as string | undefined,
        )
        break
      case 'process.output':
        streamBufferManager.handleProcessOutput(
          sessionId,
          (p?.pid as string) || '',
          (p?.data as string) || '',
          (p?.stream as string) || 'stdout',
        )
        break
      case 'process.exited':
        streamBufferManager.handleProcessExited(
          sessionId,
          (p?.pid as string) || '',
          (p?.exit_code as number) ?? -1,
        )
        break
      case 'monitor.started':
        if (sessionId) {
          useStore.getState().addStreamingSession(sessionId)
          useStore.getState().setStreamingActivity(sessionId, { phase: 'tool', detail: 'monitor' })
        }
        break
      case 'monitor.output':
        streamBufferManager.handleProcessOutput(
          sessionId,
          (p?.monitor_id as string) || '',
          (p?.data as string) || '',
          (p?.stream as string) || 'stdout',
        )
        break
      case 'monitor.exited':
        streamBufferManager.handleProcessExited(
          sessionId,
          (p?.monitor_id as string) || '',
          (p?.exit_code as number) ?? -1,
        )
        break
      case 'monitor.error': {
        const errMsg = (p?.error as string) || 'Monitor error'
        const mid = (p?.monitor_id as string) || ''
        streamBufferManager.handleProcessOutput(
          sessionId,
          mid,
          `[Monitor 错误] ${errMsg}`,
          'stderr',
        )
        streamBufferManager.handleProcessExited(sessionId, mid, -1)
        break
      }
      case 'push_notification':
        // AI 主动推送桌面通知
        window.loom.getPreference<boolean>('taskCompleteNotification', true).then((enabled: boolean) => {
          if (enabled) {
            window.loom.showNotification(
              (p?.title as string) || 'openLoom',
              (p?.body as string) || 'AI 发来一条通知',
            )
          }
        })
        break
      case 'review_findings':
        // AI 上报代码审查发现 — 展示 toast + 附加到聊天
        if (sessionId && p?.findings) {
          const arr = Array.isArray(p.findings) ? p.findings as any[] : []
          const criticals = arr.filter((f: any) => f.severity === 'critical').length
          const msg = `发现 ${arr.length} 个问题${criticals > 0 ? `（${criticals} 严重）` : ''}`
          useStore.getState().addToast({ type: criticals > 0 ? 'error' : 'info', message: msg })
          useStore.getState().appendMessage(sessionId, {
            id: crypto.randomUUID(),
            role: 'assistant',
            blocks: [{
              type: 'text',
              html: arr.map((f: any, i: number) => {
                const s = f.severity === 'critical' ? 'CRIT' : f.severity === 'important' ? 'IMPT' : 'MINOR'
                return `${i+1}. [${s}] ${f.file || '?'}:${f.line || '-'} — ${f.summary || ''}`
              }).join('<br>'),
              source: JSON.stringify(p.findings),
            }],
            timestamp: new Date().toISOString(),
          })
        }
        break
      case 'ws.replay_done':
        // replay_done is internal WS protocol, not relevant for IM
        break
    }
  })

  // Connect — onopen triggers onReconnect which loads data
  await connectWebSocket(port)
  useStore.getState().setEngineState('running')

  return () => {
    unsub()
    unsubReconnect()
    unsubImSession()
    unsubImStream()
  }
}


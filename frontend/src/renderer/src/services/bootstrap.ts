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

function applyPreference(key: string, val: unknown) {
  if (key === 'theme') {
    const themeVal = String(val)
    useStore.getState().setTheme(themeVal as any)
    window.loom.setPreference('theme', themeVal)
  } else if (key === 'font_size') {
    useStore.getState().setFontSize(String(val) as any)
    window.loom.setPreference('fontSize', String(val))
  } else if (key === 'language') {
    const langVal = String(val)
    localStorage.setItem('loom-locale', langVal)
    document.documentElement.lang = langVal
    window.dispatchEvent(new CustomEvent('loom-locale-changed', { detail: langVal }))
    window.loom.setPreference('language', langVal)
  } else if (key === 'app_zoom') {
    const zoom = Number(val) || 1
    document.documentElement.style.setProperty('--app-zoom', String(zoom))
    window.loom.setPreference('appZoom', zoom)
  } else if (key === 'permission_mode') {
    useStore.getState().setPermissionMode(String(val) as any)
  } else if (key === 'thinking_level') {
    useStore.getState().setThinkingLevel(String(val) as any)
  } else if (key === 'send_shortcut') {
    useStore.getState().setSendShortcut(String(val) as any)
  } else if (key === 'fim_enabled') {
    const v = Boolean(val)
    useStore.getState().setFimEnabled(v)
    window.loom.setPreference('fimEnabled', v)
  } else if (key === 'thinking_expand') {
    window.loom.setPreference('thinkingExpandDefault', Boolean(val))
    window.dispatchEvent(new CustomEvent('loom-pref-changed', { detail: { key: 'thinking_expand', val: Boolean(val) } }))
  } else if (key === 'tool_expand') {
    window.loom.setPreference('toolExpandDefault', Boolean(val))
    window.dispatchEvent(new CustomEvent('loom-pref-changed', { detail: { key: 'tool_expand', val: Boolean(val) } }))
  } else if (key === 'skill_expand') {
    window.loom.setPreference('skillExpandDefault', Boolean(val))
    window.dispatchEvent(new CustomEvent('loom-pref-changed', { detail: { key: 'skill_expand', val: Boolean(val) } }))
  } else if (key === 'work_block_expand') {
    window.loom.setPreference('workBlockExpandDefault', Boolean(val))
    window.dispatchEvent(new CustomEvent('loom-pref-changed', { detail: { key: 'work_block_expand', val: Boolean(val) } }))
  } else if (key === 'task_notification') {
    window.loom.setPreference('taskCompleteNotification', Boolean(val))
    window.dispatchEvent(new CustomEvent('loom-pref-changed', { detail: { key: 'task_notification', val: Boolean(val) } }))
  } else if (key === 'auto_start') {
    window.loom.setPreference('autoStart', Boolean(val))
  } else if (key === 'auto_title') {
    window.loom.setPreference('autoTitle', Boolean(val))
  } else if (key === 'close_to_tray') {
    window.loom.setPreference('closeToTray', Boolean(val))
  } else if (key === 'start_to_tray') {
    window.loom.setPreference('startToTray', Boolean(val))
  } else if (key === 'disable_hw_accel') {
    window.loom.setPreference('disableHardwareAcceleration', Boolean(val))
  } else if (key === 'ui_font') {
    const font = String(val)
    if (font) {
      document.documentElement.style.setProperty('--font', font)
      if (font.includes('KaiTi') || font.includes('楷体')) {
        document.documentElement.style.setProperty('-webkit-text-stroke', '0.35px')
      } else {
        document.documentElement.style.removeProperty('-webkit-text-stroke')
      }
    } else {
      document.documentElement.style.removeProperty('--font')
      document.documentElement.style.removeProperty('-webkit-text-stroke')
    }
    window.loom.setPreference('uiFont', font)
  } else if (key === 'code_font') {
    const font = String(val)
    if (font) {
      document.documentElement.style.setProperty('--font-mono', font)
    } else {
      document.documentElement.style.removeProperty('--font-mono')
    }
    window.loom.setPreference('codeFont', font)
  } else if (key === 'custom_colors') {
    const cc = val as Record<string, unknown> | null
    if (cc && typeof cc.bg === 'string' && typeof cc.surface === 'string'
         && typeof cc.text === 'string' && typeof cc.accent === 'string') {
      const hexToRgb = (hex: string): [number, number, number] => {
        const v = parseInt(String(hex).replace('#', ''), 16)
        return [(v >> 16) & 255, (v >> 8) & 255, v & 255]
      }
      const root = document.documentElement
      const [ar, ag, ab] = hexToRgb(cc.accent)
      root.style.setProperty('--bg', cc.bg)
      root.style.setProperty('--bg-surface', cc.surface)
      root.style.setProperty('--bg-card', cc.surface)
      root.style.setProperty('--text', cc.text)
      root.style.setProperty('--accent', cc.accent)
      root.style.setProperty('--accent-rgb', `${ar},${ag},${ab}`)
      root.style.setProperty('--accent-subtle', `rgba(${ar},${ag},${ab},0.10)`)
      root.style.setProperty('--accent-medium', `rgba(${ar},${ag},${ab},0.16)`)
      root.style.setProperty('--border-accent', `rgba(${ar},${ag},${ab},0.28)`)
      window.loom.setPreference('customTheme', cc)
    }
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
      case 'tool.started': {
        const ts = p?.session_id as string | undefined
        if (!ts) break
        streamBufferManager.handleToolStarted(ts, p as any)
        import('./pet-sync').then(m => m.sendPetState('dash'))
        break
      }
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
      case 'tool.output': {
        const to = p?.session_id as string | undefined
        if (!to) break
        streamBufferManager.handleToolOutput(
          to,
          (p?.id as string) || '',
          (p?.line as string) || '',
        )
        break
      }
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
        // Mark subagent block done — read existing block to preserve token data
        // that may have been written by team.member_done
        const subBlock = lastAsst2.blocks.find((b: any) => b.type === 'subagent' && b.id === 'sub_' + childName2) as any
        const existingTokens = {
          promptTokens: subBlock?.promptTokens || 0,
          completionTokens: subBlock?.completionTokens || 0,
        }
        store2.upsertBlock(sid2, lastAsst2.id, {
          type: 'subagent',
          id: 'sub_' + childName2,
          name: childName2,
          streamStatus: 'done',
          body: subBlock?.body || '',
          summary: result2.slice(0, 120),
          ...existingTokens,
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
        if (sid) streamBufferManager.setOverrideActivity(sid, { phase: 'team', detail: (p?.team_name as string) || '' })
        import('./pet-sync').then(m => m.sendPetState('dash'))
        break
      }
      case 'team.member_done': {
        const store = useStore.getState()
        const sid = [...store.streamingSessionIds][0] || ''
        if (sid) streamBufferManager.setOverrideActivity(sid, { phase: 'team', detail: `${p?.member_name || ''}: 第${p?.round || ''}轮完成` })
        // Update SubagentCard with token usage
        const msgsDone = store.messagesBySession.get(sid) || []
        const lastDone = [...msgsDone].reverse().find((m: any) => m.role === 'assistant')
        if (lastDone) {
          const mName = (p?.member_name as string) || ''
          const promptTokens = (p?.prompt_tokens as number) || 0
          const completionTokens = (p?.completion_tokens as number) || 0
          const subId = 'sub_' + mName
          const existingBlock = lastDone.blocks.find((b: any) => b.type === 'subagent' && (b as any).id === subId) as any
          store.upsertBlock(sid, lastDone.id, {
            type: 'subagent',
            id: subId,
            name: mName,
            streamStatus: 'done',
            body: existingBlock?.body || '',
            summary: existingBlock?.summary || '',
            promptTokens,
            completionTokens,
          })
        }
        break
      }
      case 'team.round_complete': {
        const store = useStore.getState()
        const sid = [...store.streamingSessionIds][0] || ''
        if (sid) streamBufferManager.setOverrideActivity(sid, { phase: 'team', detail: `第${p?.round || ''}轮完成` })
        break
      }
      case 'team.completed': {
        const sid = (p?.session_id as string) || [...useStore.getState().streamingSessionIds][0] || ''
        if (sid) streamBufferManager.setOverrideActivity(sid, null)
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
      case 'plan.updated': {
        // Plan content changed — reload todos so the panel reflects
        // checkbox changes synced by the backend.
        useStore.getState().loadTodos(sessionId).catch(() => {})
        // 同时刷新计划列表，让 PlanPanel 及时看到新建/更新的计划（B1）
        const planStore = useStore.getState()
        const planWsRoot = sessionId
          ? ((planStore as any).sessionWorkspaces?.[sessionId] || (planStore as any).defaultWorkspace || '')
          : ((planStore as any).defaultWorkspace || '')
        if (planWsRoot) planStore.loadPlans(planWsRoot).catch(() => {})
        break
      }
      case 'process.output': {
        const po = p?.session_id as string | undefined
        if (!po) break
        streamBufferManager.handleProcessOutput(
          po,
          (p?.pid as string) || '',
          (p?.data as string) || '',
          (p?.stream as string) || 'stdout',
        )
        break
      }
      case 'process.exited': {
        const pe = p?.session_id as string | undefined
        if (!pe) break
        streamBufferManager.handleProcessExited(
          pe,
          (p?.pid as string) || '',
          (p?.exit_code as number) ?? -1,
        )
        break
      }
      case 'monitor.started':
        // Just mark the session as streaming. The actual activity comes from
        // processAcc populated by monitor.output → handleProcessOutput,
        // which deriveActivity now checks for running processes.
        if (p?.session_id) {
          useStore.getState().addStreamingSession(p.session_id as string)
        }
        break
      case 'monitor.output': {
        const mo = p?.session_id as string | undefined
        if (!mo) break
        streamBufferManager.handleProcessOutput(
          mo,
          (p?.monitor_id as string) || '',
          (p?.data as string) || '',
          (p?.stream as string) || 'stdout',
        )
        break
      }
      case 'monitor.exited': {
        const me = p?.session_id as string | undefined
        if (!me) break
        streamBufferManager.handleProcessExited(
          me,
          (p?.monitor_id as string) || '',
          (p?.exit_code as number) ?? -1,
        )
        break
      }
      case 'monitor.error': {
        const me2 = p?.session_id as string | undefined
        if (!me2) break
        const errMsg = (p?.error as string) || 'Monitor error'
        const mid = (p?.monitor_id as string) || ''
        streamBufferManager.handleProcessOutput(
          me2,
          mid,
          `[Monitor 错误] ${errMsg}`,
          'stderr',
        )
        streamBufferManager.handleProcessExited(me2, mid, -1)
        break
      }
      case 'steering.queued': {
        // User steering added to queue — add to store item list
        const sid2 = (p?.session_id as string) || ''
        const item = p?.item as { id: string; text: string } | undefined
        if (sid2 && item) useStore.getState().addSteeringItem(sid2, item)
        break
      }
      case 'steering.consumed': {
        // Backend consumed steering items — remove from queue, insert into chat
        const sid2 = (p?.session_id as string) || ''
        const items = (p?.items as Array<{ id: string; text: string }>) || []
        const remaining = (p?.remaining_count as number) || 0
        if (sid2 && items.length > 0) {
          const store = useStore.getState()
          const ids = items.map(it => it.id)
          store.removeSteeringItems(sid2, ids)
          store.setSteeringQueueCount(sid2, remaining)
          // When items are consumed, insert them as user messages into chat
          const plainEscapeHtml = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
          store.ensureSession(sid2)
          for (const it of items) {
            store.appendMessage(sid2, {
              id: crypto.randomUUID(),
              role: 'user',
              blocks: [{ type: 'text', html: plainEscapeHtml(it.text), source: it.text, isSteering: true }],
              timestamp: new Date().toISOString(),
            })
          }
        }
        break
      }
      case 'memory.extraction_started':
        // Show extraction phase in dynamic island while async pipeline runs
        useStore.getState().addStreamingSession(sessionId)
        useStore.getState().setStreamingActivity(sessionId, { phase: 'extracting' })
        break
      case 'memory.updated':
        // Backend finished entity extraction — refresh KG node list so the
        // star graph panel picks up new entities without manual reload.
        // Also clear the extracting state from the dynamic island.
        useStore.getState().removeStreamingSession(sessionId)
        import('../stores').then(({ useStore }) => {
          useStore.getState().kgListNodes()
        })
        break
      case 'preferences.changed': {
        const updates = (p?.updates ?? {}) as Record<string, unknown>
        for (const [key, val] of Object.entries(updates)) applyPreference(key, val)
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
      case 'tool.output':
        streamBufferManager.handleToolOutput(
          sessionId,
          (p?.id as string) || '',
          (p?.line as string) || '',
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
      case 'steering.queued': {
        const sid3 = (p?.session_id as string) || ''
        const item = p?.item as { id: string; text: string } | undefined
        if (sid3 && item) useStore.getState().addSteeringItem(sid3, item)
        break
      }
      case 'steering.consumed': {
        const sid3 = (p?.session_id as string) || ''
        const items = (p?.items as Array<{ id: string; text: string }>) || []
        const remaining2 = (p?.remaining_count as number) || 0
        if (sid3 && items.length > 0) {
          const store = useStore.getState()
          const ids = items.map(it => it.id)
          store.removeSteeringItems(sid3, ids)
          store.setSteeringQueueCount(sid3, remaining2)
          const plainEscapeHtml = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
          store.ensureSession(sid3)
          for (const it of items) {
            store.appendMessage(sid3, {
              id: crypto.randomUUID(),
              role: 'user',
              blocks: [{ type: 'text', html: plainEscapeHtml(it.text), source: it.text, isSteering: true }],
              timestamp: new Date().toISOString(),
            })
          }
        }
        break
      }
      case 'memory.extraction_started':
        // Show extraction phase in dynamic island while async pipeline runs
        useStore.getState().addStreamingSession(sessionId)
        useStore.getState().setStreamingActivity(sessionId, { phase: 'extracting' })
        break
      case 'memory.updated':
        // Backend finished entity extraction — refresh KG node list so the
        // star graph panel picks up new entities without manual reload.
        // Also clear the extracting state from the dynamic island.
        useStore.getState().removeStreamingSession(sessionId)
        import('../stores').then(({ useStore }) => {
          useStore.getState().kgListNodes()
        })
        break
      case 'preferences.changed': {
        const updates = p?.updates as Record<string, unknown> || {}
        for (const [key, val] of Object.entries(updates)) {
          if (key === 'theme') {
            const themeVal = String(val)
            useStore.getState().setTheme(themeVal as any)
            window.loom.setPreference('theme', themeVal)
          } else if (key === 'font_size') {
            useStore.getState().setFontSize(String(val) as any)
            window.loom.setPreference('fontSize', String(val))
          } else if (key === 'language') {
            const langVal = String(val)
            localStorage.setItem('loom-locale', langVal)
            document.documentElement.lang = langVal
            window.dispatchEvent(new CustomEvent('loom-locale-changed', { detail: langVal }))
            window.loom.setPreference('language', langVal)
          } else if (key === 'app_zoom') {
            const zoom = Number(val) || 1
            document.documentElement.style.setProperty('--app-zoom', String(zoom))
            window.loom.setPreference('appZoom', zoom)
          } else if (key === 'permission_mode') {
            const modeVal = String(val) as any
            useStore.getState().setPermissionMode(modeVal)
          } else if (key === 'thinking_level') {
            const levelVal = String(val) as any
            useStore.getState().setThinkingLevel(levelVal)
          } else if (key === 'send_shortcut') {
            const shortcutVal = String(val) as any
            useStore.getState().setSendShortcut(shortcutVal)
          } else if (key === 'fim_enabled') {
            const enabledVal = Boolean(val)
            useStore.getState().setFimEnabled(enabledVal)
            window.loom.setPreference('fimEnabled', enabledVal)
          } else if (key === 'thinking_expand') {
            window.loom.setPreference('thinkingExpandDefault', Boolean(val))
            window.dispatchEvent(new CustomEvent('loom-pref-changed', { detail: { key: 'thinking_expand', val: Boolean(val) } }))
          } else if (key === 'tool_expand') {
            window.loom.setPreference('toolExpandDefault', Boolean(val))
            window.dispatchEvent(new CustomEvent('loom-pref-changed', { detail: { key: 'tool_expand', val: Boolean(val) } }))
          } else if (key === 'skill_expand') {
            window.loom.setPreference('skillExpandDefault', Boolean(val))
            window.dispatchEvent(new CustomEvent('loom-pref-changed', { detail: { key: 'skill_expand', val: Boolean(val) } }))
          } else if (key === 'task_notification') {
            window.loom.setPreference('taskCompleteNotification', Boolean(val))
            window.dispatchEvent(new CustomEvent('loom-pref-changed', { detail: { key: 'task_notification', val: Boolean(val) } }))
          } else if (key === 'auto_start') {
            window.loom.setPreference('autoStart', Boolean(val))
          } else if (key === 'auto_title') {
            window.loom.setPreference('autoTitle', Boolean(val))
          } else if (key === 'close_to_tray') {
            window.loom.setPreference('closeToTray', Boolean(val))
          } else if (key === 'start_to_tray') {
            window.loom.setPreference('startToTray', Boolean(val))
          } else if (key === 'disable_hw_accel') {
            window.loom.setPreference('disableHardwareAcceleration', Boolean(val))
          } else if (key === 'ui_font') {
            const font = String(val)
            if (font) {
              document.documentElement.style.setProperty('--font', font)
              if (font.includes('KaiTi') || font.includes('楷体')) {
                document.documentElement.style.setProperty('-webkit-text-stroke', '0.35px')
              } else {
                document.documentElement.style.removeProperty('-webkit-text-stroke')
              }
            } else {
              document.documentElement.style.removeProperty('--font')
              document.documentElement.style.removeProperty('-webkit-text-stroke')
            }
            window.loom.setPreference('uiFont', font)
          } else if (key === 'code_font') {
            const font = String(val)
            if (font) {
              document.documentElement.style.setProperty('--font-mono', font)
            } else {
              document.documentElement.style.removeProperty('--font-mono')
            }
            window.loom.setPreference('codeFont', font)
          } else if (key === 'custom_colors') {
            const cc = val as Record<string, unknown> | null
            if (cc && typeof cc.bg === 'string' && typeof cc.surface === 'string'
                 && typeof cc.text === 'string' && typeof cc.accent === 'string') {
              const hexToRgb = (hex: string): [number, number, number] => {
                const v = parseInt(String(hex).replace('#', ''), 16)
                return [(v >> 16) & 255, (v >> 8) & 255, v & 255]
              }
              const root = document.documentElement
              const [ar, ag, ab] = hexToRgb(cc.accent)
              root.style.setProperty('--bg', cc.bg)
              root.style.setProperty('--bg-surface', cc.surface)
              root.style.setProperty('--bg-card', cc.surface)
              root.style.setProperty('--text', cc.text)
              root.style.setProperty('--accent', cc.accent)
              root.style.setProperty('--accent-rgb', `${ar},${ag},${ab}`)
              root.style.setProperty('--accent-subtle', `rgba(${ar},${ag},${ab},0.10)`)
              root.style.setProperty('--accent-medium', `rgba(${ar},${ag},${ab},0.16)`)
              root.style.setProperty('--border-accent', `rgba(${ar},${ag},${ab},0.28)`)
              window.loom.setPreference('customTheme', cc)
            }
          }
        }
        break
      }
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


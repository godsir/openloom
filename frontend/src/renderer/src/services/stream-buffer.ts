import { useStore } from '../stores'
import type { StreamingActivity } from '../stores/streaming'
import { parseContentParts } from '../stores/session'
import { loomRpc } from './jsonrpc'
import { renderMarkdown } from '../utils/markdown'
import { sanitizeHtml } from '../utils/markdown-sanitizer'
import { unclosedFenceStart } from '../utils/markdown-fence'
import { t } from '../i18n'

/** Minimum interval between full markdown-it re-renders of the stable prefix
 *  while streaming. Between re-renders, newly completed text is shown as escaped
 *  plain text (imperceptible for <100ms), bounding markdown work to ~11 passes/s
 *  instead of once per animation frame — the main fix for long-reply render jank. */
const MD_RERENDER_THROTTLE_MS = 90

interface BufferState {
  messageId: string | null
  /** Monotonic generation counter — bumped on each startStream.
   *  WS events from a previous turn (stale generation) are silently dropped
   *  so a late-arriving stream_end from a cancelled turn cannot terminate
   *  the new turn that replaced it. */
  generation: number
  textAcc: string
  thinkingAcc: string
  imageAcc: Array<{ mimeType: string; data: string }>
  processAcc: Array<{
    pid: string
    lines: Array<{ stream: string; text: string }>
    exited: boolean
    exitCode: number | null
  }>
  skillCalls: Array<{
    id: string
    name: string
    status: 'running' | 'done'
    args: Record<string, unknown>
    result?: string
  }>
  shellCalls: Array<{
    id: string
    name: string
    status: 'running' | 'done'
    args: Record<string, unknown>
    result?: string
    details?: Record<string, unknown>
  }>
  userSkills: string[]
  inThinking: boolean
  inVision: boolean
  visionDone: boolean
  visionBatches: Array<{
    batchIndex: number
    totalBatches: number
    status: 'running' | 'done' | 'error'
    result?: string
  }>
  rafId: number | null
  imOrigin: boolean
  /** External activity override set by bootstrap (team, monitor) — survives flush
   *  cycles so team/monitor phases don't get overwritten by deriveActivity. */
  overrideActivity: StreamingActivity | null
  /** Stable-prefix markdown cache (see renderMarkdown). textAcc only ever grows
   *  by appending, so the rendered stable prefix stays valid across flushes. */
  mdStable: string
  mdStableHtml: string
  mdLastRenderAt: number
  /** 正在流式输出的"未闭合代码围栏块"的节流渲染缓存（见 renderMarkdown）。
   *  围栏内的新行必须渲染在代码块内部，不能当纯文本拼在已闭合的 </code> 之后。 */
  mdFenceHtml: string
}

class StreamBufferManager {
 private buffers = new Map<string, BufferState>()
 /** Sessions whose deltas arrived via main WebSocket (loomSubscribe).
  *  Duplicate deltas forwarded by ImBridge will be skipped. */
 private wsStreamSessions = new Set<string>()
 /** Sessions whose stream has been ended (buffer deleted).
  *  Late-arriving deltas/tool events for ended sessions must be ignored. */
 private endedSessions = new Set<string>()
 /** Sessions whose current turn was intentionally cancelled via chat.stop.
  *  Absorbs exactly one stale StreamEnd event from the cancelled turn,
  *  preventing it from terminating the replacement turn that follows.
  *  Set by chat.stop → consumed by the next handleStreamEnd → cleared by
  *  clear() if the stale event never arrived.
  *  Maps sessionId → generation number at time of cancellation. */
 private cancelledSessions = new Map<string, number>()

  /** Register an existing assistant placeholder message for streaming updates.
   *  Resets any stale state from a previous stream on the same session. */
  startStream(sessionId: string, messageId: string, userSkills?: string[]): void {
    // If there's a stale buffer for this session, clean up its old placeholder
    const old = this.buffers.get(sessionId)
    if (old?.messageId && old.messageId !== messageId) {
      this.removeMessage(sessionId, old.messageId)
    }
    const buf = this.ensureBuffer(sessionId)
    buf.generation = (buf.generation || 0) + 1
    buf.messageId = messageId
    buf.textAcc = ''
    buf.thinkingAcc = ''
    buf.imageAcc = []
    buf.processAcc = []
    buf.skillCalls = []
    buf.shellCalls = []
    buf.userSkills = userSkills ?? []
    buf.inThinking = false
    buf.inVision = false
    buf.visionDone = false
    buf.visionBatches = []
    buf.overrideActivity = null
    buf.mdStable = ''
    buf.mdStableHtml = ''
    buf.mdLastRenderAt = 0
    buf.mdFenceHtml = ''
    if (buf.rafId) { cancelAnimationFrame(buf.rafId); buf.rafId = null }
    useStore.getState().setStreamingActivity(sessionId, { phase: 'generating' })
  }

  /** Remove a message from the store by ID. */
  private removeMessage(sessionId: string, messageId: string): void {
    const store = useStore.getState()
    const msgs = store.messagesBySession.get(sessionId)
    if (!msgs) return
    const idx = msgs.findIndex(m => m.id === messageId)
    if (idx < 0) return
    const next = new Map(store.messagesBySession)
    const updated = [...msgs]
    updated.splice(idx, 1)
    next.set(sessionId, updated)
    useStore.setState({ messagesBySession: next })
  }

  private ensureBuffer(sessionId: string): BufferState {
    if (!this.buffers.has(sessionId)) {
      this.buffers.set(sessionId, {
        messageId: null,
        generation: 0,
        textAcc: '',
        thinkingAcc: '',
        imageAcc: [],
        processAcc: [],
        skillCalls: [],
        shellCalls: [],
        userSkills: [],
        inThinking: false,
        inVision: false,
        visionDone: false,
        visionBatches: [],
        rafId: null,
        imOrigin: false,
        overrideActivity: null,
        mdStable: '',
        mdStableHtml: '',
        mdLastRenderAt: 0,
        mdFenceHtml: '',
      })
    }
    return this.buffers.get(sessionId)!
  }

 private ensureStreamingForIM(sessionId: string): boolean {
   const store = useStore.getState()
   if (store.streamingSessionIds.has(sessionId)) return true
   // If the stream was already ended, don't auto-create a new buffer.
   // Prevents late-arriving WS events from creating phantom messages.
   if (this.endedSessions.has(sessionId)) return false
   // IM sessions: chat.send was initiated by ImBridge (separate WS), so the
   // renderer never called `startStream`. Auto-register if the session exists.
   const msgs = store.messagesBySession.get(sessionId)
    if (!msgs) return false
    const buf = this.ensureBuffer(sessionId)
    // Adopt an existing empty assistant placeholder if available, otherwise create one.
    if (!buf.messageId) {
      const empty = msgs.find(m => m.role === 'assistant' && m.blocks.length === 0)
      if (empty) {
        buf.messageId = empty.id
      } else {
        buf.messageId = crypto.randomUUID()
        store.ensureSession(sessionId)
        store.appendMessage(sessionId, {
          id: buf.messageId,
          role: 'assistant',
          blocks: [],
          timestamp: new Date().toISOString(),
        })
      }
   }
   store.addStreamingSession(sessionId)
   store.setStreamingActivity(sessionId, { phase: 'generating' })
   // Clear ended flag — a new stream is starting for this session
   this.endedSessions.delete(sessionId)
   return true
 }

  handleStreamDelta(sessionId: string, delta: string): void {
    // Ignore deltas arriving after stream has ended (late WebSocket frames)
    if (!this.ensureStreamingForIM(sessionId)) {
      return
    }
    this.wsStreamSessions.add(sessionId)  // mark as handled by main WS
    this._doHandleStreamDelta(sessionId, delta)
  }

  /** IM bridge variant — skip if main WS already handled this session. */
  handleStreamDeltaIM(sessionId: string, delta: string): void {
    if (this.wsStreamSessions.has(sessionId)) return  // already handled by main WS
    if (!this.ensureStreamingForIM(sessionId)) return
    // Mark IM origin so handleStreamEnd triggers syncIMSessionHistory
    // for IM sessions (which need history reload from DB).
    this.ensureBuffer(sessionId).imOrigin = true
    this._doHandleStreamDelta(sessionId, delta)
  }

 handleProcessOutput(sessionId: string, pid: string, data: string, stream: string): void {
   const store = useStore.getState()
   const sid = sessionId || store.currentSessionId || 'default'
   // Only render for sessions that already have a streaming buffer.
   // Never auto-create a new buffer from process/monitor side-channel
   // events — that would leak streaming state and create phantom messages.
   if (!this.buffers.has(sid)) return

   store.ensureSession(sid)
   const buf = this.ensureBuffer(sid)
    if (!buf.messageId) {
      buf.messageId = crypto.randomUUID()
      store.appendMessage(sid, {
        id: buf.messageId,
        role: 'assistant',
        blocks: [],
        timestamp: new Date().toISOString(),
      })
    }

    let procEntry = buf.processAcc.find(p => p.pid === pid)
    if (!procEntry) {
      procEntry = { pid, lines: [], exited: false, exitCode: null }
      buf.processAcc.push(procEntry)
    }
    procEntry.lines.push({ stream, text: data })

    // Cap at 500 lines per process to prevent unbounded memory
    if (procEntry.lines.length > 500) {
      procEntry.lines = procEntry.lines.slice(-500)
    }

    this.scheduleFlush(buf, sid)
  }

 handleProcessExited(sessionId: string, pid: string, exitCode: number): void {
   const sid = sessionId || useStore.getState().currentSessionId || 'default'
   // Same guard as handleProcessOutput: don't auto-create buffers from
   // process/monitor events — that would leak streaming state and create
   // phantom messages.
   if (!this.buffers.has(sid)) return

  const buf = this.ensureBuffer(sid)
   if (!buf.messageId) {
     buf.messageId = crypto.randomUUID()
     useStore.getState().ensureSession(sid)
     useStore.getState().appendMessage(sid, {
       id: buf.messageId,
       role: 'assistant',
       blocks: [],
       timestamp: new Date().toISOString(),
     })
   }
    let procEntry = buf.processAcc.find(p => p.pid === pid)
    if (!procEntry) {
      procEntry = { pid, lines: [], exited: false, exitCode: null }
      buf.processAcc.push(procEntry)
    }
    procEntry.exitCode = exitCode
    procEntry.lines.push({ stream: 'system', text: `[进程已退出, code=${exitCode}]` })
    procEntry.exited = true
    this.scheduleFlush(buf, sid)
  }

  private _doHandleStreamDelta(sessionId: string, delta: string): void {
    const buf = this.ensureBuffer(sessionId)
    // Defensive: if buffer has no messageId, stream wasn't properly started
    if (!buf.messageId) return

    // Handle REASONING control signal
    if (delta.startsWith('\x02REASONING\x02')) {
      buf.thinkingAcc += delta.slice(11)
      buf.inThinking = true
    } else if (delta.startsWith('\x02VISION_START\x02')) {
      buf.inVision = true
      buf.visionDone = false
      buf.visionBatches = []
    } else if (delta.startsWith('\x02VISION_BATCH\x02')) {
      // Format: \x02VISION_BATCH\x02batch_index;total_batches;status;result_encoded
      // result_encoded has newlines replaced with \x03
      const payload = delta.slice(15) // prefix length
      const semi1 = payload.indexOf(';')
      const semi2 = payload.indexOf(';', semi1 + 1)
      const semi3 = payload.indexOf(';', semi2 + 1)
      if (semi1 > 0 && semi2 > semi1 && semi3 > semi2) {
        const batchIndex = parseInt(payload.slice(0, semi1), 10)
        const totalBatches = parseInt(payload.slice(semi1 + 1, semi2), 10)
        const status = payload.slice(semi2 + 1, semi3) as 'running' | 'done' | 'error'
        const resultEncoded = payload.slice(semi3 + 1)
        const result = resultEncoded ? resultEncoded.replace(/\x03/g, '\n') : undefined
        // Update or add batch entry
        const existing = buf.visionBatches.find(b => b.batchIndex === batchIndex)
        if (existing) {
          existing.status = status
          existing.result = result
        } else {
          buf.visionBatches.push({ batchIndex, totalBatches, status, result })
        }
      }
    } else if (delta.startsWith('\x02VISION_DONE\x02')) {
      buf.inVision = false
      buf.visionDone = true
    } else if (delta.startsWith('\x02IMAGE\x02')) {
      // Format: \x02IMAGE\x02{media_type};{base64_data}
      // Prefix length = 1 + "IMAGE".length + 1 = 7
      const payload = delta.slice(7)
      const semi = payload.indexOf(';')
      if (semi > 0) {
        buf.imageAcc.push({
          mimeType: payload.slice(0, semi),
          data: payload.slice(semi + 1),
        })
      }
    } else if (delta.startsWith('\x00USAGE:')) {
      try {
        const parts = delta.slice(8).split(':')
        const prompt = parseInt(parts[0], 10) || 0
        const completion = parseInt(parts[1], 10) || 0
        if (prompt || completion) {
          // Inline USAGE control delta carries no model/context — preserve any
          // previous values for this session so the ring scale stays correct.
          const prev = useStore.getState().usageBySession.get(sessionId)
          useStore.getState().setSessionUsage(sessionId, {
            prompt,
            completion,
            model: prev?.model ?? '',
            contextWindow: prev?.contextWindow ?? 0,
            cached: prev?.cached ?? 0,
            cacheRead: prev?.cacheRead ?? 0,
            cacheWrite: prev?.cacheWrite ?? 0,
          })
        }
      } catch {
        /* ignore parse errors */
      }
      return
    } else {
      // 模型从思考切换到正常文本输出，清除 thinking 标记
      buf.inThinking = false
      buf.textAcc += delta
    }

    this.scheduleFlush(buf, sessionId)
  }

  handleToolStarted(
    sessionId: string,
    tool: { id: string; name: string; args: Record<string, unknown> },
  ): void {
    if (!this.ensureStreamingForIM(sessionId)) return
    const buf = this.ensureBuffer(sessionId)
    if (!buf.messageId) return
    if (tool.name === 'use_skill') {
      const skillName = (tool.args?.skill_name as string) || 'loading...'
      buf.skillCalls.push({
        id: tool.id,
        name: skillName,
        status: 'running',
        args: tool.args ?? {},
      })
    } else if (tool.name === 'request_tools') {
      // meta-tool — invisible, no block
    } else {
      // All other tools (shell, file_write, file_read, content_search, etc.)
      // render in terminal-style ShellBlock for full visibility.
      buf.shellCalls.push({
        id: tool.id,
        name: tool.name,
        status: 'running',
        args: tool.args ?? {},
      })
    }
    this.scheduleFlush(buf, sessionId)
  }

  handleToolCompleted(
    sessionId: string,
    toolId: string,
    result?: string,
    toolName?: string,
    details?: Record<string, unknown>,
  ): void {
    if (!this.ensureStreamingForIM(sessionId)) return
    const buf = this.ensureBuffer(sessionId)
    if (!buf.messageId) return
    const shell = buf.shellCalls.find((t) => t.id === toolId)
    if (shell) {
      shell.status = 'done'
      shell.result = result
      if (details) shell.details = details
      // Flash island transient when config is updated by AI
      if (shell.name === 'update_config' && result && !result.includes('No changes applied')) {
        import('../i18n').then(({ t }) => {
          useStore.getState().showIslandTransient(t('island.configUpdated'), 2500)
        })
      }
    }
    const skill = buf.skillCalls.find((t) => t.id === toolId)
    if (skill) {
      skill.status = 'done'
      skill.result = result
      // Fix unknown name: ToolStarted args are often incomplete (first chunk only),
      // so extract the real skill name from the use_skill result content.
      if ((skill.name === 'unknown' || skill.name === 'loading...') && result) {
        const m = result.match(/^## Skill: ([^\n]+)/)
        if (m) skill.name = m[1].trim()
      }
    }
    // Fallback: if neither tool nor skill was tracked (started event missed),
    // create a completed entry based on the tool name from the completed event
    if (!skill && toolName === 'use_skill') {
      let skillName = 'unknown'
      if (result) {
        const m = result.match(/^## Skill: ([^\n]+)/)
        if (m) skillName = m[1].trim()
      }
      buf.skillCalls.push({
        id: toolId,
        name: skillName,
        status: 'done',
        args: {},
        result,
      })
    }
    this.scheduleFlush(buf, sessionId)
  }

  handleToolOutput(
    sessionId: string,
    toolId: string,
    line: string,
  ): void {
    if (!this.ensureStreamingForIM(sessionId)) return
    const buf = this.ensureBuffer(sessionId)
    if (!buf.messageId) return
    const shell = buf.shellCalls.find((t) => t.id === toolId)
    if (shell) {
      // Append line to result in real-time
      const prev = shell.result || ''
      shell.result = prev ? prev + '\n' + line : line
      this.scheduleFlush(buf, sessionId)
    }
  }

  handleStreamEnd(sessionId: string): void {
    // Absorb stale StreamEnd from a previously cancelled turn.
    // When chat.stop kills a turn, the backend sends one last StreamEnd
    // for it asynchronously. If a new turn (higher generation) has already
    // started, this stale event must be absorbed to prevent terminating
    // the new turn.
    const cancelledGen = this.cancelledSessions.get(sessionId)
    if (cancelledGen !== undefined) {
      const buf = this.buffers.get(sessionId)
      if (buf && buf.generation > cancelledGen) {
        // Stale StreamEnd from old turn — new turn already started, absorb
        this.cancelledSessions.delete(sessionId)
        return
      }
      // Same generation — this IS the StreamEnd from the cancelled turn
      this.cancelledSessions.delete(sessionId)
    }
    // Guard re-entry: stream_end can fire from up to 3 sources for the same
    // session (WS chat.stream_end, the chat.send finally, and a safety timer).
    // The first call deletes the buffer below; subsequent callers must no-op,
    // otherwise ensureBuffer recreates a fresh buffer and we re-fire
    // removeStreamingSession + maybeAutoTitle (duplicate session.auto_title RPC
    // and loadSessions).
    if (!this.buffers.has(sessionId)) return
    const buf = this.ensureBuffer(sessionId)
    if (buf.rafId) { cancelAnimationFrame(buf.rafId); buf.rafId = null }
    buf.inThinking = false
    this.flush(buf, sessionId, true)
    const usage = useStore.getState().usageBySession.get(sessionId)
    if (buf.messageId && usage && (usage.prompt || usage.completion)) {
      useStore.getState().setMessageUsage(sessionId, buf.messageId, { ...usage })
    }
   useStore.getState().removeStreamingSession(sessionId)
   // 插话已通过 chat.steer 在 turn 进行中注入（agent_loop 迭代 drain 为
   // [用户指引] System 消息，不打断当前生成）。未消费的项留在后端 queue，
   // 下次 chat.send 时注入。stream_end 不再发新消息，避免与 chat.steer 重复注入。
   // If this was an IM-originated stream, the user message (sent via IM) and
   // the final assistant response aren't in the renderer store — only the
   // streamed blocks are. Sync the full history from the engine so the
   // desktop ChatArea shows everything.
   const wasIM = buf.imOrigin
   this.buffers.delete(sessionId)
   this.wsStreamSessions.delete(sessionId)
   this.endedSessions.add(sessionId)
   // Fire and forget the sync — it updates messagesBySession asynchronously.
   if (wasIM) {
     this.syncIMSessionHistory(sessionId)
   }

    // Auto-title: trigger if session has no title and feature is enabled
    this.maybeAutoTitle(sessionId)
  }

  private async syncIMSessionHistory(sessionId: string): Promise<void> {
    try {
      const result: any = await loomRpc('session.messages', { session_id: sessionId })
      const allMsgs: any[] = result?.messages || []
      if (allMsgs.length === 0) return

      const store = useStore.getState()

      // Use parseContentParts for consistent block format (same as session store).
      // Previously used a local convertContent that produced { type, text } blocks
      // missing the html/source fields TextBlock expects — causing blank messages.
      const port = store.port

      const msgs = allMsgs
        .filter((m: any) => {
          const r = typeof m.role === 'string' ? m.role.toLowerCase() : ''
          return r !== 'tool'
        })
        .map((m: any, i: number) => ({
          id: `im-${sessionId}-${i}`,
          role: (['user', 'assistant'].includes(m.role) ? m.role : 'system') as 'user' | 'assistant' | 'system',
          blocks: parseContentParts(m.content, sessionId, port),
          timestamp: m.timestamp || new Date().toISOString(),
          usage: m.usage ? {
            prompt: m.usage.prompt_tokens || 0,
            completion: m.usage.completion_tokens || 0,
            model: m.usage.model || '',
            contextWindow: m.usage.context_window || 0,
            cached: m.usage.cached_tokens ?? 0,
            cacheRead: m.usage.cache_read_tokens ?? 0,
            cacheWrite: m.usage.cache_write_tokens ?? 0,
          } : undefined,
        }))

      store.hydrateMessages(sessionId, msgs)

      // Rebuild cumulative usage
      store.clearSessionCumulative?.(sessionId)
      for (const m of msgs) {
        if (m.role === 'assistant' && m.usage) {
          store.accumulateSessionUsage?.(sessionId, {
            prompt: m.usage.prompt || 0,
            completion: m.usage.completion || 0,
            model: m.usage.model || '',
            contextWindow: m.usage.contextWindow || 0,
            cached: m.usage.cached || 0,
            cacheRead: m.usage.cacheRead || 0,
            cacheWrite: m.usage.cacheWrite || 0,
          })
        }
      }
    } catch (e) {
      console.warn('[stream-buffer] syncIMSessionHistory failed:', e)
    }
  }

  private async maybeAutoTitle(sessionId: string): Promise<void> {
    try {
      const enabled = await window.loom.getPreference<boolean>('autoTitle', true)
      if (!enabled) return
      // Only fire for untitled sessions
      const sessions = useStore.getState().sessions
      const session = sessions.find(s => s.path === sessionId)
      if (session?.title) return
      // Call backend
      const result = await loomRpc<{ title: string }>('session.auto_title', { session_id: sessionId })
      if (result?.title) {
        // Backend already persisted the rename; just refresh the sidebar list silently.
        await useStore.getState().loadSessions()
      }
    } catch (e) {
      console.warn('[auto-title] failed:', e)
      // Best-effort, silently ignore failures
    }
  }

  private scheduleFlush(buf: BufferState, sessionId: string): void {
    if (buf.rafId) return // already scheduled for next frame — coalesce
    buf.rafId = requestAnimationFrame(() => {
      buf.rafId = null
      this.flush(buf, sessionId)
    })
  }

  private flush(buf: BufferState, sessionId: string, final = false): void {
    if (!buf.messageId) return

    const store = useStore.getState()
    const allMsgs = store.messagesBySession.get(sessionId) || []
    const msgIdx = allMsgs.findIndex((m) => m.id === buf.messageId)
    if (msgIdx < 0) return

    const blocks: Array<{ type: string; [key: string]: unknown }> = []

    // Display order: vision → thinking → shells → skills → images → text
    // Thinking above tools so the user sees reasoning first, then what was executed.

    if (buf.inVision || buf.visionBatches.length > 0) {
      const doneCount = buf.visionBatches.filter(b => b.status === 'done').length
      const totalCount = buf.visionBatches.length > 0
        ? buf.visionBatches[0].totalBatches
        : 0
      const allDone = buf.visionDone && doneCount === totalCount && totalCount > 0

      blocks.push({
        type: 'vision_processing',
        status: allDone ? 'done' : buf.inVision ? 'running' : 'waiting',
        content: totalCount > 1
          ? t('chat.visionProgress', { done: doneCount, total: totalCount })
          : buf.inVision
            ? t('chat.visionProcessingImage')
            : t('chat.visionCompleted'),
        batches: buf.visionBatches.map(b => ({
          batchIndex: b.batchIndex,
          totalBatches: b.totalBatches,
          status: b.status,
          result: b.result,
        })),
      })
    }

    for (const img of buf.imageAcc) {
      blocks.push({
        type: 'image',
        mimeType: img.mimeType,
        thumbnail: `data:${img.mimeType};base64,${img.data}`,
        name: 'generated-image',
      })
    }

    if (buf.thinkingAcc) {
      blocks.push({
        type: 'thinking',
        content: buf.thinkingAcc,
        sealed: !buf.inThinking,
      })
    }

    // Terminal-style blocks for shell/file tools — rendered below thinking
    for (const sc of buf.shellCalls) {
      blocks.push({
        type: 'shell',
        id: sc.id,
        toolName: sc.name,
        status: sc.status,
        args: sc.args || {},
        result: sc.result,
        details: sc.details,
        sealed: sc.status === 'done',
      })
    }

    for (const sc of buf.skillCalls) {
      blocks.push({
        type: 'skill',
        id: sc.id,
        name: sc.name,
        status: sc.status,
        result: sc.result,
        sealed: sc.status === 'done',
      })
    }

    for (const proc of buf.processAcc) {
      if (proc.lines.length > 0) {
        blocks.push({
          type: 'process_output',
          pid: proc.pid,
          lines: proc.lines.map(l => ({ stream: l.stream, text: l.text })),
          sealed: proc.exited,
          exitCode: proc.exitCode,
        })
      }
    }

    if (buf.textAcc) {
      const html = this.renderMarkdown(buf, final)
      blocks.push({ type: 'text', html, source: buf.textAcc })
    }

    // Preserve team and subagent blocks added by bootstrap events across flushes.
    // All other block types are fully rebuilt from buffer state each flush.
    const existingBlocks = allMsgs[msgIdx].blocks
    for (const b of existingBlocks) {
      if ((b as any).type === 'team' || (b as any).type === 'subagent') {
        blocks.push(b)
      }
    }

    // Diff guard: skip setState if blocks are structurally identical
    if (blocks.length === existingBlocks.length) {
      let same = true
      for (let i = 0; i < blocks.length; i++) {
        // Shallow compare keys of each block (same as what React memo checks)
        const a = blocks[i] as Record<string, unknown>
        const b = existingBlocks[i] as Record<string, unknown>
        const aKeys = Object.keys(a)
        const bKeys = Object.keys(b)
        if (aKeys.length !== bKeys.length) { same = false; break }
        for (const k of aKeys) {
          if (a[k] !== b[k]) { same = false; break }
        }
        if (!same) break
      }
      if (same) {
        // Still push activity (phase may have changed even if blocks haven't)
        this.pushActivity(sessionId, buf)
        return
      }
    }

    const next = new Map(store.messagesBySession)
    const updatedMsgs = [...allMsgs]
    updatedMsgs[msgIdx] = { ...allMsgs[msgIdx], blocks }
    next.set(sessionId, updatedMsgs)
    useStore.setState({ messagesBySession: next })

    // 同步生成子阶段到灵动岛
    this.pushActivity(sessionId, buf)
  }

  // Render markdown at newline boundaries to avoid flicker (tables, code fences).
  // When final=true (stream end), render the entire source through markdown
  // to avoid leaving the last line as escaped HTML.
  //
  // Performance: a full markdown-it + highlight + sanitize pass over the whole
  // accumulated source every animation frame is O(N) per frame and janks on long
  // replies. Since textAcc only grows by appending, the rendered "stable" prefix
  // (everything up to the last newline) stays valid across flushes — so we cache
  // it and:
  //   • reuse the cache outright while only the in-progress tail line grows;
  //   • re-render the stable prefix at most once per MD_RERENDER_THROTTLE_MS,
  //     showing any brand-new complete lines as escaped plain text in between
  //     (they're properly formatted on the next re-render — imperceptible).
  // The final render always runs the full pipeline, so the persisted/final view
  // is byte-for-byte the same as before this optimization.
  private renderMarkdown(buf: BufferState, final = false): string {
    const source = buf.textAcc
    if (final) {
      buf.mdStable = ''
      buf.mdStableHtml = ''
      buf.mdFenceHtml = ''
      return sanitizeHtml(renderMarkdown(source))
    }
    const lastNewline = source.lastIndexOf('\n')
    const stable = lastNewline >= 0 ? source.slice(0, lastNewline) : ''
    const tail = lastNewline >= 0 ? source.slice(lastNewline + 1) : source
    // No complete line yet — render just the in-progress line (bounded cost,
    // matches prior behavior).
    if (!stable) {
      return sanitizeHtml(renderMarkdown(tail))
    }
    // ── 围栏感知 ──────────────────────────────────────────────────────────
    // 若 stable 前缀里存在未闭合代码围栏（正在流式输出代码块），绝不能把新行
    // 当纯文本拼到已渲染 HTML 之后——markdown-it 会把未闭合围栏在 stable 末尾
    // 自动闭合，新行就落在 </code></pre> 外面，产生"代码先闪在块外、下次重渲
    // 才吸进去"的跳变（流式代码时肉眼可见）。此时把 stable 边界前移到开栏行
    // 之前：围栏之前的前缀走缓存（零开销），当前代码块（开栏行到文末）单独
    // 节流重渲（最多滞后一帧，但结构始终正确）。
    const fenceStart = unclosedFenceStart(stable)
    if (fenceStart >= 0) {
      const pre = source.slice(0, fenceStart) // 围栏前的稳定前缀
      const codeBlock = source.slice(fenceStart) // 当前代码块（含未闭合开栏行）
      if (pre !== buf.mdStable) {
        buf.mdStable = pre
        buf.mdStableHtml = pre ? sanitizeHtml(renderMarkdown(pre)) : ''
        buf.mdFenceHtml = '' // 前缀变了，代码块缓存作废
      }
      const now = performance.now()
      if (!buf.mdFenceHtml || now - buf.mdLastRenderAt >= MD_RERENDER_THROTTLE_MS) {
        buf.mdFenceHtml = sanitizeHtml(renderMarkdown(codeBlock))
        buf.mdLastRenderAt = now
      }
      return buf.mdStableHtml + buf.mdFenceHtml
    }
    // ── 非围栏场景：原有稳定前缀缓存优化 ──────────────────────────────────
    // 刚从围栏模式切回（代码块闭合）：mdStable 语义已变，强制整段重渲一次，
    // 避免闭合的代码块短暂以纯文本闪现。
    const wasFence = buf.mdFenceHtml !== ''
    // Fast path: stable prefix unchanged since the last render (token streaming
    // within a single line) — zero markdown work this frame.
    if (stable === buf.mdStable && buf.mdStableHtml && !wasFence) {
      return buf.mdStableHtml + '\n' + escapeHtml(tail)
    }
    const now = performance.now()
    if (wasFence || !buf.mdStableHtml || now - buf.mdLastRenderAt >= MD_RERENDER_THROTTLE_MS) {
      buf.mdStable = stable
      buf.mdStableHtml = sanitizeHtml(renderMarkdown(stable))
      buf.mdFenceHtml = ''
      buf.mdLastRenderAt = now
      return buf.mdStableHtml + '\n' + escapeHtml(tail)
    }
    // Throttled: keep the previously rendered stable prefix and show everything
    // appended after it as escaped plain text until the next re-render window.
    const appended = source.slice(buf.mdStable.length)
    return buf.mdStableHtml + escapeHtml(appended)
  }

  /** 从 buffer 状态推导当前生成子阶段，供灵动岛流转显示 */
  private deriveActivity(buf: BufferState): StreamingActivity {
    // 视觉处理优先级最高
    if (buf.inVision || buf.visionBatches.some(b => b.status === 'running')) {
      const done = buf.visionBatches.filter(b => b.status === 'done').length
      const total = buf.visionBatches[0]?.totalBatches ?? 0
      return { phase: 'vision', visionDone: done, visionTotal: total }
    }
    if (buf.inThinking) {
      return { phase: 'thinking' }
    }
    const runningSkill = buf.skillCalls.find(s => s.status === 'running')
    if (runningSkill) {
      return { phase: 'skill', detail: runningSkill.name }
    }
    const runningTool = buf.shellCalls.find(s => s.status === 'running')
    if (runningTool) {
      // Show the actual operation, not just the tool name.
      const args = runningTool.args
      const cmd = (args?.command as string) || (args?.path as string) || (args?.pattern as string) || ''
      if (cmd) {
        return { phase: 'tool', detail: `${runningTool.name}: ${String(cmd).slice(0, 60)}` }
      }
      // update_config: show which keys are being changed
      if (runningTool.name === 'update_config' && args?.updates) {
        const keys = Object.keys(args.updates as Record<string, unknown>)
        if (keys.length > 0) {
          const suffix = keys.length > 2 ? `${keys.slice(0, 2).join(', ')}...` : keys.join(', ')
          return { phase: 'tool', detail: `update_config: ${suffix}` }
        }
      }
      return { phase: 'tool', detail: runningTool.name }
    }
    // Background processes / monitors — show as tool phase while any are still running
    const runningProc = buf.processAcc.find(p => !p.exited)
    if (runningProc) {
      const label = runningProc.pid.length > 8
        ? `进程 ${runningProc.pid.slice(0, 8)}...`
        : `进程 ${runningProc.pid}`
      return { phase: 'tool', detail: label }
    }
    return { phase: 'generating' }
  }

  private pushActivity(sessionId: string, buf: BufferState): void {
    useStore.getState().setStreamingActivity(sessionId, buf.overrideActivity ?? this.deriveActivity(buf))
  }

  /** Set an external activity override that survives flush cycles.
   *  Used by bootstrap for team/monitor phases that don't originate from
   *  shell/skill call state.  Pass null to clear the override and let
   *  deriveActivity take over again. */
  setOverrideActivity(sessionId: string, activity: StreamingActivity | null): void {
    const buf = this.ensureBuffer(sessionId)
    buf.overrideActivity = activity
    // Push immediately so the Dynamic Island updates without waiting for the next flush
    useStore.getState().setStreamingActivity(sessionId, activity ?? this.deriveActivity(buf))
  }

  snapshot(sessionId: string): BufferState | null {
    return this.buffers.get(sessionId) ?? null
  }

  /** Record the current generation as cancelled so the next handleStreamEnd
   *  can distinguish the stale StreamEnd from the cancelled turn vs a fresh one.
   *  Called by handleStop / handleForceSend BEFORE starting the replacement turn. */
  markCancelled(sessionId: string): void {
    const buf = this.buffers.get(sessionId)
    const gen = buf ? buf.generation : 0
    this.cancelledSessions.set(sessionId, gen)
  }

  clear(sessionId: string): void {
    const buf = this.buffers.get(sessionId)
    if (buf?.rafId) cancelAnimationFrame(buf.rafId)
    this.buffers.delete(sessionId)
    this.cancelledSessions.delete(sessionId)
  }
}

export const streamBufferManager = new StreamBufferManager()

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

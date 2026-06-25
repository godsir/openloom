import { useStore } from '../stores'
import type { StreamingActivity } from '../stores/streaming'
import { renderMarkdown } from '../utils/markdown'
import { sanitizeHtml } from '../utils/markdown-sanitizer'
import { t } from '../i18n'

const FLUSH_INTERVAL = 16

interface BufferState {
  messageId: string | null
  textAcc: string
  thinkingAcc: string
  imageAcc: Array<{ mimeType: string; data: string }>
  moodAcc: { yuan: string; text: string }
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
  _lastFlush: number
  _lastTextLen: number
  _lastThinkLen: number
}

class StreamBufferManager {
  private buffers = new Map<string, BufferState>()

  /** Register an existing assistant placeholder message for streaming updates.
   *  Resets any stale state from a previous stream on the same session. */
  startStream(sessionId: string, messageId: string, userSkills?: string[]): void {
    // If there's a stale buffer for this session, clean up its old placeholder
    const old = this.buffers.get(sessionId)
    if (old?.messageId && old.messageId !== messageId) {
      this.removeMessage(sessionId, old.messageId)
    }
    const buf = this.ensureBuffer(sessionId)
    buf.messageId = messageId
    buf.textAcc = ''
    buf.thinkingAcc = ''
    buf.imageAcc = []
    buf.skillCalls = []
    buf.shellCalls = []
    buf.userSkills = userSkills ?? []
    buf.inThinking = false
    buf.inVision = false
    buf.visionDone = false
    buf.visionBatches = []
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
        textAcc: '',
        thinkingAcc: '',
        moodAcc: { yuan: '', text: '' },
        skillCalls: [],
        shellCalls: [],
        userSkills: [],
        inThinking: false,
        inVision: false,
        visionDone: false,
        visionBatches: [],
        rafId: null,
        _lastFlush: 0,
        _lastTextLen: 0,
        _lastThinkLen: 0,
      })
    }
    return this.buffers.get(sessionId)!
  }

  handleStreamDelta(sessionId: string, delta: string): void {
    // Ignore deltas arriving after stream has ended (late WebSocket frames)
    if (!useStore.getState().streamingSessionIds.has(sessionId)) {
      console.warn('[stream-buffer] Delta dropped — session not streaming:', sessionId, 'delta:', delta.slice(0, 40))
      return
    }
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
      buf.textAcc += delta
    }

    this.scheduleFlush(buf, sessionId)
  }

  handleToolStarted(
    sessionId: string,
    tool: { id: string; name: string; args: Record<string, unknown> },
  ): void {
    if (!useStore.getState().streamingSessionIds.has(sessionId)) return
    const buf = this.ensureBuffer(sessionId)
    if (!buf.messageId) return
    console.log('[stream-buffer] Tool started:', tool.name, tool.id)
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
    if (!useStore.getState().streamingSessionIds.has(sessionId)) return
    const buf = this.ensureBuffer(sessionId)
    if (!buf.messageId) return
    const shell = buf.shellCalls.find((t) => t.id === toolId)
    if (shell) {
      shell.status = 'done'
      shell.result = result
      if (details) shell.details = details
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

  handleStreamEnd(sessionId: string): void {
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
    this.buffers.delete(sessionId)

    // Auto-title: trigger if session has no title and feature is enabled
    this.maybeAutoTitle(sessionId)
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
      const { loomRpc: rpc } = await import('../services/jsonrpc')
      const result = await rpc<{ title: string }>('session.auto_title', { session_id: sessionId })
      if (result?.title) {
        // Backend already persisted the rename; just refresh the sidebar list silently.
        await useStore.getState().loadSessions()
      }
    } catch (e) {
      console.warn('[auto-title] failed:', e)
      // Best-effort, silently ignore failures
    }
  }

  private createPlaceholderIfNeeded(
    buf: BufferState,
    sessionId: string,
  ): void {
    if (buf.messageId) return
    // If we reach here, the stream started without a registered placeholder.
    // Check if there's an existing empty assistant message we can adopt.
    const msgs = useStore.getState().messagesBySession.get(sessionId)
    if (msgs) {
      const empty = msgs.find(m => m.role === 'assistant' && m.blocks.length === 0)
      if (empty) {
        buf.messageId = empty.id
        useStore.getState().addStreamingSession(sessionId)
        return
      }
    }
    // No existing placeholder — create one as last resort
    buf.messageId = crypto.randomUUID()
    useStore.getState().addStreamingSession(sessionId)
    useStore.getState().ensureSession(sessionId)
    useStore.getState().appendMessage(sessionId, {
      id: buf.messageId,
      role: 'assistant',
      blocks: [],
      timestamp: new Date().toISOString(),
    })
  }

  private scheduleFlush(buf: BufferState, sessionId: string): void {
    if (buf.rafId) return
    const now = performance.now()
    const elapsed = now - buf._lastFlush
    if (elapsed < FLUSH_INTERVAL) {
      // Too soon since last flush — schedule a delayed rAF
      buf.rafId = requestAnimationFrame(() => {
        buf.rafId = null
        this.flush(buf, sessionId)
        buf._lastFlush = performance.now()
      })
    } else {
      // Flush immediately on next frame
      buf.rafId = requestAnimationFrame(() => {
        buf.rafId = null
        this.flush(buf, sessionId)
        buf._lastFlush = performance.now()
      })
    }
  }

  private flush(buf: BufferState, sessionId: string, final = false): void {
    if (!buf.messageId) return

    const blocks: Array<{ type: string; [key: string]: unknown }> = []

    console.log('[stream-buffer] Flushing buffer:', {
      skillCalls: buf.skillCalls.length,
      thinking: buf.thinkingAcc.length > 0,
      text: buf.textAcc.length > 0,
      final,
    })

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

    if (buf.moodAcc.text) {
      blocks.push({ type: 'mood', ...buf.moodAcc })
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
      console.log('[stream-buffer] Creating skill block:', sc.name, sc.status)
      blocks.push({
        type: 'skill',
        id: sc.id,
        name: sc.name,
        status: sc.status,
        result: sc.result,
        sealed: sc.status === 'done',
      })
    }

    if (buf.textAcc) {
      const html = this.renderMarkdown(buf.textAcc, final)
      blocks.push({ type: 'text', html, source: buf.textAcc })
    }

    // Replace all blocks in the message
    const store = useStore.getState()
    const msgs = store.messagesBySession.get(sessionId) || []
    const msgIdx = msgs.findIndex((m) => m.id === buf.messageId)
    if (msgIdx >= 0) {
      const next = new Map(store.messagesBySession)
      const updatedMsgs = [...msgs]
      updatedMsgs[msgIdx] = { ...msgs[msgIdx], blocks }
      next.set(sessionId, updatedMsgs)
      useStore.setState({ messagesBySession: next })
    }

    // 同步生成子阶段到灵动岛
    this.pushActivity(sessionId, buf)
  }

  // Render markdown at newline boundaries to avoid flicker (tables, code fences).
  // When final=true (stream end), render the entire source through markdown
  // to avoid leaving the last line as escaped HTML.
  private renderMarkdown(source: string, final = false): string {
    if (final) return sanitizeHtml(renderMarkdown(source))
    // Only render complete lines — incomplete last line stays as source
    const lastNewline = source.lastIndexOf('\n')
    const stable = lastNewline >= 0 ? source.slice(0, lastNewline) : ''
    const tail = lastNewline >= 0 ? source.slice(lastNewline + 1) : source
    if (!stable) return sanitizeHtml(renderMarkdown(tail))
    return sanitizeHtml(renderMarkdown(stable)) + '\n' + escapeHtml(tail)
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
      return { phase: 'tool', detail: runningTool.name }
    }
    return { phase: 'generating' }
  }

  private pushActivity(sessionId: string, buf: BufferState): void {
    useStore.getState().setStreamingActivity(sessionId, this.deriveActivity(buf))
  }

  snapshot(sessionId: string): BufferState | null {
    return this.buffers.get(sessionId) ?? null
  }

  clear(sessionId: string): void {
    const buf = this.buffers.get(sessionId)
    if (buf?.rafId) cancelAnimationFrame(buf.rafId)
    this.buffers.delete(sessionId)
  }
}

export const streamBufferManager = new StreamBufferManager()

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

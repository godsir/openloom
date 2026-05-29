import { useStore } from '../stores'
import { renderMarkdown } from '../utils/markdown'
import { sanitizeHtml } from '../utils/markdown-sanitizer'

const FLUSH_INTERVAL = 200

interface BufferState {
  messageId: string | null
  textAcc: string
  thinkingAcc: string
  moodAcc: { yuan: string; text: string }
  toolCalls: Array<{
    id: string
    name: string
    status: 'running' | 'done'
    elapsed: number
    args: Record<string, unknown>
    result?: string
  }>
  inThinking: boolean
  flushTimer: ReturnType<typeof setTimeout> | null
  _lastTextLen: number
  _lastThinkLen: number
  _lastToolCount: number
}

class StreamBufferManager {
  private buffers = new Map<string, BufferState>()

  /** Register an existing assistant placeholder message for streaming updates.
   *  Resets any stale state from a previous stream on the same session. */
  startStream(sessionId: string, messageId: string): void {
    // If there's a stale buffer for this session, clean up its old placeholder
    const old = this.buffers.get(sessionId)
    if (old?.messageId && old.messageId !== messageId) {
      this.removeMessage(sessionId, old.messageId)
    }
    const buf = this.ensureBuffer(sessionId)
    buf.messageId = messageId
    buf.textAcc = ''
    buf.thinkingAcc = ''
    buf.toolCalls = []
    buf.inThinking = false
    if (buf.flushTimer) { clearTimeout(buf.flushTimer); buf.flushTimer = null }
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
        toolCalls: [],
        inThinking: false,
        flushTimer: null,
        _lastTextLen: 0,
        _lastThinkLen: 0,
        _lastToolCount: 0,
      })
    }
    return this.buffers.get(sessionId)!
  }

  handleStreamDelta(sessionId: string, delta: string): void {
    // Ignore deltas arriving after stream has ended (late WebSocket frames)
    if (!useStore.getState().streamingSessionIds.has(sessionId)) return
    const buf = this.ensureBuffer(sessionId)
    // Defensive: if buffer has no messageId, stream wasn't properly started
    if (!buf.messageId) return

    // Handle REASONING control signal
    if (delta.startsWith('\x02REASONING\x02')) {
      buf.thinkingAcc += delta.slice(11)
      buf.inThinking = true
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
    buf.toolCalls.push({
      ...tool,
      status: 'running',
      elapsed: 0,
      args: tool.args ?? {},
    })
    this.scheduleFlush(buf, sessionId)
  }

  handleToolCompleted(
    sessionId: string,
    toolId: string,
    result?: string,
  ): void {
    if (!useStore.getState().streamingSessionIds.has(sessionId)) return
    const buf = this.ensureBuffer(sessionId)
    if (!buf.messageId) return
    const tool = buf.toolCalls.find((t) => t.id === toolId)
    if (tool) {
      tool.status = 'done'
      tool.result = result
    }
    this.scheduleFlush(buf, sessionId)
  }

  handleStreamEnd(sessionId: string): void {
    const buf = this.ensureBuffer(sessionId)
    if (buf.flushTimer) clearTimeout(buf.flushTimer)
    buf.inThinking = false
    this.flush(buf, sessionId)
    const usage = useStore.getState().usageBySession.get(sessionId)
    if (buf.messageId && usage && (usage.prompt || usage.completion)) {
      useStore.getState().setMessageUsage(sessionId, buf.messageId, { ...usage })
    }
    useStore.getState().removeStreamingSession(sessionId)
    this.buffers.delete(sessionId)
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
    if (buf.flushTimer) return
    buf.flushTimer = setTimeout(() => {
      buf.flushTimer = null
      this.flush(buf, sessionId)
    }, FLUSH_INTERVAL)
  }

  private flush(buf: BufferState, sessionId: string): void {
    if (!buf.messageId) return

    const blocks: Array<{ type: string; [key: string]: unknown }> = []

    // Display order: thinking → mood → tool_group → text
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

    if (buf.toolCalls.length > 0) {
      blocks.push({
        type: 'tool_group',
        tools: buf.toolCalls,
        collapsed: buf.toolCalls.every((t) => t.status === 'done'),
      })
    }

    if (buf.textAcc) {
      const html = this.renderMarkdown(buf.textAcc)
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
  }

  // Render markdown at newline boundaries to avoid flicker (tables, code fences).
  private renderMarkdown(source: string): string {
    // Only render complete lines — incomplete last line stays as source
    const lastNewline = source.lastIndexOf('\n')
    const stable = lastNewline >= 0 ? source.slice(0, lastNewline) : ''
    const tail = lastNewline >= 0 ? source.slice(lastNewline + 1) : source
    if (!stable) return sanitizeHtml(renderMarkdown(tail))
    return sanitizeHtml(renderMarkdown(stable)) + '\n' + escapeHtml(tail)
  }

  snapshot(sessionId: string): BufferState | null {
    return this.buffers.get(sessionId) ?? null
  }

  clear(sessionId: string): void {
    const buf = this.buffers.get(sessionId)
    if (buf?.flushTimer) clearTimeout(buf.flushTimer)
    this.buffers.delete(sessionId)
  }
}

export const streamBufferManager = new StreamBufferManager()

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

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
}

class StreamBufferManager {
  private buffers = new Map<string, BufferState>()

  /** Register an existing assistant placeholder message for streaming updates. */
  startStream(sessionId: string, messageId: string): void {
    const buf = this.ensureBuffer(sessionId)
    buf.messageId = messageId
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
      })
    }
    return this.buffers.get(sessionId)!
  }

  handleStreamDelta(sessionId: string, delta: string): void {
    const buf = this.ensureBuffer(sessionId)

    // Handle REASONING control signal
    if (delta.startsWith('\x02REASONING\x02')) {
      buf.thinkingAcc += delta.slice(11) // skip \x02 + "REASONING" + \x02 = 11 chars
      buf.inThinking = true
    } else if (delta.startsWith('\x00USAGE:')) {
      // Token usage control signal: \x00USAGE:{"prompt":N,"completion":M}
      try {
        const usageJson = delta.slice(8)
        const usage = JSON.parse(usageJson) as {
          prompt: number
          completion: number
        }
        if (usage.prompt || usage.completion) {
          useStore.getState().setTokenUsage({
            prompt: usage.prompt || 0,
            completion: usage.completion || 0,
          })
        }
      } catch {
        /* ignore parse errors */
      }
      return
    } else {
      buf.textAcc += delta
    }

    this.createPlaceholderIfNeeded(buf, sessionId)
    this.scheduleFlush(buf, sessionId)
  }

  handleToolStarted(
    sessionId: string,
    tool: { id: string; name: string; args: Record<string, unknown> },
  ): void {
    const buf = this.ensureBuffer(sessionId)
    buf.toolCalls.push({
      ...tool,
      status: 'running',
      elapsed: 0,
      args: tool.args ?? {},
    })
    this.createPlaceholderIfNeeded(buf, sessionId)
    this.scheduleFlush(buf, sessionId)
  }

  handleToolCompleted(
    sessionId: string,
    toolId: string,
    result?: string,
  ): void {
    const buf = this.ensureBuffer(sessionId)
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
    useStore.getState().removeStreamingSession(sessionId)
    this.buffers.delete(sessionId)
  }

  private createPlaceholderIfNeeded(
    buf: BufferState,
    sessionId: string,
  ): void {
    if (buf.messageId) return
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

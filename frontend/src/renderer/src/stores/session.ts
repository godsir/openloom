import { StateCreator } from 'zustand'
import { loomRpc } from '../services/jsonrpc'
import { rpc } from '../services/rpc-toast'
import { renderMarkdown } from '../utils/markdown'
import { sanitizeHtml } from '../utils/markdown-sanitizer'
import { t } from '../i18n'

// Matches actual backend SessionData JSON response.
// Fields are camelCase as returned by serde Serialize.
export interface SessionSummary {
  path: string        // session ID (UUID)
  title: string | null
  firstMessage: string
  modified: string    // ISO 8601 — last active time (updated_at)
  createdAt: string   // ISO 8601 — creation time (created_at)
  messageCount: number
  agentId: string | null
  agentName: string | null
  cwd: string | null
  pinnedAt: string | null
}

export interface SessionSlice {
  sessions: SessionSummary[]
  currentSessionId: string | null
  pinnedIds: Set<string>
  selectedSessionIds: Set<string>
  sessionWorkspaces: Record<string, string>
  defaultWorkspace: string | null
  setSessions: (sessions: SessionSummary[]) => void
  setCurrentSessionId: (id: string | null) => void
  createSession: () => Promise<string>
  switchSession: (id: string) => Promise<void>
  renameSession: (id: string, title: string) => Promise<void>
  deleteSession: (id: string) => Promise<void>
  deleteSessions: (ids: string[]) => Promise<void>
  pinSession: (id: string) => void
  unpinSession: (id: string) => void
  pinSessions: (ids: string[]) => void
  unpinSessions: (ids: string[]) => void
  toggleSessionSelect: (id: string) => void
  selectAllSessions: () => void
  deselectAllSessions: () => void
  loadSessions: () => Promise<void>
  setSessionWorkspace: (id: string, path: string) => void
  closeCurrentSession: () => Promise<void>
  selectNextSession: () => void
  selectPrevSession: () => void
}

export const createSessionSlice: StateCreator<SessionSlice> = (set, get) => ({
  sessions: [],
  currentSessionId: null,
  pinnedIds: new Set(),
  selectedSessionIds: new Set(),
  sessionWorkspaces: {},
  defaultWorkspace: null,

  setSessions: (sessions) => set({ sessions }),
  setCurrentSessionId: (currentSessionId) => set({ currentSessionId }),

  createSession: async () => {
    const result = await rpc<{ session_id: string }>('session.create', undefined, t('sessions.created'))
    await get().loadSessions()
    return result.session_id
  },

  switchSession: async (id) => {
    if (get().selectedSessionIds.size > 0) {
      get().toggleSessionSelect(id)
      return
    }
    set({ currentSessionId: id })
    try {
      await loomRpc('session.switch', { session_id: id })
    } catch {
      ;(get() as any).addToast?.({ type: 'warning', message: t('sessions.switchFailed') })
    }
    // If this session is currently streaming and we already have its messages
    // cached locally, skip fetching from backend. The backend may not have
    // persisted the in-progress assistant message yet, and overwriting the
    // local state would drop accumulated streaming content + break the flush
    // mechanism (message ID mismatch after hydrateMessages replaces IDs).
    const isStreaming = (get() as any).streamingSessionIds?.has(id)
    const hasCached = (get() as any).messagesBySession?.has(id)
    if (isStreaming && hasCached) {
      return
    }
    try {
      const result = await loomRpc<{ messages: any[] }>('session.messages', { session_id: id })
      const allMsgs: any[] = result.messages || []

      // Merge tool_result content into the preceding assistant message so
      // parseContentParts can pair tool_call ↔ tool_result within one array.
      for (let i = 0; i < allMsgs.length; i++) {
        const m = allMsgs[i]
        const role = typeof m.role === 'string' ? m.role.toLowerCase() : ''
        if (role === 'tool' && i > 0) {
          // Find the preceding assistant message (skip over other tool msgs)
          let prev = i - 1
          while (prev >= 0) {
            const pr = typeof allMsgs[prev].role === 'string' ? allMsgs[prev].role.toLowerCase() : ''
            if (pr === 'assistant') break
            prev--
          }
          if (prev >= 0) {
            const assistantContent = allMsgs[prev].content
            if (Array.isArray(assistantContent)) {
              const toolParts = Array.isArray(m.content) ? m.content : []
              assistantContent.push(...toolParts)
            }
          }
        }
      }

      const rawMsgs = allMsgs
        .filter((m: any) => {
          const role = typeof m.role === 'string' ? m.role.toLowerCase() : ''
          return role !== 'tool'
        })
        .map((m: any, i: number) => ({
        id: `hist-${id}-${i}`,
        role: parseRole(m.role),
        blocks: parseContentParts(m.content, id, get().port),
        timestamp: m.timestamp || new Date().toISOString(),
        usage: m.usage ? {
          prompt: m.usage.prompt_tokens || 0,
          completion: m.usage.completion_tokens || 0,
          cached: m.usage.cached_tokens || 0,
          cacheRead: m.usage.cache_read_tokens || 0,
          cacheWrite: m.usage.cache_write_tokens || 0,
          contextWindow: m.usage.context_window || 0,
        } : undefined,
      }))

      // Merge consecutive assistant messages into one (agent loop iterations
      // produce multiple Assistant rows per turn; the frontend expects a single
      // message with all blocks combined).
      const msgs = rawMsgs.reduce((acc: typeof rawMsgs, msg) => {
        if (msg.role === 'assistant' && acc.length > 0 && acc[acc.length - 1].role === 'assistant') {
          const prev = acc[acc.length - 1]
          prev.blocks = [...prev.blocks, ...msg.blocks]
          // Use the last message's usage (final turn has complete token count)
          if (msg.usage) prev.usage = msg.usage
          prev.timestamp = msg.timestamp
          return acc
        }
        acc.push(msg)
        return acc
      }, [] as typeof rawMsgs)

      ;(get() as any).hydrateMessages?.(id, msgs)

      // Rebuild sessionCumulative from hydrated message history
      const store = get() as any
      store.clearSessionUsage?.(id)
      store.clearSessionCumulative?.(id)
      let latestUsage: any = null
      for (const m of msgs) {
        if (m.role === 'assistant' && m.usage) {
          latestUsage = m.usage
          store.accumulateSessionUsage?.(id, {
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
      if (latestUsage) {
        store.setSessionUsage?.(id, {
          prompt: latestUsage.prompt || 0,
          completion: latestUsage.completion || 0,
          model: latestUsage.model || store.currentModel || '',
          contextWindow: latestUsage.contextWindow || 0,
          cached: latestUsage.cached || 0,
          cacheRead: latestUsage.cacheRead || 0,
          cacheWrite: latestUsage.cacheWrite || 0,
        })
      }
    } catch {
      ;(get() as any).addToast?.({ type: 'error', message: t('sessions.loadFailed') })
    }
  },

  renameSession: async (id, title) => {
    await rpc('session.rename', { session_id: id, title }, t('sessions.renamed'))
    await get().loadSessions()
  },

  deleteSession: async (id) => {
    await rpc('session.delete', { session_id: id }, t('sessions.deleted'))
    if (get().currentSessionId === id) {
      set({ currentSessionId: null })
    }
    await get().loadSessions()
  },

  deleteSessions: async (ids) => {
    for (const id of ids) {
      await loomRpc('session.delete', { session_id: id })
    }
    const currentId = get().currentSessionId
    if (currentId && ids.includes(currentId)) {
      set({ currentSessionId: null })
    }
    set({ selectedSessionIds: new Set() })
    await get().loadSessions()
  },

  pinSession: (id) => {
    const next = new Set(get().pinnedIds)
    next.add(id)
    set({ pinnedIds: next })
    window.loom.setPreference('pinnedIds', [...next])
  },

  unpinSession: (id) => {
    const next = new Set(get().pinnedIds)
    next.delete(id)
    set({ pinnedIds: next })
    window.loom.setPreference('pinnedIds', [...next])
  },

  pinSessions: (ids) => {
    const next = new Set(get().pinnedIds)
    for (const id of ids) next.add(id)
    set({ pinnedIds: next })
    window.loom.setPreference('pinnedIds', [...next])
  },

  unpinSessions: (ids) => {
    const next = new Set(get().pinnedIds)
    for (const id of ids) next.delete(id)
    set({ pinnedIds: next })
    window.loom.setPreference('pinnedIds', [...next])
  },

  toggleSessionSelect: (id) => {
    const next = new Set(get().selectedSessionIds)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    set({ selectedSessionIds: next })
  },

  selectAllSessions: () => {
    const ids = new Set(get().sessions.map(s => s.path))
    set({ selectedSessionIds: ids })
  },

  deselectAllSessions: () => {
    set({ selectedSessionIds: new Set() })
  },

  loadSessions: async () => {
    const result = await loomRpc<{ sessions: any[] }>('session.list')
    const mapped: SessionSummary[] = (result.sessions || []).map((s: any) => ({
      path: s.id || s.path || '',
      title: s.title || null,
      firstMessage: '',
      modified: s.updated_at || s.created_at || '',
      createdAt: s.created_at || s.updated_at || '',
      messageCount: s.message_count ?? 0,
      agentId: null,
      agentName: s.agent_config_name || null,
      cwd: null,
      pinnedAt: null,
    })).filter((s: SessionSummary) => !(s.title || '').startsWith('[写]'))
    set({ sessions: mapped })
    // Restore agent bindings for all sessions
    const bindings: Record<string, string> = {}
    for (const s of mapped) {
      if (s.agentName && s.agentName !== 'default') {
        bindings[s.path] = s.agentName
      }
    }
    set({ sessionAgentBindings: bindings } as any)
    // Load workspace bindings for all sessions in parallel (was serial — an
    // N+1 await that ran one round-trip per session and made reconnects slow).
    const workspaces: Record<string, string> = {}
    await Promise.all(
      mapped.map(async (s) => {
        try {
          const result = await loomRpc<{ workspace: string | null }>('workspace.get', { session_id: s.path })
          if (result.workspace) {
            workspaces[s.path] = result.workspace
          }
        } catch {
          // Ignore errors for individual sessions
        }
      }),
    )
    // Also load the default workspace (no session_id)
    let defaultWorkspace: string | null = null
    try {
      const result = await loomRpc<{ workspace: string | null }>('workspace.get', {})
      defaultWorkspace = result.workspace
    } catch { /* ignore */ }
    set({ sessionWorkspaces: workspaces, defaultWorkspace })
  },

  setSessionWorkspace: (id, path) => {
    set((state) => ({
      sessionWorkspaces: { ...state.sessionWorkspaces, [id]: path },
    }))
  },

  closeCurrentSession: async () => {
    const { currentSessionId, sessions, deleteSession } = get()
    if (!currentSessionId) return
    const idx = sessions.findIndex((s) => s.path === currentSessionId)
    if (idx < sessions.length - 1) {
      get().switchSession(sessions[idx + 1].path)
    } else if (idx > 0) {
      get().switchSession(sessions[idx - 1].path)
    }
    await deleteSession(currentSessionId)
  },

  selectNextSession: () => {
    const { currentSessionId, sessions, switchSession } = get()
    if (!currentSessionId || sessions.length <= 1) return
    const idx = sessions.findIndex((s) => s.path === currentSessionId)
    if (idx < 0 || idx >= sessions.length - 1) return
    switchSession(sessions[idx + 1].path)
  },

  selectPrevSession: () => {
    const { currentSessionId, sessions, switchSession } = get()
    if (!currentSessionId || sessions.length <= 1) return
    const idx = sessions.findIndex((s) => s.path === currentSessionId)
    if (idx <= 0) return
    switchSession(sessions[idx - 1].path)
  },
})

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

/**
 * Parse the role field from backend Message.
 * Rust's Role enum may serialize as a lowercase string ("user", "assistant", "system", "tool")
 * or as a tagged enum object like {"User": {}}.
 */
export function parseRole(role: any): 'user' | 'assistant' {
  if (typeof role === 'string') {
    const lower = role.toLowerCase()
    if (lower === 'assistant' || lower === 'system') return 'assistant'
    return 'user'
  }
  // Tagged enum object: check key names
  if (role && typeof role === 'object') {
    const key = Object.keys(role)[0]?.toLowerCase()
    if (key === 'assistant' || key === 'system') return 'assistant'
    return 'user'
  }
  return 'user'
}

/**
 * Parse backend ContentPart[] into frontend ContentBlock[].
 *
 * Rust's ContentPart enum with serde default tagging serializes as snake_case:
 *   { "text": { "text": "hello" } }
 *   { "tool_call": { "id": "...", "name": "...", "arguments": "..." } }
 *   { "tool_result": { "tool_call_id": "...", "content": "..." } }
 *   { "image": { "source_type": "...", "media_type": "...", "data": "..." } }
 *
 * Or possibly flat objects like { "type": "text", "text": "hello" }.
 */
export function parseContentParts(content: any, sessionId: string, port: number): any[] {
  // If content is a plain string (legacy format), treat as single text block
  if (typeof content === 'string') {
    return [{ type: 'text', html: sanitizeHtml(renderMarkdown(content)), source: content }]
  }

  // If not an array, wrap it
  if (!Array.isArray(content)) {
    const text = JSON.stringify(content)
    return [{ type: 'text', html: sanitizeHtml(renderMarkdown(text)), source: text }]
  }

  const blocks: any[] = []
  // Track pending use_skill block so we can pair it with its tool_result
  let pendingSkillBlock: any = null
  let pendingSkillCallId: string | null = null

  for (const part of content) {
    if (!part || typeof part !== 'object') continue

    // Serde tagged enum format (snake_case): { "text": { "text": "..." } }
    if ('thinking' in part) {
      const thinking = part.thinking
      const text = typeof thinking === 'string' ? thinking : (thinking?.text || '')
      blocks.push({ type: 'thinking', content: text, sealed: true })
    } else if ('text' in part) {
      const t = part.text
      // Handle both { text: "string" } and { text: { text: "string" } }
      const text = typeof t === 'string' ? t : (t?.text || t?.content || '')
      blocks.push({ type: 'text', html: sanitizeHtml(renderMarkdown(text)), source: text })
    } else if ('tool_call' in part) {
      const tc = part.tool_call
      // arguments may be a JSON string (OpenAI format) or already an object (serde Value)
      let args: Record<string, unknown> = {}
      if (typeof tc.arguments === 'string') {
        try { args = JSON.parse(tc.arguments || '{}') } catch { /* ignore */ }
      } else if (tc.arguments && typeof tc.arguments === 'object') {
        args = tc.arguments as Record<string, unknown>
      }
      if (tc.name === 'use_skill') {
        // Defer push — wait for matching tool_result to capture the result content
        pendingSkillCallId = tc.id || null
        pendingSkillBlock = {
          type: 'skill',
          name: (args.skill_name as string) || 'unknown',
          status: 'done',
          sealed: true,
        }
      } else if (tc.name === 'request_tools') {
        // meta-tool — skip
      } else {
        blocks.push({
          type: 'shell',
          toolName: tc.name || 'unknown',
          status: 'done',
          args,
          sealed: true,
        })
      }
    } else if ('tool_result' in part) {
      const tr = part.tool_result
      // Pair use_skill tool_call with its tool_result: capture result content
      if (pendingSkillBlock && pendingSkillCallId && tr.tool_call_id === pendingSkillCallId) {
        const content = tr.content
        pendingSkillBlock.result = typeof content === 'string' ? content : (content != null ? JSON.stringify(content) : '')
        blocks.push(pendingSkillBlock)
        pendingSkillBlock = null
        pendingSkillCallId = null
      } else {
        // Pair non-skill tool_call (shell, file, etc.) with its tool_result
        const content = tr.content
        const resultText = typeof content === 'string' ? content : (content != null ? JSON.stringify(content) : '')
        // Find the last shell block without a result and attach it
        let paired = false
        for (let j = blocks.length - 1; j >= 0; j--) {
          if (blocks[j].type === 'shell' && !blocks[j].result) {
            blocks[j].result = resultText
            paired = true
            break
          }
        }
        // If no matching shell block found, create a standalone entry
        if (!paired) {
          const toolName = tr.name || 'unknown'
          if (toolName !== 'request_tools') {
            blocks.push({
              type: 'shell',
              toolName,
              status: 'done',
              args: {},
              result: resultText,
              sealed: true,
            })
          }
        }
      }
      continue
    } else if ('image' in part) {
      const img = part.image || {}
      const mimeType = img.media_type || 'image/png'
      const data = img.data || ''
      blocks.push({
        type: 'image',
        path: '',
        name: '',
        mimeType,
        thumbnail: data ? `data:${mimeType};base64,${data}` : '',
      })
    } else if ('image_ref' in part) {
      const ir = part.image_ref || {}
      const mimeType = ir.media_type || 'image/png'
      const fileId = ir.file_id || ''
      const url = fileId ? `http://127.0.0.1:${port}/sessions/${sessionId}/images/${fileId}` : ''
      blocks.push({
        type: 'image',
        path: '',
        name: fileId,
        mimeType,
        thumbnail: url,
      })
    }
    // Flat object format: { type: "text", text: "..." }
    else if (part.type === 'text' || part.type === 'Text') {
      const text = part.text || ''
      blocks.push({ type: 'text', html: sanitizeHtml(renderMarkdown(text)), source: text })
    } else if (part.type === 'tool_call' || part.type === 'ToolCall') {
      let args: Record<string, unknown> = {}
      if (typeof part.arguments === 'string') {
        try { args = JSON.parse(part.arguments || '{}') } catch { /* ignore */ }
      } else if (part.arguments && typeof part.arguments === 'object') {
        args = part.arguments as Record<string, unknown>
      }
      if (part.name === 'use_skill') {
        // Defer push — wait for matching tool_result to capture the result content
        pendingSkillCallId = part.id || null
        pendingSkillBlock = {
          type: 'skill',
          name: (args.skill_name as string) || 'unknown',
          status: 'done',
          sealed: true,
        }
      } else if (part.name === 'request_tools') {
        // meta-tool — skip
      } else {
        blocks.push({
          type: 'shell',
          toolName: part.name || 'unknown',
          status: 'done',
          args,
          sealed: true,
        })
      }
    } else if (part.type === 'tool_result' || part.type === 'ToolResult') {
      // Pair use_skill tool_call with its tool_result: capture result content
      const toolCallId = part.tool_call_id || ''
      if (pendingSkillBlock && pendingSkillCallId && toolCallId === pendingSkillCallId) {
        const content = part.content
        pendingSkillBlock.result = typeof content === 'string' ? content : (content != null ? JSON.stringify(content) : '')
        blocks.push(pendingSkillBlock)
        pendingSkillBlock = null
        pendingSkillCallId = null
      } else {
        // Pair non-skill tool_call (shell, file, etc.) with its tool_result
        const content = part.content
        const resultText = typeof content === 'string' ? content : (content != null ? JSON.stringify(content) : '')
        for (let j = blocks.length - 1; j >= 0; j--) {
          if (blocks[j].type === 'shell' && !blocks[j].result) {
            blocks[j].result = resultText
            break
          }
        }
      }
      continue
    } else {
      // Unknown format — try to extract text content before falling back to JSON
      const extracted = part.content || part.text || part.data || ''
      const text = typeof extracted === 'string' ? extracted : JSON.stringify(part)
      blocks.push({ type: 'text', html: sanitizeHtml(renderMarkdown(text)), source: text })
    }
  }

  // If a use_skill block was never paired with a tool_result, push it anyway
  if (pendingSkillBlock) {
    blocks.push(pendingSkillBlock)
  }

  // If no blocks were produced, return a fallback empty text block
  if (blocks.length === 0) {
    blocks.push({ type: 'text', html: '', source: '' })
  }

  return blocks
}

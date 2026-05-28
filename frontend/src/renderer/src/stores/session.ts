import { StateCreator } from 'zustand'
import { loomRpc } from '../services/jsonrpc'
import { rpc } from '../services/rpc-toast'
import { renderMarkdown } from '../utils/markdown'
import { sanitizeHtml } from '../utils/markdown-sanitizer'

// Matches actual backend SessionData JSON response.
// Fields are camelCase as returned by serde Serialize.
export interface SessionSummary {
  path: string        // session ID (UUID)
  title: string | null
  firstMessage: string
  modified: string    // ISO 8601 date
  messageCount: number
  agentId: string | null
  agentName: string | null
  cwd: string | null
  permissionMode: string | null
  pinnedAt: string | null
}

export interface SessionSlice {
  sessions: SessionSummary[]
  currentSessionId: string | null
  pinnedIds: Set<string>
  setSessions: (sessions: SessionSummary[]) => void
  setCurrentSessionId: (id: string | null) => void
  createSession: () => Promise<string>
  switchSession: (id: string) => Promise<void>
  renameSession: (id: string, title: string) => Promise<void>
  deleteSession: (id: string) => Promise<void>
  pinSession: (id: string) => void
  unpinSession: (id: string) => void
  loadSessions: () => Promise<void>
}

export const createSessionSlice: StateCreator<SessionSlice> = (set, get) => ({
  sessions: [],
  currentSessionId: null,
  pinnedIds: new Set(),

  setSessions: (sessions) => set({ sessions }),
  setCurrentSessionId: (currentSessionId) => set({ currentSessionId }),

  createSession: async () => {
    const result = await rpc<{ session_id: string }>('session.create', undefined, '会话已创建')
    await get().loadSessions()
    return result.session_id
  },

  switchSession: async (id) => {
    // Set immediately so UI responds before RPC completes
    set({ currentSessionId: id })
    try {
      await loomRpc('session.switch', { session_id: id })
    } catch {
      // Non-critical — session might already exist
    }
    // Load existing messages for this session
    try {
      const result = await loomRpc<{ messages: any[] }>('session.messages', { session_id: id })
      if (result.messages?.length) {
        const msgs = result.messages.map((m: any, i: number) => ({
          id: `hist-${id}-${i}`,
          role: parseRole(m.role),
          blocks: parseContentParts(m.content),
          timestamp: m.timestamp || new Date().toISOString(),
          usage: m.usage ? {
            prompt: m.usage.prompt_tokens || 0,
            completion: m.usage.completion_tokens || 0,
          } : undefined,
        }))
        ;(get() as any).hydrateMessages?.(id, msgs)
      }
    } catch {
      // No persisted messages
    }
  },

  renameSession: async (id, title) => {
    await rpc('session.rename', { session_id: id, title }, '已重命名')
    await get().loadSessions()
  },

  deleteSession: async (id) => {
    await rpc('session.delete', { session_id: id }, '会话已删除')
    if (get().currentSessionId === id) {
      set({ currentSessionId: null })
    }
    await get().loadSessions()
  },

  pinSession: (id) => {
    const next = new Set(get().pinnedIds)
    next.add(id)
    set({ pinnedIds: next })
    window.hana.setPreference('pinnedIds', [...next])
  },

  unpinSession: (id) => {
    const next = new Set(get().pinnedIds)
    next.delete(id)
    set({ pinnedIds: next })
    window.hana.setPreference('pinnedIds', [...next])
  },

  loadSessions: async () => {
    const result = await loomRpc<{ sessions: any[] }>('session.list')
    const mapped: SessionSummary[] = (result.sessions || []).map((s: any) => ({
      path: s.id || s.path || '',
      title: s.title || null,
      firstMessage: '',
      modified: s.created_at || '',
      messageCount: s.message_count ?? 0,
      agentId: null,
      agentName: s.agent_config_name || null,
      cwd: null,
      permissionMode: null,
      pinnedAt: null,
    }))
    set({ sessions: mapped })
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
function parseRole(role: any): 'user' | 'assistant' {
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
function parseContentParts(content: any): any[] {
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

  for (const part of content) {
    if (!part || typeof part !== 'object') continue

    // Serde tagged enum format (snake_case): { "text": { "text": "..." } }
    if ('thinking' in part) {
      const text = part.thinking?.text || ''
      blocks.push({ type: 'thinking', content: text, sealed: true })
    } else if ('text' in part) {
      const text = part.text?.text || ''
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
      blocks.push({
        type: 'tool_group',
        tools: [{
          id: tc.id || `tc-${blocks.length}`,
          name: tc.name || 'unknown',
          status: 'done' as const,
          elapsed: 0,
          args,
          result: undefined,
        }],
        collapsed: true,
      })
    } else if ('tool_result' in part) {
      // Skip — already represented by the corresponding ToolCall block
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
      blocks.push({
        type: 'tool_group',
        tools: [{
          id: part.id || `tc-${blocks.length}`,
          name: part.name || 'unknown',
          status: 'done' as const,
          elapsed: 0,
          args,
          result: undefined,
        }],
        collapsed: true,
      })
    } else if (part.type === 'tool_result' || part.type === 'ToolResult') {
      continue
    } else {
      // Unknown format — render as text
      const text = JSON.stringify(part)
      blocks.push({ type: 'text', html: sanitizeHtml(renderMarkdown(text)), source: text })
    }
  }

  // If no blocks were produced, return a fallback empty text block
  if (blocks.length === 0) {
    blocks.push({ type: 'text', html: '', source: '' })
  }

  return blocks
}

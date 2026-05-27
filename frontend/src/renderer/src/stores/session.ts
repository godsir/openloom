import { StateCreator } from 'zustand'
import { loomRpc } from '../services/jsonrpc'

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
    const result = await loomRpc<{ session_id: string }>('session.create')
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
          role: m.role || 'user',
          blocks: [{ type: 'text', html: escapeHtml(m.content || ''), source: m.content || '' }],
          timestamp: m.timestamp || new Date().toISOString(),
        }))
        ;(get() as any).hydrateMessages?.(id, msgs)
      }
    } catch {
      // No persisted messages
    }
  },

  renameSession: async (id, title) => {
    await loomRpc('session.rename', { session_id: id, title })
    await get().loadSessions()
  },

  deleteSession: async (id) => {
    await loomRpc('session.delete', { session_id: id })
    if (get().currentSessionId === id) {
      set({ currentSessionId: null })
    }
    await get().loadSessions()
  },

  pinSession: (id) => {
    const next = new Set(get().pinnedIds)
    next.add(id)
    set({ pinnedIds: next })
  },

  unpinSession: (id) => {
    const next = new Set(get().pinnedIds)
    next.delete(id)
    set({ pinnedIds: next })
  },

  loadSessions: async () => {
    const result = await loomRpc<{ sessions: SessionSummary[] }>('session.list')
    set({ sessions: result.sessions })
  },
})

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

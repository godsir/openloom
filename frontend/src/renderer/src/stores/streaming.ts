import { StateCreator } from 'zustand'

export interface StreamingSlice {
  streamingSessionIds: Set<string>
  inlineErrors: Map<string, { text: string; timer: ReturnType<typeof setTimeout> | null }>
  addStreamingSession: (id: string) => void
  removeStreamingSession: (id: string) => void
  setInlineError: (sessionId: string, text: string) => void
  clearInlineError: (sessionId: string) => void
}

export const createStreamingSlice: StateCreator<StreamingSlice> = (set, get) => ({
  streamingSessionIds: new Set(),
  inlineErrors: new Map(),

  addStreamingSession: (id) => {
    const next = new Set(get().streamingSessionIds)
    next.add(id)
    set({ streamingSessionIds: next })
  },

  removeStreamingSession: (id) => {
    const next = new Set(get().streamingSessionIds)
    next.delete(id)
    set({ streamingSessionIds: next })
  },

  setInlineError: (sessionId, text) => {
    const prev = get().inlineErrors.get(sessionId)
    if (prev?.timer) clearTimeout(prev.timer)

    const timer = setTimeout(() => {
      get().clearInlineError(sessionId)
    }, 5000)

    const next = new Map(get().inlineErrors)
    next.set(sessionId, { text, timer })
    set({ inlineErrors: next })
  },

  clearInlineError: (sessionId) => {
    const prev = get().inlineErrors.get(sessionId)
    if (prev?.timer) clearTimeout(prev.timer)

    const next = new Map(get().inlineErrors)
    next.delete(sessionId)
    set({ inlineErrors: next })
  },
})

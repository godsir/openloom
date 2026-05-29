import { StateCreator } from 'zustand'
import type { ModelListItem } from '../types/bindings'

export type ThinkingLevel = 'off' | 'auto' | 'low' | 'medium' | 'high' | 'xhigh'

/** Per-session token usage snapshot — sourced from the last assistant turn.
 *  `model` and `contextWindow` are the values reported by the backend at the
 *  time that turn ran, so a mid-session model switch is reflected naturally
 *  on the next turn instead of being mixed with a stale ring scale. */
export interface SessionUsage {
  prompt: number
  completion: number
  model: string
  contextWindow: number
}

export interface ModelSlice {
  models: ModelListItem[]
  currentModel: string
  thinkingLevel: ThinkingLevel
  /** Per-session usage, keyed by session_id. */
  usageBySession: Map<string, SessionUsage>
  setModels: (models: ModelListItem[]) => void
  setCurrentModel: (model: string) => void
  setThinkingLevel: (level: ThinkingLevel) => void
  setSessionUsage: (sessionId: string, usage: SessionUsage) => void
  clearSessionUsage: (sessionId: string) => void
}

export const createModelSlice: StateCreator<ModelSlice> = (set, get) => ({
  models: [],
  currentModel: '',
  thinkingLevel: 'auto',
  usageBySession: new Map(),
  setModels: (models) => set({ models }),
  setCurrentModel: (currentModel) => set({ currentModel }),
  setThinkingLevel: (thinkingLevel) => set({ thinkingLevel }),
  setSessionUsage: (sessionId, usage) => {
    const next = new Map(get().usageBySession)
    next.set(sessionId, usage)
    set({ usageBySession: next })
  },
  clearSessionUsage: (sessionId) => {
    const next = new Map(get().usageBySession)
    if (next.delete(sessionId)) set({ usageBySession: next })
  },
})

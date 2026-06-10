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
  cached: number
  cacheRead: number
  cacheWrite: number
}

/** Per-session cumulative usage across all turns this app session */
export interface SessionCumulative {
  prompt: number
  completion: number
  cacheRead: number
  cacheWrite: number
  requests: number
  cost: number
}

export interface ModelSlice {
  models: ModelListItem[]
  currentModel: string
  thinkingLevel: ThinkingLevel
  /** Per-session usage of the latest turn, keyed by session_id. */
  usageBySession: Map<string, SessionUsage>
  /** Per-session cumulative usage across all turns, keyed by session_id. */
  sessionCumulative: Map<string, SessionCumulative>
  setModels: (models: ModelListItem[]) => void
  setCurrentModel: (model: string) => void
  setThinkingLevel: (level: ThinkingLevel) => void
  setSessionUsage: (sessionId: string, usage: SessionUsage) => void
  accumulateSessionUsage: (sessionId: string, usage: SessionUsage) => void
  clearSessionUsage: (sessionId: string) => void
  clearSessionCumulative: (sessionId: string) => void
}

export const createModelSlice: StateCreator<ModelSlice> = (set, get) => ({
  models: [],
  currentModel: '',
  thinkingLevel: 'medium',
  usageBySession: new Map(),
  sessionCumulative: new Map(),
  setModels: (models) => set({ models }),
  setCurrentModel: (currentModel) => set({ currentModel }),
  setThinkingLevel: (thinkingLevel) => {
    window.loom.setPreference('thinkingLevel', thinkingLevel)
    set({ thinkingLevel })
  },
  setSessionUsage: (sessionId, usage) => {
    const next = new Map(get().usageBySession)
    next.set(sessionId, usage)
    set({ usageBySession: next })
  },
  accumulateSessionUsage: (sessionId, usage) => {
    // Accumulate into session cumulative — only called from chat.token_usage (final turn usage)
    const cumNext = new Map(get().sessionCumulative)
    const prev = cumNext.get(sessionId) || { prompt: 0, completion: 0, cacheRead: 0, cacheWrite: 0, requests: 0, cost: 0 }
    const m = get().models.find(x => x.name === usage.model)
    const inputPrice = m?.input_price || 0
    const outputPrice = m?.output_price || 0
    const cacheReadPrice = m?.cache_read_price || 0
    const cacheWritePrice = m?.cache_write_price || 0
    const promptNonCache = Math.max(0, usage.prompt - usage.cacheRead)
    const turnCost =
      (promptNonCache * inputPrice +
       usage.cacheRead * cacheReadPrice +
       usage.cacheWrite * cacheWritePrice +
       usage.completion * outputPrice) / 1_000_000
    cumNext.set(sessionId, {
      prompt: prev.prompt + usage.prompt,
      completion: prev.completion + usage.completion,
      cacheRead: prev.cacheRead + usage.cacheRead,
      cacheWrite: prev.cacheWrite + usage.cacheWrite,
      requests: prev.requests + 1,
      cost: prev.cost + turnCost,
    })
    set({ sessionCumulative: cumNext })
  },
  clearSessionUsage: (sessionId) => {
    const next = new Map(get().usageBySession)
    if (next.delete(sessionId)) set({ usageBySession: next })
  },
  clearSessionCumulative: (sessionId) => {
    const cumNext = new Map(get().sessionCumulative)
    if (cumNext.delete(sessionId)) set({ sessionCumulative: cumNext })
  },
})

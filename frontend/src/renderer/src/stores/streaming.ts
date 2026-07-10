import { StateCreator } from 'zustand'

/** AI 生成过程中的子阶段，用于灵动岛流转显示 */
export type StreamPhase = 'thinking' | 'vision' | 'skill' | 'tool' | 'team' | 'generating'

export interface StreamingActivity {
  phase: StreamPhase
  /** 工具/技能名称（skill/tool 阶段） */
  detail?: string
  /** 视觉处理进度 */
  visionDone?: number
  visionTotal?: number
}

export interface StreamingSlice {
  streamingSessionIds: Set<string>
  /** 按 sessionId 索引的生成子状态，供灵动岛读取 */
  streamingActivity: Record<string, StreamingActivity>
  inlineErrors: Map<string, { text: string; timer: ReturnType<typeof setTimeout> | null }>
  /** 灵动岛瞬态反馈（复制成功等），短暂显示后自动清除 */
  islandTransient: { text: string; platform?: string; timer: ReturnType<typeof setTimeout> | null } | null
  addStreamingSession: (id: string) => void
  removeStreamingSession: (id: string) => void
  setStreamingActivity: (id: string, activity: StreamingActivity | null) => void
  setInlineError: (sessionId: string, text: string) => void
  clearInlineError: (sessionId: string) => void
  showIslandTransient: (text: string, duration?: number, platform?: string) => void
  clearIslandTransient: () => void
}

export const createStreamingSlice: StateCreator<StreamingSlice> = (set, get) => ({
  streamingSessionIds: new Set(),
  streamingActivity: {},
  inlineErrors: new Map(),
  islandTransient: null,

  addStreamingSession: (id) => {
    const next = new Set(get().streamingSessionIds)
    next.add(id)
    set({ streamingSessionIds: next })
  },

  removeStreamingSession: (id) => {
    const next = new Set(get().streamingSessionIds)
    next.delete(id)
    const nextActivity = { ...get().streamingActivity }
    delete nextActivity[id]
    set({ streamingSessionIds: next, streamingActivity: nextActivity })
  },

  setStreamingActivity: (id, activity) => {
    const prev = get().streamingActivity
    if (activity === null) {
      if (!(id in prev)) return
      const next = { ...prev }
      delete next[id]
      set({ streamingActivity: next })
    } else {
      // 浅比较，避免无变化时触发重渲染
      const old = prev[id]
      if (old && old.phase === activity.phase && old.detail === activity.detail
        && old.visionDone === activity.visionDone && old.visionTotal === activity.visionTotal) return
      set({ streamingActivity: { ...prev, [id]: activity } })
    }
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

  showIslandTransient: (text, duration = 1500, platform) => {
    const prev = get().islandTransient
    if (prev?.timer) clearTimeout(prev.timer)
    const timer = setTimeout(() => get().clearIslandTransient(), duration)
    set({ islandTransient: { text, platform, timer } })
  },

  clearIslandTransient: () => {
    const prev = get().islandTransient
    if (prev?.timer) clearTimeout(prev.timer)
    set({ islandTransient: null })
  },
})

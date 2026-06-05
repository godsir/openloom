import { StateCreator } from 'zustand'
import { loomRpc } from '../services/jsonrpc'

export interface TokenUsageRecord {
  session_id: string
  model: string
  prompt: number
  completion: number
  cached: number
  latency_ms: number
  context_window: number
}

export interface TokenSummary {
  total_prompt_tokens: number
  total_completion_tokens: number
  total_cached_tokens: number
  total_requests: number
  avg_latency_ms: number
  cache_hit_rate: number
  total_cost: number
  by_model: Array<{
    model: string
    prompt: number
    completion: number
    cached: number
    cache_miss_tokens: number
    cache_hit_tokens: number
    cache_write_tokens: number
    requests: number
    avg_latency_ms: number
    avg_context_utilization: number
    input_price: number
    output_price: number
    cache_read_price: number
    cache_write_price: number
    cost: number
  }>
}

export interface TokenHistoryPoint {
  date: string
  prompt: number
  completion: number
  cached: number
  requests: number
  by_model: Record<string, { prompt: number; completion: number; requests: number }>
}

export interface TokenStatsSlice {
  sessionTotal: { prompt: number; completion: number; cached: number; requests: number }
  sessionByModel: Record<string, { prompt: number; completion: number; cached: number; requests: number }>
  summary: TokenSummary | null
  history: TokenHistoryPoint[]
  loading: boolean
  loadError: string | null
  timeRange: 'all' | 'today' | '7d' | '30d'

  recordUsage: (usage: TokenUsageRecord) => void
  loadSummary: (from: string, to: string) => Promise<void>
  loadHistory: (from: string, to: string, granularity: string) => Promise<void>
  setTimeRange: (range: 'all' | 'today' | '7d' | '30d') => void
  resetTokenUsage: () => Promise<void>
}

export const createTokenStatsSlice: StateCreator<TokenStatsSlice> = (set, get) => ({
  sessionTotal: { prompt: 0, completion: 0, cached: 0, requests: 0 },
  sessionByModel: {},
  summary: null,
  history: [],
  loading: false,
  loadError: null,
  timeRange: 'all',

  recordUsage: (usage) => {
    set((s) => {
      const total = {
        prompt: s.sessionTotal.prompt + usage.prompt,
        completion: s.sessionTotal.completion + usage.completion,
        cached: s.sessionTotal.cached + usage.cached,
        requests: s.sessionTotal.requests + 1,
      }
      const prev = s.sessionByModel[usage.model] || { prompt: 0, completion: 0, cached: 0, requests: 0 }
      const byModel = {
        ...s.sessionByModel,
        [usage.model]: {
          prompt: prev.prompt + usage.prompt,
          completion: prev.completion + usage.completion,
          cached: prev.cached + usage.cached,
          requests: prev.requests + 1,
        },
      }
      return { sessionTotal: total, sessionByModel: byModel }
    })
  },

  loadSummary: async (from, to) => {
    set({ loading: true, loadError: null })
    try {
      const data = await loomRpc<TokenSummary>('stats.token_summary', { from, to })
      set({ summary: data, loading: false })
    } catch (e: any) {
      set({ loading: false, loadError: e?.message ?? '获取Token摘要失败' })
    }
  },

  loadHistory: async (from, to, granularity) => {
    set({ loading: true, loadError: null })
    try {
      const data = await loomRpc<{ points: TokenHistoryPoint[] }>('stats.token_history', { from, to, granularity })
      set({ history: data.points || [], loading: false })
    } catch (e: any) {
      set({ loading: false, loadError: e?.message ?? '获取历史数据失败' })
    }
  },

  setTimeRange: (range) => {
    set({ timeRange: range })
    const now = new Date()
    const today = now.toISOString().slice(0, 10)
    let from = '1970-01-01'
    if (range === 'today') {
      from = today + ' 00:00:00'
    } else if (range === '7d') {
      const d = new Date(now);
      d.setDate(d.getDate() - 7);
      from = d.toISOString().slice(0, 10) + ' 00:00:00'
    } else if (range === '30d') {
      const d = new Date(now);
      d.setDate(d.getDate() - 30);
      from = d.toISOString().slice(0, 10) + ' 00:00:00'
    }
    const to = range === 'all'
      ? '2099-12-31'
      : range === 'today'
        ? today + ' 23:59:59'
        : now.toISOString().slice(0, 10) + ' 23:59:59'
    get().loadSummary(from, to)
    get().loadHistory(from, to, 'day')
  },

  resetTokenUsage: async () => {
    await loomRpc('stats.reset')
    set({
      sessionTotal: { prompt: 0, completion: 0, cached: 0, requests: 0 },
      sessionByModel: {},
      summary: null,
      history: [],
    })
    // Reload to show empty state
    get().setTimeRange(get().timeRange)
  },
})

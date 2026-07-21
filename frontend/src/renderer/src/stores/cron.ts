import { StateCreator } from 'zustand'

// --- Cron types (shared between store and UI) ---

export interface CronJobSummary {
  id: string
  name: string
  cron_expression: string
  prompt: string
  enabled: boolean
  session_mode: 'isolated' | 'current'
  /** 每任务模型覆盖（缺省 = 当前激活模型） */
  model?: string
  last_run: number | null
  next_run: number | null
  run_count: number
  error_count: number
  last_status: 'running' | 'completed' | 'failed' | 'timed_out' | null
}

export interface CronRunHistory {
  id: string
  job_id: string
  started_at: number
  finished_at: number | null
  status: 'running' | 'completed' | 'failed' | 'timed_out'
  response: string | null
  error_message: string | null
}

// --- Slice ---

export interface CronSlice {
  cronJobs: CronJobSummary[]
  cronLoading: boolean
  cronError: string | null
  cronHistoryJobId: string | null
  cronHistory: CronRunHistory[]
  cronHistoryLoading: boolean
  cronEditJobId: string | null
  setCronJobs: (jobs: CronJobSummary[]) => void
  setCronLoading: (loading: boolean) => void
  setCronError: (error: string | null) => void
  setCronHistoryJobId: (jobId: string | null) => void
  setCronHistory: (history: CronRunHistory[]) => void
  setCronHistoryLoading: (loading: boolean) => void
  setCronEditJobId: (jobId: string | null) => void
}

export const createCronSlice: StateCreator<CronSlice> = (set) => ({
  cronJobs: [],
  cronLoading: false,
  cronError: null,
  cronHistoryJobId: null,
  cronHistory: [],
  cronHistoryLoading: false,
  cronEditJobId: null,

  setCronJobs: (jobs) => set({ cronJobs: jobs }),

  setCronLoading: (loading) => set({ cronLoading: loading }),

  setCronError: (error) => set({ cronError: error }),

  setCronHistoryJobId: (jobId) => set({ cronHistoryJobId: jobId }),

  setCronHistory: (history) => set({ cronHistory: history }),

  setCronHistoryLoading: (loading) => set({ cronHistoryLoading: loading }),

  setCronEditJobId: (jobId) => set({ cronEditJobId: jobId }),
})

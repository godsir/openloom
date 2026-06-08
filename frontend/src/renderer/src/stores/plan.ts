import { StateCreator } from 'zustand'

export interface PlanArtifact {
  id: string
  workspace_root: string
  thread_id: string | null
  title: string
  relative_path: string
  source_request: string
  status: 'drafting' | 'ready' | 'building' | 'completed' | 'error'
  created_at: string
  updated_at: string
}

export interface PlanSlice {
  plans: PlanArtifact[]
  activePlanId: string | null
  planContent: string
  planPanelOpen: boolean
  loadPlans: (workspaceRoot: string) => Promise<void>
  setActivePlan: (planId: string | null) => void
  setPlanContent: (content: string) => void
  savePlanContent: (planId: string, content: string) => Promise<void>
  togglePlanPanel: () => void
}

export const createPlanSlice: StateCreator<PlanSlice> = (set, get) => ({
  plans: [],
  activePlanId: null,
  planContent: '',
  planPanelOpen: false,

  loadPlans: async (workspaceRoot: string) => {
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      const plans = await loomRpc<PlanArtifact[]>('plan.list', { workspace_root: workspaceRoot })
      set({ plans })
    } catch { /* plans list failed silently */ }
  },

  setActivePlan: async (planId) => {
    set({ activePlanId: planId })
    if (planId) {
      try {
        const loomRpc = (await import('../services/jsonrpc')).loomRpc
        const result = await loomRpc<{ plan: PlanArtifact, content: string }>('plan.get', { plan_id: planId })
        set({ planContent: result.content })
      } catch { set({ planContent: '' }) }
    }
  },

  setPlanContent: (content) => set({ planContent: content }),

  savePlanContent: async (planId: string, content: string) => {
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      await loomRpc('plan.update', { plan_id: planId, content })
    } catch { /* silent fail */ }
  },

  togglePlanPanel: () => set(s => ({ planPanelOpen: !s.planPanelOpen })),
})

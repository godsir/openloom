import { StateCreator } from 'zustand'
import { t as _t } from '../i18n'

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
  /** 当前 planContent 所属的 plan id；自动保存只在它与 activePlanId 一致时执行，
   *  防止切换计划时把旧内容写进新计划（B5） */
  planContentPlanId: string | null
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
  planContentPlanId: null,
  planPanelOpen: false,

  loadPlans: async (workspaceRoot: string) => {
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      const plans = await loomRpc<PlanArtifact[]>('plan.list', { workspace_root: workspaceRoot })
      set({ plans })
    } catch { /* plans list failed silently */ }
  },

  setActivePlan: async (planId) => {
    // 立即清空旧内容并解除内容与计划的绑定，避免切换瞬间旧内容被自动保存
    // 到新计划上造成覆盖（B5）
    set({ activePlanId: planId, planContent: '', planContentPlanId: null })
    if (planId) {
      try {
        const loomRpc = (await import('../services/jsonrpc')).loomRpc
        const result = await loomRpc<{ plan: PlanArtifact, content: string }>('plan.get', { plan_id: planId })
        // 加载回来时若用户已切走，丢弃结果，避免串档
        if (get().activePlanId === planId) {
          set({ planContent: result.content, planContentPlanId: planId })
        }
      } catch {
        if (get().activePlanId === planId) {
          set({ planContent: '', planContentPlanId: planId })
        }
      }
    }
  },

  setPlanContent: (content) => set({ planContent: content }),

  savePlanContent: async (planId: string, content: string) => {
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      await loomRpc('plan.update', { plan_id: planId, content })
      // Reload todos so the panel reflects checkbox changes from the plan markdown.
      ;(get() as any).loadTodos?.((get() as any).currentSessionId).catch(() => {})
    } catch {
      // 自动保存失败时提示，而非静默丢改动（B15）
      ;(get() as any).addToast?.({ type: 'error', message: _t('plan.saveFailed') })
    }
  },

  togglePlanPanel: () => set(s => ({ planPanelOpen: !s.planPanelOpen })),
})

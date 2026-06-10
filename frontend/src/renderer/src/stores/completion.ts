import { StateCreator } from 'zustand'

export interface CompletionSlice {
  fimEnabled: boolean
  fimLoading: boolean
  lastCompletion: string | null
  setFimEnabled: (enabled: boolean) => void
  setFimLoading: (loading: boolean) => void
  setLastCompletion: (text: string | null) => void
}

export const createCompletionSlice: StateCreator<CompletionSlice> = (set) => ({
  fimEnabled: false,
  fimLoading: false,
  lastCompletion: null,
  setFimEnabled: (enabled) => {
    set({ fimEnabled: enabled })
    // Persist across app restarts
    try { window.loom?.setPreference?.('fimEnabled', enabled) } catch {}
  },
  setFimLoading: (loading) => set({ fimLoading: loading }),
  setLastCompletion: (text) => set({ lastCompletion: text }),
})

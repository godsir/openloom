import { StateCreator } from 'zustand'

export interface SelectionSlice {
  selectedIds: Set<string>
  selectMode: boolean
  toggleSelect: (messageId: string) => void
  selectAll: (messageIds: string[]) => void
  clearSelection: () => void
  setSelectMode: (on: boolean) => void
}

export const createSelectionSlice: StateCreator<SelectionSlice> = (set, get) => ({
  selectedIds: new Set(),
  selectMode: false,

  toggleSelect: (messageId) => {
    const next = new Set(get().selectedIds)
    if (next.has(messageId)) {
      next.delete(messageId)
      if (next.size === 0) set({ selectMode: false })
    } else {
      next.add(messageId)
    }
    set({ selectedIds: next })
  },

  selectAll: (messageIds) => {
    set({ selectedIds: new Set(messageIds) })
  },

  clearSelection: () => {
    set({ selectedIds: new Set(), selectMode: false })
  },

  setSelectMode: (selectMode) => set({ selectMode }),
})

import { StateCreator } from 'zustand'

export interface QuotedSelection {
  id: string
  text: string
  filePath?: string
  startLine?: number
  endLine?: number
  charCount: number
}

export interface SelectionContextSlice {
  quotedSelections: QuotedSelection[]
  inlineInputOpen: boolean
  inlineInputText: string
  inlineInputRect: { top: number; left: number } | null
  inlineInputFilePath: string
  inlineInputStartLine: number
  inlineInputEndLine: number
  addQuotedSelection: (sel: Omit<QuotedSelection, 'id'>) => void
  removeQuotedSelection: (id: string) => void
  clearQuotedSelections: () => void
  openInlineInput: (rect: DOMRect, filePath?: string, startLine?: number, endLine?: number) => void
  closeInlineInput: () => void
  setInlineInputText: (text: string) => void
}

export const createSelectionContextSlice: StateCreator<SelectionContextSlice> = (set, get) => ({
  quotedSelections: [],
  inlineInputOpen: false,
  inlineInputText: '',
  inlineInputRect: null,
  inlineInputFilePath: '',
  inlineInputStartLine: 0,
  inlineInputEndLine: 0,

  addQuotedSelection: (sel) => {
    const id = crypto.randomUUID()
    set(s => ({ quotedSelections: [...s.quotedSelections, { ...sel, id }] }))
  },

  removeQuotedSelection: (id) => {
    set(s => ({ quotedSelections: s.quotedSelections.filter(q => q.id !== id) }))
  },

  clearQuotedSelections: () => set({ quotedSelections: [] }),

  openInlineInput: (rect, filePath, startLine, endLine) => {
    set({
      inlineInputOpen: true,
      inlineInputText: '',
      inlineInputRect: { top: rect.bottom + 8, left: rect.left },
      inlineInputFilePath: filePath || '',
      inlineInputStartLine: startLine || 0,
      inlineInputEndLine: endLine || 0,
    })
  },

  closeInlineInput: () => set({ inlineInputOpen: false, inlineInputText: '', inlineInputRect: null }),

  setInlineInputText: (text) => set({ inlineInputText: text }),
})

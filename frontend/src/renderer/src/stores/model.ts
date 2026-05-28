import { StateCreator } from 'zustand'
import type { ModelListItem } from '../types/bindings'

export type ThinkingLevel = 'off' | 'auto' | 'low' | 'medium' | 'high' | 'xhigh'

export interface ModelSlice {
  models: ModelListItem[]
  currentModel: string
  thinkingLevel: ThinkingLevel
  tokenUsage: { prompt: number; completion: number }
  setModels: (models: ModelListItem[]) => void
  setCurrentModel: (model: string) => void
  setThinkingLevel: (level: ThinkingLevel) => void
  setTokenUsage: (usage: { prompt: number; completion: number }) => void
}

export const createModelSlice: StateCreator<ModelSlice> = (set) => ({
  models: [],
  currentModel: '',
  thinkingLevel: 'auto',
  tokenUsage: { prompt: 0, completion: 0 },
  setModels: (models) => set({ models }),
  setCurrentModel: (currentModel) => set({ currentModel }),
  setThinkingLevel: (thinkingLevel) => set({ thinkingLevel }),
  setTokenUsage: (tokenUsage) => set({ tokenUsage }),
})

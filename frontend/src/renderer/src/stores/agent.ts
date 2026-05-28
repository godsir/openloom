import { StateCreator } from 'zustand'

export interface AgentSummary {
  id?: string
  name: string
  status?: string
  persona?: string
  model?: string
}

export interface AgentSlice {
  agents: AgentSummary[]
  currentAgentId: string | null
  setAgents: (agents: AgentSummary[]) => void
  setCurrentAgentId: (id: string | null) => void
}

export const createAgentSlice: StateCreator<AgentSlice> = (set) => ({
  agents: [],
  currentAgentId: null,
  setAgents: (agents) => set({ agents }),
  setCurrentAgentId: (currentAgentId) => set({ currentAgentId }),
})

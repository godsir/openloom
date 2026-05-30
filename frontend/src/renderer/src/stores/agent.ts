import { StateCreator } from 'zustand'

export interface AgentSummary {
  id?: string
  name: string
  status?: string
  persona?: string
  model?: string
  avatar?: string
}

export interface AgentSlice {
  agents: AgentSummary[]
  currentAgentId: string | null
  sessionAgentBindings: Record<string, string>
  setAgents: (agents: AgentSummary[]) => void
  setCurrentAgentId: (id: string | null) => void
  setSessionAgentBinding: (sessionId: string, agentName: string) => void
  getSessionAgent: (sessionId: string) => AgentSummary | undefined
}

export const createAgentSlice: StateCreator<AgentSlice> = (set, get) => ({
  agents: [],
  currentAgentId: null,
  sessionAgentBindings: {},

  setAgents: (agents) => set({ agents }),
  setCurrentAgentId: (currentAgentId) => set({ currentAgentId }),

  setSessionAgentBinding: (sessionId, agentName) => {
    const next = { ...get().sessionAgentBindings }
    if (agentName === 'default' || !agentName) {
      delete next[sessionId]
    } else {
      next[sessionId] = agentName
    }
    set({ sessionAgentBindings: next })
  },

  getSessionAgent: (sessionId) => {
    const state = get()
    const name = state.sessionAgentBindings[sessionId]
    if (name) return state.agents.find(a => a.name === name)
    // Fall back to default agent when no specific agent is bound
    return state.agents.find(a => a.name === 'default')
  },
})

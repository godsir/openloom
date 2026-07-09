import { StateCreator } from 'zustand'

export interface TeamConfig {
  id: string
  name: string
  description: string
  strategy: 'synthesize' | 'debate'
  captain: { model?: string; system_prompt_override?: string }
  members: TeamMember[]
}

export type TeamMember =
  | { name: string; source: { persona: string; model?: string; temperature?: number } }
  | { name: string; source: string }

export interface TeamSlice {
  teams: TeamConfig[]
  currentTeamId: string | null
  sessionTeamBindings: Record<string, string>
  setTeams: (teams: TeamConfig[]) => void
  setCurrentTeamId: (id: string | null) => void
  setSessionTeamBinding: (sessionId: string, teamId: string) => void
  getSessionTeam: (sessionId: string) => TeamConfig | undefined
}

export const createTeamSlice: StateCreator<TeamSlice> = (set, get) => ({
  teams: [],
  currentTeamId: null,
  sessionTeamBindings: {},

  setTeams: (teams) => set({ teams }),
  setCurrentTeamId: (currentTeamId) => set({ currentTeamId }),

  setSessionTeamBinding: (sessionId, teamId) => {
    const next = { ...get().sessionTeamBindings }
    if (!teamId) {
      delete next[sessionId]
    } else {
      next[sessionId] = teamId
    }
    set({ sessionTeamBindings: next })
  },

  getSessionTeam: (sessionId) => {
    const state = get()
    const id = state.sessionTeamBindings[sessionId]
    if (id) return state.teams.find((t) => t.id === id)
    return undefined
  },
})

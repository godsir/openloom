import { StateCreator } from 'zustand'

export type PermissionMode = 'operate' | 'ask' | 'read_only'

export interface AttachedFile {
  path: string
  name: string
  size: number
  mimeType: string
  thumbnail?: string
}

export interface Draft {
  text: string
  attachedFiles: AttachedFile[]
}

export interface InputSlice {
  draftBySession: Map<string, Draft>
  permissionMode: PermissionMode
  saveDraft: (sessionId: string, draft: Draft) => void
  restoreDraft: (sessionId: string) => Draft | null
  setPermissionMode: (mode: PermissionMode) => void
}

export const createInputSlice: StateCreator<InputSlice> = (set, get) => ({
  draftBySession: new Map(),
  permissionMode: 'ask',

  saveDraft: (sessionId, draft) => {
    const next = new Map(get().draftBySession)
    next.set(sessionId, draft)
    set({ draftBySession: next })
  },

  restoreDraft: (sessionId) => {
    return get().draftBySession.get(sessionId) ?? null
  },

  setPermissionMode: (permissionMode) => set({ permissionMode }),
})

import { StateCreator } from 'zustand'

export type PermissionMode = 'operate' | 'ask' | 'read_only' | 'plan'
export type SendShortcut = 'enter' | 'ctrl+enter' | 'shift+enter'

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
  sendShortcut: SendShortcut
  saveDraft: (sessionId: string, draft: Draft) => void
  restoreDraft: (sessionId: string) => Draft | null
  setPermissionMode: (mode: PermissionMode) => void
  setSendShortcut: (shortcut: SendShortcut) => void
}

export const createInputSlice: StateCreator<InputSlice> = (set, get) => ({
  draftBySession: new Map(),
  permissionMode: 'ask',
  sendShortcut: 'enter',

  saveDraft: (sessionId, draft) => {
    const next = new Map(get().draftBySession)
    next.set(sessionId, draft)
    set({ draftBySession: next })
  },

  restoreDraft: (sessionId) => {
    return get().draftBySession.get(sessionId) ?? null
  },

  setPermissionMode: (permissionMode) => {
    window.loom.setPreference('permissionMode', permissionMode)
    set({ permissionMode })
  },

  setSendShortcut: (sendShortcut) => {
    window.loom.setPreference('sendShortcut', sendShortcut)
    set({ sendShortcut })
  },
})

import { StateCreator } from 'zustand'

export interface ContentBlock {
  type: string
  [key: string]: unknown
}

export interface Message {
  id: string
  role: 'user' | 'assistant'
  blocks: ContentBlock[]
  timestamp: string
}

const MAX_CACHED_SESSIONS = 8

export interface ChatSlice {
  messagesBySession: Map<string, Message[]>
  ensureSession: (sessionId: string) => void
  appendMessage: (sessionId: string, message: Message) => void
  upsertBlock: (sessionId: string, messageId: string, block: ContentBlock) => void
  appendBlock: (sessionId: string, messageId: string, block: ContentBlock) => void
  patchBlockByTaskId: (sessionId: string, taskId: string, patch: Partial<ContentBlock>) => void
  hydrateMessages: (sessionId: string, messages: Message[]) => void
  deleteMessage: (sessionId: string, messageId: string) => void
  evictSession: (sessionId: string) => void
}

export const createChatSlice: StateCreator<ChatSlice> = (set, get) => ({
  messagesBySession: new Map(),

  ensureSession: (sessionId) => {
    const map = get().messagesBySession
    if (!map.has(sessionId)) {
      const next = new Map(map)
      next.set(sessionId, [])
      set({ messagesBySession: next })
    }
  },

  appendMessage: (sessionId, message) => {
    const next = new Map(get().messagesBySession)
    const msgs = [...(next.get(sessionId) || []), message]
    next.set(sessionId, msgs)
    set({ messagesBySession: next })
  },

  upsertBlock: (sessionId, messageId, block) => {
    const next = new Map(get().messagesBySession)
    const msgs = [...(next.get(sessionId) || [])]
    const idx = msgs.findIndex((m) => m.id === messageId)
    if (idx === -1) return

    const msg = { ...msgs[idx], blocks: [...msgs[idx].blocks] }
    const existingIdx = msg.blocks.findIndex((b) => b.type === block.type)
    if (existingIdx >= 0) {
      msg.blocks[existingIdx] = block
    } else {
      msg.blocks.push(block)
    }
    msgs[idx] = msg
    next.set(sessionId, msgs)
    set({ messagesBySession: next })
  },

  appendBlock: (sessionId, messageId, block) => {
    const next = new Map(get().messagesBySession)
    const msgs = [...(next.get(sessionId) || [])]
    const idx = msgs.findIndex((m) => m.id === messageId)
    if (idx === -1) return

    const msg = { ...msgs[idx], blocks: [...msgs[idx].blocks, block] }
    msgs[idx] = msg
    next.set(sessionId, msgs)
    set({ messagesBySession: next })
  },

  patchBlockByTaskId: (sessionId, taskId, patch) => {
    const next = new Map(get().messagesBySession)
    const msgs = [...(next.get(sessionId) || [])]
    for (let i = msgs.length - 1; i >= 0; i--) {
      const msg = msgs[i]
      const blockIdx = msg.blocks.findIndex(
        (b) => (b as any).taskId === taskId,
      )
      if (blockIdx >= 0) {
        const newBlocks = [...msg.blocks]
        newBlocks[blockIdx] = { ...newBlocks[blockIdx], ...patch }
        msgs[i] = { ...msg, blocks: newBlocks }
        next.set(sessionId, msgs)
        set({ messagesBySession: next })
        return
      }
    }
  },

  hydrateMessages: (sessionId, messages) => {
    const next = new Map(get().messagesBySession)
    next.set(sessionId, messages)

    if (next.size > MAX_CACHED_SESSIONS) {
      const keys = [...next.keys()]
      const currentId = (get() as any).currentSessionId
      for (const key of keys) {
        if (next.size <= MAX_CACHED_SESSIONS) break
        if (key !== currentId) next.delete(key)
      }
    }

    set({ messagesBySession: next })
  },

  deleteMessage: (sessionId, messageId) => {
    const next = new Map(get().messagesBySession)
    const msgs = (next.get(sessionId) || []).filter((m) => m.id !== messageId)
    next.set(sessionId, msgs)
    set({ messagesBySession: next })
  },

  evictSession: (sessionId) => {
    const next = new Map(get().messagesBySession)
    next.delete(sessionId)
    set({ messagesBySession: next })
  },
})

import { StateCreator } from 'zustand'

export interface TodoItem {
  id: string
  content: string
  status: 'pending' | 'in_progress' | 'completed'
  source?: {
    plan_id: string
    relative_path: string
    ordinal: number
    content_hash: string
  }
  created_at: string
  updated_at: string
}

export interface ThreadGoal {
  session_id: string
  description: string
  status: 'active' | 'paused' | 'completed'
  created_at: string
}

export interface TodoSlice {
  todos: TodoItem[]
  goal: ThreadGoal | null
  todoPanelOpen: boolean
  loadTodos: (sessionId: string) => Promise<void>
  toggleTodoStatus: (todoId: string) => Promise<void>
  setGoal: (sessionId: string, description: string) => Promise<void>
  toggleTodoPanel: () => void
}

export const createTodoSlice: StateCreator<TodoSlice> = (set, get) => ({
  todos: [],
  goal: null,
  todoPanelOpen: false,

  loadTodos: async (sessionId: string) => {
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      const todos = await loomRpc<TodoItem[]>('todo.list', { session_id: sessionId })
      set({ todos })
    } catch { /* silent fail */ }
  },

  toggleTodoStatus: async (todoId: string) => {
    const todo = get().todos.find(t => t.id === todoId)
    if (!todo) return
    const nextStatus: TodoItem['status'] =
      todo.status === 'pending' ? 'in_progress' :
      todo.status === 'in_progress' ? 'completed' : 'pending'

    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      await loomRpc('todo.update_status', { session_id: '', todo_id: todoId, status: nextStatus })
      set(s => ({
        todos: s.todos.map(t => t.id === todoId ? { ...t, status: nextStatus } : t)
      }))
    } catch { /* silent fail */ }
  },

  setGoal: async (sessionId: string, description: string) => {
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      const goal = await loomRpc<ThreadGoal>('goal.set', { session_id: sessionId, description })
      set({ goal })
    } catch { /* silent fail */ }
  },

  toggleTodoPanel: () => set(s => ({ todoPanelOpen: !s.todoPanelOpen })),
})

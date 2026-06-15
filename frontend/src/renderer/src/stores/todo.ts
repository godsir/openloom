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
  todoLoading: boolean
  currentTodoSessionId: string | null
  loadTodos: (sessionId: string) => Promise<void>
  toggleTodoStatus: (todoId: string) => Promise<void>
  setTodoStatus: (sessionId: string, todoId: string, status: TodoItem['status']) => Promise<void>
  setGoal: (sessionId: string, description: string) => Promise<void>
  clearTodos: (sessionId: string) => Promise<void>
  toggleTodoPanel: () => void
  /** Replaced by WS event — full list replacement from backend */
  handleTodoReplaced: (todos: TodoItem[]) => void
}

export const createTodoSlice: StateCreator<TodoSlice> = (set, get) => ({
  todos: [],
  goal: null,
  todoPanelOpen: false,
  todoLoading: false,
  currentTodoSessionId: null,

  loadTodos: async (sessionId: string) => {
    set({ todoLoading: true, currentTodoSessionId: sessionId })
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      const todos = await loomRpc<TodoItem[]>('todo.list', { session_id: sessionId })
      // Guard: don't overwrite if the user switched sessions during the fetch
      if (get().currentTodoSessionId === sessionId) {
        set({ todos, todoLoading: false })
      }
    } catch { set({ todoLoading: false }) }
  },

  toggleTodoStatus: async (todoId: string) => {
    const state = get()
    const todo = state.todos.find(t => t.id === todoId)
    if (!todo) return
    const nextStatus: TodoItem['status'] =
      todo.status === 'pending' ? 'in_progress' :
      todo.status === 'in_progress' ? 'completed' : 'pending'

    // Optimistic update
    set(s => ({
      todos: s.todos.map(t => t.id === todoId ? { ...t, status: nextStatus } : t)
    }))

    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      await loomRpc('todo.update_status', {
        session_id: state.currentTodoSessionId || '',
        todo_id: todoId,
        status: nextStatus,
      })
    } catch {
      // Revert on failure
      set(s => ({
        todos: s.todos.map(t => t.id === todoId ? { ...t, status: todo.status } : t)
      }))
    }
  },

  setTodoStatus: async (sessionId: string, todoId: string, status: TodoItem['status']) => {
    set(s => ({
      todos: s.todos.map(t => t.id === todoId ? { ...t, status } : t)
    }))
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      await loomRpc('todo.update_status', {
        session_id: sessionId,
        todo_id: todoId,
        status,
      })
    } catch { /* silent fail */ }
  },

  setGoal: async (sessionId: string, description: string) => {
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      const goal = await loomRpc<ThreadGoal>('goal.set', { session_id: sessionId, description })
      set({ goal })
    } catch { /* silent fail */ }
  },

  clearTodos: async (sessionId: string) => {
    set({ todos: [] })
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      await loomRpc('todo.clear', { session_id: sessionId })
    } catch { /* silent fail */ }
  },

  toggleTodoPanel: () => set(s => ({ todoPanelOpen: !s.todoPanelOpen })),

  handleTodoReplaced: (todos: TodoItem[]) => {
    // Auto-open the panel when the AI creates todos
    set({ todos, todoPanelOpen: todos.length > 0 ? true : get().todoPanelOpen })
  },
})

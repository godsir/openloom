import { StateCreator } from 'zustand'
import { t as _t } from '../i18n'

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
  /** 最近由 AI 新建的待办 id，用于入场高亮（B17），短暂保留后清空 */
  freshTodoIds: string[]
  loadTodos: (sessionId: string) => Promise<void>
  toggleTodoStatus: (todoId: string) => Promise<void>
  setTodoStatus: (sessionId: string, todoId: string, status: TodoItem['status']) => Promise<void>
  setGoal: (sessionId: string, description: string) => Promise<void>
  clearTodos: (sessionId: string) => Promise<void>
  toggleTodoPanel: () => void
  /** Replaced by WS event — full list replacement from backend */
  handleTodoReplaced: (todos: TodoItem[]) => void
}

// 新增待办高亮的清除定时器（模块级，避免重复堆叠）
let freshTimer: ReturnType<typeof setTimeout> | null = null

export const createTodoSlice: StateCreator<TodoSlice> = (set, get) => ({
  todos: [],
  goal: null,
  todoPanelOpen: false,
  todoLoading: false,
  currentTodoSessionId: null,
  freshTodoIds: [],

  loadTodos: async (sessionId: string) => {
    // 切换会话时先清空旧列表，避免加载期间/失败后仍显示上一会话的待办（B4）
    if (get().currentTodoSessionId !== sessionId) set({ todos: [] })
    set({ todoLoading: true, currentTodoSessionId: sessionId })
    try {
      const loomRpc = (await import('../services/jsonrpc')).loomRpc
      const todos = await loomRpc<TodoItem[]>('todo.list', { session_id: sessionId })
      // Guard: don't overwrite if the user switched sessions during the fetch
      if (get().currentTodoSessionId === sessionId) {
        set({ todos, todoLoading: false })
      }
    } catch {
      // 失败时保持空列表而非留下旧数据，并停止加载指示（B4）
      if (get().currentTodoSessionId === sessionId) {
        set({ todos: [], todoLoading: false })
      }
    }
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
      // Revert on failure，并提示用户，而非静默回滚让人误以为已生效（B11）
      set(s => ({
        todos: s.todos.map(t => t.id === todoId ? { ...t, status: todo.status } : t)
      }))
      ;(get() as any).addToast?.({ type: 'error', message: _t('todo.toggleFailed') })
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
    // 计算本次新增的待办 id，供面板做入场高亮（B17）
    const prevIds = new Set(get().todos.map(x => x.id))
    const fresh = todos.filter(x => !prevIds.has(x.id)).map(x => x.id)
    set({ todos, todoPanelOpen: todos.length > 0 ? true : get().todoPanelOpen, freshTodoIds: fresh })
    if (fresh.length > 0) {
      if (freshTimer) clearTimeout(freshTimer)
      freshTimer = setTimeout(() => set({ freshTodoIds: [] }), 2600)
    }
  },
})

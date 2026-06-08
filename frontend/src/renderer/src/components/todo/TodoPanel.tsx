import React from 'react'
import { useStore } from '../../stores'

export const TodoPanel: React.FC = () => {
  const todos = useStore(s => s.todos)
  const goal = useStore(s => s.goal)
  const todoPanelOpen = useStore(s => s.todoPanelOpen)
  const toggleTodoStatus = useStore(s => s.toggleTodoStatus)
  const toggleTodoPanel = useStore(s => s.toggleTodoPanel)

  const pending = todos.filter(t => t.status === 'pending').length
  const inProgress = todos.filter(t => t.status === 'in_progress').length
  const completed = todos.filter(t => t.status === 'completed').length

  if (!todoPanelOpen) return null

  return (
    <div style={{
      width: 340, borderLeft: '1px solid var(--border)',
      display: 'flex', flexDirection: 'column', height: '100%',
      backgroundColor: 'var(--bg-card)'
    }}>
      <div style={{ padding: '12px 16px', borderBottom: '1px solid var(--border)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <span style={{ fontWeight: 600, fontSize: 14 }}>Todo</span>
        <button onClick={toggleTodoPanel} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', fontSize: 18 }}>x</button>
      </div>

      {goal && (
        <div style={{ padding: '8px 16px', fontSize: 13, borderBottom: '1px solid var(--border)', backgroundColor: 'var(--surface-subtle)' }}>
          <span style={{ fontWeight: 500 }}>Goal:</span> {goal.description}
          <span style={{ marginLeft: 8, padding: '1px 6px', borderRadius: 3, fontSize: 10, backgroundColor: 'var(--accent-soft)', color: 'var(--accent)' }}>{goal.status}</span>
        </div>
      )}

      <div style={{ padding: '8px 16px', fontSize: 12, color: 'var(--text-muted)', display: 'flex', gap: 12 }}>
        <span>{pending} pending</span>
        <span>{inProgress} in progress</span>
        <span>{completed} done</span>
      </div>

      <div style={{ flex: 1, overflow: 'auto', padding: '8px 16px' }}>
        {todos.map(todo => (
          <div key={todo.id} onClick={() => toggleTodoStatus(todo.id)} style={{
            display: 'flex', alignItems: 'flex-start', gap: 8, padding: '6px 0',
            cursor: 'pointer', fontSize: 13, color: 'var(--text)',
            opacity: todo.status === 'completed' ? 0.5 : 1
          }}>
            <span style={{ marginTop: 1 }}>
              {todo.status === 'completed' ? '☑' : todo.status === 'in_progress' ? '◐' : '☐'}
            </span>
            <span style={{
              textDecoration: todo.status === 'completed' ? 'line-through' : 'none',
              flex: 1
            }}>
              {todo.content}
            </span>
          </div>
        ))}
        {todos.length === 0 && (
          <div style={{ textAlign: 'center', color: 'var(--text-muted)', padding: 24, fontSize: 13 }}>
            No todos yet. Create a plan with <code>/plan</code> to generate tasks.
          </div>
        )}
      </div>
    </div>
  )
}

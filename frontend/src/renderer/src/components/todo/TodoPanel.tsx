import { useEffect } from 'react'
import { Circle, PlayCircle, CheckCircle2 } from 'lucide-react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import styles from './TodoPanel.module.css'

export const TodoPanel: React.FC = () => {
  const { t } = useLocale()
  const todos = useStore(s => s.todos)
  const todoPanelOpen = useStore(s => s.todoPanelOpen)
  const currentTodoSessionId = useStore(s => s.currentTodoSessionId)
  const currentSessionId = useStore(s => s.currentSessionId)
  const toggleTodoStatus = useStore(s => s.toggleTodoStatus)
  const toggleTodoPanel = useStore(s => s.toggleTodoPanel)
  const loadTodos = useStore(s => s.loadTodos)

  useEffect(() => {
    if (currentSessionId && currentSessionId !== currentTodoSessionId) {
      loadTodos(currentSessionId)
    }
  }, [currentSessionId])

  const count = todos.filter(t => t.status !== 'completed').length

  if (!todoPanelOpen) return null

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <span>{t('todo.panelTitle')}{todos.length > 0 ? ` · ${count} 项` : ''}</span>
        <button onClick={toggleTodoPanel} className={styles.closeBtn}>×</button>
      </div>
      <div className={styles.list}>
        {todos.length === 0 ? (
          <div className={styles.empty}>{t('todo.empty')}</div>
        ) : (
          todos.map(todo => {
            const isCompleted = todo.status === 'completed'
            const isInProgress = todo.status === 'in_progress'
            return (
              <div
                key={todo.id}
                className={`${styles.item} ${isCompleted ? styles.completed : ''} ${isInProgress ? styles.inProgress : ''}`}
                onClick={() => toggleTodoStatus(todo.id)}
              >
                <span className={styles.statusIcon}>
                  {isCompleted ? <CheckCircle2 size={15} /> :
                   isInProgress ? <PlayCircle size={15} /> :
                   <Circle size={15} />}
                </span>
                <span className={styles.contentText}>{todo.content}</span>
              </div>
            )
          })
        )}
      </div>
    </div>
  )
}

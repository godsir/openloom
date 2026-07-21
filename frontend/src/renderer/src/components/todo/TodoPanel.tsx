import { useEffect } from 'react'
import { Circle, PlayCircle, CheckCircle2 } from 'lucide-react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import styles from './TodoPanel.module.css'

export const TodoPanel: React.FC = () => {
  const { t } = useLocale()
  const todos = useStore(s => s.todos)
  const todoPanelOpen = useStore(s => s.todoPanelOpen)
  const todoLoading = useStore(s => s.todoLoading)
  const freshTodoIds = useStore(s => s.freshTodoIds)
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

  const count = todos.filter(td => td.status !== 'completed').length
  const freshSet = new Set(freshTodoIds)

  if (!todoPanelOpen) return null

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <span>{t('todo.panelTitle')}{todos.length > 0 ? ` · ${t('todo.count', { n: count })}` : ''}</span>
        <button onClick={toggleTodoPanel} className={styles.closeBtn}>×</button>
      </div>
      <div className={styles.list}>
        {todoLoading && todos.length === 0 ? (
          <div className={styles.empty}>{t('common.loading')}</div>
        ) : todos.length === 0 ? (
          <div className={styles.empty}>
            <div>{t('todo.empty')}</div>
            {/* 空态补充引导，告诉用户待办从何而来（B18） */}
            <div className={styles.emptyHint}>{t('todo.emptyHint')}</div>
          </div>
        ) : (
          todos.map(todo => {
            const isCompleted = todo.status === 'completed'
            const isInProgress = todo.status === 'in_progress'
            const isFresh = freshSet.has(todo.id)
            return (
              <div
                key={todo.id}
                className={[
                  styles.item,
                  isCompleted ? styles.completed : '',
                  isInProgress ? styles.inProgress : '',
                  isFresh ? styles.itemFresh : '',
                ].filter(Boolean).join(' ')}
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

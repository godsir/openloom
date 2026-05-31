import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import styles from './WorkspaceTab.module.css'

export default function WorkspaceTab() {
  const [defaultPath, setDefaultPath] = useState('')
  const [loading, setLoading] = useState(true)
  const sessions = useStore(s => s.sessions)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)

  useEffect(() => {
    loomRpc<{ workspace: string | null }>('workspace.get')
      .then(result => {
        setDefaultPath(result.workspace || '')
        setLoading(false)
      })
      .catch(() => setLoading(false))
  }, [])

  const handleSelectFolder = async () => {
    const path = await window.loom.selectFolder()
    if (path) {
      setDefaultPath(path)
      await rpc('workspace.set_default', { path }, '默认工作空间已更新')
    }
  }

  const handleClear = async () => {
    setDefaultPath('')
    await rpc('workspace.set_default', { path: '' }, '默认工作空间已清除')
  }

  const handleResetSession = async (sid: string) => {
    useStore.getState().setSessionWorkspace(sid, defaultPath)
    await rpc('workspace.set_session', { session_id: sid, path: defaultPath }, '已重置为默认工作空间')
  }

  // Only show sessions that explicitly have a workspace set (different from default)
  const sessionsWithWorkspace = Object.entries(sessionWorkspaces)
    .filter(([, path]) => path && path !== defaultPath)
    .map(([sid, path]) => {
      const session = sessions.find(s => s.path === sid)
      return { sid, path, title: session?.title || null }
    })

  if (loading) {
    return <div className={styles.loading}>加载中...</div>
  }

  return (
    <div className={styles.container}>
      <div className={styles.section}>
        <h3 className={styles.title}>默认工作空间</h3>
        <p className={styles.description}>
          所有会话默认使用的工作目录。AI 会在此目录下创建和读取文件。
          相对路径将基于此目录解析。
        </p>
        <div className={styles.pathRow}>
          <span className={styles.pathLabel}>当前路径：</span>
          <span className={styles.pathValue}>
            {defaultPath || '未设置'}
          </span>
        </div>
        <div className={styles.actions}>
          <button onClick={handleSelectFolder} className={styles.selectBtn}>
            选择文件夹
          </button>
          {defaultPath && (
            <button onClick={handleClear} className={styles.clearBtn}>
              清除
            </button>
          )}
        </div>
      </div>

      <div className={styles.section}>
        <h3 className={styles.title}>会话工作空间</h3>
        <p className={styles.description}>
          为每个会话覆盖默认工作空间。右键点击侧边栏中的会话可快速设置。重置后恢复为默认路径。
        </p>
        {sessionsWithWorkspace.length === 0 ? (
          <p className={styles.description} style={{ color: 'var(--text-muted)', fontStyle: 'italic' }}>
            暂无覆盖了工作空间的会话
          </p>
        ) : (
          <div className={styles.sessionList}>
            {sessionsWithWorkspace.map(({ sid, path, title }) => (
              <div key={sid} className={styles.sessionRow}>
                <div className={styles.sessionInfo}>
                  <span className={styles.sessionTitle}>
                    {title || sid.slice(0, 8)}
                  </span>
                  <span className={styles.sessionPath}>{path}</span>
                </div>
                <button
                  onClick={() => handleResetSession(sid)}
                  className={styles.clearBtn}
                  style={{ flexShrink: 0, padding: '4px 8px', fontSize: '11px' }}
                >
                  重置
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

import { useEffect, type ReactNode } from 'react'
import { useStore } from '../../stores'
import Sidebar from './Sidebar'
import WindowControls from './WindowControls'
import ChatWorkspace from '../chat/ChatWorkspace'
import { IconPanelLeftClose, IconPanelLeft } from '../../utils/icons'
import styles from './AppShell.module.css'
import logoDev from '../../assets/loom_logo_dev.png'
import logoRelease from '../../assets/loom_logo.png'

export default function AppShell({ children }: { children?: ReactNode }) {
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const currentSessionId = useStore(s => s.currentSessionId)
  const sessions = useStore(s => s.sessions)
  const wsState = useStore(s => s.wsState)
  const isPackaged = window.__isPackaged__ ?? true

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'b') {
        e.preventDefault()
        toggleSidebar()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [toggleSidebar])

  const currentTitle = currentSessionId
    ? sessions.find(s => s.path === currentSessionId)?.title || 'openLoom'
    : 'openLoom'

  return (
    <div className={styles.shell}>
      <header className={styles.titlebar}>
        <div className={styles.titlebarLeft}>
          <img
            src={isPackaged ? logoRelease : logoDev}
            alt="openLoom"
            className={styles.titlebarIcon}
          />
          <button onClick={toggleSidebar} className={styles.toggleBtn} title="⌘B 切换侧边栏">
            {sidebarOpen ? <IconPanelLeftClose size={16} /> : <IconPanelLeft size={16} />}
          </button>
        </div>

        <div className={styles.titlebarCenter}>
          <span className={styles.sessionTitle}>{currentTitle}</span>
        </div>

        <div className={styles.titlebarRight}>
          <div className={styles.connectionStatus}>
            <span
              className={styles.connectionDot}
              data-state={wsState}
            />
            <span className={styles.connectionText}>
              {wsState === 'connected' ? '已连接' : wsState === 'reconnecting' ? '重连中' : '离线'}
            </span>
          </div>
          <WindowControls />
        </div>
      </header>

      <div className={styles.body}>
        <div
          className={`${styles.sidebarSlot} ${sidebarOpen ? styles.sidebarSlotOpen : ''}`}
        >
          <Sidebar />
        </div>
        <main className={styles.main} data-content>
          <ChatWorkspace />
          {children}
        </main>
      </div>
    </div>
  )
}

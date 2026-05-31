import { useEffect, type ReactNode } from 'react'
import { useStore } from '../../stores'
import Sidebar from './Sidebar'
import WindowControls from './WindowControls'
import ChatWorkspace from '../chat/ChatWorkspace'
import { IconPanelLeftClose, IconPanelLeft } from '../../utils/icons'
import styles from './AppShell.module.css'

export default function AppShell({ children }: { children?: ReactNode }) {
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const wsState = useStore(s => s.wsState)

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

  return (
    <div className={styles.shell}>
      <header className={styles.titlebar}>
        <div className={styles.titlebarLeft}>
          <button onClick={toggleSidebar} className={styles.toggleBtn} title="⌘B 切换侧边栏">
            {sidebarOpen ? <IconPanelLeftClose size={16} /> : <IconPanelLeft size={16} />}
          </button>
        </div>

        <div className={styles.titlebarCenter}>
          <span className={styles.appTitle}>openLoom</span>
        </div>

        <div className={styles.titlebarRight}>
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
          <div className={styles.connectionStatus}>
            <span className={styles.connectionDot} data-state={wsState} />
            <span className={styles.connectionText}>
              {wsState === 'connected' ? '已连接' : wsState === 'reconnecting' ? '重连中' : '离线'}
            </span>
          </div>
        </main>
      </div>
    </div>
  )
}

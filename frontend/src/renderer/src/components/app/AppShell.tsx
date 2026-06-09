import { useEffect, useState, type ReactNode } from 'react'
import { useStore } from '../../stores'
import Sidebar from './Sidebar'
import WindowControls from './WindowControls'
import ChatWorkspace from '../chat/ChatWorkspace'
import { WriteWorkspaceView } from '../write/WriteWorkspaceView'
import { PlanPanel } from '../plan/PlanPanel'
import { TodoPanel } from '../todo/TodoPanel'
import { IconPanelLeftClose, IconPanelLeft, IconAlertCircle, IconWifiOff, IconRefresh, IconRotateCcw, IconSettings, IconEdit, IconMessageSquare } from '../../utils/icons'
import { connectWebSocket } from '../../services/websocket'
import styles from './AppShell.module.css'

export default function AppShell({ children }: { children?: ReactNode }) {
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const writeFileSidebarOpen = useStore(s => s.writeFileSidebarOpen)
  const toggleWriteFileSidebar = useStore(s => s.toggleWriteFileSidebar)
  const setSettingsOpen = useStore(s => s.setSettingsOpen)
  const wsState = useStore(s => s.wsState)
  const engineState = useStore(s => s.engineState)
  const port = useStore(s => s.port)
  const appMode = useStore(s => s.appMode)
  const setAppMode = useStore(s => s.setAppMode)
  const [reconnecting, setReconnecting] = useState(false)
  const [restarting, setRestarting] = useState(false)

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'b') {
        e.preventDefault()
        if (appMode === 'write') {
          toggleWriteFileSidebar()
        } else {
          toggleSidebar()
        }
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [appMode, toggleSidebar, toggleWriteFileSidebar])

  const handleReconnect = async () => {
    setReconnecting(true)
    try {
      await connectWebSocket(port)
    } catch {
      // State will be set by onclose handler
    } finally {
      setReconnecting(false)
    }
  }

  const handleRestartEngine = async () => {
    setRestarting(true)
    try {
      const newPort = await window.loom.restartEngine()
      useStore.getState().setPort(newPort)
      await connectWebSocket(newPort)
      useStore.getState().setEngineState('running')
    } catch {
      // State stays as stopped
    } finally {
      setRestarting(false)
    }
  }

  return (
    <div className={styles.shell}>
      <header className={styles.titlebar}>
        <div className={styles.titlebarLeft}>
          <button
            onClick={() => {
              if (appMode === 'write') {
                toggleWriteFileSidebar()
              } else {
                toggleSidebar()
              }
            }}
            className={styles.toggleBtn}
            title="⌘B 切换侧边栏"
          >
            {appMode === 'write'
              ? (writeFileSidebarOpen ? <IconPanelLeftClose size={16} /> : <IconPanelLeft size={16} />)
              : (sidebarOpen ? <IconPanelLeftClose size={16} /> : <IconPanelLeft size={16} />)
            }
          </button>
          <button onClick={() => setSettingsOpen(true)} className={styles.toggleBtn} title="设置">
            <IconSettings size={16} />
          </button>
        </div>

        <div className={styles.titlebarCenter}>
          <div className={styles.modeToggle} data-active={appMode} role="radiogroup" aria-label="模式切换">
            <button
              className={`${styles.modeToggleOption} ${appMode === 'chat' ? styles.modeToggleOptionActive : ''}`}
              onClick={() => {
                if (appMode === 'chat') return
                setAppMode('chat')
                if (!sidebarOpen) toggleSidebar()
              }}
            >
              <IconMessageSquare size={13} />
              <span>对话</span>
            </button>
            <button
              className={`${styles.modeToggleOption} ${appMode === 'write' ? styles.modeToggleOptionActive : ''}`}
              onClick={() => {
                if (appMode === 'write') return
                setAppMode('write')
                if (sidebarOpen) toggleSidebar()
              }}
            >
              <IconEdit size={13} />
              <span>写作</span>
            </button>
          </div>
        </div>

        <div className={styles.titlebarRight}>
          <WindowControls />
        </div>
      </header>

      <div className={styles.body}>
        {/* 写作模式下隐藏会话侧边栏 */}
        {appMode !== 'write' && (
          <div
            className={`${styles.sidebarSlot} ${sidebarOpen ? styles.sidebarSlotOpen : ''}`}
          >
            <Sidebar />
          </div>
        )}
        <main className={styles.main} data-content>
          {appMode === 'write' ? <WriteWorkspaceView /> : <ChatWorkspace />}
          {children}

          {/* Engine crashed banner */}
          {engineState === 'stopped' && (
            <div className={styles.crashBanner}>
              <IconAlertCircle size={18} />
              <span className={styles.crashMessage}>引擎已停止</span>
              <button
                className={styles.crashBtn}
                onClick={handleRestartEngine}
                disabled={restarting}
              >
                <IconRotateCcw size={14} className={restarting ? styles.spin : ''} />
                <span>{restarting ? '重启中...' : '重启引擎'}</span>
              </button>
            </div>
          )}

          {/* WebSocket disconnected banner (engine still running but WS lost) */}
          {engineState !== 'stopped' && wsState === 'disconnected' && (
            <div className={styles.crashBanner}>
              <IconWifiOff size={18} />
              <span className={styles.crashMessage}>引擎连接断开</span>
              <button
                className={styles.crashBtn}
                onClick={handleReconnect}
                disabled={reconnecting}
              >
                <IconRefresh size={14} className={reconnecting ? styles.spin : ''} />
                <span>{reconnecting ? '连接中...' : '重新连接'}</span>
              </button>
            </div>
          )}

          {/* Normal connection status (connected / reconnecting) */}
          {engineState !== 'stopped' && wsState !== 'disconnected' && (
            <div className={styles.connectionStatus}>
              <span className={styles.connectionDot} data-state={wsState} />
              <span className={styles.connectionText}>
                {wsState === 'connected' ? '已连接' : '重连中'}
              </span>
            </div>
          )}
        </main>
        {appMode === 'chat' && <PlanPanel />}
        {appMode === 'chat' && <TodoPanel />}
      </div>
    </div>
  )
}

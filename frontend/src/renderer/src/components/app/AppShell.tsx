import { useState, useRef, type ReactNode } from 'react'
import { useStore } from '../../stores'
import Sidebar from './Sidebar'
import WindowControls from './WindowControls'
import ChatWorkspace from '../chat/ChatWorkspace'
import { WriteWorkspaceView } from '../write/WriteWorkspaceView'
import SettingsPage from '../settings/SettingsPage'
import { PlanPanel } from '../plan/PlanPanel'
import { IconPanelLeftClose, IconPanelLeft, IconAlertCircle, IconWifiOff, IconRefresh, IconRotateCcw, IconSettings, IconEdit, IconMessageSquare, IconArrowLeft } from '../../utils/icons'
import { connectWebSocket } from '../../services/websocket'
import { useLocale } from '../../i18n'
import styles from './AppShell.module.css'

export default function AppShell({ children }: { children?: ReactNode }) {
  const { t } = useLocale()
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const writeFileSidebarOpen = useStore(s => s.writeFileSidebarOpen)
  const toggleWriteFileSidebar = useStore(s => s.toggleWriteFileSidebar)
  const wsState = useStore(s => s.wsState)
  const engineState = useStore(s => s.engineState)
  const port = useStore(s => s.port)
  const appMode = useStore(s => s.appMode)
  const setAppMode = useStore(s => s.setAppMode)
  const prevModeRef = useRef<'chat' | 'write'>('chat')
  const [reconnecting, setReconnecting] = useState(false)
  const [restarting, setRestarting] = useState(false)

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
          {appMode === 'settings' ? (
            <button
              onClick={() => setAppMode(prevModeRef.current)}
              className={styles.toggleBtn}
              title={t('common.back')}
            >
              <IconArrowLeft size={16} />
            </button>
          ) : (
            <>
              <button
                onClick={() => {
                  if (appMode === 'write') {
                    toggleWriteFileSidebar()
                  } else {
                    toggleSidebar()
                  }
                }}
                className={styles.toggleBtn}
                title={`⌘B ${t('app.toggleSidebar')}`}
              >
                {appMode === 'write'
                  ? (writeFileSidebarOpen ? <IconPanelLeftClose size={16} /> : <IconPanelLeft size={16} />)
                  : (sidebarOpen ? <IconPanelLeftClose size={16} /> : <IconPanelLeft size={16} />)
                }
              </button>
              <button
                onClick={() => {
                  prevModeRef.current = appMode === 'write' ? 'write' : 'chat'
                  setAppMode('settings')
                }}
                className={styles.toggleBtn}
                title={t('app.settings')}
              >
                <IconSettings size={16} />
              </button>
            </>
          )}
        </div>

        <div className={styles.titlebarCenter}>
          {appMode === 'settings' ? (
            <span className={styles.titlebarPageTitle}>{t('app.settings')}</span>
          ) : (
            <div className={styles.modeToggle} data-active={appMode} role="radiogroup" aria-label={t('app.modeSwitch')}>
              <button
                className={`${styles.modeToggleOption} ${appMode === 'chat' ? styles.modeToggleOptionActive : ''}`}
                onClick={() => {
                  if (appMode === 'chat') return
                  setAppMode('chat')
                  if (!sidebarOpen) toggleSidebar()
                }}
              >
                <IconMessageSquare size={13} />
                <span>{t('app.chat')}</span>
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
                <span>{t('app.write')}</span>
              </button>
            </div>
          )}
        </div>

        <div className={styles.titlebarRight}>
          <WindowControls />
        </div>
      </header>

      <div className={styles.body}>
        {/* 写作模式和设置模式下隐藏会话侧边栏 */}
        {appMode !== 'write' && appMode !== 'settings' && (
          <div
            className={`${styles.sidebarSlot} ${sidebarOpen ? styles.sidebarSlotOpen : ''}`}
          >
            <Sidebar />
          </div>
        )}
        <main className={styles.main} data-content>
          {appMode === 'settings' ? <SettingsPage /> : appMode === 'write' ? <WriteWorkspaceView /> : <ChatWorkspace />}
          {children}

          {/* Engine crashed banner */}
          {engineState === 'stopped' && (
            <div className={styles.crashBanner}>
              <IconAlertCircle size={18} />
              <span className={styles.crashMessage}>{t('app.engineStopped')}</span>
              <button
                className={styles.crashBtn}
                onClick={handleRestartEngine}
                disabled={restarting}
              >
                <IconRotateCcw size={14} className={restarting ? styles.spin : ''} />
                <span>{restarting ? t('app.restarting') : t('app.restartEngine')}</span>
              </button>
            </div>
          )}

          {/* WebSocket disconnected banner (engine still running but WS lost) */}
          {engineState !== 'stopped' && wsState === 'disconnected' && (
            <div className={styles.crashBanner}>
              <IconWifiOff size={18} />
              <span className={styles.crashMessage}>{t('app.engineDisconnected')}</span>
              <button
                className={styles.crashBtn}
                onClick={handleReconnect}
                disabled={reconnecting}
              >
                <IconRefresh size={14} className={reconnecting ? styles.spin : ''} />
                <span>{reconnecting ? t('app.connecting') : t('app.reconnect')}</span>
              </button>
            </div>
          )}

          {/* Normal connection status (connected / reconnecting) */}
          {engineState !== 'stopped' && wsState !== 'disconnected' && (
            <div className={styles.connectionStatus}>
              <span className={styles.connectionDot} data-state={wsState} />
              <span className={styles.connectionText}>
                {wsState === 'connected' ? t('app.connected') : t('app.reconnecting')}
              </span>
            </div>
          )}
          {appMode === 'chat' && (
            <div className={styles.rightPanels}>
              <PlanPanel />
            </div>
          )}
        </main>
      </div>
    </div>
  )
}

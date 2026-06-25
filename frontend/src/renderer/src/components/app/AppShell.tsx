import { useState, useRef, useEffect, type ReactNode } from 'react'
import { useStore } from '../../stores'
import Sidebar from './Sidebar'
import WindowControls from './WindowControls'
import DynamicIslandCenter from './DynamicIslandCenter'
import ChatWorkspace from '../chat/ChatWorkspace'
import { WriteWorkspaceView } from '../write/WriteWorkspaceView'
import SettingsPage from '../settings/SettingsPage'
import { PlanPanel } from '../plan/PlanPanel'
import { IconPanelLeftClose, IconPanelLeft, IconAlertCircle, IconWifiOff, IconRefresh, IconRotateCcw, IconSettings, IconEdit, IconMessageSquare, IconArrowLeft } from '../../utils/icons'
import { connectWebSocket } from '../../services/websocket'
import { useLocale } from '../../i18n'
import TextEditingContextMenu from '../shared/TextEditingContextMenu'
import logoRelease from '@asset/icon.png'
import logoDev from '@asset/icon_dev.png'
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
  const isDev = !(window.__isPackaged__ ?? true)

  const streamingIds = useStore(s => s.streamingSessionIds)
  const update = useStore(s => s.update)
  const isStreaming = streamingIds.size > 0
  const isDownloading = update.status === 'downloading'
  const isUpdateAvailable = update.status === 'available'
  const isEngineStopped = engineState === 'stopped'

  const activeState: string = isEngineStopped ? 'crash' : (isStreaming && isDownloading) ? 'split' : update.status === 'downloaded' ? 'downloaded' : update.status === 'error' ? 'uperror' : isDownloading ? 'download' : isUpdateAvailable ? 'update' : isStreaming ? 'streaming' : 'idle'

  const islandExpanded = useStore(s => s.islandExpanded)
  const setIslandExpanded = useStore(s => s.setIslandExpanded)

  // idle/transient/split 不展开；其余状态可展开成详情卡片
  const expandable = activeState === 'streaming' || activeState === 'download' || activeState === 'downloaded' || activeState === 'uperror' || activeState === 'update' || activeState === 'crash'

  const ISLAND_SIZE: Record<string, { w: number; h: number }> = {
    idle: { w: 250, h: 38 },
    transient: { w: 200, h: 38 },
    split: { w: 340, h: 38 },
    streaming: { w: 260, h: 84 },
    download: { w: 280, h: 84 },
    downloaded: { w: 260, h: 84 },
    uperror: { w: 240, h: 84 },
    update: { w: 260, h: 84 },
    crash: { w: 260, h: 84 },
  }
  const EXPANDED_SIZE = { w: 340, h: 180 }
  const size = (islandExpanded && expandable) ? EXPANDED_SIZE : (ISLAND_SIZE[activeState] ?? ISLAND_SIZE.idle)

  // 状态消失时自动收起展开
  useEffect(() => {
    if (!expandable && islandExpanded) setIslandExpanded(false)
  }, [expandable, islandExpanded, setIslandExpanded])

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
        <div className={styles.titlebarBrand}>
          <img src={isDev ? logoDev : logoRelease} alt="OpenLoom" className={styles.titlebarLogo} />
          <span className={styles.titlebarBrandName}>OpenLoom</span>
        </div>
        <div
          className={styles.titlebarIsland}
          data-dynamic={activeState}
          data-expanded={islandExpanded && expandable ? 'true' : 'false'}
          style={{ width: size.w, height: size.h }}
          onClick={expandable ? () => setIslandExpanded(!islandExpanded) : undefined}
        >
          {appMode === 'settings' ? (
            <>
              <button
                onClick={() => setAppMode(prevModeRef.current)}
                className={`${styles.toggleBtn} ${styles.islandLeftBtn}`}
                title={t('common.back')}
              >
                <IconArrowLeft size={16} />
              </button>
              <DynamicIslandCenter />
            </>
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
                className={`${styles.toggleBtn} ${styles.islandLeftBtn}`}
                title={`⌘B ${t('app.toggleSidebar')}`}
              >
                {appMode === 'write'
                  ? (writeFileSidebarOpen ? <IconPanelLeftClose size={16} /> : <IconPanelLeft size={16} />)
                  : (sidebarOpen ? <IconPanelLeftClose size={16} /> : <IconPanelLeft size={16} />)
                }
              </button>
              <DynamicIslandCenter />
              <button
                onClick={() => {
                  prevModeRef.current = appMode === 'write' ? 'write' : 'chat'
                  setAppMode('settings')
                }}
                className={`${styles.toggleBtn} ${styles.islandRightBtn}`}
                title={t('app.settings')}
              >
                <IconSettings size={16} />
              </button>
            </>
          )}
        </div>
        <div className={styles.windowControls}>
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
          <div key={appMode} className={styles.modeView}>
            {appMode === 'settings' ? <SettingsPage /> : appMode === 'write' ? <WriteWorkspaceView /> : <ChatWorkspace />}
          </div>
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

      <TextEditingContextMenu />
    </div>
  )
}

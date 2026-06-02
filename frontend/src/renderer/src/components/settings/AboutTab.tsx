import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import styles from '../shared/SettingsModal.module.css'
import logoDev from '../../assets/loom_logo_dev.png'
import logoRelease from '../../assets/loom_logo.png'

interface SystemHealth {
  status: string
  version: string
  agent_count: number
  tool_count: number
}

export default function AboutTab({ wsState }: { wsState: string }) {
  const [health, setHealth] = useState<SystemHealth | null>(null)
  const [healthError, setHealthError] = useState(false)
  const [appVersion, setAppVersion] = useState('...')
  const [dataDir, setDataDir] = useState('')
  const update = useStore((s) => s.update)
  const currentModel = useStore((s) => s.currentModel)
  const port = useStore((s) => s.port)
  const checkUpdate = useStore((s) => s.checkUpdate)
  const downloadUpdate = useStore((s) => s.downloadUpdate)
  const installUpdate = useStore((s) => s.installUpdate)
  const simulateUpdateFlow = useStore((s) => s.simulateUpdateFlow)
  const isDev = !(window.__isPackaged__ ?? true)
  const connected = wsState === 'connected'

  useEffect(() => {
    let cancelled = false
    window.loom.getAppVersion().then((v) => { if (!cancelled) setAppVersion(v) })
    window.loom.getLoomDir().then((d) => { if (!cancelled) setDataDir(d) })
    loomRpc<SystemHealth>('system.health')
      .then((data) => { if (!cancelled) setHealth(data) })
      .catch(() => { if (!cancelled) setHealthError(true) })
    return () => { cancelled = true }
  }, [])

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>关于</h3>
        <p className={styles.sectionDesc}>版本、更新和连接信息</p>
      </div>
      <div className={styles.contentBody}>
        <div className={styles.aboutSection}>
          {/* App info card */}
          <div className={styles.aboutCard}>
            <div className={styles.aboutAppRow}>
              <img
                src={isDev ? logoDev : logoRelease}
                alt="openLoom"
                className={styles.aboutAppIcon}
              />
              <div>
                <h4 className={styles.aboutAppName}>openLoom</h4>
                <p className={styles.aboutAppVer}>v{appVersion}</p>
              </div>
            </div>
            <p className={styles.aboutAppTag}>
              本地优先的私人 AI 助理。所有数据存储在本地。
            </p>
            <a
              className={styles.aboutGitLink}
              href="https://github.com/godsir/openloom"
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => { e.preventDefault(); window.loom.openExternal('https://github.com/godsir/openloom') }}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
              </svg>
              github.com/godsir/openloom
            </a>
            {dataDir && (
              <div className={styles.aboutDataRow}>
                <span className={styles.aboutDataLabel}>数据目录</span>
                <span className={styles.aboutDataPath}>{dataDir}</span>
              </div>
            )}
          </div>


          {/* System status */}
          <div className={styles.aboutCard}>
            <h4 className={styles.aboutCardTitle}>系统状态</h4>
            <div className={styles.aboutStatGrid}>
              <div className={styles.aboutStatItem}>
                <span className={styles.aboutStatLabel}>后端连接</span>
                <span className={`${styles.aboutStatValue} ${connected ? styles.aboutStatOk : styles.aboutStatWarn}`}>
                  {connected ? `localhost:${port}` : wsState}
                </span>
              </div>
              <div className={styles.aboutStatItem}>
                <span className={styles.aboutStatLabel}>当前模型</span>
                <span className={styles.aboutStatValue}>{currentModel || '未选择'}</span>
              </div>
              {health && (
                <>
                  <div className={styles.aboutStatItem}>
                    <span className={styles.aboutStatLabel}>Agent 数量</span>
                    <span className={styles.aboutStatValue}>{health.agent_count}</span>
                  </div>
                  <div className={styles.aboutStatItem}>
                    <span className={styles.aboutStatLabel}>工具数量</span>
                    <span className={styles.aboutStatValue}>{health.tool_count}</span>
                  </div>
                </>
              )}
            </div>
            {healthError && <p className={styles.toolsError}>系统信息加载失败</p>}
          </div>

          {/* Auto-update */}
          <div className={styles.aboutCard}>
            <h4 className={styles.aboutCardTitle}>自动更新</h4>
            <div className={styles.aboutUpdateBody}>
              {update.status === 'checking' && (
                <p className={styles.updateStatusText}>正在检查更新...</p>
              )}
              {update.status === 'available' && (
                <p className={styles.updateStatusAccent}>
                  {update.version ? `发现新版本 ${update.version}` : '发现新版本'}
                </p>
              )}
              {update.status === 'downloading' && (
                <>
                  <p className={styles.updateStatusAccent}>{update.progress.toFixed(0)}% 下载中</p>
                  <div className={styles.updateProgressBar}>
                    <div className={styles.updateProgressFill} style={{ width: `${update.progress}%` }} />
                  </div>
                </>
              )}
              {update.status === 'downloaded' && (
                <p className={styles.updateStatusAccent}>更新已就绪，重启后生效</p>
              )}
              {(update.status === 'no-update' || update.status === 'idle') && (
                <p className={styles.updateStatusText}>已是最新版本</p>
              )}
              {update.status === 'error' && (
                <p className={styles.updateStatusError}>{update.error ?? '检查更新失败'}</p>
              )}
            </div>
            <div className={styles.aboutUpdateActions}>
              {(update.status === 'idle' || update.status === 'no-update' || update.status === 'error') && (
                <>
                  <button className={styles.mcpConnectBtn} onClick={checkUpdate}>
                    检查更新
                  </button>
                  {isDev && (
                    <button className={styles.mcpDisconnectBtn} onClick={simulateUpdateFlow}>
                      测试更新
                    </button>
                  )}
                </>
              )}
              {update.status === 'available' && (
                <button className={styles.mcpConnectBtn} onClick={downloadUpdate}>
                  下载更新
                </button>
              )}
              {update.status === 'downloaded' && (
                <button className={styles.mcpConnectBtn} onClick={installUpdate}>
                  立即重启
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </>
  )
}

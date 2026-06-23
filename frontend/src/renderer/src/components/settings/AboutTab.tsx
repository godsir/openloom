import { useState, useEffect, useMemo } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { useLocale } from '../../i18n'
import Select, { SelectOption } from '../shared/Select'
import styles from '../shared/SettingsModal.module.css'
import logoDev from '@asset/icon_dev.png'
import logoRelease from '@asset/icon.png'

interface SystemHealth {
  status: string
  version: string
  agent_count: number
  tool_count: number
}

export default function AboutTab({ wsState }: { wsState: string }) {
  const { t } = useLocale()
  const [health, setHealth] = useState<SystemHealth | null>(null)
  const [healthError, setHealthError] = useState(false)
  const [appVersion, setAppVersion] = useState('...')
  const [dataDir, setDataDir] = useState('')
  const [updateChannel, setUpdateChannel] = useState<'stable' | 'beta'>('stable')
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
    window.loom.getUpdateChannel().then((ch: string) => { if (!cancelled) setUpdateChannel(ch as 'stable' | 'beta') })
    return () => { cancelled = true }
  }, [])

  const channelOptions = useMemo<SelectOption<'stable' | 'beta'>[]>(() => [
    { value: 'stable', label: t('about.channelStable') },
    { value: 'beta', label: t('about.channelBeta') },
  ], [t])

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>{t('about.title')}</h3>
        <p className={styles.sectionDesc}>{t('about.subtitle')}</p>
      </div>
      <div className={styles.contentBody}>
        <div className={styles.aboutLayout}>
          {/* Hero — app identity, visually heaviest */}
          <div className={styles.aboutHero}>
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
              {t('about.descriptionFull')}
            </p>
            <div className={styles.aboutHeroMeta}>
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
                <span className={styles.aboutHeroData}>
                  <span className={styles.aboutDataLabel}>{t('about.dataDir')}</span>
                  <span className={styles.aboutDataPath}>{dataDir}</span>
                </span>
              )}
            </div>
          </div>

          {/* Supporting cards — secondary */}
          <div className={styles.aboutGrid}>
            {/* System status */}
            <div className={styles.aboutCard}>
              <h4 className={styles.aboutCardTitle}>{t('about.systemStatus')}</h4>
              <div className={styles.aboutStatGrid}>
                <div className={styles.aboutStatItem}>
                  <span className={styles.aboutStatLabel}>{t('about.backendConnection')}</span>
                  <span className={`${styles.aboutStatValue} ${connected ? styles.aboutStatOk : styles.aboutStatWarn}`}>
                    {connected ? `localhost:${port}` : wsState}
                  </span>
                </div>
                <div className={styles.aboutStatItem}>
                  <span className={styles.aboutStatLabel}>{t('about.currentModel')}</span>
                  <span className={styles.aboutStatValue}>{currentModel || t('about.noModel')}</span>
                </div>
                {health && (
                  <>
                    <div className={styles.aboutStatItem}>
                      <span className={styles.aboutStatLabel}>{t('about.agentCount')}</span>
                      <span className={styles.aboutStatValue}>{health.agent_count}</span>
                    </div>
                    <div className={styles.aboutStatItem}>
                      <span className={styles.aboutStatLabel}>{t('about.toolCount')}</span>
                      <span className={styles.aboutStatValue}>{health.tool_count}</span>
                    </div>
                  </>
                )}
              </div>
              {healthError && <p className={styles.toolsError}>{t('about.systemLoadFailed')}</p>}
            </div>

            {/* Auto-update — fixed vertical structure, no wrap */}
            <div className={styles.aboutCard}>
              <div className={styles.aboutUpdateHead}>
                <span className={styles.aboutCardTitle}>{t('about.autoUpdate')}</span>
                <Select
                  value={updateChannel}
                  options={channelOptions}
                  onChange={ch => { setUpdateChannel(ch); window.loom.setUpdateChannel(ch) }}
                  variant="pill"
                />
              </div>
              <div className={styles.aboutUpdateStatus}>
                {update.status === 'checking' && (
                  <p className={styles.updateStatusText}>{t('about.checkingUpdate')}</p>
                )}
                {update.status === 'available' && (
                  <p className={styles.updateStatusAccent}>
                    {update.version ? t('about.newVersionAvailable', { version: update.version }) : t('about.newVersionFound')}
                  </p>
                )}
                {update.status === 'downloading' && (
                  <p className={styles.updateStatusAccent}>{t('about.downloading', { progress: update.progress.toFixed(0) })}</p>
                )}
                {update.status === 'downloaded' && (
                  <p className={styles.updateStatusAccent}>{t('about.readyToInstall')}</p>
                )}
                {(update.status === 'no-update' || update.status === 'idle') && (
                  <p className={styles.updateStatusText}>{t('about.upToDate')}</p>
                )}
                {update.status === 'error' && (
                  <p className={styles.updateStatusError}>{update.error ?? t('updates.checkFailed')}</p>
                )}
                {update.status === 'downloading' && (
                  <div className={styles.updateProgressBar}>
                    <div className={styles.updateProgressFill} style={{ width: `${update.progress}%` }} />
                  </div>
                )}
              </div>
              <div className={styles.aboutUpdateActions}>
                {(update.status === 'idle' || update.status === 'no-update' || update.status === 'error') && (
                  <>
                    <button className={styles.mcpConnectBtn} onClick={checkUpdate}>
                      {t('about.checkUpdate')}
                    </button>
                    {isDev && (
                      <button className={styles.mcpDisconnectBtn} onClick={simulateUpdateFlow}>
                        {t('about.testUpdate')}
                      </button>
                    )}
                  </>
                )}
                {update.status === 'available' && (
                  <button className={styles.mcpConnectBtn} onClick={downloadUpdate}>
                    {t('about.downloadUpdate')}
                  </button>
                )}
                {update.status === 'downloaded' && (
                  <button className={styles.mcpConnectBtn} onClick={installUpdate}>
                    {t('about.restartNow')}
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </>
  )
}

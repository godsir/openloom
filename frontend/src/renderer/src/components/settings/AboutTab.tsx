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
    window.loom.getAppVersion().then((v: string) => { if (!cancelled) setAppVersion(v) })
    window.loom.getLoomDir().then((d: string) => { if (!cancelled) setDataDir(d) })
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
    <div className={styles.aboutLayout}>

          {/* ── Hero ── */}
          <div className={styles.aboutHeroC3}>
            <img
              src={isDev ? logoDev : logoRelease}
              alt="openLoom"
              className={styles.aboutHeroC3Icon}
            />
            <h4 className={styles.aboutHeroC3Name}>openLoom</h4>
            <span className={styles.aboutHeroC3Version}>v{appVersion}</span>
            <p className={styles.aboutHeroC3Desc}>{t('about.descriptionFull')}</p>
            <a
              className={styles.aboutHeroC3Link}
              href="https://github.com/godsir/openloom"
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => { e.preventDefault(); window.loom.openExternal('https://github.com/godsir/openloom') }}
            >
              github.com/godsir/openloom
            </a>
          </div>

          {/* ── Info List ── */}
          <div className={styles.aboutList}>

            {/* System group */}
            <div className={styles.aboutListGroup}>
              <div className={styles.aboutListGroupLabel}>{t('about.groupSystem')}</div>
              <div className={styles.aboutListRow}>
                <span className={styles.aboutListRowLabel}>{t('about.currentModel')}</span>
                <span className={styles.aboutListRowValue}>{currentModel || t('about.noModel')}</span>
              </div>
              <div className={styles.aboutListRow}>
                <span className={styles.aboutListRowLabel}>{t('about.connection')}</span>
                <span className={`${styles.aboutListRowValue} ${connected ? styles.aboutListRowOk : styles.aboutListRowWarn}`}>
                  {connected ? `localhost:${port}` : wsState}
                </span>
              </div>
              {health && (
                <>
                  <div className={styles.aboutListRow}>
                    <span className={styles.aboutListRowLabel}>{t('about.agent')}</span>
                    <span className={styles.aboutListRowValue}>{health.agent_count}</span>
                  </div>
                  <div className={styles.aboutListRow}>
                    <span className={styles.aboutListRowLabel}>{t('about.tools')}</span>
                    <span className={styles.aboutListRowValue}>{health.tool_count}</span>
                  </div>
                </>
              )}
              {healthError && <p className={styles.toolsError}>{t('about.systemLoadFailed')}</p>}
            </div>

            {/* Storage & Update group */}
            <div className={styles.aboutListGroup}>
              <div className={styles.aboutListGroupLabel}>{t('about.groupStorage')}</div>
              {dataDir && (
                <div className={styles.aboutListRow}>
                  <span className={styles.aboutListRowLabel}>{t('about.data')}</span>
                  <span className={styles.aboutListRowValueMono}>{dataDir}</span>
                </div>
              )}
              <div className={styles.aboutListRow}>
                <span className={styles.aboutListRowLabel}>{t('about.update')}</span>
                <span className={styles.aboutListRowValue}>
                  {update.status === 'checking' ? t('about.checkingUpdate') :
                   update.status === 'available' ? t('about.newVersionFound') :
                   update.status === 'downloading' ? t('about.downloading', { progress: update.progress.toFixed(0) }) :
                   update.status === 'downloaded' ? t('about.readyToInstall') :
                   update.status === 'error' ? update.error :
                   t('about.upToDate')}
                </span>
              </div>

              {/* Update Controls */}
              <div className={styles.aboutUpdateControls}>
                <Select
                  value={updateChannel}
                  options={channelOptions}
                  onChange={async ch => {
                    setUpdateChannel(ch)
                    // The preference has already been persisted. A failed
                    // network check must not turn this UI event into an
                    // unhandled rejection.
                    await window.loom.setUpdateChannel(ch).catch(() => {})
                  }}
                  variant="pill"
                />
                {(update.status === 'idle' || update.status === 'no-update' || update.status === 'error') && (
                  <button className={styles.aboutActionBtnPrimary} onClick={checkUpdate}>
                    {t('about.checkUpdate')}
                  </button>
                )}
                {update.status === 'available' && (
                  <button className={styles.aboutActionBtnPrimary} onClick={downloadUpdate}>
                    {t('about.downloadUpdate')}
                  </button>
                )}
                {update.status === 'downloaded' && (
                  <button className={styles.aboutActionBtnPrimary} onClick={installUpdate}>
                    {t('about.restartNow')}
                  </button>
                )}
                {isDev && (
                  <button className={styles.aboutActionBtn} onClick={simulateUpdateFlow}>
                    {t('about.testUpdate')}
                  </button>
                )}
              </div>

              {/* Download Progress */}
              {update.status === 'downloading' && (
                <div className={styles.aboutProgressBar}>
                  <div className={styles.aboutProgressFill} style={{ width: `${update.progress}%` }} />
                </div>
              )}
            </div>
          </div>

          {/* ── Actions ── */}
          <div className={styles.aboutActionsC3}>
            <a
              className={styles.aboutActionBtn}
              href="https://github.com/godsir/openloom"
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => { e.preventDefault(); window.loom.openExternal('https://github.com/godsir/openloom') }}
            >
              GitHub
            </a>
          </div>
    </div>
  )
}

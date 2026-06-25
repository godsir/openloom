import { useMemo } from 'react'
import { useStore } from '../../stores'
import type { StreamPhase } from '../../stores/streaming'
import { IconMessageSquare, IconEdit, IconAlertCircle, IconDownload, IconSparkles, IconRotateCcw, IconBrain, IconEye, IconTerminal, IconCheck, IconChevronDown } from '../../utils/icons'
import { useLocale } from '../../i18n'
import styles from './AppShell.module.css'

export default function DynamicIslandCenter() {
  const { t } = useLocale()

  const appMode = useStore(s => s.appMode)
  const setAppMode = useStore(s => s.setAppMode)
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const streamingIds = useStore(s => s.streamingSessionIds)
  const streamingActivity = useStore(s => s.streamingActivity)
  const currentSessionId = useStore(s => s.currentSessionId)
  const engineState = useStore(s => s.engineState)
  const update = useStore(s => s.update)
  const islandTransient = useStore(s => s.islandTransient)
  const islandExpanded = useStore(s => s.islandExpanded)
  const setIslandExpanded = useStore(s => s.setIslandExpanded)

  const isStreaming = streamingIds.size > 0
  const isDownloading = update.status === 'downloading'
  const isDownloaded = update.status === 'downloaded'
  const isUpdateError = update.status === 'error'
  const isUpdateAvailable = update.status === 'available'
  const isEngineStopped = engineState === 'stopped'
  const hasTransient = !!islandTransient
  const isSplit = isStreaming && isDownloading
  const expandable = isStreaming || isDownloading || isDownloaded || isUpdateAvailable || isUpdateError || isEngineStopped

  const activeState = useMemo(() => {
    if (hasTransient) return 'transient' as const
    if (isEngineStopped) return 'crash' as const
    if (isSplit) return 'split' as const
    if (isDownloaded) return 'downloaded' as const
    if (isUpdateError) return 'uperror' as const
    if (isDownloading) return 'download' as const
    if (isUpdateAvailable) return 'update' as const
    if (isStreaming) return 'streaming' as const
    return 'idle' as const
  }, [hasTransient, isEngineStopped, isSplit, isDownloaded, isUpdateError, isDownloading, isUpdateAvailable, isStreaming])

  const activeStreamId = (currentSessionId && streamingIds.has(currentSessionId))
    ? currentSessionId
    : streamingIds.values().next().value
  const activity = activeStreamId ? streamingActivity[activeStreamId] : undefined
  const phase: StreamPhase = activity?.phase ?? 'generating'

  const pct = Math.round(update.progress * 100)

  const handleRestart = async () => {
    const newPort = await window.loom.restartEngine()
    useStore.getState().setPort(newPort)
    useStore.getState().setEngineState('running')
  }

  const phaseMeta: Record<StreamPhase, { icon: typeof IconBrain; title: string; sub?: string }> = {
    thinking: { icon: IconBrain, title: t('island.thinking'), sub: t('island.thinkingHint') },
    vision: {
      icon: IconEye,
      title: t('island.vision'),
      sub: activity?.visionTotal ? t('island.visionProgress', { done: activity.visionDone ?? 0, total: activity.visionTotal }) : t('island.visionHint'),
    },
    skill: { icon: IconSparkles, title: t('island.skill'), sub: activity?.detail ?? '' },
    tool: { icon: IconTerminal, title: t('island.tool'), sub: activity?.detail ?? '' },
    generating: { icon: IconSparkles, title: t('app.generating'), sub: t('app.generatingHint') },
  }
  const meta = phaseMeta[phase]
  const PhaseIcon = meta.icon

  // 展开态：详情卡片
  if (islandExpanded && expandable) {
    return (
      <div className={styles.islandExpanded} onClick={(e) => e.stopPropagation()}>
        <button
          className={styles.islandCollapseBtn}
          onClick={() => setIslandExpanded(false)}
          title={t('common.close')}
        >
          <IconChevronDown size={14} />
        </button>

        {activeState === 'streaming' && (
          <div className={styles.expandedBody}>
            <div className={styles.expandedHeader}>
              <span className={styles.dynamicPulse} />
              <PhaseIcon size={15} className={styles.dynamicIcon} />
              <span className={styles.expandedTitle}>{meta.title}</span>
            </div>
            <div className={styles.expandedDetail}>{meta.sub}</div>
            <div className={styles.expandedHint}>{t('island.streamingHint')}</div>
          </div>
        )}

        {activeState === 'download' && (
          <div className={styles.expandedBody}>
            <div className={styles.expandedHeader}>
              <IconDownload size={15} className={styles.dynamicIcon} />
              <span className={styles.expandedTitle}>{t('app.downloading')} v{update.version ?? ''}</span>
            </div>
            <div className={styles.expandedProgressRow}>
              <div className={styles.progressTrack}>
                <div className={styles.progressFill} style={{ width: `${pct}%` }} />
              </div>
              <span className={styles.expandedPct}>{pct}%</span>
            </div>
            <div className={styles.expandedHint}>
              {update.bytesPerSecond > 0
                ? `${(update.bytesPerSecond / 1024 / 1024).toFixed(1)} MB/s`
                : t('island.preparing')}
            </div>
          </div>
        )}

        {activeState === 'update' && (
          <div className={styles.expandedBody}>
            <div className={styles.expandedHeader}>
              <IconDownload size={15} className={styles.dynamicIcon} />
              <span className={styles.expandedTitle}>{t('app.updateAvailable')} v{update.version ?? ''}</span>
            </div>
            <div className={styles.expandedReleaseNotes}>
              {update.releaseNotes ?? t('island.noReleaseNotes')}
            </div>
            <div className={styles.expandedActions}>
              <button
                className={styles.islandActionBtn}
                onClick={() => useStore.setState({ updateModalOpen: true })}
              >
                {t('app.updateNow')}
              </button>
              <button
                className={styles.islandDismissBtn}
                onClick={() => useStore.getState().dismissUpdate()}
              >
                {t('common.dismiss')}
              </button>
            </div>
          </div>
        )}

        {activeState === 'crash' && (
          <div className={styles.expandedBody}>
            <div className={styles.expandedHeader}>
              <IconAlertCircle size={15} className={styles.dynamicIconCrash} />
              <span className={styles.expandedTitle}>{t('app.engineStopped')}</span>
            </div>
            <div className={styles.expandedHint}>{t('island.crashHint')}</div>
            <button className={styles.islandActionBtn} onClick={handleRestart}>
              <IconRotateCcw size={12} />
              {t('app.restartEngine')}
            </button>
          </div>
        )}
      </div>
    )
  }

  return (
    <div className={styles.islandCenter}>
      {/* Idle: mode toggle or settings title */}
      <div className={styles.dynamicLayer} data-active={activeState === 'idle' ? 'true' : 'false'}>
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

      {/* Split: streaming + download 并存分屏 */}
      <div className={styles.dynamicLayer} data-active={activeState === 'split' ? 'true' : 'false'}>
        <div className={styles.splitContainer}>
          <div className={styles.splitSide}>
            <span className={styles.dynamicPulse} />
            <PhaseIcon size={12} className={styles.dynamicIcon} />
            <span className={styles.dynamicTitle}>{meta.title}</span>
          </div>
          <div className={styles.splitDivider} />
          <div className={styles.splitSide}>
            <IconDownload size={12} className={styles.dynamicIcon} />
            <div className={styles.progressTrack}>
              <div className={styles.progressFill} style={{ width: `${pct}%` }} />
            </div>
            <span className={styles.dynamicTitle}>{pct}%</span>
          </div>
        </div>
      </div>

      {/* Transient feedback (复制成功等) */}
      <div className={styles.dynamicLayer} data-active={activeState === 'transient' ? 'true' : 'false'}>
        <div className={styles.layerRow}>
          <IconCheck size={13} className={styles.dynamicIconCheck} />
          <span className={styles.dynamicTitle}>{islandTransient?.text}</span>
        </div>
      </div>

      {/* Streaming — 按 phase 流转 */}
      <div className={styles.dynamicLayer} data-active={activeState === 'streaming' ? 'true' : 'false'}>
        <div className={styles.phaseContent} key={phase}>
          <div className={styles.layerRow}>
            <span className={styles.dynamicPulse} />
            <PhaseIcon size={13} className={styles.dynamicIcon} />
            <span className={styles.dynamicTitle}>{meta.title}</span>
          </div>
          <div className={styles.layerRow}>
            <span className={styles.dynamicSub}>{meta.sub}</span>
          </div>
        </div>
      </div>

      {/* Downloading */}
      <div className={styles.dynamicLayer} data-active={activeState === 'download' ? 'true' : 'false'}>
        <div className={styles.layerRow}>
          <IconDownload size={13} className={styles.dynamicIcon} />
          <span className={styles.dynamicTitle}>{t('app.downloading')} <span className={styles.dynamicNum}>{pct}%</span></span>
        </div>
        <div className={styles.layerRow}>
          <div className={styles.progressTrack}>
            <div className={styles.progressFill} style={{ width: `${pct}%` }} />
          </div>
        </div>
      </div>

      {/* Downloaded — 待安装 */}
      <div className={styles.dynamicLayer} data-active={activeState === 'downloaded' ? 'true' : 'false'}>
        <div className={styles.layerRow}>
          <IconCheck size={13} className={styles.dynamicIconCheck} />
          <span className={styles.dynamicTitle}>{t('island.downloaded')}</span>
        </div>
        <div className={styles.layerRow}>
          <button
            className={styles.islandActionBtn}
            onClick={() => useStore.getState().installUpdate()}
          >
            <IconRotateCcw size={12} />
            {t('island.restartInstall')}
          </button>
        </div>
      </div>

      {/* Update error */}
      <div className={styles.dynamicLayer} data-active={activeState === 'uperror' ? 'true' : 'false'}>
        <div className={styles.layerRow}>
          <IconAlertCircle size={13} className={styles.dynamicIconCrash} />
          <span className={styles.dynamicTitle}>{t('island.updateFailed')}</span>
        </div>
        <div className={styles.layerRow}>
          <button
            className={styles.islandActionBtn}
            onClick={() => useStore.getState().downloadUpdate()}
          >
            {t('island.retry')}
          </button>
        </div>
      </div>

      {/* Update available */}
      <div className={styles.dynamicLayer} data-active={activeState === 'update' ? 'true' : 'false'}>
        <div className={styles.layerRow}>
          <IconDownload size={13} className={styles.dynamicIcon} />
          <span className={styles.dynamicTitle}>{t('app.updateAvailable')}</span>
        </div>
        <div className={styles.layerRow}>
          <button
            className={styles.islandActionBtn}
            onClick={() => useStore.setState({ updateModalOpen: true })}
          >
            {t('app.updateNow')}
          </button>
          <button
            className={styles.islandDismissBtn}
            title={t('common.dismiss')}
            onClick={() => useStore.getState().dismissUpdate()}
          >
            ✕
          </button>
        </div>
      </div>

      {/* Engine crashed */}
      <div className={styles.dynamicLayer} data-active={activeState === 'crash' ? 'true' : 'false'}>
        <div className={styles.layerRow}>
          <IconAlertCircle size={13} className={styles.dynamicIconCrash} />
          <span className={styles.dynamicTitle}>{t('app.engineStopped')}</span>
        </div>
        <div className={styles.layerRow}>
          <button className={styles.islandActionBtn} onClick={handleRestart}>
            <IconRotateCcw size={12} />
            {t('app.restartEngine')}
          </button>
        </div>
      </div>
    </div>
  )
}

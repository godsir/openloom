import { useMemo, useEffect, useRef, useState } from 'react'
import { useStore } from '../../stores'
import { useIMStore, PLATFORM_LABELS, type Platform } from '../../stores/im'
import type { StreamPhase } from '../../stores/streaming'
import { IconMessageSquare, IconEdit, IconAlertCircle, IconDownload, IconSparkles, IconRotateCcw, IconBrain, IconEye, IconTerminal, IconCheck, IconChevronDown, IconUsers, IconX, IconArchive } from '../../utils/icons'
import { useLocale } from '../../i18n'
import PlatformIcon from '../shared/PlatformIcon'
import { streamBufferManager } from '../../services/stream-buffer'
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
  const imSessionSources = useIMStore(s => s.imSessionSources)
  const sessionTeamBindings = useStore(s => s.sessionTeamBindings)
  const isCompacting = useStore(s => s.isCompacting)

  const isStreaming = streamingIds.size > 0
  const isDownloading = update.status === 'downloading'
  const isDownloaded = update.status === 'downloaded'
  const isUpdateError = update.status === 'error'
  const isUpdateAvailable = update.status === 'available'
  const isEngineStopped = engineState === 'stopped'
  const hasTransient = !!islandTransient
  const isSplit = isStreaming && isDownloading
  const expandable = isStreaming || isDownloading || isDownloaded || isUpdateAvailable || isUpdateError || isEngineStopped || isCompacting

  const activeState = useMemo(() => {
    if (hasTransient) return 'transient' as const
    if (isCompacting) return 'compact' as const
    if (isEngineStopped) return 'crash' as const
    if (isSplit) return 'split' as const
    if (isDownloaded) return 'downloaded' as const
    if (isUpdateError) return 'uperror' as const
    if (isDownloading) return 'download' as const
    if (isUpdateAvailable) return 'update' as const
    if (isStreaming) return 'streaming' as const
    return 'idle' as const
  }, [hasTransient, isCompacting, isEngineStopped, isSplit, isDownloaded, isUpdateError, isDownloading, isUpdateAvailable, isStreaming])

  const activeStreamId = (currentSessionId && streamingIds.has(currentSessionId))
    ? currentSessionId
    : streamingIds.values().next().value
  const imSource = imSessionSources[currentSessionId || '']
  const streamImSource = activeStreamId ? imSessionSources[activeStreamId] : undefined
  const activity = activeStreamId ? streamingActivity[activeStreamId] : undefined
  const effectiveActivity = useMemo(() => {
    if (!activity) return undefined
    // If this session is using a team, show team phase instead of generic generating
    const hasTeam = !!(activeStreamId && sessionTeamBindings[activeStreamId])
    if (hasTeam && activity.phase === 'generating') {
      return { ...activity, phase: 'team' as StreamPhase }
    }
    return activity
  }, [activity, activeStreamId, sessionTeamBindings])
  const phase: StreamPhase = effectiveActivity?.phase ?? 'generating'

  // Per-session token usage for the actively streaming session
  const usageBySession = useStore(s => s.usageBySession)
  const currentModel = useStore(s => s.currentModel)
  const streamUsage = activeStreamId ? usageBySession.get(activeStreamId) : undefined

  // Streaming duration — how long the current turn has been running
  const streamStartRef = useRef(Date.now())
  const [streamDuration, setStreamDuration] = useState(0)
  useEffect(() => {
    if (isStreaming) {
      streamStartRef.current = Date.now()
      const iv = setInterval(() => setStreamDuration(Math.floor((Date.now() - streamStartRef.current) / 1000)), 1000)
      return () => clearInterval(iv)
    } else {
      setStreamDuration(0)
    }
  }, [isStreaming])

  // Compact duration — how long compaction has been running
  const compactStartRef = useRef(Date.now())
  const [compactDuration, setCompactDuration] = useState(0)
  useEffect(() => {
    if (isCompacting) {
      compactStartRef.current = Date.now()
      const iv = setInterval(() => setCompactDuration(Math.floor((Date.now() - compactStartRef.current) / 1000)), 1000)
      return () => clearInterval(iv)
    } else {
      setCompactDuration(0)
    }
  }, [isCompacting])

  const pct = Math.round(update.progress)

  // Auto-collapse expanded island when streaming ends — avoid leaving
  // the detail card open after the agent finishes replying.
  const wasStreamingRef = useRef(isStreaming)
  useEffect(() => {
    if (wasStreamingRef.current && !isStreaming && islandExpanded) {
      const t = setTimeout(() => setIslandExpanded(false), 1000)
      return () => clearTimeout(t)
    }
    wasStreamingRef.current = isStreaming
  }, [isStreaming, islandExpanded, setIslandExpanded])

  // 重启进行中标志：防止按钮无 loading 时被连点触发多次重启
  const [restarting, setRestarting] = useState(false)

  const handleRestart = async () => {
    if (restarting) return
    setRestarting(true)
    try {
      const newPort = await window.loom.restartEngine()
      useStore.getState().setPort(newPort)
      useStore.getState().setEngineState('running')
    } catch {
      // 引擎状态保持 stopped，由 crash 态继续展示
    } finally {
      setRestarting(false)
    }
  }

  // IM channel connect/disconnect → transient island notification
  const prevImConnectedRef = useRef<Record<string, boolean>>({})
  useEffect(() => {
    const unsub = (window as any).loom?.onIMChannelStatus?.((status: any) => {
      const key = `${status.platform}:${status.instanceId}`
      const prev = prevImConnectedRef.current[key]
      const label = PLATFORM_LABELS[status.platform] ?? status.platform
      if (status.connected && prev !== true) {
        useStore.getState().showIslandTransient(t('island.imConnected', { platform: label }), 3000)
      } else if (!status.connected && prev === true) {
        useStore.getState().showIslandTransient(t('island.imDisconnected', { platform: label }), 3000)
      }
      prevImConnectedRef.current[key] = status.connected
    })
    return () => unsub?.()
  }, [t])

  // IM inbound message → transient island notification ("{渠道图标} {渠道名} 收到一条消息")
  // - agent 流式回复中不打断灵动岛 streaming 显示
  // - 同一会话 3 秒节流，避免群聊刷屏
  const lastInboundRef = useRef<Record<string, number>>({})
  useEffect(() => {
    const unsub = (window as any).loom?.onIMMessage?.((msg: any) => {
      if (!msg) return
      if (useStore.getState().streamingSessionIds.size > 0) return
      const convKey = `${msg.platform}:${msg.conversationId}`
      const now = Date.now()
      if (now - (lastInboundRef.current[convKey] || 0) < 3000) return
      lastInboundRef.current[convKey] = now
      const label = PLATFORM_LABELS[msg.platform as keyof typeof PLATFORM_LABELS] ?? msg.platform
      useStore.getState().showIslandTransient(t('island.imMessageReceived', { platform: label }), 3500, msg.platform)
    })
    return () => unsub?.()
  }, [t])

  const phaseMeta: Record<StreamPhase, { icon: typeof IconBrain; title: string; sub?: string }> = {
    thinking: { icon: IconBrain, title: t('island.thinking'), sub: t('island.thinkingHint') },
    vision: { icon: IconEye, title: t('island.vision'), sub: activity?.visionTotal ? t('island.visionProgress', { done: activity.visionDone ?? 0, total: activity.visionTotal }) : t('island.visionHint') },
    skill: { icon: IconSparkles, title: t('island.skill'), sub: activity?.detail ?? '' },
    tool: { icon: IconTerminal, title: t('island.tool'), sub: activity?.detail ?? '' },
    team: { icon: IconUsers, title: t('island.team'), sub: activity?.detail ?? '' },
    generating: { icon: IconSparkles, title: t('app.generating'), sub: t('app.generatingHint') },
    extracting: { icon: IconBrain, title: t('island.extracting'), sub: t('island.extractingHint') },
  }
  const meta = phaseMeta[phase]
  const PhaseIcon = meta.icon

  // Live buffer snapshot when expanded — shows running tools in the detail card.
  // Computed inline (cheap: just filters a few arrays). The snapshot returns the
  // live mutable BufferState, so repeated calls reflect the latest state.
  const allRunningLive = islandExpanded && activeStreamId
    ? (() => {
        const snap = streamBufferManager.snapshot(activeStreamId)
        if (!snap) return [] as Array<{ id: string; kind: 'skill' | 'tool' | 'proc'; name: string; detail: string }>
        const shells = snap.shellCalls.filter(s => s.status === 'running')
        const skills = snap.skillCalls.filter(s => s.status === 'running')
        const procs = snap.processAcc.filter(p => !p.exited)
        return [
          ...skills.map(s => ({ id: s.id, kind: 'skill' as const, name: s.name, detail: '' })),
          ...shells.map(s => {
            const args = s.args ?? {}
            const cmd = (args.command as string) || (args.path as string) || (args.pattern as string) || ''
            return { id: s.id, kind: 'tool' as const, name: s.name, detail: cmd ? String(cmd).slice(0, 80) : '' }
          }),
          ...procs.map(p => ({ id: p.pid, kind: 'proc' as const, name: p.pid, detail: `${p.lines.length} lines` })),
        ]
      })()
    : []

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
              {streamImSource && (
                <PlatformIcon platform={streamImSource.platform} size={14} />
              )}
            </div>
            {/* Running items — what the AI is actually doing right now */}
            {allRunningLive.length > 0 && (
              <div className={styles.expandedDetail}>
                {allRunningLive.slice(0, 5).map((item) => (
                  <div key={item.id} className={styles.expandedItem}>
                    <span className={styles.expandedItemKind}>
                      {item.kind === 'skill' ? <IconSparkles size={10} /> : item.kind === 'proc' ? <IconTerminal size={10} /> : <IconTerminal size={10} />}
                      {item.name}
                    </span>
                    {item.detail && <span className={styles.expandedItemDetail}>{item.detail}</span>}
                  </div>
                ))}
                {allRunningLive.length > 5 && (
                  <span className={styles.expandedMore}>{t('island.andMore', { n: allRunningLive.length - 5 })}</span>
                )}
              </div>
            )}
            {/* Phase-specific real content */}
            {allRunningLive.length === 0 && (
              <div className={styles.expandedDetail}>
                {meta.sub && <span>{meta.sub}</span>}
              </div>
            )}
            {/* Token & model info — always visible during streaming */}
            <div className={styles.expandedHint}>
              {streamUsage ? (
                <span>
                  {streamUsage.model || currentModel}
                  {' · '}
                  {streamUsage.prompt + streamUsage.completion >= 1000
                    ? `${((streamUsage.prompt + streamUsage.completion) / 1000).toFixed(1)}k`
                    : `${streamUsage.prompt + streamUsage.completion}`} tokens
                </span>
              ) : (
                <span>{currentModel || t('island.streamingHint')}</span>
              )}
              {' · '}
              <span>{Math.floor(streamDuration / 60)}:{(streamDuration % 60).toString().padStart(2, '0')}</span>
            </div>
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
            <div className={styles.layerRow}>
              <button
                className={styles.islandDismissBtn}
                onClick={(e) => { e.stopPropagation(); useStore.getState().cancelDownload() }}
              >
                {t('updates.cancelDownload')}
              </button>
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
            <button className={styles.islandActionBtn} onClick={handleRestart} disabled={restarting}>
              <IconRotateCcw size={12} />
              {restarting ? t('app.restarting') : t('app.restartEngine')}
            </button>
          </div>
        )}

        {activeState === 'compact' && (
          <div className={styles.expandedBody}>
            <div className={styles.expandedHeader}>
              <span className={styles.dynamicPulse} />
              <IconArchive size={15} className={styles.dynamicIcon} />
              <span className={styles.expandedTitle}>{t('island.compacting')}</span>
            </div>
            <div className={styles.expandedHint}>{t('island.compactingHint')}</div>
            <div className={styles.expandedHint}>
              <span>{Math.floor(compactDuration / 60)}:{(compactDuration % 60).toString().padStart(2, '0')}</span>
            </div>
          </div>
        )}
      </div>
    )
  }

  return (
    <div className={styles.islandCenter}>
      {/* Idle: mode toggle or IM session or settings title */}
      <div className={styles.dynamicLayer} data-active={activeState === 'idle' ? 'true' : 'false'}>
        {appMode === 'settings' ? (
          <span className={styles.titlebarPageTitle}>{t('app.settings')}</span>
        ) : imSource ? (
          <div className={styles.layerRow}>
            <PlatformIcon platform={imSource.platform} size={16} />
            <span className={styles.dynamicTitle}>{PLATFORM_LABELS[imSource.platform]}</span>
          </div>
        ) : (
          <div className={styles.modeToggle} data-active={appMode} role="radiogroup" aria-label={t('app.modeSwitch')}>
            <button
              className={`${styles.modeToggleOption} ${appMode === 'chat' ? styles.modeToggleOptionActive : ''}`}
              onClick={(e) => { e.stopPropagation(); if (appMode === 'chat') return; setAppMode('chat'); if (!sidebarOpen) toggleSidebar() }}
            >
              <IconMessageSquare size={13} />
              <span>{t('app.chat')}</span>
            </button>
            <button
              className={`${styles.modeToggleOption} ${appMode === 'write' ? styles.modeToggleOptionActive : ''}`}
              onClick={(e) => { e.stopPropagation(); if (appMode === 'write') return; setAppMode('write'); if (sidebarOpen) toggleSidebar() }}
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

      {/* Compacting — /compact slash command */}
      <div className={styles.dynamicLayer} data-active={activeState === 'compact' ? 'true' : 'false'}>
        <div className={styles.layerRow}>
          <span className={styles.dynamicPulse} />
          <IconArchive size={13} className={styles.dynamicIcon} />
          <span className={styles.dynamicTitle}>{t('island.compacting')}</span>
          <span className={styles.dynamicNum}>{compactDuration}s</span>
        </div>
        <div className={styles.layerRow}>
          <span className={styles.dynamicSub}>{t('island.compactingHint')}</span>
        </div>
      </div>

      {/* Transient feedback (复制成功等) */}
      <div className={styles.dynamicLayer} data-active={activeState === 'transient' ? 'true' : 'false'}>
        <div className={styles.layerRow}>
          {islandTransient?.platform ? (
            <PlatformIcon platform={islandTransient.platform as Platform} size={13} />
          ) : (
            <IconCheck size={13} className={styles.dynamicIconCheck} />
          )}
          <span className={styles.dynamicTitle}>{islandTransient?.text}</span>
        </div>
      </div>

      {/* Streaming — phase transitions without remount */}
      <div className={styles.dynamicLayer} data-active={activeState === 'streaming' ? 'true' : 'false'}>
        <div className={styles.phaseContent} data-phase={phase}>
          <div className={styles.layerRow}>
            <span className={styles.dynamicPulse} />
            <PhaseIcon size={13} className={styles.dynamicIcon} />
            <span className={styles.dynamicTitle}>{meta.title}</span>
            {streamImSource && (
              <PlatformIcon platform={streamImSource.platform} size={12} />
            )}
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
            onClick={(e) => { e.stopPropagation(); useStore.getState().installUpdate() }}
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
            onClick={(e) => { e.stopPropagation(); useStore.getState().downloadUpdate() }}
          >
            {t('island.retry')}
          </button>
          <button
            className={styles.islandDismissBtn}
            title={t('common.dismiss')}
            onClick={(e) => { e.stopPropagation(); useStore.getState().dismissUpdate() }}
          >
            {t('common.close')}
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
            onClick={(e) => { e.stopPropagation(); useStore.setState({ updateModalOpen: true }) }}
          >
            {t('app.updateNow')}
          </button>
          <button
            className={styles.islandDismissBtn}
            title={t('common.dismiss')}
            onClick={(e) => { e.stopPropagation(); useStore.getState().dismissUpdate() }}
          >
            <IconX size={11} />
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
          <button className={styles.islandActionBtn} onClick={(e) => { e.stopPropagation(); handleRestart() }} disabled={restarting}>
            <IconRotateCcw size={12} />
            {restarting ? t('app.restarting') : t('app.restartEngine')}
          </button>
        </div>
      </div>
    </div>
  )
}

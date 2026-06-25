import { useState } from 'react'
import { useLocale } from '../../i18n'
import { useStore } from '../../stores'
import TypingIndicator from '../shared/TypingIndicator'
import { IconDownload, IconCheck, IconAlertCircle } from '../../utils/icons'
import styles from '../shared/SettingsModal.module.css'
import tabStyles from './DevTestTab.module.css'

/** 迷你更新弹窗预览卡片：点击设置对应状态（不弹窗），高亮当前激活态 */
function UpdatePreviewCard({ label, labelCn, onClick }: { label: string; labelCn: string; onClick: () => void }) {
  const update = useStore((s) => s.update)
  const { t } = useLocale()
  const status = update.status
  const matched = label.toLowerCase() === status
  const pct = Math.round(update.progress)

  const render = () => {
    if (label === 'Available') {
      return (
        <div className={tabStyles.previewBody}>
          <div className={tabStyles.previewHeader}><IconDownload size={14} className={tabStyles.previewIcon} />{t('updates.found', { version: update.version ?? '' })}</div>
          <div className={tabStyles.previewNotes}>{update.releaseNotes?.slice(0, 60) ?? ''}</div>
          <div className={tabStyles.previewActions}><span className={tabStyles.previewPrimary}>{t('updates.download')}</span></div>
        </div>
      )
    }
    if (label === 'Downloading') {
      return (
        <div className={tabStyles.previewBody}>
          <div className={tabStyles.previewHeader}><IconDownload size={14} className={tabStyles.previewIcon} />{t('updates.downloading', { version: update.version ?? '' })}</div>
          <div className={tabStyles.previewPct}>{pct}%</div>
          <div className={tabStyles.previewBar}><div className={tabStyles.previewBarFill} style={{ width: `${pct}%` }} /></div>
        </div>
      )
    }
    if (label === 'Downloaded') {
      return (
        <div className={tabStyles.previewBody}>
          <div className={tabStyles.previewHeader}><IconCheck size={14} className={tabStyles.previewIconSuccess} />{t('updates.downloadComplete', { version: update.version ?? '' })}</div>
          <div className={tabStyles.previewActions}><span className={tabStyles.previewPrimary}>{t('updates.restartNow')}</span></div>
        </div>
      )
    }
    return (
      <div className={tabStyles.previewBody}>
        <div className={tabStyles.previewHeader}><IconAlertCircle size={14} className={tabStyles.previewIconError} />{t('updates.failed')}</div>
        <div className={tabStyles.previewError}>{update.error ?? ''}</div>
      </div>
    )
  }

  return (
    <button
      className={`${tabStyles.updatePreviewCard} ${matched ? tabStyles.previewActive : ''}`}
      onClick={onClick}
    >
      <div className={tabStyles.previewLabel}>{labelCn}{matched && ' ●'}</div>
      {render()}
    </button>
  )
}

export default function DevTestTab() {
  const { t } = useLocale()
  const showPermissionConfirm = useStore((s) => s.showPermissionConfirm)
  const showConfirm = useStore((s) => s.showConfirm)
  const addToast = useStore((s) => s.addToast)
  const setShowOnboarding = useStore((s) => s.setShowOnboarding)
  const dismissUpdate = useStore((s) => s.dismissUpdate)
  const set = useStore.setState

  const [showTyping, setShowTyping] = useState(false)

  // ─── Dialog triggers ───

  const triggerHighRiskPermission = () => {
    showPermissionConfirm(
      t('permissions.toolConfirm'),
      `${t('permissions.highRisk')}\n${t('permissions.targetPath', { path: 'rm -rf /tmp/test' })}\n${t('permissions.confirmPrompt')}`,
      'bash',
      true,
    ).catch(() => {})
  }

  const triggerMediumRiskPermission = () => {
    showPermissionConfirm(
      t('permissions.toolConfirm'),
      `${t('permissions.mediumRisk')}\n${t('permissions.targetPath', { path: '/etc/config.json' })}\n${t('permissions.confirmPrompt')}`,
      'read',
    ).catch(() => {})
  }

  const triggerConfirmDanger = () => {
    showConfirm(
      'Delete Session',
      'Are you sure you want to permanently delete this session?\n\nThis action cannot be undone.',
      true,
    ).then((ok) => {
      addToast({ type: ok ? 'success' : 'info', message: ok ? 'Session deleted (simulated)' : 'Deletion cancelled' })
    })
  }

  const triggerConfirmNormal = () => {
    showConfirm('Save Changes', 'Do you want to save your changes before closing?').then((ok) => {
      addToast({ type: ok ? 'success' : 'info', message: ok ? 'Changes saved' : 'Discarded' })
    })
  }

  // ─── Toast triggers ───

  const showToast = (type: 'info' | 'success' | 'warning' | 'error') => {
    const messages: Record<string, string> = {
      info: 'This is an informational message with some helpful context.',
      success: 'Operation completed successfully! Your changes have been saved.',
      warning: 'Something might need your attention. Please review before continuing.',
      error: 'An error occurred while processing your request. Please try again.',
    }
    addToast({ type, message: messages[type] })
  }

  const showActionToast = () => {
    addToast({
      type: 'info',
      message: 'File "config.json" was modified externally.',
      action: { label: 'Reload', onClick: () => addToast({ type: 'success', message: 'File reloaded' }) },
      duration: 0,
    })
  }

  const showPersistentToast = () => {
    addToast({ type: 'warning', message: '⚠ This toast stays until dismissed (duration: 0).', duration: 0 })
  }

  const showStackedToasts = () => {
    const types: Array<'info' | 'success' | 'warning' | 'error'> = ['info', 'success', 'warning', 'error']
    types.forEach((type, i) => {
      setTimeout(() => {
        addToast({ type, message: `[Toast #${i + 1}] ${type.toUpperCase()} — testing stacked appearance.`, duration: 6000 })
      }, i * 200)
    })
  }

  const showLongToast = () => {
    addToast({
      type: 'info',
      message: 'This is a very long toast message to test how the container handles overflow, wrapping, and truncation with really long text content that spans multiple lines.',
      duration: 8000,
    })
  }

  // ─── Update modal states ───

  const triggerUpdateAvailable = () => {
    set({
      update: {
        status: 'available' as const,
        version: '9.9.9-test',
        releaseNotes: '## What\'s Changed\n\n- New feature A\n- Bug fix B\n- Performance improvements',
        progress: 0, bytesPerSecond: 0, transferred: 0, total: 0, error: null,
      },
      updateModalOpen: true,
    })
  }

  const triggerUpdateDownloading = () => {
    set({
      update: {
        status: 'downloading' as const,
        version: '9.9.9-test', releaseNotes: null,
        progress: 67, bytesPerSecond: 1024 * 1024 * 2, transferred: 35_000_000, total: 52_000_000, error: null,
      },
      updateModalOpen: true,
    })
  }

  const triggerUpdateDownloaded = () => {
    set({
      update: {
        status: 'downloaded' as const,
        version: '9.9.9-test', releaseNotes: null,
        progress: 100, bytesPerSecond: 0, transferred: 52_000_000, total: 52_000_000, error: null,
      },
      updateModalOpen: true,
    })
  }

  const triggerUpdateError = () => {
    set({
      update: {
        status: 'error' as const,
        version: '9.9.9-test', releaseNotes: null,
        progress: 0, bytesPerSecond: 0, transferred: 0, total: 0,
        error: 'Failed to download update: network timeout after 30s',
      },
      updateModalOpen: true,
    })
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>{t('devTest.title')}</h3>
        <p className={styles.sectionDesc}>{t('devTest.subtitle')}</p>
      </div>
      <div className={styles.contentBody}>
        {/* ── Permission & Confirm Dialogs ── */}
        <div className={tabStyles.section}>
          <h4 className={tabStyles.sectionTitle}>{t('devTest.dialogsSection')}</h4>
          <p className={tabStyles.sectionDesc}>{t('devTest.dialogsDesc')}</p>
          <div className={tabStyles.btnRow}>
            <button className={tabStyles.btnDanger} onClick={triggerHighRiskPermission}>高风险权限</button>
            <button className={tabStyles.btnWarning} onClick={triggerMediumRiskPermission}>中风险权限</button>
            <button className={tabStyles.btnDangerOutline} onClick={triggerConfirmDanger}>危险确认</button>
            <button className={tabStyles.btnOutline} onClick={triggerConfirmNormal}>普通确认</button>
          </div>
        </div>

        <div className={tabStyles.divider} />

        {/* ── Toast Variants ── */}
        <div className={tabStyles.section}>
          <h4 className={tabStyles.sectionTitle}>{t('devTest.toastSection')}</h4>
          <p className={tabStyles.sectionDesc}>{t('devTest.toastDesc')}</p>
          <div className={tabStyles.btnRow}>
            <button className={tabStyles.btnToastInfo} onClick={() => showToast('info')}>信息</button>
            <button className={tabStyles.btnToastSuccess} onClick={() => showToast('success')}>成功</button>
            <button className={tabStyles.btnToastWarning} onClick={() => showToast('warning')}>警告</button>
            <button className={tabStyles.btnToastError} onClick={() => showToast('error')}>错误</button>
            <button className={tabStyles.btnOutline} onClick={showLongToast}>长文本</button>
            <button className={tabStyles.btnAccent} onClick={showActionToast}>带操作</button>
            <button className={tabStyles.btnAccentOutline} onClick={showPersistentToast}>持久化</button>
            <button className={tabStyles.btnOutline} onClick={showStackedToasts}>堆叠 4 个</button>
          </div>
        </div>

        <div className={tabStyles.divider} />

        {/* ── Update Modal States ── */}
        <div className={tabStyles.section}>
          <h4 className={tabStyles.sectionTitle}>更新弹窗</h4>
          <p className={tabStyles.sectionDesc}>模拟更新流程各状态 UI。</p>
          <div className={tabStyles.btnRow}>
            <button className={tabStyles.btnOutline} onClick={triggerUpdateAvailable}>有更新</button>
            <button className={tabStyles.btnOutline} onClick={triggerUpdateDownloading}>下载中</button>
            <button className={tabStyles.btnOutline} onClick={triggerUpdateDownloaded}>已下载</button>
            <button className={tabStyles.btnDangerOutline} onClick={triggerUpdateError}>错误</button>
            <button className={tabStyles.btnOutline} onClick={dismissUpdate}>忽略</button>
          </div>

          {/* 内联预览：不弹窗，直接展示各状态 */}
          <p className={tabStyles.sectionDesc} style={{ marginTop: 14 }}>内联预览（不弹窗）：</p>
          <div className={tabStyles.updatePreviewRow}>
            <UpdatePreviewCard
              label="Available"
              labelCn="有更新"
              onClick={() => set({ update: { status: 'available' as const, version: '9.9.9-test', releaseNotes: '## What\'s Changed\n\n- New feature A\n- Bug fix B', progress: 0, bytesPerSecond: 0, transferred: 0, total: 0, error: null }, updateModalOpen: false })}
            />
            <UpdatePreviewCard
              label="Downloading"
              labelCn="下载中"
              onClick={() => set({ update: { status: 'downloading' as const, version: '9.9.9-test', releaseNotes: null, progress: 67, bytesPerSecond: 1024 * 1024 * 2, transferred: 35_000_000, total: 52_000_000, error: null }, updateModalOpen: false })}
            />
            <UpdatePreviewCard
              label="Downloaded"
              labelCn="已下载"
              onClick={() => set({ update: { status: 'downloaded' as const, version: '9.9.9-test', releaseNotes: null, progress: 100, bytesPerSecond: 0, transferred: 52_000_000, total: 52_000_000, error: null }, updateModalOpen: false })}
            />
            <UpdatePreviewCard
              label="Error"
              labelCn="错误"
              onClick={() => set({ update: { status: 'error' as const, version: '9.9.9-test', releaseNotes: null, progress: 0, bytesPerSecond: 0, transferred: 0, total: 0, error: 'Failed to download update: network timeout after 30s' }, updateModalOpen: false })}
            />
          </div>
        </div>

        <div className={tabStyles.divider} />

        {/* ── Dynamic Island States ── */}
        <div className={tabStyles.section}>
          <h4 className={tabStyles.sectionTitle}>Dynamic Island</h4>
          <p className={tabStyles.sectionDesc}>模拟灵动岛状态，直接在设置页查看效果。</p>
          <div className={tabStyles.btnRow}>
            <button className={tabStyles.btnOutline} onClick={() => {
              useStore.getState().removeStreamingSession('test-dynamic')
              useStore.setState({ update: { ...useStore.getState().update, status: 'idle' }, engineState: 'running' })
            }}>重置 (空闲)</button>
            <button className={tabStyles.btnAccentOutline} onClick={() => {
              useStore.getState().addStreamingSession('test-dynamic')
              useStore.getState().setStreamingActivity('test-dynamic', { phase: 'generating' })
            }}>AI 生成中</button>
            <button className={tabStyles.btnOutline} onClick={() => {
              useStore.getState().addStreamingSession('test-dynamic')
              useStore.getState().setStreamingActivity('test-dynamic', { phase: 'thinking' })
            }}>思考中</button>
            <button className={tabStyles.btnOutline} onClick={() => {
              useStore.getState().addStreamingSession('test-dynamic')
              useStore.getState().setStreamingActivity('test-dynamic', { phase: 'tool', detail: 'read_file' })
            }}>工具调用</button>
            <button className={tabStyles.btnOutline} onClick={() => {
              useStore.getState().addStreamingSession('test-dynamic')
              useStore.getState().setStreamingActivity('test-dynamic', { phase: 'skill', detail: 'web_search' })
            }}>技能调用</button>
            <button className={tabStyles.btnOutline} onClick={() => {
              useStore.getState().addStreamingSession('test-dynamic')
              useStore.getState().setStreamingActivity('test-dynamic', { phase: 'vision', visionDone: 2, visionTotal: 3 })
            }}>视觉处理</button>
            <button className={tabStyles.btnAccentOutline} onClick={() => {
              // 自动流转演示：thinking → tool → generating
              useStore.getState().addStreamingSession('test-dynamic')
              const s = useStore.getState()
              s.setStreamingActivity('test-dynamic', { phase: 'thinking' })
              setTimeout(() => useStore.getState().setStreamingActivity('test-dynamic', { phase: 'tool', detail: 'read_file' }), 1500)
              setTimeout(() => useStore.getState().setStreamingActivity('test-dynamic', { phase: 'generating' }), 3000)
            }}>自动流转</button>
            <button className={tabStyles.btnOutline} onClick={() => {
              useStore.getState().removeStreamingSession('test-dynamic')
              useStore.setState({ update: { ...useStore.getState().update, status: 'downloading', version: '9.9.9-test', progress: 0.67 } })
            }}>下载中</button>
            <button className={tabStyles.btnOutline} onClick={() => {
              useStore.getState().removeStreamingSession('test-dynamic')
              useStore.setState({ update: { ...useStore.getState().update, status: 'available', version: '9.9.9-test' } })
            }}>有更新</button>
            <button className={tabStyles.btnDangerOutline} onClick={() => {
              useStore.getState().removeStreamingSession('test-dynamic')
              useStore.setState({ update: { ...useStore.getState().update, status: 'idle' }, engineState: 'stopped' })
            }}>引擎崩溃</button>
            <button className={tabStyles.btnToastSuccess} onClick={() => {
              useStore.getState().showIslandTransient('已复制')
            }}>瞬态反馈 (已复制)</button>
            <button className={tabStyles.btnAccentOutline} onClick={() => {
              useStore.getState().addStreamingSession('test-dynamic')
              useStore.getState().setStreamingActivity('test-dynamic', { phase: 'generating' })
              useStore.setState({ update: { ...useStore.getState().update, status: 'downloading', version: '9.9.9-test', progress: 0.45 } })
            }}>分屏 (生成+下载)</button>
          </div>
        </div>

        <div className={tabStyles.divider} />

        {/* ── Status Bars & Misc ── */}
        <div className={tabStyles.section}>
          <h4 className={tabStyles.sectionTitle}>状态栏与其他</h4>
          <p className={tabStyles.sectionDesc}>预览内联状态指示器和小型 UI 元素。</p>

          {/* Status bar previews */}
          <div className={tabStyles.statusPreviewGroup}>
            <div className={tabStyles.statusBar}>
              <span className={tabStyles.statusDot} /> AI 正在回复...
            </div>
            <div className={`${tabStyles.statusBar} ${tabStyles.statusBarPurple}`}>
              <span className={tabStyles.statusLabel}>子代理：</span> 扫描文件中...
            </div>
            <div className={`${tabStyles.statusBar} ${tabStyles.statusBarRed}`}>
              <span>!</span> 连接已断开 — 3 秒后重连
            </div>
          </div>

          {/* Typing indicator & overlays */}
          <div className={tabStyles.btnRow} style={{ marginTop: 14 }}>
            <button className={tabStyles.btnOutline} onClick={() => setShowTyping(!showTyping)}>
              {showTyping ? '隐藏' : '显示'}打字指示器
            </button>
            <button className={tabStyles.btnAccentOutline} onClick={() => setShowOnboarding(true)}>
              显示引导页
            </button>
          </div>
          {showTyping && (
            <div className={tabStyles.inlinePreview}>
              <TypingIndicator />
              <span className={tabStyles.inlineLabel}>AI 思考中...</span>
            </div>
          )}
        </div>
      </div>
    </>
  )
}

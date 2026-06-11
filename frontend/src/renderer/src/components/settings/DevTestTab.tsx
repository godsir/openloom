import { useState } from 'react'
import { useLocale } from '../../i18n'
import { useStore } from '../../stores'
import TypingIndicator from '../shared/TypingIndicator'
import styles from '../shared/SettingsModal.module.css'
import tabStyles from './DevTestTab.module.css'

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
            <button className={tabStyles.btnDanger} onClick={triggerHighRiskPermission}>High‑Risk Permission</button>
            <button className={tabStyles.btnWarning} onClick={triggerMediumRiskPermission}>Medium‑Risk Permission</button>
            <button className={tabStyles.btnDangerOutline} onClick={triggerConfirmDanger}>Confirm (Danger)</button>
            <button className={tabStyles.btnOutline} onClick={triggerConfirmNormal}>Confirm (Normal)</button>
          </div>
        </div>

        <div className={tabStyles.divider} />

        {/* ── Toast Variants ── */}
        <div className={tabStyles.section}>
          <h4 className={tabStyles.sectionTitle}>{t('devTest.toastSection')}</h4>
          <p className={tabStyles.sectionDesc}>{t('devTest.toastDesc')}</p>
          <div className={tabStyles.btnRow}>
            <button className={tabStyles.btnToastInfo} onClick={() => showToast('info')}>Info</button>
            <button className={tabStyles.btnToastSuccess} onClick={() => showToast('success')}>Success</button>
            <button className={tabStyles.btnToastWarning} onClick={() => showToast('warning')}>Warning</button>
            <button className={tabStyles.btnToastError} onClick={() => showToast('error')}>Error</button>
            <button className={tabStyles.btnOutline} onClick={showLongToast}>Long Text</button>
            <button className={tabStyles.btnAccent} onClick={showActionToast}>With Action</button>
            <button className={tabStyles.btnAccentOutline} onClick={showPersistentToast}>Persistent</button>
            <button className={tabStyles.btnOutline} onClick={showStackedToasts}>Stack 4</button>
          </div>
        </div>

        <div className={tabStyles.divider} />

        {/* ── Update Modal States ── */}
        <div className={tabStyles.section}>
          <h4 className={tabStyles.sectionTitle}>Update Modal</h4>
          <p className={tabStyles.sectionDesc}>Simulate each update flow UI state.</p>
          <div className={tabStyles.btnRow}>
            <button className={tabStyles.btnOutline} onClick={triggerUpdateAvailable}>Available</button>
            <button className={tabStyles.btnOutline} onClick={triggerUpdateDownloading}>Downloading</button>
            <button className={tabStyles.btnOutline} onClick={triggerUpdateDownloaded}>Downloaded</button>
            <button className={tabStyles.btnDangerOutline} onClick={triggerUpdateError}>Error</button>
            <button className={tabStyles.btnOutline} onClick={dismissUpdate}>Dismiss</button>
          </div>
        </div>

        <div className={tabStyles.divider} />

        {/* ── Status Bars & Misc ── */}
        <div className={tabStyles.section}>
          <h4 className={tabStyles.sectionTitle}>Status Bars & Misc</h4>
          <p className={tabStyles.sectionDesc}>Preview inline status indicators and small UI elements.</p>

          {/* Status bar previews */}
          <div className={tabStyles.statusPreviewGroup}>
            <div className={tabStyles.statusBar}>
              <span className={tabStyles.statusDot} /> AI is replying...
            </div>
            <div className={`${tabStyles.statusBar} ${tabStyles.statusBarPurple}`}>
              <span className={tabStyles.statusLabel}>Subagent:</span> scanning files...
            </div>
            <div className={`${tabStyles.statusBar} ${tabStyles.statusBarRed}`}>
              <span>!</span> Connection lost — reconnecting in 3s
            </div>
          </div>

          {/* Typing indicator & overlays */}
          <div className={tabStyles.btnRow} style={{ marginTop: 14 }}>
            <button className={tabStyles.btnOutline} onClick={() => setShowTyping(!showTyping)}>
              {showTyping ? 'Hide' : 'Show'} Typing Indicator
            </button>
            <button className={tabStyles.btnAccentOutline} onClick={() => setShowOnboarding(true)}>
              Show Onboarding
            </button>
          </div>
          {showTyping && (
            <div className={tabStyles.inlinePreview}>
              <TypingIndicator />
              <span className={tabStyles.inlineLabel}>AI is thinking...</span>
            </div>
          )}
        </div>
      </div>
    </>
  )
}

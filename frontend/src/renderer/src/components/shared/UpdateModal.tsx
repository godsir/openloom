import { useMemo, useRef, useEffect, useCallback } from 'react'
import Overlay from './Overlay'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'
import { IconCheck, IconAlertCircle, IconDownload } from '../../utils/icons'
import styles from './UpdateModal.module.css'

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function formatSpeed(bps: number): string {
  if (bps < 1024) return `${bps} B/s`
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(1)} KB/s`
  return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`
}

export default function UpdateModal() {
  const { t } = useLocale()
  const update = useStore((s) => s.update)
  const modalOpen = useStore((s) => s.updateModalOpen)
  const dismissUpdate = useStore((s) => s.dismissUpdate)
  const downloadUpdate = useStore((s) => s.downloadUpdate)
  const backgroundDownload = useStore((s) => s.backgroundDownload)
  const cancelDownload = useStore((s) => s.cancelDownload)
  const installUpdate = useStore((s) => s.installUpdate)

  const { status, version, releaseNotes, progress, bytesPerSecond, transferred, total, error } = update

  // electron-updater returns release notes as HTML from the GitHub atom feed.
  // Sanitize (strip scripts / dangerous attrs) then render as formatted HTML.
  const safeNotes = useMemo(
    () => (releaseNotes ? sanitizeHtml(releaseNotes) : ''),
    [releaseNotes],
  )

  // Open external links in the system browser instead of navigating the window.
  const notesRef = useRef<HTMLDivElement>(null)
  const handleNotesClick = useCallback((e: MouseEvent) => {
    const link = (e.target as HTMLElement).closest('a')
    if (link) {
      const href = link.getAttribute('href') || ''
      if (/^https?:\/\//i.test(href)) {
        e.preventDefault()
        window.loom.openExternal(href)
      }
    }
  }, [])
  useEffect(() => {
    const el = notesRef.current
    if (!el) return
    el.addEventListener('click', handleNotesClick)
    return () => el.removeEventListener('click', handleNotesClick)
  }, [handleNotesClick, safeNotes])

  const show = modalOpen && (status === 'available' || status === 'downloading' || status === 'downloaded' || status === 'error')

  const handleClose = () => {
    // 下载中：关弹窗但保留下载状态（后台继续，灵动岛仍显示进度）
    if (status === 'downloading') {
      useStore.getState().closeUpdateModal()
      return
    }
    dismissUpdate()
  }

  return (
    <Overlay open={show} onClose={handleClose} title={t('updates.title')} size="md">
      <div className={styles.container}>
        {status === 'available' && (
          <>
            <div className={styles.versionHeader}>
              <IconDownload size={20} className={styles.headerIcon} />
              {t('updates.found', { version: version || '' })}
            </div>
            <div className={styles.versionSub}>{t('updates.recommend')}</div>
            {safeNotes && (
              <div
                ref={notesRef}
                className={styles.releaseNotes}
                dangerouslySetInnerHTML={{ __html: safeNotes }}
              />
            )}
            <div className={styles.actions}>
              <button className={styles.dismissBtn} onClick={dismissUpdate}>{t('updates.later')}</button>
              <button className={styles.secondaryBtn} onClick={backgroundDownload}>{t('updates.backgroundDownload')}</button>
              <button className={styles.primaryBtn} onClick={downloadUpdate}>{t('updates.download')}</button>
            </div>
          </>
        )}

        {status === 'downloading' && (
          <>
            <div className={styles.versionHeader}>
              <IconDownload size={20} className={styles.headerIcon} />
              {t('updates.downloading', { version: version || '' })}
            </div>
            <div className={styles.progressSection}>
              <div className={styles.progressPercent}>{progress.toFixed(0)}%</div>
              <div className={styles.progressBarOuter}>
                <div className={styles.progressBarInner} style={{ width: `${progress}%` }} />
              </div>
              <div className={styles.progressStats}>
                <span>{formatBytes(transferred)} / {formatBytes(total)}</span>
                <span>{formatSpeed(bytesPerSecond)}</span>
              </div>
            </div>
            <div className={styles.actions}>
              <button className={styles.dismissBtn} onClick={cancelDownload}>{t('updates.cancelDownload')}</button>
            </div>
          </>
        )}

        {status === 'downloaded' && (
          <>
            <div className={styles.versionHeader}>
              <IconCheck size={20} className={styles.headerIconSuccess} />
              {t('updates.downloadComplete', { version: version || '' })}
            </div>
            <div className={styles.downloadedInfo}>
              {t('updates.readyToInstall')}
            </div>
            <div className={styles.actions}>
              <button className={styles.dismissBtn} onClick={dismissUpdate}>{t('updates.restartLater')}</button>
              <button className={styles.primaryBtn} onClick={installUpdate}>{t('updates.restartNow')}</button>
            </div>
          </>
        )}

        {status === 'error' && (
          <>
            <div className={styles.versionHeader}>
              <IconAlertCircle size={20} className={styles.headerIconError} />
              {t('updates.failed')}
            </div>
            <div className={styles.errorMessage}>{error}</div>
            <div className={styles.actions}>
              <button className={styles.dismissBtn} onClick={dismissUpdate}>{t('common.close')}</button>
            </div>
          </>
        )}
      </div>
    </Overlay>
  )
}

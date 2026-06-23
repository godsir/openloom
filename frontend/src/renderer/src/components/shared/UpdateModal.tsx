import Overlay from './Overlay'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
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
  const installUpdate = useStore((s) => s.installUpdate)

  const { status, version, releaseNotes, progress, bytesPerSecond, transferred, total, error } = update

  // electron-updater returns release notes as HTML from the GitHub API.
  // Strip tags and decode entities for a clean plain-text display.
  const cleanNotes = releaseNotes
    ? releaseNotes
        .replace(/<br\s*\/?>/gi, '\n')
        .replace(/<[^>]*>/g, '')
        .replace(/&amp;/g, '&')
        .replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>')
        .replace(/&quot;/g, '"')
        .replace(/&#(\d+);/g, (_, n) => String.fromCharCode(Number(n)))
        .replace(/\n{3,}/g, '\n\n')
        .trim()
    : null

  const show = modalOpen && (status === 'available' || status === 'downloading' || status === 'downloaded' || status === 'error')

  const handleClose = () => {
    if (status === 'downloading') return
    dismissUpdate()
  }

  return (
    <Overlay open={show} onClose={handleClose} title={t('updates.title')} size="md">
      <div className={styles.container}>
        {status === 'available' && (
          <>
            <div className={styles.versionHeader}>
              {t('updates.found', { version: version || '' })}
            </div>
            <div className={styles.versionSub}>{t('updates.recommend')}</div>
            {cleanNotes && (
              <div className={styles.releaseNotes}>{cleanNotes}</div>
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
          </>
        )}

        {status === 'downloaded' && (
          <>
            <div className={styles.versionHeader}>
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
            <div className={styles.versionHeader}>{t('updates.failed')}</div>
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

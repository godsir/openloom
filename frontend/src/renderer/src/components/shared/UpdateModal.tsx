import Overlay from './Overlay'
import { useStore } from '../../stores'
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
  const update = useStore((s) => s.update)
  const modalOpen = useStore((s) => s.updateModalOpen)
  const dismissUpdate = useStore((s) => s.dismissUpdate)
  const downloadUpdate = useStore((s) => s.downloadUpdate)
  const backgroundDownload = useStore((s) => s.backgroundDownload)
  const installUpdate = useStore((s) => s.installUpdate)

  const { status, version, releaseNotes, progress, bytesPerSecond, transferred, total, error } = update

  const show = modalOpen && (status === 'available' || status === 'downloading' || status === 'downloaded' || status === 'error')

  const handleClose = () => {
    if (status === 'downloading') return
    dismissUpdate()
  }

  return (
    <Overlay open={show} onClose={handleClose} title="更新" size="md">
      <div className={styles.container}>
        {status === 'available' && (
          <>
            <div className={styles.versionHeader}>
              发现新版本 {version}
            </div>
            <div className={styles.versionSub}>建议更新到最新版本以获得新功能与安全修复</div>
            {releaseNotes && (
              <div className={styles.releaseNotes}>{releaseNotes}</div>
            )}
            <div className={styles.actions}>
              <button className={styles.dismissBtn} onClick={dismissUpdate}>稍后再说</button>
              <button className={styles.secondaryBtn} onClick={backgroundDownload}>后台下载</button>
              <button className={styles.primaryBtn} onClick={downloadUpdate}>下载更新</button>
            </div>
          </>
        )}

        {status === 'downloading' && (
          <>
            <div className={styles.versionHeader}>
              正在下载 {version}
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
              {version} 下载完成
            </div>
            <div className={styles.downloadedInfo}>
              更新已准备就绪，重启应用即可生效。
            </div>
            <div className={styles.actions}>
              <button className={styles.dismissBtn} onClick={dismissUpdate}>稍后重启</button>
              <button className={styles.primaryBtn} onClick={installUpdate}>立即重启</button>
            </div>
          </>
        )}

        {status === 'error' && (
          <>
            <div className={styles.versionHeader}>更新失败</div>
            <div className={styles.errorMessage}>{error}</div>
            <div className={styles.actions}>
              <button className={styles.dismissBtn} onClick={dismissUpdate}>关闭</button>
            </div>
          </>
        )}
      </div>
    </Overlay>
  )
}

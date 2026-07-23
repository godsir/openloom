import { IconWinMin, IconWinMax, IconWinClose } from '../../utils/icons'
import { useLocale } from '../../i18n'
import styles from './WindowControls.module.css'

interface WindowControlsProps {
  onMinimize?: () => void
  onMaximize?: () => void
  onClose?: () => void
}

export default function WindowControls({
  onMinimize = () => window.loom.windowMinimize(),
  onMaximize = () => window.loom.windowMaximize(),
  onClose = () => window.loom.windowClose(),
}: WindowControlsProps = {}) {
  const { t } = useLocale()
  return (
    <div className={styles.controls}>
      <button onClick={onMinimize} className={styles.btn} aria-label={t('app.minimize')}>
        <IconWinMin size={14} />
      </button>
      <button onClick={onMaximize} className={styles.btn} aria-label={t('app.maximize')}>
        <IconWinMax size={14} />
      </button>
      <button onClick={onClose} className={styles.closeBtn} aria-label={t('common.close')}>
        <IconWinClose size={14} />
      </button>
    </div>
  )
}

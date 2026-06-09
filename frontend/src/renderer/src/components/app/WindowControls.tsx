import { IconWinMin, IconWinMax, IconWinClose } from '../../utils/icons'
import { useLocale } from '../../i18n'
import styles from './WindowControls.module.css'

export default function WindowControls() {
  const { t } = useLocale()
  return (
    <div className={styles.controls}>
      <button onClick={() => window.loom.windowMinimize()} className={styles.btn} aria-label={t('app.minimize')}>
        <IconWinMin size={14} />
      </button>
      <button onClick={() => window.loom.windowMaximize()} className={styles.btn} aria-label={t('app.maximize')}>
        <IconWinMax size={14} />
      </button>
      <button onClick={() => window.loom.windowClose()} className={styles.closeBtn} aria-label={t('common.close')}>
        <IconWinClose size={14} />
      </button>
    </div>
  )
}

import { IconWinMin, IconWinMax, IconWinClose } from '../../utils/icons'
import styles from './WindowControls.module.css'

export default function WindowControls() {
  return (
    <div className={styles.controls}>
      <button onClick={() => window.loom.windowMinimize()} className={styles.btn} aria-label="最小化">
        <IconWinMin size={14} />
      </button>
      <button onClick={() => window.loom.windowMaximize()} className={styles.btn} aria-label="最大化">
        <IconWinMax size={14} />
      </button>
      <button onClick={() => window.loom.windowClose()} className={styles.closeBtn} aria-label="关闭">
        <IconWinClose size={14} />
      </button>
    </div>
  )
}

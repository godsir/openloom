import TokenUsagePanel from '../shared/TokenUsagePanel'
import styles from '../shared/SettingsModal.module.css'

export default function TokenTab() {
  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>Token 用量</h3>
        <p className={styles.sectionDesc}>查看 Token 消耗统计和历史趋势</p>
      </div>
      <div className={styles.contentBody}>
        <TokenUsagePanel />
      </div>
    </>
  )
}

import { useLocale } from '../../i18n'
import TokenUsagePanel from '../shared/TokenUsagePanel'
import styles from '../shared/SettingsModal.module.css'

export default function TokenTab() {
  const { t } = useLocale()
  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>{t('token.title')}</h3>
        <p className={styles.sectionDesc}>{t('token.description')}</p>
      </div>
      <div className={styles.contentBody}>
        <TokenUsagePanel />
      </div>
    </>
  )
}

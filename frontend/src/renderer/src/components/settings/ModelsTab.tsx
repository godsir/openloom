import { useLocale } from '../../i18n'
import ModelConfigPanel from '../shared/ModelConfigPanel'
import VisionConfigSection from '../shared/VisionConfigSection'
import AuxiliaryConfigSection from '../shared/AuxiliaryConfigSection'
import FimConfigSection from '../shared/FimConfigSection'
import styles from '../shared/SettingsModal.module.css'

export default function ModelsTab() {
  const { t } = useLocale()
  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>{t('settings.models')}</h3>
        <p className={styles.sectionDesc}>{t('models.description')}</p>
      </div>
      <div className={styles.contentBody}>
        <ModelConfigPanel />
        <VisionConfigSection />
        <AuxiliaryConfigSection />
        <FimConfigSection />
      </div>
    </>
  )
}

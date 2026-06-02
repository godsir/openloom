import ModelConfigPanel from '../shared/ModelConfigPanel'
import VisionConfigSection from '../shared/VisionConfigSection'
import AuxiliaryConfigSection from '../shared/AuxiliaryConfigSection'
import styles from '../shared/SettingsModal.module.css'

export default function ModelsTab() {
  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>模型</h3>
        <p className={styles.sectionDesc}>配置推理模型和 API 密钥</p>
      </div>
      <div className={styles.contentBody}>
        <ModelConfigPanel />
        <VisionConfigSection />
        <AuxiliaryConfigSection />
      </div>
    </>
  )
}

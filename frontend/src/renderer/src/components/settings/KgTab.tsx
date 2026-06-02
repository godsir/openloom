import KnowledgeGraphPanel from '../kg/KnowledgeGraphPanel'
import styles from '../shared/SettingsModal.module.css'

export default function KgTab() {
  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>记忆系统</h3>
        <p className={styles.sectionDesc}>浏览和管理 AI 的知识图谱与认知记录</p>
      </div>
      <div className={styles.contentBody}>
        <KnowledgeGraphPanel />
      </div>
    </>
  )
}

import { useState } from 'react'
import KnowledgeGraphTab from './KnowledgeGraphTab'
import MaintenanceTab from './MaintenanceTab'
import styles from './KnowledgeGraphPanel.module.css'

export default function KnowledgeGraphPanel() {
  const [activeTab, setActiveTab] = useState<'kg' | 'maintenance'>('kg')

  return (
    <div className={styles.panel}>
      <div className={styles.mainTabs}>
        <button
          className={`${styles.mainTab} ${activeTab === 'kg' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('kg')}
        >知识图谱</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'maintenance' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('maintenance')}
        >维护</button>
      </div>
      {activeTab === 'kg' && <KnowledgeGraphTab />}
      {activeTab === 'maintenance' && <MaintenanceTab />}
    </div>
  )
}

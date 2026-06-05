import { useState } from 'react'
import { useStore } from '../../stores'
import KnowledgeGraphTab from './KnowledgeGraphTab'
import MaintenanceTab from './MaintenanceTab'
import MemoryHealthPanel from './MemoryHealthPanel'
import { PersonaPanelConnected } from './PersonaPanel'
import PatternPanel from './PatternPanel'
import VectorSearchPanel from './VectorSearchPanel'
import styles from './KnowledgeGraphPanel.module.css'

export default function KnowledgeGraphPanel() {
  const [activeTab, setActiveTab] = useState<'kg' | 'vector' | 'persona' | 'patterns' | 'health' | 'maintenance'>('kg')

  // ── Pattern data (connected wrapper inline) ──
  const patternReport = useStore(s => s.patternReport)
  const kgLoadPatterns = useStore(s => s.kgLoadPatterns)

  return (
    <div className={styles.panel}>
      <div className={styles.mainTabs}>
        <button
          className={`${styles.mainTab} ${activeTab === 'kg' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('kg')}
        >知识图谱</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'vector' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('vector')}
        >语义</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'persona' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('persona')}
        >画像</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'patterns' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('patterns')}
        >模式</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'health' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('health')}
        >健康</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'maintenance' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('maintenance')}
        >维护</button>
      </div>
      {activeTab === 'kg' && <KnowledgeGraphTab />}
      {activeTab === 'vector' && (
        <VectorSearchPanel
          onEntitySelected={() => setActiveTab('kg')}
        />
      )}
      {activeTab === 'persona' && <PersonaPanelConnected />}
      {activeTab === 'patterns' && (
        <PatternPanel
          report={patternReport}
          onRefresh={kgLoadPatterns}
        />
      )}
      {activeTab === 'health' && <MemoryHealthPanel />}
      {activeTab === 'maintenance' && <MaintenanceTab />}
    </div>
  )
}

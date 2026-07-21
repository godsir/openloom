import { useState, useEffect, useRef } from 'react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import KnowledgeGraphTab, { type KgScopeFilter } from './KnowledgeGraphTab'
import MaintenanceTab from './MaintenanceTab'
import MemoryHealthPanel from './MemoryHealthPanel'
import { PersonaPanelConnected } from './PersonaPanel'
import PatternPanel from './PatternPanel'
import styles from './KnowledgeGraphPanel.module.css'

export default function KnowledgeGraphPanel() {
  const { t } = useLocale()
  const [activeTab, setActiveTab] = useState<'graph' | 'kg' | 'persona' | 'patterns' | 'health' | 'maintenance'>('graph')

  // 列表搜索态提升到面板层，切换主 tab（graph/kg/…）再切回时不丢失搜索条件（B20.1）
  const [kgQuery, setKgQuery] = useState('')
  const [kgScope, setKgScope] = useState<KgScopeFilter>('all')
  const [kgSearchActive, setKgSearchActive] = useState(false)

  // ── Pattern data (connected wrapper inline) ──
  const patternReport = useStore(s => s.patternReport)
  const kgLoadPatterns = useStore(s => s.kgLoadPatterns)

  // Auto-load patterns data when tab becomes active
  const loadedRef = useRef<Set<string>>(new Set())
  useEffect(() => {
    if (activeTab === 'patterns' && !loadedRef.current.has('patterns')) {
      loadedRef.current.add('patterns')
      kgLoadPatterns()
    }
  }, [activeTab, kgLoadPatterns])

  return (
    <div className={styles.panel}>
      <div className={styles.mainTabs}>
        <button
          className={`${styles.mainTab} ${activeTab === 'graph' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('graph')}
        >{t('kg.tab.graph')}</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'kg' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('kg')}
        >{t('kg.tab.knowledgeBase')}</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'persona' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('persona')}
        >{t('kg.tab.persona')}</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'patterns' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('patterns')}
        >{t('kg.tab.patterns')}</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'health' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('health')}
        >{t('kg.tab.health')}</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'maintenance' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('maintenance')}
        >{t('kg.tab.maintenance')}</button>
      </div>
      {activeTab === 'graph' && (
        <KnowledgeGraphTab
          initialSubTab="graph"
          query={kgQuery} setQuery={setKgQuery}
          scopeFilter={kgScope} setScopeFilter={setKgScope}
          searchActive={kgSearchActive} setSearchActive={setKgSearchActive}
        />
      )}
      {activeTab === 'kg' && (
        <KnowledgeGraphTab
          initialSubTab="list"
          onRequestGraphView={() => setActiveTab('graph')}
          query={kgQuery} setQuery={setKgQuery}
          scopeFilter={kgScope} setScopeFilter={setKgScope}
          searchActive={kgSearchActive} setSearchActive={setKgSearchActive}
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

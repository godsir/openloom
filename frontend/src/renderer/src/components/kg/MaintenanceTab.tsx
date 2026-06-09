import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import Select from '../shared/Select'
import type { Cognition, CognitionHistory } from '../../types/bindings'
import PromoteDialog from './PromoteDialog'
import mt from './MaintenanceTab.module.css'

function formatTimestamp(ts: number): string {
  const date = new Date(ts * 1000)
  return date.toISOString().split('T')[0]
}

function ScopeBadge({ scope }: { scope: string }) {
  if (scope === 'global') return null
  return <span className={mt.scopeBadge}>{scope.slice(0, 6)}</span>
}

function EvolutionTimeline({ snapshots, t }: {
  snapshots: CognitionHistory[]
  t: (key: string, vars?: Record<string, string | number>) => string
}) {
  if (snapshots.length === 0) return <div className={mt.timelineEmpty}>{t('kg.maintenance.noHistory')}</div>
  const sorted = [...snapshots].sort((a, b) => a.version - b.version)
  return (
    <div className={mt.timeline}>
      {sorted.map((snap, i) => (
        <div key={snap.id} className={mt.timelineItem}>
          <div className={mt.timelineVersion}>{t('kg.maintenance.versionN', { n: String(snap.version) })}</div>
          <div className={mt.timelineContent}>
            <div className={mt.timelineValue}>{snap.value}</div>
            <div className={mt.timelineMeta}>
              {t('kg.confidence')} {(snap.confidence * 100).toFixed(0)}% | {formatTimestamp(snap.snapshot_at)}
            </div>
          </div>
          {i < sorted.length - 1 && <div className={mt.timelineArrow}>&rarr;</div>}
        </div>
      ))}
    </div>
  )
}

function CognitionRow({ cognition, t }: {
  cognition: Cognition
  t: (key: string, vars?: Record<string, string | number>) => string
}) {
  const [expanded, setExpanded] = useState(false)
  const cognitionSnapshots = useStore(s => s.cognitionSnapshots)
  const cognitionLoadSnapshots = useStore(s => s.cognitionLoadSnapshots)
  const cognitionDelete = useStore(s => s.cognitionDelete)
  const addToast = useStore(s => s.addToast)
  const snapshots = cognitionSnapshots[cognition.id] ?? []

  const traitLabel = (() => {
    const name = cognition.trait_name
    // Direct trait mapping
    const directKey = `kg.trait.${name}`
    const direct = t(directKey)
    if (direct !== directKey) return direct
    // entity_{type}
    if (name.startsWith('entity_')) {
      const typeName = name.slice(7)
      const capType = typeName.charAt(0).toUpperCase() + typeName.slice(1)
      const typeLabel = t(`kg.entityType.${capType}`)
      const targetKey = `kg.entityType.${capType}`
      const resolvedType = typeLabel !== targetKey ? typeLabel : typeName
      return `${t('kg.trait.entityPrefix')}${resolvedType}`
    }
    // interest_{keyword}
    if (name.startsWith('interest_')) {
      const kw = name.slice(9)
      const kwKey = `kg.interest.${kw}`
      const kwLabel = t(kwKey)
      const resolvedKw = kwLabel !== kwKey ? kwLabel : kw
      return `${t('kg.trait.interestPrefix')}${resolvedKw}`
    }
    return name.replace(/_/g, ' ')
  })()

  const handleExpand = () => {
    if (!expanded && snapshots.length === 0) {
      cognitionLoadSnapshots(cognition.id)
    }
    setExpanded(!expanded)
  }

  const handleDelete = async (e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      const ok = await cognitionDelete(cognition.id)
      if (ok) {
        addToast?.({ type: 'success', message: t('kg.maintenance.cognitionDeleted', { name: cognition.trait_name }) })
      }
    } catch (err) {
      addToast?.({ type: 'error', message: t('kg.maintenance.deleteFailed', { error: String(err) }) })
    }
  }

  return (
    <div className={mt.cognitionRow}>
      <div className={mt.cognitionHeader} onClick={handleExpand}>
        <button className={mt.expandToggle}>{expanded ? '▼' : '▶'}</button>
        <span className={mt.cognitionTrait}>{traitLabel}</span>
        <span className={mt.cognitionValue}>{cognition.value}</span>
        <span className={mt.cognitionConf}>{(cognition.confidence * 100).toFixed(0)}%</span>
        <span className={mt.cognitionVersion}>
          {cognition.version > 1 ? t('kg.maintenance.updatedNTimes', { n: String(cognition.version - 1) }) : t('kg.maintenance.firstRecord')}
        </span>
        <ScopeBadge scope={cognition.scope} />
        <button className={mt.deleteBtn} onClick={handleDelete} title={t('kg.maintenance.deleteCognition')}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14"/></svg>
        </button>
      </div>
      {expanded && (
        <div className={mt.cognitionExpanded}>
          <EvolutionTimeline snapshots={snapshots} t={t} />
        </div>
      )}
    </div>
  )
}

// ── Layer colour mapping ──
const LAYER_COLORS: Record<string, string> = {
  working: '#f59e0b',
  episodic: '#3b82f6',
  semantic: '#8b5cf6',
  global: '#10b981',
}

function layerColor(layer: string): string {
  return LAYER_COLORS[layer] ?? 'var(--text-muted)'
}

const layerOrder = ['working', 'episodic', 'semantic', 'global']
const pipelineStages = ['extraction', 'consolidation', 'generalization', 'active_forgetting', 'self_evaluation']

export default function MaintenanceTab() {
  const { t } = useLocale()

  // ── Existing state ──
  const cognitionList = useStore(s => s.cognitionList)
  const cognitionPage = useStore(s => s.cognitionPage)
  const cognitionPageSize = useStore(s => s.cognitionPageSize)
  const cognitionSetPage = useStore(s => s.cognitionSetPage)
  const cognitionSubjects = useStore(s => s.cognitionSubjects)
  const cognitionListBySubject = useStore(s => s.cognitionListBySubject)
  const cognitionListSubjects = useStore(s => s.cognitionListSubjects)
  const kgStats = useStore(s => s.kgStats)
  const kgLoadStats = useStore(s => s.kgLoadStats)
  const kgPrune = useStore(s => s.kgPrune)
  const showConfirm = useStore(s => s.showConfirm)
  const currentSessionId = useStore(s => s.currentSessionId)

  // ── New state ──
  const consolidationReport = useStore(s => s.consolidationReport)
  const forgettingReport = useStore(s => s.forgettingReport)
  const pipelineStatus = useStore(s => s.pipelineStatus)
  const layerStats = useStore(s => s.layerStats)
  const kgNodeList = useStore(s => s.kgNodeList)
  const kgListNodes = useStore(s => s.kgListNodes)
  const kgRunConsolidation = useStore(s => s.kgRunConsolidation)
  const kgRunForgetting = useStore(s => s.kgRunForgetting)
  const kgLoadPipelineStatus = useStore(s => s.kgLoadPipelineStatus)
  const kgLoadLayerStats = useStore(s => s.kgLoadLayerStats)
  const kgPromoteToLayer = useStore(s => s.kgPromoteToLayer)
  const addToast = useStore(s => s.addToast)

  // ── Local state ──
  const [subject, setSubject] = useState('USER')
  const [scopeFilter, setScopeFilter] = useState('all')
  const [pruning, setPruning] = useState(false)
  const [promoteOpen, setPromoteOpen] = useState(false)

  // Forgetting params
  const [forgetImportance, setForgetImportance] = useState(0.3)
  const [forgetMaxAge, setForgetMaxAge] = useState(60)
  const [forgetting, setForgetting] = useState(false)

  // Consolidation
  const [consolidating, setConsolidating] = useState(false)

  // Layer view mode
  const [layerViewMode, setLayerViewMode] = useState<'chart' | 'entities'>('chart')

  useEffect(() => {
    cognitionListSubjects()
    kgLoadStats()
    kgLoadPipelineStatus()
    kgLoadLayerStats()
    kgListNodes()
  }, [cognitionListSubjects, kgLoadStats, kgLoadPipelineStatus, kgLoadLayerStats, kgListNodes])

  useEffect(() => {
    const effectiveScope = scopeFilter === 'all' ? undefined
      : scopeFilter === 'session' ? (currentSessionId ?? undefined)
      : scopeFilter
    cognitionListBySubject(subject, effectiveScope)
  }, [subject, scopeFilter, cognitionListBySubject, currentSessionId])

  const handlePrune = async () => {
    const ok = await showConfirm(
      t('kg.maintenance.pruneTitle'),
      t('kg.maintenance.pruneConfirm'),
      true
    )
    if (!ok) return
    setPruning(true)
    try {
      await kgPrune(30)
    } finally {
      setPruning(false)
    }
  }

  // ── Consolidation ──
  const handleConsolidation = async () => {
    setConsolidating(true)
    try {
      await kgRunConsolidation()
      addToast?.({ type: 'success', message: t('kg.maintenance.consolidationDone') })
    } catch (err) {
      console.error('Consolidation failed:', err)
      addToast?.({ type: 'error', message: t('kg.maintenance.consolidationFailed', { error: String(err) }) })
    } finally {
      setConsolidating(false)
    }
  }

  // ── Forgetting ──
  const handleForgetting = async () => {
    setForgetting(true)
    try {
      await kgRunForgetting(forgetImportance, forgetMaxAge)
      addToast?.({ type: 'success', message: t('kg.maintenance.forgettingDone') })
    } catch (err) {
      console.error('Forgetting failed:', err)
      addToast?.({ type: 'error', message: t('kg.maintenance.forgettingFailed', { error: String(err) }) })
    } finally {
      setForgetting(false)
    }
  }

  // ── Layer promote/demote ──
  const handlePromoteToLayer = async (nodeName: string, targetLayer: string) => {
    try {
      await kgPromoteToLayer(nodeName, targetLayer)
      addToast?.({ type: 'success', message: t('kg.maintenance.promotedToLayer', { name: nodeName, layer: t(`kg.layer.${targetLayer}`) }) })
    } catch (err) {
      addToast?.({ type: 'error', message: t('kg.maintenance.promoteFailed', { error: String(err) }) })
    }
  }

  // ── Compute layer distribution ──
  const totalLayerNodes = layerStats.reduce((sum, l) => sum + l.node_count, 0)

  // Group nodes by layer for entity view
  const nodesByLayer: Record<string, typeof kgNodeList> = {}
  for (const n of kgNodeList) {
    const l = n.layer || 'semantic'
    if (!nodesByLayer[l]) nodesByLayer[l] = []
    nodesByLayer[l].push(n)
  }

  // ── Pagination ──
  const totalPages = Math.max(1, Math.ceil(cognitionList.length / cognitionPageSize))
  const paginatedCognitions = cognitionList.slice(
    cognitionPage * cognitionPageSize,
    (cognitionPage + 1) * cognitionPageSize
  )

  const layerNameShort = (name: string) => t(`kg.layer.${name}.short`) ?? name.slice(0, 3)

  return (
    <div className={mt.maintenanceTab}>
      {/* ══════════════════════════════════════════════════════════════════
          Management Cards (2-column grid)
          ══════════════════════════════════════════════════════════════════ */}
      <div className={mt.mgmtGrid}>
        {/* Card: KG Maintenance */}
        <div className={mt.mgmtCard}>
          <div className={mt.mgmtCardTitle}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><circle cx="12" cy="12" r="3"/><path d="M12 2v4m0 12v4M2 12h4m12 0h4"/></svg>
            {t('kg.maintenance.graphMaintenance')}
          </div>
          <div className={mt.mgmtCardDesc}>
            {kgStats ? t('kg.maintenance.graphStats', { nodes: String(kgStats.node_count), edges: String(kgStats.edge_count) }) : t('common.loading')}
          </div>
          <div className={mt.mgmtCardActions}>
            <button className={mt.promoteBtn} onClick={() => setPromoteOpen(true)}>{t('kg.maintenance.promoteSession')}</button>
            <button className={mt.pruneBtn} onClick={handlePrune} disabled={pruning}>
              {pruning ? t('kg.maintenance.pruning') : t('kg.maintenance.pruneLowConfidence')}
            </button>
          </div>
        </div>

        {/* Card: Consolidation */}
        <div className={mt.mgmtCard}>
          <div className={mt.mgmtCardTitle}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M12 20V10m0 0l-4 4m4-4l4 4"/><path d="M4 17v1a2 2 0 002 2h12a2 2 0 002-2v-1"/></svg>
            {t('kg.maintenance.memoryConsolidation')}
          </div>
          <div className={mt.mgmtCardDesc}>{t('kg.maintenance.consolidationDesc')}</div>
          <div className={mt.mgmtCardActions}>
            <button className={mt.runBtn} onClick={handleConsolidation} disabled={consolidating}>
              {consolidating ? t('kg.maintenance.consolidating') : t('kg.maintenance.runConsolidation')}
            </button>
          </div>
          {consolidationReport && (
            <div className={mt.resultGrid}>
              <div className={mt.resultItem}><span className={mt.resultLabel}>{t('kg.maintenance.mergedNodes')}</span><span className={mt.resultValue}>{consolidationReport.merged_nodes}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>{t('kg.maintenance.mergedCognitions')}</span><span className={mt.resultValue}>{consolidationReport.merged_cognitions}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>{t('kg.maintenance.promotedEntities')}</span><span className={mt.resultValue}>{consolidationReport.promoted_count}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>{t('kg.maintenance.edgeRerouted')}</span><span className={mt.resultValue}>{consolidationReport.edge_rerouted}</span></div>
            </div>
          )}
        </div>

        {/* Card: Forgetting */}
        <div className={mt.mgmtCard}>
          <div className={mt.mgmtCardTitle}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14"/></svg>
            {t('kg.maintenance.activeForgetting')}
          </div>
          <div className={mt.mgmtCardDesc}>{t('kg.maintenance.forgettingDesc')}</div>
          <div className={mt.forgettingControls}>
            <div className={mt.forgettingParams}>
              <div className={mt.sliderGroup}>
                <span className={mt.sliderLabel}>{t('kg.maintenance.threshold')} {forgetImportance.toFixed(2)}</span>
                <input type="range" className={mt.slider} min={0} max={1} step={0.05} value={forgetImportance} onChange={e => setForgetImportance(parseFloat(e.target.value))} />
              </div>
              <div className={mt.inputGroup}>
                <span className={mt.inputLabel}>{t('kg.maintenance.maxDays')}</span>
                <input type="number" className={mt.numberInput} min={1} max={365} value={forgetMaxAge} onChange={e => setForgetMaxAge(Math.max(1, Math.min(365, parseInt(e.target.value, 10) || 60)))} />
              </div>
            </div>
            <button className={mt.runBtnDanger} onClick={handleForgetting} disabled={forgetting}>
              {forgetting ? t('kg.maintenance.executing') : t('kg.maintenance.runForgetting')}
            </button>
          </div>
          {forgettingReport && (
            <div className={mt.resultGrid}>
              <div className={mt.resultItem}><span className={mt.resultLabel}>{t('kg.maintenance.removedNodes')}</span><span className={mt.resultValue}>{forgettingReport.nodes_removed}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>{t('kg.maintenance.removedEdges')}</span><span className={mt.resultValue}>{forgettingReport.edges_removed}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>{t('kg.maintenance.removedCognitions')}</span><span className={mt.resultValue}>{forgettingReport.cognitions_removed}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>{t('kg.maintenance.protected')}</span><span className={mt.resultValue}>{forgettingReport.skipped_protected}</span></div>
            </div>
          )}
        </div>

        {/* Card: Pipeline Status */}
        <div className={mt.mgmtCard}>
          <div className={mt.mgmtCardTitle}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></svg>
            {t('kg.maintenance.pipelineStatus')}
          </div>
          <div className={mt.pipelineGrid}>
            {pipelineStages.map(stage => {
              const label = t(`kg.pipeline.${stage}`) ?? stage
              const status = pipelineStatus.length > 0 ? pipelineStatus[0] : null
              const isHealthy = status?.status === 'active'
              return (
                <div key={stage} className={mt.stageCard}>
                  <div className={mt.stageHeader}>
                    <span className={`${mt.stageDot} ${isHealthy ? mt.stageDotActive : mt.stageDotIdle}`} />
                    <span className={mt.stageName}>{label}</span>
                  </div>
                  <div className={mt.stageTime}>{isHealthy ? t('kg.maintenance.active') : t('kg.maintenance.idle')}</div>
                </div>
              )
            })}
          </div>
        </div>
      </div>

      {/* ══════════════════════════════════════════════════════════════════
          Layer Stats
          ══════════════════════════════════════════════════════════════════ */}
      <div className={mt.section}>
        <div className={mt.layerRefreshBar}>
          <span className={mt.sectionTitle} style={{ marginBottom: 0 }}>{t('kg.maintenance.memoryLayers')}</span>
          <div className={mt.layerModeToggle}>
            <button className={layerViewMode === 'chart' ? mt.layerModeBtnActive : mt.layerModeBtn} onClick={() => setLayerViewMode('chart')}>{t('kg.maintenance.distributionChart')}</button>
            <button className={layerViewMode === 'entities' ? mt.layerModeBtnActive : mt.layerModeBtn} onClick={() => setLayerViewMode('entities')}>{t('kg.maintenance.entityManagement')}</button>
          </div>
        </div>

        {layerViewMode === 'chart' ? (
          <div className={mt.layerBarList}>
            {layerStats.length === 0 ? (
              <div className={mt.loadingText}>{t('kg.maintenance.noLayerData')}</div>
            ) : (
              layerOrder
                .map(layerName => layerStats.find(ls => ls.layer_name === layerName))
                .filter((ls): ls is NonNullable<typeof ls> => !!ls)
                .map(ls => {
                const pct = totalLayerNodes > 0 ? Math.round((ls.node_count / totalLayerNodes) * 100) : 0
                return (
                  <div key={ls.layer_name} className={mt.layerBarItem}>
                    <span className={mt.layerBarName}>{t(`kg.layer.${ls.layer_name}`) ?? ls.layer_name}</span>
                    <div className={mt.layerBarTrack}>
                      <div className={mt.layerBarFill} style={{ width: `${pct}%`, background: layerColor(ls.layer_name) }} />
                      <span className={mt.layerBarCount}>{ls.node_count}</span>
                    </div>
                    <span className={mt.layerBarPct}>{pct}%</span>
                  </div>
                )
              })
            )}
          </div>
        ) : (
          <div>
            {layerOrder.map(layer => {
              const nodes = nodesByLayer[layer] || []
              if (nodes.length === 0) return null
              const otherLayers = layerOrder.filter(l => l !== layer)
              return (
                <div key={layer} className={mt.layerEntitySection}>
                  <div className={mt.layerEntityTitle}>{t(`kg.layer.${layer}`) ?? layer} ({nodes.length})</div>
                  <div className={mt.layerEntityList}>
                    {nodes.map(n => (
                      <div key={n.node_id || n.name} className={mt.layerEntityRow}>
                        <div className={mt.layerEntityInfo}>
                          <span className={mt.layerEntityName}>{n.name}</span>
                          <span className={mt.layerEntityType}>{n.entity_type}</span>
                        </div>
                        <div className={mt.layerEntityActions}>
                          {otherLayers.map(tl => (
                            <button key={tl} className={mt.layerActionBtn} onClick={() => handlePromoteToLayer(n.name, tl)} title={t('kg.maintenance.moveTo', { layer: t(`kg.layer.${tl}`) })}>
                              {layerNameShort(tl)}
                            </button>
                          ))}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </div>

      {/* ══════════════════════════════════════════════════════════════════
          Cognition Records
          ══════════════════════════════════════════════════════════════════ */}
      <div className={mt.sectionFill}>
        <div className={mt.sectionTitle}>{t('kg.maintenance.cognitionRecords')}</div>
        <div className={mt.filterRow}>
          <label className={mt.filterLabel}>{t('kg.maintenance.subject')}</label>
          <Select
            value={subject}
            options={cognitionSubjects.length === 0
              ? [{ value: 'USER', label: 'USER' }]
              : cognitionSubjects.map(s => ({ value: s, label: s }))
            }
            onChange={setSubject}
            variant="form"
          />
          <label className={mt.filterLabel}>{t('kg.maintenance.scope')}</label>
          <Select
            value={scopeFilter}
            options={[
              { value: 'all', label: t('kg.scope.all') },
              { value: 'global', label: t('kg.scope.global') },
              { value: 'session', label: t('kg.scope.session') },
            ]}
            onChange={setScopeFilter}
            variant="form"
          />
        </div>
        <div className={mt.cognitionList}>
          {cognitionList.length === 0 ? (
            <div className={mt.emptyState}>{t('kg.maintenance.noCognitions')}</div>
          ) : (
            paginatedCognitions.map(c => <CognitionRow key={c.id} cognition={c} t={t} />)
          )}
        </div>
        {cognitionList.length > 0 && (
          <div className={mt.pagination}>
            <span className={mt.pageInfo}>
              {t('kg.maintenance.pageInfo', { total: String(cognitionList.length), current: String(cognitionPage + 1), pages: String(totalPages) })}
            </span>
            <div className={mt.pageControls}>
              <button
                className={mt.pageBtn}
                disabled={cognitionPage === 0}
                onClick={() => cognitionSetPage(0)}
              >{t('kg.maintenance.firstPage')}</button>
              <button
                className={mt.pageBtn}
                disabled={cognitionPage === 0}
                onClick={() => cognitionSetPage(cognitionPage - 1)}
              >{t('kg.previousPage')}</button>
              <button
                className={mt.pageBtn}
                disabled={cognitionPage >= totalPages - 1}
                onClick={() => cognitionSetPage(cognitionPage + 1)}
              >{t('kg.nextPage')}</button>
              <button
                className={mt.pageBtn}
                disabled={cognitionPage >= totalPages - 1}
                onClick={() => cognitionSetPage(totalPages - 1)}
              >{t('kg.maintenance.lastPage')}</button>
            </div>
          </div>
        )}
      </div>

      <PromoteDialog open={promoteOpen} onClose={() => setPromoteOpen(false)} />
    </div>
  )
}

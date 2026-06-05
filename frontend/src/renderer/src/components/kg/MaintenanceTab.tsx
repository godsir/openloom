import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import Select from '../shared/Select'
import type { Cognition, CognitionHistory } from '../../types/bindings'
import PromoteDialog from './PromoteDialog'
import styles from './KnowledgeGraphPanel.module.css'
import mt from './MaintenanceTab.module.css'

function formatTimestamp(ts: number): string {
  const date = new Date(ts * 1000)
  return date.toISOString().split('T')[0]
}

function translateTraitName(name: string): string {
  // Direct mappings
  const direct: Record<string, string> = {
    preference: '偏好', goal: '目标', need: '需求', habit: '习惯',
    working_on: '正在做', using: '正在使用', project: '项目',
    company: '公司', team: '团队', last_topic: '最近话题',
    chat_frequency: '聊天频率', pattern_aggregator: '行为模式',
  }
  if (direct[name]) return direct[name]

  // entity_{type}
  if (name.startsWith('entity_')) {
    const typeMap: Record<string, string> = {
      person: '人物', technology: '技术', project: '项目', concept: '概念',
      tool: '工具', topic: '话题', organization: '组织',
    }
    return `关注${typeMap[name.slice(7)] ?? name.slice(7)}`
  }

  // interest_{keyword}
  if (name.startsWith('interest_')) {
    const kwMap: Record<string, string> = {
      rust: 'Rust', python: 'Python', typescript: 'TypeScript', golang: 'Go',
      ai: '人工智能', machine_learning: '机器学习', openloom: 'openLoom',
      mcp: 'MCP 协议', lsp: 'LSP 协议', agent: '智能体',
      skill: '技能', plugin: '插件',
    }
    const kw = name.slice(9)
    return `兴趣: ${kwMap[kw] ?? kw}`
  }

  // Fallback: replace underscores with spaces
  return name.replace(/_/g, ' ')
}

function ScopeBadge({ scope }: { scope: string }) {
  if (scope === 'global') return null
  return <span className={styles.scopeBadge}>{scope.slice(0, 6)}</span>
}

function EvolutionTimeline({ snapshots }: { snapshots: CognitionHistory[] }) {
  if (snapshots.length === 0) return <div className={styles.timelineEmpty}>暂无历史记录</div>
  const sorted = [...snapshots].sort((a, b) => a.version - b.version)
  return (
    <div className={styles.timeline}>
      {sorted.map((snap, i) => (
        <div key={snap.id} className={styles.timelineItem}>
          <div className={styles.timelineVersion}>第 {snap.version} 版</div>
          <div className={styles.timelineContent}>
            <div className={styles.timelineValue}>{snap.value}</div>
            <div className={styles.timelineMeta}>
              确信 {(snap.confidence * 100).toFixed(0)}% | {formatTimestamp(snap.snapshot_at)}
            </div>
          </div>
          {i < sorted.length - 1 && <div className={styles.timelineArrow}>&rarr;</div>}
        </div>
      ))}
    </div>
  )
}

function CognitionRow({ cognition }: { cognition: Cognition }) {
  const [expanded, setExpanded] = useState(false)
  const cognitionSnapshots = useStore(s => s.cognitionSnapshots)
  const cognitionLoadSnapshots = useStore(s => s.cognitionLoadSnapshots)
  const snapshots = cognitionSnapshots[cognition.id] ?? []

  const handleExpand = () => {
    if (!expanded && snapshots.length === 0) {
      cognitionLoadSnapshots(cognition.id)
    }
    setExpanded(!expanded)
  }

  return (
    <div className={styles.cognitionRow}>
      <div className={styles.cognitionHeader} onClick={handleExpand}>
        <button className={styles.expandToggle}>{expanded ? '▼' : '▶'}</button>
        <span className={styles.cognitionTrait}>{translateTraitName(cognition.trait_name)}</span>
        <span className={styles.cognitionValue}>{cognition.value}</span>
        <span className={styles.cognitionConf}>{(cognition.confidence * 100).toFixed(0)}%</span>
        <span className={styles.cognitionVersion}>
          {cognition.version > 1 ? `已更新 ${cognition.version - 1} 次` : '首次记录'}
        </span>
        <ScopeBadge scope={cognition.scope} />
      </div>
      {expanded && (
        <div className={styles.cognitionExpanded}>
          <EvolutionTimeline snapshots={snapshots} />
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

// ── Pipeline stage labels ──
const STAGE_LABELS: Record<string, string> = {
  extraction: '实体提取',
  consolidation: '记忆整合',
  generalization: '泛化归纳',
  active_forgetting: '主动遗忘',
  self_evaluation: '自我评估',
  quality_audit: '质量审计',
}

export default function MaintenanceTab() {
  // ── Existing state ──
  const cognitionList = useStore(s => s.cognitionList)
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
      '清理图谱',
      '确定清理 30 天以上低置信度实体？此操作不可撤销。',
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
      addToast?.({ type: 'success', message: '记忆整合已完成' })
    } catch (err) {
      console.error('Consolidation failed:', err)
      addToast?.({ type: 'error', message: `整合失败: ${String(err)}` })
    } finally {
      setConsolidating(false)
    }
  }

  // ── Forgetting ──
  const handleForgetting = async () => {
    setForgetting(true)
    try {
      await kgRunForgetting(forgetImportance, forgetMaxAge)
      addToast?.({ type: 'success', message: '主动遗忘已完成' })
    } catch (err) {
      console.error('Forgetting failed:', err)
      addToast?.({ type: 'error', message: `遗忘失败: ${String(err)}` })
    } finally {
      setForgetting(false)
    }
  }

  // ── Layer promote/demote ──
  const handlePromoteToLayer = async (nodeName: string, targetLayer: string) => {
    try {
      await kgPromoteToLayer(nodeName, targetLayer)
      addToast?.({ type: 'success', message: `已将 "${nodeName}" 提升到 ${targetLayer} 层` })
    } catch (err) {
      addToast?.({ type: 'error', message: `提升失败: ${String(err)}` })
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

  const layerOrder = ['working', 'episodic', 'semantic', 'global']
  const pipelineStages = ['extraction', 'consolidation', 'generalization', 'active_forgetting', 'self_evaluation']

  return (
    <div className={styles.maintenanceTab}>
      {/* ══════════════════════════════════════════════════════════════════
          Existing: KG Maintenance
          ══════════════════════════════════════════════════════════════════ */}
      <div className={styles.section}>
        <div className={styles.sectionTitle}>图谱维护</div>
        {kgStats && (
          <div className={styles.maintenanceStats}>
            当前统计: 实体 {kgStats.node_count}, 关系 {kgStats.edge_count}
          </div>
        )}
        <div className={styles.maintenanceActions}>
          <button
            className={styles.promoteBtn}
            onClick={() => setPromoteOpen(true)}
          >
            提升会话记忆为全局...
          </button>
          <button
            className={styles.pruneBtn}
            onClick={handlePrune}
            disabled={pruning}
          >
            {pruning ? '清理中...' : '清理 30 天以上低置信度实体'}
          </button>
        </div>
      </div>

      {/* ══════════════════════════════════════════════════════════════════
          NEW: Consolidation
          ══════════════════════════════════════════════════════════════════ */}
      <div className={mt.mtSection}>
        <div className={mt.mtSectionTitle}>记忆整合</div>
        <div className={mt.consolidationActions}>
          <button
            className={mt.runBtn}
            onClick={handleConsolidation}
            disabled={consolidating}
          >
            {consolidating ? '整合中...' : '执行整合'}
          </button>
        </div>

        {consolidationReport && (
          <div className={mt.resultBox}>
            <div className={mt.resultGrid}>
              <div className={mt.resultItem}>
                <span className={mt.resultLabel}>合并节点</span>
                <span className={mt.resultValue}>{consolidationReport.merged_nodes}</span>
              </div>
              <div className={mt.resultItem}>
                <span className={mt.resultLabel}>合并认知</span>
                <span className={mt.resultValue}>{consolidationReport.merged_cognitions}</span>
              </div>
              <div className={mt.resultItem}>
                <span className={mt.resultLabel}>提升实体</span>
                <span className={mt.resultValue}>{consolidationReport.promoted_count}</span>
              </div>
              <div className={mt.resultItem}>
                <span className={mt.resultLabel}>边重路由</span>
                <span className={mt.resultValue}>{consolidationReport.edge_rerouted}</span>
              </div>
            </div>
            {consolidationReport.summary && (
              <div className={mt.resultSummary}>{consolidationReport.summary}</div>
            )}
            {consolidationReport.errors && consolidationReport.errors.length > 0 && (
              <div className={mt.errorBox}>
                {consolidationReport.errors.map((e, i) => (
                  <div key={i} className={mt.errorText}>{e}</div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {/* ══════════════════════════════════════════════════════════════════
          NEW: Active Forgetting
          ══════════════════════════════════════════════════════════════════ */}
      <div className={mt.mtSection}>
        <div className={mt.mtSectionTitle}>主动遗忘</div>
        <div className={mt.forgettingControls}>
          <div className={mt.sliderGroup}>
            <span className={mt.sliderLabel}>重要性阈值</span>
            <div className={mt.sliderRow}>
              <input
                type="range"
                className={mt.slider}
                min={0}
                max={1}
                step={0.05}
                value={forgetImportance}
                onChange={e => setForgetImportance(parseFloat(e.target.value))}
              />
              <span className={mt.sliderValue}>{forgetImportance.toFixed(2)}</span>
            </div>
          </div>
          <div className={mt.inputGroup}>
            <span className={mt.inputLabel}>最大存活天数</span>
            <input
              type="number"
              className={mt.numberInput}
              min={1}
              max={365}
              value={forgetMaxAge}
              onChange={e => setForgetMaxAge(Math.max(1, Math.min(365, parseInt(e.target.value, 10) || 60)))}
            />
          </div>
          <button
            className={mt.runBtnDanger}
            onClick={handleForgetting}
            disabled={forgetting}
          >
            {forgetting ? '执行中...' : '执行遗忘'}
          </button>
        </div>

        {forgettingReport && (
          <div className={mt.resultBox}>
            <div className={mt.resultGrid}>
              <div className={mt.resultItem}>
                <span className={mt.resultLabel}>移除节点</span>
                <span className={mt.resultValue}>{forgettingReport.nodes_removed}</span>
              </div>
              <div className={mt.resultItem}>
                <span className={mt.resultLabel}>移除边</span>
                <span className={mt.resultValue}>{forgettingReport.edges_removed}</span>
              </div>
              <div className={mt.resultItem}>
                <span className={mt.resultLabel}>移除认知</span>
                <span className={mt.resultValue}>{forgettingReport.cognitions_removed}</span>
              </div>
              <div className={mt.resultItem}>
                <span className={mt.resultLabel}>受保护(跳过)</span>
                <span className={mt.resultValue}>{forgettingReport.skipped_protected}</span>
              </div>
            </div>
            {forgettingReport.summary && (
              <div className={mt.resultSummary}>{forgettingReport.summary}</div>
            )}
          </div>
        )}
      </div>

      {/* ══════════════════════════════════════════════════════════════════
          NEW: Pipeline Status
          ══════════════════════════════════════════════════════════════════ */}
      <div className={mt.mtSection}>
        <div className={mt.mtSectionTitle}>管线状态</div>

        {/* Pipeline summary */}
        {pipelineStatus.length > 0 && pipelineStatus[0] && (
          <div className={mt.pipelineSummary}>
            状态: <span>{pipelineStatus[0].status}</span>
            {' | '}
            节点: <span>{pipelineStatus[0].node_count}</span>
            {' | '}
            边: <span>{pipelineStatus[0].edge_count}</span>
            {' | '}
            认知: <span>{pipelineStatus[0].cognition_count}</span>
            {' | '}
            24h提取: <span>{pipelineStatus[0].recent_extractions_24h}</span>
            {pipelineStatus[0].last_consolidation && pipelineStatus[0].last_consolidation !== 'never' && (
              <>
                {' | '}最后整合: <span>{pipelineStatus[0].last_consolidation}</span>
              </>
            )}
          </div>
        )}

        {pipelineStatus.length === 0 && (
          <div className={mt.loadingText}>暂无管线数据</div>
        )}

        {/* Stage cards */}
        <div className={mt.pipelineGrid}>
          {pipelineStages.map(stage => {
            const label = STAGE_LABELS[stage] ?? stage
            const status = pipelineStatus.length > 0 ? pipelineStatus[0] : null
            const isHealthy = status?.status === 'active'
            const dotClass = isHealthy ? mt.stageDotActive : mt.stageDotIdle

            return (
              <div key={stage} className={mt.stageCard}>
                <div className={mt.stageHeader}>
                  <span className={`${mt.stageDot} ${dotClass}`} />
                  <span className={mt.stageName}>{label}</span>
                </div>
                <div className={mt.stageTime}>
                  管线: {isHealthy ? '活跃' : '空闲'}
                </div>
                <button
                  className={mt.stageTrigger}
                  onClick={() => {
                    if (stage === 'consolidation') handleConsolidation()
                    else if (stage === 'active_forgetting') handleForgetting()
                    else kgLoadPipelineStatus()
                  }}
                >
                  手动触发
                </button>
              </div>
            )
          })}
        </div>
      </div>

      {/* ══════════════════════════════════════════════════════════════════
          NEW: Layer Stats
          ══════════════════════════════════════════════════════════════════ */}
      <div className={mt.mtSection}>
        <div className={mt.layerRefreshBar}>
          <span className={mt.mtSectionTitle} style={{ marginBottom: 0 }}>记忆分层</span>
          <div className={mt.layerModeToggle}>
            <button
              className={layerViewMode === 'chart' ? mt.layerModeBtnActive : mt.layerModeBtn}
              onClick={() => setLayerViewMode('chart')}
            >
              分布图
            </button>
            <button
              className={layerViewMode === 'entities' ? mt.layerModeBtnActive : mt.layerModeBtn}
              onClick={() => setLayerViewMode('entities')}
            >
              实体管理
            </button>
          </div>
        </div>

        {layerViewMode === 'chart' ? (
          /* ── Distribution chart, sorted by layerOrder ── */
          <div className={mt.layerBarList}>
            {layerStats.length === 0 ? (
              <div className={mt.loadingText}>暂无分层数据</div>
            ) : (
              layerOrder
                .map(layerName => layerStats.find(ls => ls.layer_name === layerName))
                .filter((ls): ls is NonNullable<typeof ls> => !!ls)
                .map(ls => {
                const pct = totalLayerNodes > 0 ? Math.round((ls.node_count / totalLayerNodes) * 100) : 0
                return (
                  <div key={ls.layer_name} className={mt.layerBarItem}>
                    <span className={mt.layerBarName}>{ls.layer_name}</span>
                    <div className={mt.layerBarTrack}>
                      <div
                        className={mt.layerBarFill}
                        style={{
                          width: `${pct}%`,
                          background: layerColor(ls.layer_name),
                        }}
                      />
                      <span className={mt.layerBarCount}>{ls.node_count}</span>
                    </div>
                    <span className={mt.layerBarPct}>{pct}%</span>
                  </div>
                )
              })
            )}
          </div>
        ) : (
          /* ── Per-entity management ── */
          <div>
            {layerOrder.map(layer => {
              const nodes = nodesByLayer[layer] || []
              if (nodes.length === 0) return null
              const otherLayers = layerOrder.filter(l => l !== layer)
              return (
                <div key={layer} className={mt.layerEntitySection}>
                  <div className={mt.layerEntityTitle}>
                    {layer.charAt(0).toUpperCase() + layer.slice(1)} ({nodes.length})
                  </div>
                  <div className={mt.layerEntityList}>
                    {nodes.map(n => (
                      <div key={n.node_id || n.name} className={mt.layerEntityRow}>
                        <div className={mt.layerEntityInfo}>
                          <span className={mt.layerEntityName}>{n.name}</span>
                          <span className={mt.layerEntityType}>{n.entity_type}</span>
                          <span className={mt.layerEntityCurr}>{n.layer || 'semantic'}</span>
                        </div>
                        <div className={mt.layerEntityActions}>
                          {otherLayers.map(tl => (
                            <button
                              key={tl}
                              className={mt.layerActionBtn}
                              onClick={() => handlePromoteToLayer(n.name, tl)}
                              title={`移至 ${tl}`}
                            >
                              {tl.slice(0, 3)}
                            </button>
                          ))}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )
            })}
            {Object.keys(nodesByLayer).length === 0 && (
              <div className={mt.loadingText}>暂无实体数据。请先加载实体列表。</div>
            )}
          </div>
        )}
      </div>

      {/* ══════════════════════════════════════════════════════════════════
          Existing: Cognition Records
          ══════════════════════════════════════════════════════════════════ */}
      <div className={styles.section}>
        <div className={styles.sectionTitle}>认知记录</div>
        <div className={styles.filterRow}>
          <label className={styles.filterLabel}>主体:</label>
          <Select
            value={subject}
            options={cognitionSubjects.length === 0
              ? [{ value: 'USER', label: 'USER' }]
              : cognitionSubjects.map(s => ({ value: s, label: s }))
            }
            onChange={setSubject}
            variant="form"
          />
          <label className={styles.filterLabel}>范围:</label>
          <Select
            value={scopeFilter}
            options={[
              { value: 'all', label: '全部' },
              { value: 'global', label: '全局' },
              { value: 'session', label: '会话级' },
            ]}
            onChange={setScopeFilter}
            variant="form"
          />
        </div>
        <div className={styles.cognitionList}>
          {cognitionList.length === 0 ? (
            <div className={styles.emptyState}>暂无认知记录</div>
          ) : (
            cognitionList.map(c => <CognitionRow key={c.id} cognition={c} />)
          )}
        </div>
      </div>

      <PromoteDialog open={promoteOpen} onClose={() => setPromoteOpen(false)} />
    </div>
  )
}

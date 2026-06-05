import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import Select from '../shared/Select'
import type { Cognition, CognitionHistory } from '../../types/bindings'
import PromoteDialog from './PromoteDialog'
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
  return <span className={mt.scopeBadge}>{scope.slice(0, 6)}</span>
}

function EvolutionTimeline({ snapshots }: { snapshots: CognitionHistory[] }) {
  if (snapshots.length === 0) return <div className={mt.timelineEmpty}>暂无历史记录</div>
  const sorted = [...snapshots].sort((a, b) => a.version - b.version)
  return (
    <div className={mt.timeline}>
      {sorted.map((snap, i) => (
        <div key={snap.id} className={mt.timelineItem}>
          <div className={mt.timelineVersion}>第 {snap.version} 版</div>
          <div className={mt.timelineContent}>
            <div className={mt.timelineValue}>{snap.value}</div>
            <div className={mt.timelineMeta}>
              确信 {(snap.confidence * 100).toFixed(0)}% | {formatTimestamp(snap.snapshot_at)}
            </div>
          </div>
          {i < sorted.length - 1 && <div className={mt.timelineArrow}>&rarr;</div>}
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
    <div className={mt.cognitionRow}>
      <div className={mt.cognitionHeader} onClick={handleExpand}>
        <button className={mt.expandToggle}>{expanded ? '▼' : '▶'}</button>
        <span className={mt.cognitionTrait}>{translateTraitName(cognition.trait_name)}</span>
        <span className={mt.cognitionValue}>{cognition.value}</span>
        <span className={mt.cognitionConf}>{(cognition.confidence * 100).toFixed(0)}%</span>
        <span className={mt.cognitionVersion}>
          {cognition.version > 1 ? `已更新 ${cognition.version - 1} 次` : '首次记录'}
        </span>
        <ScopeBadge scope={cognition.scope} />
      </div>
      {expanded && (
        <div className={mt.cognitionExpanded}>
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

function translateLayerName(name: string): string {
  const map: Record<string, string> = {
    working: '工作记忆',
    episodic: '情景记忆',
    semantic: '语义记忆',
    global: '全局记忆',
  }
  return map[name] ?? name
}

function translateLayerNameShort(name: string): string {
  const map: Record<string, string> = {
    working: '工作',
    episodic: '情景',
    semantic: '语义',
    global: '全局',
  }
  return map[name] ?? name.slice(0, 3)
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

  // ── Pagination ──
  const totalPages = Math.max(1, Math.ceil(cognitionList.length / cognitionPageSize))
  const paginatedCognitions = cognitionList.slice(
    cognitionPage * cognitionPageSize,
    (cognitionPage + 1) * cognitionPageSize
  )

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
            图谱维护
          </div>
          <div className={mt.mgmtCardDesc}>
            {kgStats ? `实体 ${kgStats.node_count}，关系 ${kgStats.edge_count}` : '加载中...'}
          </div>
          <div className={mt.mgmtCardActions}>
            <button className={mt.promoteBtn} onClick={() => setPromoteOpen(true)}>提升会话记忆为全局...</button>
            <button className={mt.pruneBtn} onClick={handlePrune} disabled={pruning}>
              {pruning ? '清理中...' : '清理低置信度实体'}
            </button>
          </div>
        </div>

        {/* Card: Consolidation */}
        <div className={mt.mgmtCard}>
          <div className={mt.mgmtCardTitle}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M12 20V10m0 0l-4 4m4-4l4 4"/><path d="M4 17v1a2 2 0 002 2h12a2 2 0 002-2v-1"/></svg>
            记忆整合
          </div>
          <div className={mt.mgmtCardDesc}>去重实体与认知，合并置信度，提升高频实体层级</div>
          <div className={mt.mgmtCardActions}>
            <button className={mt.runBtn} onClick={handleConsolidation} disabled={consolidating}>
              {consolidating ? '整合中...' : '执行整合'}
            </button>
          </div>
          {consolidationReport && (
            <div className={mt.resultGrid}>
              <div className={mt.resultItem}><span className={mt.resultLabel}>合并节点</span><span className={mt.resultValue}>{consolidationReport.merged_nodes}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>合并认知</span><span className={mt.resultValue}>{consolidationReport.merged_cognitions}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>提升实体</span><span className={mt.resultValue}>{consolidationReport.promoted_count}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>边重路由</span><span className={mt.resultValue}>{consolidationReport.edge_rerouted}</span></div>
            </div>
          )}
        </div>

        {/* Card: Forgetting */}
        <div className={mt.mgmtCard}>
          <div className={mt.mgmtCardTitle}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14"/></svg>
            主动遗忘
          </div>
          <div className={mt.mgmtCardDesc}>基于重要性评分自动清理低价值陈旧记忆</div>
          <div className={mt.forgettingControls}>
            <div className={mt.forgettingParams}>
              <div className={mt.sliderGroup}>
                <span className={mt.sliderLabel}>阈值 {forgetImportance.toFixed(2)}</span>
                <input type="range" className={mt.slider} min={0} max={1} step={0.05} value={forgetImportance} onChange={e => setForgetImportance(parseFloat(e.target.value))} />
              </div>
              <div className={mt.inputGroup}>
                <span className={mt.inputLabel}>最大天数</span>
                <input type="number" className={mt.numberInput} min={1} max={365} value={forgetMaxAge} onChange={e => setForgetMaxAge(Math.max(1, Math.min(365, parseInt(e.target.value, 10) || 60)))} />
              </div>
            </div>
            <button className={mt.runBtnDanger} onClick={handleForgetting} disabled={forgetting}>
              {forgetting ? '执行中...' : '执行遗忘'}
            </button>
          </div>
          {forgettingReport && (
            <div className={mt.resultGrid}>
              <div className={mt.resultItem}><span className={mt.resultLabel}>移除节点</span><span className={mt.resultValue}>{forgettingReport.nodes_removed}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>移除边</span><span className={mt.resultValue}>{forgettingReport.edges_removed}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>移除认知</span><span className={mt.resultValue}>{forgettingReport.cognitions_removed}</span></div>
              <div className={mt.resultItem}><span className={mt.resultLabel}>受保护</span><span className={mt.resultValue}>{forgettingReport.skipped_protected}</span></div>
            </div>
          )}
        </div>

        {/* Card: Pipeline Status */}
        <div className={mt.mgmtCard}>
          <div className={mt.mgmtCardTitle}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></svg>
            管线状态
          </div>
          <div className={mt.pipelineGrid}>
            {pipelineStages.map(stage => {
              const label = STAGE_LABELS[stage] ?? stage
              const status = pipelineStatus.length > 0 ? pipelineStatus[0] : null
              const isHealthy = status?.status === 'active'
              return (
                <div key={stage} className={mt.stageCard}>
                  <div className={mt.stageHeader}>
                    <span className={`${mt.stageDot} ${isHealthy ? mt.stageDotActive : mt.stageDotIdle}`} />
                    <span className={mt.stageName}>{label}</span>
                  </div>
                  <div className={mt.stageTime}>{isHealthy ? '活跃' : '空闲'}</div>
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
          <span className={mt.sectionTitle} style={{ marginBottom: 0 }}>记忆分层</span>
          <div className={mt.layerModeToggle}>
            <button className={layerViewMode === 'chart' ? mt.layerModeBtnActive : mt.layerModeBtn} onClick={() => setLayerViewMode('chart')}>分布图</button>
            <button className={layerViewMode === 'entities' ? mt.layerModeBtnActive : mt.layerModeBtn} onClick={() => setLayerViewMode('entities')}>实体管理</button>
          </div>
        </div>

        {layerViewMode === 'chart' ? (
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
                    <span className={mt.layerBarName}>{translateLayerName(ls.layer_name)}</span>
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
                  <div className={mt.layerEntityTitle}>{translateLayerName(layer)} ({nodes.length})</div>
                  <div className={mt.layerEntityList}>
                    {nodes.map(n => (
                      <div key={n.node_id || n.name} className={mt.layerEntityRow}>
                        <div className={mt.layerEntityInfo}>
                          <span className={mt.layerEntityName}>{n.name}</span>
                          <span className={mt.layerEntityType}>{n.entity_type}</span>
                        </div>
                        <div className={mt.layerEntityActions}>
                          {otherLayers.map(tl => (
                            <button key={tl} className={mt.layerActionBtn} onClick={() => handlePromoteToLayer(n.name, tl)} title={`移至 ${translateLayerName(tl)}`}>
                              {translateLayerNameShort(tl)}
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
        <div className={mt.sectionTitle}>认知记录</div>
        <div className={mt.filterRow}>
          <label className={mt.filterLabel}>主体:</label>
          <Select
            value={subject}
            options={cognitionSubjects.length === 0
              ? [{ value: 'USER', label: 'USER' }]
              : cognitionSubjects.map(s => ({ value: s, label: s }))
            }
            onChange={setSubject}
            variant="form"
          />
          <label className={mt.filterLabel}>范围:</label>
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
        <div className={mt.cognitionList}>
          {cognitionList.length === 0 ? (
            <div className={mt.emptyState}>暂无认知记录</div>
          ) : (
            paginatedCognitions.map(c => <CognitionRow key={c.id} cognition={c} />)
          )}
        </div>
        {cognitionList.length > 0 && (
          <div className={mt.pagination}>
            <span className={mt.pageInfo}>
              共 {cognitionList.length} 条，第 {cognitionPage + 1}/{totalPages} 页
            </span>
            <div className={mt.pageControls}>
              <button
                className={mt.pageBtn}
                disabled={cognitionPage === 0}
                onClick={() => cognitionSetPage(0)}
              >首页</button>
              <button
                className={mt.pageBtn}
                disabled={cognitionPage === 0}
                onClick={() => cognitionSetPage(cognitionPage - 1)}
              >上一页</button>
              <button
                className={mt.pageBtn}
                disabled={cognitionPage >= totalPages - 1}
                onClick={() => cognitionSetPage(cognitionPage + 1)}
              >下一页</button>
              <button
                className={mt.pageBtn}
                disabled={cognitionPage >= totalPages - 1}
                onClick={() => cognitionSetPage(totalPages - 1)}
              >末页</button>
            </div>
          </div>
        )}
      </div>

      <PromoteDialog open={promoteOpen} onClose={() => setPromoteOpen(false)} />
    </div>
  )
}

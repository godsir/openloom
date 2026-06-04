import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import Select from '../shared/Select'
import type { Cognition, CognitionHistory } from '../../types/bindings'
import PromoteDialog from './PromoteDialog'
import styles from './KnowledgeGraphPanel.module.css'

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

export default function MaintenanceTab() {
  const cognitionList = useStore(s => s.cognitionList)
  const cognitionSubjects = useStore(s => s.cognitionSubjects)
  const cognitionListBySubject = useStore(s => s.cognitionListBySubject)
  const cognitionListSubjects = useStore(s => s.cognitionListSubjects)
  const kgStats = useStore(s => s.kgStats)
  const kgLoadStats = useStore(s => s.kgLoadStats)
  const kgPrune = useStore(s => s.kgPrune)
  const showConfirm = useStore(s => s.showConfirm)
  const currentSessionId = useStore(s => s.currentSessionId)

  const [subject, setSubject] = useState('USER')
  const [scopeFilter, setScopeFilter] = useState('all')
  const [pruning, setPruning] = useState(false)
  const [promoteOpen, setPromoteOpen] = useState(false)

  useEffect(() => {
    cognitionListSubjects()
    kgLoadStats()
  }, [cognitionListSubjects, kgLoadStats])

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

  return (
    <div className={styles.maintenanceTab}>
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

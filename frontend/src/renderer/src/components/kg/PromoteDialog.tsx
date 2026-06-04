import { useState, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import Select from '../shared/Select'
import type { KgNode, Cognition } from '../../types/bindings'
import styles from './PromoteDialog.module.css'

interface PromoteDialogProps {
  open: boolean
  onClose: () => void
}

const THRESHOLD_OPTIONS = [
  { value: '0', label: '0%' },
  { value: '0.3', label: '30%' },
  { value: '0.4', label: '40%' },
  { value: '0.5', label: '50%' },
  { value: '0.6', label: '60%' },
  { value: '0.7', label: '70%' },
  { value: '0.8', label: '80%' },
]

function ConfBar({ value }: { value: number }) {
  const pct = Math.round(value * 100)
  const color = pct >= 70 ? 'var(--green)' : pct >= 40 ? 'var(--amber)' : 'var(--red)'
  return (
    <div className={styles.confBar}>
      <div className={styles.confBarTrack}>
        <div className={styles.confBarFill} style={{ width: `${pct}%`, background: color }} />
      </div>
      <span className={styles.confLabel}>{pct}%</span>
    </div>
  )
}

export default function PromoteDialog({ open, onClose }: PromoteDialogProps) {
  const sessions = useStore(s => s.sessions)
  const loadSessions = useStore(s => s.loadSessions)

  const [selectedSessionId, setSelectedSessionId] = useState('')
  const [entities, setEntities] = useState<KgNode[]>([])
  const [cognitions, setCognitions] = useState<Cognition[]>([])
  const [checkedEntities, setCheckedEntities] = useState<Set<string>>(new Set())
  const [checkedCognitions, setCheckedCognitions] = useState<Set<number>>(new Set())
  const [threshold, setThreshold] = useState(0.4)
  const [loading, setLoading] = useState(false)
  const [promoting, setPromoting] = useState(false)

  useEffect(() => {
    if (open) loadSessions()
  }, [open, loadSessions])

  const sessionOptions = sessions.map(s => ({
    value: s.path,
    label: `${s.title || s.path.slice(0, 20)} (${s.messageCount ?? 0} 条消息)`,
  }))

  const loadSessionData = useCallback(async (sessionId: string) => {
    if (!sessionId) { setEntities([]); setCognitions([]); return }
    setLoading(true)
    try {
      const [entResult, cogResult] = await Promise.all([
        loomRpc<{ nodes: KgNode[] }>('kg.list', { limit: 200, scope: sessionId }),
        loomRpc<{ rows: Cognition[] }>('cognitions.list', { subject: 'USER', scope: sessionId, limit: 200, offset: 0 }),
      ])
      const ents = (entResult.nodes || []).filter(e => e.confidence >= threshold)
      const cogs = (cogResult.rows || []).filter(c => c.confidence >= threshold)
      setEntities(ents)
      setCognitions(cogs)
      setCheckedEntities(new Set(ents.map(e => e.name)))
      setCheckedCognitions(new Set(cogs.map(c => c.id)))
    } catch (err) {
      console.error('Failed to load session data:', err)
    } finally {
      setLoading(false)
    }
  }, [threshold])

  useEffect(() => {
    loadSessionData(selectedSessionId)
  }, [selectedSessionId, threshold, loadSessionData])

  const toggleEntity = (name: string) => {
    setCheckedEntities(prev => {
      const next = new Set(prev)
      next.has(name) ? next.delete(name) : next.add(name)
      return next
    })
  }

  const toggleCognition = (id: number) => {
    setCheckedCognitions(prev => {
      const next = new Set(prev)
      next.has(id) ? next.delete(id) : next.add(id)
      return next
    })
  }

  const selectAllEntities = () => setCheckedEntities(new Set(entities.map(e => e.name)))
  const deselectAllEntities = () => setCheckedEntities(new Set())
  const selectAllCognitions = () => setCheckedCognitions(new Set(cognitions.map(c => c.id)))
  const deselectAllCognitions = () => setCheckedCognitions(new Set())

  const handlePromote = async () => {
    if (!selectedSessionId) return
    setPromoting(true)
    try {
      const nodeNames = entities.filter(e => checkedEntities.has(e.name)).map(e => e.name)
      const cognitionIds = cognitions.filter(c => checkedCognitions.has(c.id)).map(c => c.id)
      const result = await loomRpc<{ promoted_nodes: number; promoted_cognitions: number }>(
        'memory.promote',
        {
          session_id: selectedSessionId,
          min_confidence: 0.0,
          node_names: nodeNames,
          cognition_ids: cognitionIds,
        },
      )
      await loadSessionData(selectedSessionId)
      if (result.promoted_nodes > 0 || result.promoted_cognitions > 0) {
        const store = useStore.getState() as any
        store.addToast?.({
          type: 'success',
          message: `已提升 ${result.promoted_nodes} 个实体、${result.promoted_cognitions} 条认知为全局记忆`,
        })
      }
    } catch (err) {
      console.error('Promote failed:', err)
      const store = useStore.getState() as any
      store.addToast?.({ type: 'error', message: '提升失败，请重试' })
    } finally {
      setPromoting(false)
    }
  }

  if (!open) return null

  const entityCount = checkedEntities.size
  const cogCount = checkedCognitions.size

  return (
    <div className={styles.overlay}>
      <div className={styles.backdrop} onClick={onClose} />
      <div className={styles.dialog}>
        <div className={styles.header}>
          <h2 className={styles.title}>提升会话记忆为全局</h2>
          <button className={styles.closeBtn} onClick={onClose}>&times;</button>
        </div>

        <div className={styles.toolbar}>
          <div className={styles.toolbarField}>
            <label className={styles.label}>选择会话</label>
            <Select
              value={selectedSessionId}
              options={sessionOptions}
              onChange={setSelectedSessionId}
              placeholder="-- 选择要提升的会话 --"
              variant="form"
            />
          </div>
          <div className={styles.toolbarField}>
            <label className={styles.label}>置信度阈值</label>
            <Select
              value={String(threshold)}
              options={THRESHOLD_OPTIONS}
              onChange={v => setThreshold(parseFloat(v))}
              variant="form"
            />
          </div>
        </div>

        <div className={styles.content}>
          {!selectedSessionId ? (
            <div className={styles.emptyState}>请先选择一个会话</div>
          ) : loading ? (
            <div className={styles.emptyState}>加载中...</div>
          ) : (
            <>
              <div className={styles.sectionHeader}>
                <span className={styles.sectionTitle}>实体 ({entities.length})</span>
                <div className={styles.sectionActions}>
                  <button className={styles.linkBtn} onClick={selectAllEntities}>全选</button>
                  <button className={styles.linkBtn} onClick={deselectAllEntities}>取消</button>
                </div>
              </div>
              {entities.length === 0 ? (
                <div className={styles.emptyHint}>暂无符合条件的实体</div>
              ) : (
                <div className={styles.list}>
                  {entities.map(e => (
                    <label key={e.name} className={styles.item}>
                      <input
                        type="checkbox"
                        checked={checkedEntities.has(e.name)}
                        onChange={() => toggleEntity(e.name)}
                        className={styles.checkbox}
                      />
                      <div className={styles.itemBody}>
                        <div className={styles.itemName}>
                          {e.name}
                          <span className={styles.itemType}>{e.entity_type}</span>
                        </div>
                        {e.description && <div className={styles.itemDesc}>{e.description}</div>}
                      </div>
                      <ConfBar value={e.confidence} />
                    </label>
                  ))}
                </div>
              )}

              <div className={styles.sectionHeader}>
                <span className={styles.sectionTitle}>认知记录 ({cognitions.length})</span>
                <div className={styles.sectionActions}>
                  <button className={styles.linkBtn} onClick={selectAllCognitions}>全选</button>
                  <button className={styles.linkBtn} onClick={deselectAllCognitions}>取消</button>
                </div>
              </div>
              {cognitions.length === 0 ? (
                <div className={styles.emptyHint}>暂无符合条件的认知记录</div>
              ) : (
                <div className={styles.list}>
                  {cognitions.map(c => (
                    <label key={c.id} className={styles.item}>
                      <input
                        type="checkbox"
                        checked={checkedCognitions.has(c.id)}
                        onChange={() => toggleCognition(c.id)}
                        className={styles.checkbox}
                      />
                      <div className={styles.itemBody}>
                        <div className={styles.itemName}>
                          {c.trait_name.replace(/_/g, ' ')}
                          <span className={styles.itemValue}>{c.value}</span>
                        </div>
                      </div>
                      <ConfBar value={c.confidence} />
                    </label>
                  ))}
                </div>
              )}
            </>
          )}
        </div>

        <div className={styles.footer}>
          <div className={styles.footerStats}>
            已选 {entityCount} 个实体 + {cogCount} 条认知
          </div>
          <div className={styles.footerActions}>
            <button className={styles.cancelBtn} onClick={onClose}>取消</button>
            <button
              className={styles.promoteBtn}
              onClick={handlePromote}
              disabled={promoting || !selectedSessionId || (entityCount === 0 && cogCount === 0)}
            >
              {promoting ? '提升中...' : `提升为全局记忆 (${entityCount + cogCount})`}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

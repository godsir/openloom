import { useEffect, useMemo } from 'react'
import { BarChart3 } from 'lucide-react'
import { onWsConnected } from '../../services/websocket'
import { useStore } from '../../stores'
import styles from './TokenUsagePanel.module.css'

const LOCAL_BACKENDS = new Set(['LmStudio', 'Ollama'])

const BACKEND_LABELS: Record<string, string> = {
  Anthropic: 'Anthropic',
  OpenAI: 'OpenAI',
  DeepSeek: 'DeepSeek',
  LmStudio: 'LM Studio',
  Ollama: 'Ollama',
  Custom: '自定义',
}

function getProviderLabel(backend: string, backendLabel?: string): string {
  if (backend === 'Custom' && backendLabel) return backendLabel
  return BACKEND_LABELS[backend] ?? backend
}

function isLocalModel(backend: string): boolean {
  return LOCAL_BACKENDS.has(backend)
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return n.toLocaleString()
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(2) + 'K'
  return n.toLocaleString()
}

function formatCost(n: number): string {
  if (n <= 0) return '¥0'
  if (n < 0.01) return '<¥0.01'
  return '¥' + n.toFixed(2)
}

const RANK_COLORS = ['#fbbf24', '#94a3b8', '#cd7f32'] // gold, silver, bronze

export default function TokenUsagePanel() {
  const sessionTotal = useStore((s) => s.sessionTotal)
  const summary = useStore((s) => s.summary)
  const loading = useStore((s) => s.loading)
  const timeRange = useStore((s) => s.timeRange)
  const setTimeRange = useStore((s) => s.setTimeRange)
  const models = useStore((s) => s.models)

  // Build a lookup: model name → { backend, backend_label }
  const modelLookup = useMemo(() => {
    const map = new Map<string, { backend: string; backendLabel?: string }>()
    for (const m of models) {
      map.set(m.name.trim(), { backend: m.backend, backendLabel: m.backend_label })
      if (m.model) map.set(m.model.trim(), { backend: m.backend, backendLabel: m.backend_label })
    }
    return map
  }, [models])

  useEffect(() => {
    setTimeRange('all')
    onWsConnected(() => {
      setTimeRange('all')
    })
  }, [])

  const rankedModels = useMemo(() => {
    if (!summary?.by_model) return []
    return [...summary.by_model]
      .map((m) => ({
        ...m,
        total: m.prompt + m.completion,
      }))
      .sort((a, b) => b.total - a.total)
  }, [summary])

  const grandTotal = useMemo(() => {
    if (!summary) return 0
    return (summary.total_prompt_tokens || 0) + (summary.total_completion_tokens || 0)
  }, [summary])

  const totalCost = summary?.total_cost ?? 0
  const maxModelTotal = rankedModels.length > 0 ? rankedModels[0].total : 1

  const hasData = (summary && summary.total_requests > 0) || sessionTotal.requests > 0

  return (
    <div className={styles.panel}>
      {/* Big totals */}
      <div className={styles.totalHero}>
        <div className={styles.heroNumbers}>
          <div className={styles.heroMain}>
            <div className={styles.totalHeroValue}>{formatTokens(grandTotal)}</div>
            <div className={styles.totalHeroLabel}>总 Token 消耗</div>
          </div>
          {totalCost > 0 && (
            <div className={styles.heroMain}>
              <div className={`${styles.totalHeroValue} ${styles.costValue}`}>{formatCost(totalCost)}</div>
              <div className={styles.totalHeroLabel}>预估费用</div>
            </div>
          )}
        </div>
        <div className={styles.totalHeroBreakdown}>
          <span className={styles.totalHeroBreakdownItem}>
            <span className={styles.totalHeroDot} style={{ background: '#22d3ee' }} />
            Prompt {formatNumber(summary?.total_prompt_tokens || 0)}
          </span>
          <span className={styles.totalHeroBreakdownItem}>
            <span className={styles.totalHeroDot} style={{ background: '#a78bfa' }} />
            Completion {formatNumber(summary?.total_completion_tokens || 0)}
          </span>
          <span className={styles.totalHeroBreakdownItem}>
            <span className={styles.totalHeroDot} style={{ background: '#34d399' }} />
            Cached {formatNumber(summary?.total_cached_tokens || 0)}
          </span>
        </div>
      </div>

      {/* Session real-time badge */}
      {sessionTotal.requests > 0 && (
        <div className={styles.sessionBadge}>
          本次会话已消耗：<strong>{formatNumber(sessionTotal.prompt + sessionTotal.completion)}</strong> tokens ({sessionTotal.requests} 请求)
        </div>
      )}

      {/* Time range selector */}
      <div className={styles.timeRangeRow}>
        <span className={styles.dataPointInfo}>
          {loading ? '加载中...' : hasData ? `${rankedModels.length} 个模型 / ${summary?.total_requests || 0} 次请求` : ''}
        </span>
        <div className={styles.timeRangeToggle}>
          {(['all', '7d', '30d'] as const).map((r) => (
            <button
              key={r}
              className={`${styles.timeRangeBtn} ${timeRange === r ? styles.timeRangeBtnActive : ''}`}
              onClick={() => setTimeRange(r)}
            >
              {r === 'all' ? '全部' : r === '7d' ? '近7天' : '近30天'}
            </button>
          ))}
          {hasData && (
            <button
              className={styles.resetBtn}
              onClick={async () => {
                const ok = await useStore.getState().showConfirm('重置用量', '确定要清除所有 Token 用量记录吗？此操作不可撤销。', true)
                if (ok) useStore.getState().resetTokenUsage()
              }}
              title="清除所有记录"
            >
              重置
            </button>
          )}
        </div>
      </div>

      {!hasData && !loading ? (
        <div className={styles.emptyState}>
          <div className={styles.emptyIcon}><BarChart3 size={32} /></div>
          <h4 className={styles.emptyTitle}>暂无数据</h4>
          <p className={styles.emptyDesc}>发送消息后，Token 消耗会自动记录并在此展示</p>
        </div>
      ) : (
        <>
          {/* Model ranking leaderboard */}
          {rankedModels.length > 0 && (
            <div className={styles.leaderboard}>
              <h4 className={styles.sectionTitle}>模型消耗排名</h4>
              <div className={styles.leaderboardList}>
                {rankedModels.map((m, i) => {
                  const barWidth = Math.max((m.total / maxModelTotal) * 100, 2)
                  const pct = grandTotal > 0 ? ((m.total / grandTotal) * 100).toFixed(1) : '0'
                  const hasPrice = m.input_price > 0 || m.output_price > 0
                  return (
                    <div key={m.model} className={styles.rankItem}>
                      <div className={styles.rankBadge} style={i < 3 ? { background: RANK_COLORS[i], color: i === 0 ? '#1e293b' : '#fff' } : {}}>
                        {i + 1}
                      </div>
                      <div className={styles.rankBody}>
                        <div className={styles.rankHeader}>
                          <span className={styles.rankModel}>{m.model}</span>
                          {(() => {
                            const info = modelLookup.get(m.model)
                            if (!info) return null
                            return (
                              <span className={styles.modelBadges}>
                                <span className={`${styles.modelBadge} ${isLocalModel(info.backend) ? styles.badgeLocal : styles.badgeCloud}`}>
                                  {isLocalModel(info.backend) ? '本地' : '云端'}
                                </span>
                                <span className={styles.modelBadgeProvider}>
                                  {getProviderLabel(info.backend, info.backendLabel)}
                                </span>
                              </span>
                            )
                          })()}
                          <span className={styles.rankTokens}>
                            <strong>{formatTokens(m.total)}</strong>
                            {hasPrice && <span className={styles.rankCost}>{formatCost(m.cost)}</span>}
                            <span className={styles.rankTokensPct}>{pct}%</span>
                          </span>
                        </div>
                        <div className={styles.rankBarTrack}>
                          <div
                            className={styles.rankBarFill}
                            style={{ width: `${barWidth}%`, background: i === 0 ? 'linear-gradient(90deg, #22d3ee, #6366f1)' : i === 1 ? 'linear-gradient(90deg, #a78bfa, #6366f1)' : 'var(--border)' }}
                          />
                        </div>
                        <div className={styles.rankMeta}>
                          <span>P: {formatNumber(m.prompt)}</span>
                          <span>C: {formatNumber(m.completion)}</span>
                          <span>Cache: {formatNumber(m.cached)}</span>
                          <span>{m.requests} 次</span>
                          {hasPrice && (
                            <span className={styles.rankMetaPrice}>
                              ¥{m.input_price}/¥{m.output_price} /百万
                            </span>
                          )}
                        </div>
                      </div>
                    </div>
                  )
                })}
              </div>
            </div>
          )}

          {/* Model detail table */}
          {rankedModels.length > 0 && (
            <div className={styles.modelTableWrapper}>
              <h4 className={styles.sectionTitle}>模型明细</h4>
              <table className={styles.modelTable}>
                <thead>
                  <tr>
                    <th>#</th>
                    <th>模型</th>
                    <th>供应商</th>
                    <th>请求</th>
                    <th>Prompt</th>
                    <th>Completion</th>
                    <th>合计</th>
                    <th>占比</th>
                    <th>费用</th>
                  </tr>
                </thead>
                <tbody>
                  {rankedModels.map((m, i) => {
                    const info = modelLookup.get(m.model)
                    const local = info ? isLocalModel(info.backend) : false
                    const provider = info ? getProviderLabel(info.backend, info.backendLabel) : ''
                    return (
                      <tr key={m.model}>
                        <td className={styles.rankCell}>{i + 1}</td>
                        <td className={styles.modelNameCell}>
                          <div className={styles.modelNameInner}>
                            {m.model}
                            {local && <span className={`${styles.modelBadge} ${styles.badgeLocal}`}>本地</span>}
                          </div>
                        </td>
                        <td className={styles.providerCell}>
                          {provider && <span className={styles.modelBadgeProvider}>{provider}</span>}
                        </td>
                        <td>{m.requests}</td>
                        <td>{formatNumber(m.prompt)}</td>
                        <td>{formatNumber(m.completion)}</td>
                        <td className={styles.totalCell}>{formatNumber(m.total)}</td>
                        <td className={styles.pctCell}>{grandTotal > 0 ? ((m.total / grandTotal) * 100).toFixed(1) : '0'}%</td>
                        <td className={styles.costCell}>{m.cost > 0 ? formatCost(m.cost) : '—'}</td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </div>
  )
}

import { useEffect, useMemo, useCallback, useState } from 'react'
import { BarChart3, TrendingUp, Zap, Database, AlertCircle } from 'lucide-react'
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

function formatLatency(ms: number): string {
  if (ms >= 1000) return (ms / 1000).toFixed(1) + 's'
  return ms.toFixed(0) + 'ms'
}

function formatPercent(n: number): string {
  return (n * 100).toFixed(1) + '%'
}

const RANK_COLORS = ['#fbbf24', '#94a3b8', '#cd7f32'] // gold, silver, bronze

// ── Tooltip helper component ──

interface TooltipData {
  x: number
  y: number
  visible: boolean
  date: string
  prompt: number
  completion: number
  cached: number
}

// ── Loading skeleton ──

function LoadingSkeleton() {
  return (
    <div className={styles.loadingWrap}>
      <div className={`${styles.skeleton} ${styles.skeletonHero}`} />
      <div className={styles.skeletonRow}>
        <div className={`${styles.skeleton} ${styles.skeletonPill}`} />
        <div className={`${styles.skeleton} ${styles.skeletonPill}`} />
        <div className={`${styles.skeleton} ${styles.skeletonPill}`} />
      </div>
      <div className={`${styles.skeleton} ${styles.skeletonChart}`} />
      <div className={`${styles.skeleton} ${styles.skeletonList}`} />
    </div>
  )
}

// ── Error state ──

function ErrorState({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className={styles.errorState}>
      <div className={styles.errorIcon}><AlertCircle size={28} /></div>
      <h4 className={styles.errorTitle}>加载失败</h4>
      <p className={styles.errorDesc}>{message || '无法加载 Token 用量数据'}</p>
      <button className={styles.errorRetryBtn} onClick={onRetry}>重试</button>
    </div>
  )
}

// ── SVG trend chart ──

const CHART_HEIGHT = 160
const CHART_PADDING_TOP = 16
const CHART_PADDING_BOTTOM = 24
const CHART_PADDING_LEFT = 0
const CHART_PADDING_RIGHT = 0
const BAR_MAX_WIDTH = 36
const BAR_GAP = 4

interface TrendChartProps {
  history: Array<{ date: string; prompt: number; completion: number; cached: number; requests: number }>
}

function TrendChart({ history }: TrendChartProps) {
  const [tooltip, setTooltip] = useState<TooltipData>({
    x: 0, y: 0, visible: false, date: '', prompt: 0, completion: 0, cached: 0,
  })

  const chartData = useMemo(() => {
    if (!history || history.length === 0) return { bars: [] as Array<{ date: string; prompt: number; completion: number; total: number }>, maxVal: 1, chartW: 0, barW: 0 }
    const bars = history.map(h => ({
      date: h.date,
      prompt: h.prompt,
      completion: h.completion,
      total: h.prompt + h.completion,
    }))
    const maxVal = Math.max(...bars.map(b => b.total), 1)
    return { bars, maxVal }
  }, [history])

  const { bars, maxVal } = chartData
  if (bars.length === 0) return null

  const plotH = CHART_HEIGHT - CHART_PADDING_TOP - CHART_PADDING_BOTTOM
  // Responsive bar width: fit all bars, capped at BAR_MAX_WIDTH
  const totalAvailable = 600 // approximate available width
  const rawBarW = Math.min(BAR_MAX_WIDTH, (totalAvailable - (bars.length - 1) * BAR_GAP) / bars.length)
  const barW = Math.max(4, rawBarW)
  const chartW = bars.length * (barW + BAR_GAP) - BAR_GAP

  const handleBarHover = useCallback((e: React.MouseEvent, bar: typeof bars[0], idx: number) => {
    // Position: fixed is relative to the viewport — use clientX/Y directly
    setTooltip({ x: e.clientX, y: e.clientY, visible: true, date: bar.date, prompt: bar.prompt, completion: bar.completion, cached: 0 })
  }, [])

  const handleBarLeave = useCallback(() => {
    setTooltip(prev => ({ ...prev, visible: false }))
  }, [])

  // Y-axis ticks
  const yTicks = useMemo(() => {
    const ticks: { value: number; y: number; label: string }[] = []
    const step = maxVal / 4
    for (let i = 0; i <= 4; i++) {
      const value = step * i
      ticks.push({
        value,
        y: CHART_PADDING_TOP + plotH - (value / maxVal) * plotH,
        label: formatNumber(value),
      })
    }
    return ticks
  }, [maxVal, plotH])

  return (
    <div className={styles.chartContainer}>
      <div className={styles.chartHeader}>
        <h4 className={styles.sectionTitle}>用量趋势</h4>
        <div className={styles.chartLegend}>
          <span className={styles.chartLegendItem}>
            <span className={styles.chartLegendDot} style={{ background: '#22d3ee' }} />
            Prompt
          </span>
          <span className={styles.chartLegendItem}>
            <span className={styles.chartLegendDot} style={{ background: '#a78bfa' }} />
            Completion
          </span>
        </div>
      </div>
      <div className={styles.chartScroll} style={{ overflowX: 'auto', overflowY: 'hidden' }}>
        <svg
          className={styles.chartSvg}
          viewBox={`0 0 ${Math.max(chartW + CHART_PADDING_LEFT + CHART_PADDING_RIGHT, 100)} ${CHART_HEIGHT}`}
          width={Math.max(chartW + CHART_PADDING_LEFT + CHART_PADDING_RIGHT, 100)}
          height={CHART_HEIGHT}
        >
          {/* Grid lines */}
          {yTicks.map((t, i) => (
            <g key={`ytick-${i}`}>
              <line
                x1={CHART_PADDING_LEFT}
                y1={t.y}
                x2={chartW + CHART_PADDING_LEFT}
                y2={t.y}
                stroke="rgba(255,255,255,0.05)"
                strokeDasharray="3 3"
              />
            </g>
          ))}
          {/* Bars */}
          {bars.map((bar, i) => {
            const x = CHART_PADDING_LEFT + i * (barW + BAR_GAP)
            const promptH = (bar.prompt / maxVal) * plotH
            const completionH = (bar.completion / maxVal) * plotH
            const totalH = promptH + completionH
            return (
              <g
                key={i}
                className={styles.chartBarGroup}
                onMouseEnter={(e) => handleBarHover(e, bar, i)}
                onMouseLeave={handleBarLeave}
              >
                <rect
                  x={x}
                  y={CHART_PADDING_TOP + plotH - promptH}
                  width={barW}
                  height={Math.max(promptH, 0.5)}
                  fill="#22d3ee"
                  rx="2"
                  className={styles.chartBarRect}
                />
                <rect
                  x={x}
                  y={CHART_PADDING_TOP + plotH - totalH}
                  width={barW}
                  height={Math.max(completionH, 0.5)}
                  fill="#a78bfa"
                  rx="2"
                  className={styles.chartBarRect}
                />
                {barW >= 14 && (
                  <text
                    x={x + barW / 2}
                    y={CHART_PADDING_TOP + plotH - totalH - 4}
                    textAnchor="middle"
                    fontSize="9"
                    fill="var(--text-muted)"
                    fontFamily="var(--font-mono)"
                  >
                    {formatNumber(bar.total)}
                  </text>
                )}
              </g>
            )
          })}
        </svg>
      </div>
      {/* Tooltip */}
      {tooltip.visible && (
        <div
          className={styles.tooltip}
          style={{ left: tooltip.x + 12, top: tooltip.y - 10 }}
        >
          <div className={styles.tooltipDate}>{tooltip.date}</div>
          <div className={styles.tooltipRow}>
            <span className={styles.tooltipDot} style={{ background: '#22d3ee' }} />
            Prompt: {formatNumber(tooltip.prompt)}
          </div>
          <div className={styles.tooltipRow}>
            <span className={styles.tooltipDot} style={{ background: '#a78bfa' }} />
            Completion: {formatNumber(tooltip.completion)}
          </div>
          <div className={styles.tooltipRow}>
            合计: {formatNumber(tooltip.prompt + tooltip.completion)}
          </div>
        </div>
      )}
    </div>
  )
}

// ── Main component ──

export default function TokenUsagePanel() {
  const sessionTotal = useStore((s) => s.sessionTotal)
  const summary = useStore((s) => s.summary)
  const loading = useStore((s) => s.loading)
  const timeRange = useStore((s) => s.timeRange)
  const setTimeRange = useStore((s) => s.setTimeRange)
  const models = useStore((s) => s.models)
  const history = useStore((s) => s.history)
  // Track error state locally since the store doesn't expose it
  const [loadError, setLoadError] = useState<string | null>(null)

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

  // Track errors from summary/history loading
  useEffect(() => {
    if (loading) {
      setLoadError(null)
    }
  }, [loading])

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
  const avgLatency = summary?.avg_latency_ms ?? 0
  const cacheHitRate = summary?.cache_hit_rate ?? 0
  const totalRequests = summary?.total_requests ?? 0

  const hasData = (summary && summary.total_requests > 0) || sessionTotal.requests > 0

  // Cost by provider aggregation
  const costByProvider = useMemo(() => {
    if (!summary?.by_model || summary.by_model.length === 0) return []
    const map = new Map<string, { provider: string; cost: number; tokens: number; requests: number }>()
    for (const m of summary.by_model) {
      const info = modelLookup.get(m.model)
      const provider = info ? getProviderLabel(info.backend, info.backendLabel) : m.model
      const existing = map.get(provider)
      if (existing) {
        existing.cost += m.cost || 0
        existing.tokens += m.prompt + m.completion
        existing.requests += m.requests
      } else {
        map.set(provider, { provider, cost: m.cost || 0, tokens: m.prompt + m.completion, requests: m.requests })
      }
    }
    return [...map.values()].sort((a, b) => b.cost - a.cost)
  }, [summary, modelLookup])

  const maxProviderCost = Math.max(...costByProvider.map(p => p.cost), 0.01)

  // Handlers
  const handleTimeRangeChange = useCallback((r: 'all' | '7d' | '30d') => {
    setLoadError(null)
    setTimeRange(r)
  }, [setTimeRange])

  const handleReset = useCallback(async () => {
    const ok = await useStore.getState().showConfirm('重置用量', '确定要清除所有 Token 用量记录吗？此操作不可撤销。', true)
    if (ok) {
      setLoadError(null)
      useStore.getState().resetTokenUsage()
    }
  }, [])

  const handleRetry = useCallback(() => {
    setLoadError(null)
    setTimeRange(timeRange)
  }, [timeRange, setTimeRange])

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

      {/* Key metrics cards */}
      {hasData && !loading && (
        <div className={styles.metricsRow}>
          <div className={styles.metricCard}>
            <div className={styles.metricIcon} style={{ background: 'rgba(34, 211, 238, 0.1)', color: '#22d3ee' }}>
              <TrendingUp size={14} />
            </div>
            <div className={styles.metricBody}>
              <div className={styles.metricValue}>{formatNumber(totalRequests)}</div>
              <div className={styles.metricLabel}>请求次数</div>
            </div>
          </div>
          {avgLatency > 0 && (
            <div className={styles.metricCard}>
              <div className={styles.metricIcon} style={{ background: 'rgba(250, 204, 21, 0.1)', color: '#facc15' }}>
                <Zap size={14} />
              </div>
              <div className={styles.metricBody}>
                <div className={styles.metricValue}>{formatLatency(avgLatency)}</div>
                <div className={styles.metricLabel}>平均延迟</div>
              </div>
            </div>
          )}
          {cacheHitRate > 0 && (
            <div className={styles.metricCard}>
              <div className={styles.metricIcon} style={{ background: 'rgba(52, 211, 153, 0.1)', color: '#34d399' }}>
                <Database size={14} />
              </div>
              <div className={styles.metricBody}>
                <div className={styles.metricValue}>{formatPercent(cacheHitRate)}</div>
                <div className={styles.metricLabel}>缓存命中率</div>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Time range selector */}
      <div className={styles.timeRangeRow}>
        <span className={styles.dataPointInfo}>
          {loading ? '加载中...' : hasData ? `${rankedModels.length} 个模型 / ${totalRequests} 次请求` : ''}
        </span>
        <div className={styles.timeRangeToggle}>
          {(['all', '7d', '30d'] as const).map((r) => (
            <button
              key={r}
              className={`${styles.timeRangeBtn} ${timeRange === r ? styles.timeRangeBtnActive : ''}`}
              onClick={() => handleTimeRangeChange(r)}
            >
              {r === 'all' ? '全部' : r === '7d' ? '近7天' : '近30天'}
            </button>
          ))}
          {hasData && (
            <button
              className={styles.resetBtn}
              onClick={handleReset}
              title="清除所有记录"
            >
              重置
            </button>
          )}
        </div>
      </div>

      {/* Loading state */}
      {loading && !hasData && <LoadingSkeleton />}

      {/* Error state */}
      {loadError && !loading && <ErrorState message={loadError} onRetry={handleRetry} />}

      {!hasData && !loading && !loadError ? (
        <div className={styles.emptyState}>
          <div className={styles.emptyIcon}><BarChart3 size={32} /></div>
          <h4 className={styles.emptyTitle}>暂无数据</h4>
          <p className={styles.emptyDesc}>发送消息后，Token 消耗会自动记录并在此展示</p>
          <p className={styles.emptyHint}>选择时间范围后数据将在此显示</p>
        </div>
      ) : (
        <>
          {/* Loading state with stale data shown underneath */}
          {loading && hasData && (
            <div className={styles.loadingOverlay}>
              <div className={styles.loadingSpinner} />
              <span className={styles.loadingText}>刷新中...</span>
            </div>
          )}

          {/* Trend chart */}
          {history && history.length > 1 && <TrendChart history={history} />}

          {/* Cost by provider breakdown */}
          {costByProvider.length > 1 && (
            <div className={styles.providerCostSection}>
              <h4 className={styles.sectionTitle}>供应商费用分布</h4>
              <div className={styles.providerCostList}>
                {costByProvider.map((p) => {
                  const barPct = maxProviderCost > 0 ? (p.cost / maxProviderCost) * 100 : 0
                  return (
                    <div key={p.provider} className={styles.providerCostItem}>
                      <div className={styles.providerCostHeader}>
                        <span className={styles.providerCostName}>{p.provider}</span>
                        <span className={styles.providerCostValue}>{formatCost(p.cost)}</span>
                      </div>
                      <div className={styles.providerCostBarTrack}>
                        <div
                          className={styles.providerCostBarFill}
                          style={{ width: `${Math.max(barPct, 2)}%` }}
                        />
                      </div>
                      <div className={styles.providerCostMeta}>
                        <span>{formatTokens(p.tokens)} tokens</span>
                        <span>{p.requests} 请求</span>
                      </div>
                    </div>
                  )
                })}
              </div>
            </div>
          )}

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
                          <span className={styles.rankModel} title={m.model}>{m.model}</span>
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
                          <span>输入: {formatNumber(m.prompt)}</span>
                          <span>命中: {formatNumber((m as any).cache_hit_tokens ?? 0)}</span>
                          <span>输出: {formatNumber(m.completion)}</span>
                          <span>{m.requests} 次</span>
                          {hasPrice && (
                            <span className={styles.rankMetaPrice}>
                              输入 ¥{m.input_price} | 命中 ¥{m.cache_read_price} | 写入 ¥{m.cache_write_price} | 输出 ¥{m.output_price} /百万
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
              <div className={styles.modelTableScroll}>
                <table className={styles.modelTable}>
                  <thead>
                    <tr>
                      <th>#</th>
                      <th>模型</th>
                      <th>供应商</th>
                      <th>请求</th>
                      <th>输入(未命中)</th>
                      <th>输入(命中)</th>
                      <th>缓写</th>
                      <th>输出</th>
                      <th>合计</th>
                      <th>费用</th>
                    </tr>
                  </thead>
                  <tbody>
                    {rankedModels.map((m, i) => {
                      const info = modelLookup.get(m.model)
                      const local = info ? isLocalModel(info.backend) : false
                      const provider = info ? getProviderLabel(info.backend, info.backendLabel) : ''
                      const cacheHit = (m as any).cache_hit_tokens ?? 0
                      const cacheMiss = (m as any).cache_miss_tokens ?? (m.prompt - cacheHit || 0)
                      const cacheWrite = (m as any).cache_write_tokens ?? m.cached ?? 0
                      return (
                        <tr key={m.model}>
                          <td className={styles.rankCell}>{i + 1}</td>
                          <td className={styles.modelNameCell}>
                            <div className={styles.modelNameInner} title={m.model}>
                              {m.model}
                              {local && <span className={`${styles.modelBadge} ${styles.badgeLocal}`}>本地</span>}
                            </div>
                          </td>
                          <td className={styles.providerCell}>
                            {provider && <span className={styles.modelBadgeProvider}>{provider}</span>}
                          </td>
                          <td>{m.requests}</td>
                          <td>{formatNumber(cacheMiss)}</td>
                          <td>{formatNumber(cacheHit)}</td>
                          <td>{formatNumber(cacheWrite)}</td>
                          <td>{formatNumber(m.completion)}</td>
                          <td className={styles.totalCell}>{formatNumber(m.total)}</td>
                          <td className={styles.costCell}>{m.cost > 0 ? formatCost(m.cost) : '—'}</td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </>
      )}
    </div>
  )
}

import { useEffect, useMemo, useCallback, useState, useRef } from 'react'
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

const CHART_HEIGHT = 180
const CHART_PADDING_TOP = 12
const CHART_PADDING_BOTTOM = 36
const CHART_PADDING_LEFT = 42
const CHART_PADDING_RIGHT = 8
const BAR_GAP = 4

function formatDateLabel(dateStr: string): string {
  const parts = dateStr.split('-')
  if (parts.length === 3) return `${parseInt(parts[1])}/${parseInt(parts[2])}`
  return dateStr
}

interface TrendChartProps {
  history: Array<{ date: string; prompt: number; completion: number; cached: number; requests: number }>
}

function TrendChart({ history }: TrendChartProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const [containerW, setContainerW] = useState(600)
  const [tooltip, setTooltip] = useState<TooltipData>({
    x: 0, y: 0, visible: false, date: '', prompt: 0, completion: 0, cached: 0,
  })

  // Measure actual container width for responsive bar sizing
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const ro = new ResizeObserver(entries => {
      for (const entry of entries) setContainerW(entry.contentRect.width)
    })
    ro.observe(el)
    setContainerW(el.clientWidth)
    return () => ro.disconnect()
  }, [])

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
  // Bar width adapts to container: few bars → wide, many bars → narrow, capped at 120px
  const rawBarW = Math.min(120, (containerW - CHART_PADDING_LEFT - CHART_PADDING_RIGHT - (bars.length - 1) * BAR_GAP) / bars.length)
  const barW = Math.max(4, rawBarW)
  const chartW = bars.length * (barW + BAR_GAP) - BAR_GAP
  const labelInterval = bars.length > 25 ? 7 : bars.length > 14 ? 4 : bars.length > 7 ? 2 : 1

  const handleBarHover = useCallback((e: React.MouseEvent, bar: typeof bars[0], idx: number) => {
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
      <div className={styles.chartScroll} ref={containerRef} style={{ overflowX: chartW > containerW ? 'auto' : 'hidden', overflowY: 'hidden' }}>
        <svg
          className={styles.chartSvg}
          viewBox={`0 0 ${Math.max(chartW + CHART_PADDING_LEFT + CHART_PADDING_RIGHT, 100)} ${CHART_HEIGHT}`}
          width={Math.max(chartW + CHART_PADDING_LEFT + CHART_PADDING_RIGHT, 100)}
          height={CHART_HEIGHT}
        >
          <defs>
            <linearGradient id="promptGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#22d3ee" />
              <stop offset="100%" stopColor="#0891b2" />
            </linearGradient>
            <linearGradient id="completionGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#a78bfa" />
              <stop offset="100%" stopColor="#7c3aed" />
            </linearGradient>
          </defs>
          {/* Y-axis labels + grid lines */}
          {yTicks.map((t, i) => (
            <g key={`ytick-${i}`}>
              <line
                x1={CHART_PADDING_LEFT}
                y1={t.y}
                x2={chartW + CHART_PADDING_LEFT}
                y2={t.y}
                stroke="var(--border-light)"
                strokeDasharray="3 3"
              />
              <text
                x={CHART_PADDING_LEFT - 6}
                y={t.y + 3}
                textAnchor="end"
                fontSize="9"
                fill="var(--text-muted)"
                fontFamily="var(--font-mono)"
              >
                {t.label}
              </text>
            </g>
          ))}
          {/* Bars */}
          {bars.map((bar, i) => {
            const x = CHART_PADDING_LEFT + i * (barW + BAR_GAP)
            const promptH = Math.max((bar.prompt / maxVal) * plotH, bar.prompt > 0 ? 1 : 0)
            const completionH = Math.max((bar.completion / maxVal) * plotH, bar.completion > 0 ? 1 : 0)
            const totalH = promptH + completionH
            const gap = promptH > 0 && completionH > 0 ? 1 : 0
            return (
              <g
                key={i}
                className={styles.chartBarGroup}
                onMouseEnter={(e) => handleBarHover(e, bar, i)}
                onMouseLeave={handleBarLeave}
              >
                {/* Prompt bar */}
                {promptH > 0 && (
                  <rect
                    x={x}
                    y={CHART_PADDING_TOP + plotH - promptH}
                    width={barW}
                    height={promptH - gap}
                    fill="url(#promptGrad)"
                    rx="2"
                    className={styles.chartBarRect}
                  />
                )}
                {/* Completion bar */}
                {completionH > 0 && (
                  <rect
                    x={x}
                    y={CHART_PADDING_TOP + plotH - totalH}
                    width={barW}
                    height={completionH}
                    fill="url(#completionGrad)"
                    rx="2"
                    className={styles.chartBarRect}
                  />
                )}
                {/* Total label above bar */}
                {barW >= 14 && totalH > 14 && (
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
                {/* X-axis date label */}
                {i % labelInterval === 0 && (
                  <text
                    x={x + barW / 2}
                    y={CHART_HEIGHT - 6}
                    textAnchor="middle"
                    fontSize="9"
                    fill="var(--text-muted)"
                  >
                    {formatDateLabel(bar.date)}
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
  const loadError = useStore((s) => s.loadError)
  const timeRange = useStore((s) => s.timeRange)
  const setTimeRange = useStore((s) => s.setTimeRange)
  const models = useStore((s) => s.models)
  const history = useStore((s) => s.history)

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
  const handleTimeRangeChange = useCallback((r: 'all' | 'today' | '7d' | '30d') => {
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
            <div className={styles.metricIcon} style={{ background: 'rgba(var(--accent-rgb), 0.1)', color: 'var(--accent)' }}>
              <TrendingUp size={14} />
            </div>
            <div className={styles.metricBody}>
              <div className={styles.metricValue}>{formatNumber(totalRequests)}</div>
              <div className={styles.metricLabel}>请求次数</div>
            </div>
          </div>
          {avgLatency > 0 && (
            <div className={styles.metricCard}>
              <div className={styles.metricIcon} style={{ background: 'rgba(var(--amber-rgb, 245, 158, 11), 0.1)', color: 'var(--amber)' }}>
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
              <div className={styles.metricIcon} style={{ background: 'rgba(var(--green-rgb, 45, 212, 191), 0.1)', color: 'var(--green)' }}>
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
          {(['today', '7d', '30d', 'all'] as const).map((r) => (
            <button
              key={r}
              className={`${styles.timeRangeBtn} ${timeRange === r ? styles.timeRangeBtnActive : ''}`}
              onClick={() => handleTimeRangeChange(r)}
            >
              {r === 'all' ? '全部' : r === 'today' ? '今天' : r === '7d' ? '近7天' : '近30天'}
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

          {/* Model ranking — podium (top 3) */}
          {rankedModels.length > 0 && (() => {
            const top3 = rankedModels.slice(0, 3)
            // Display order: silver(2nd) left, gold(1st) center, bronze(3rd) right
            const podiumEntries = [
              { model: top3[1], rank: 2, cls: styles.podiumSilver, medal: '🥈' },
              { model: top3[0], rank: 1, cls: styles.podiumGold,   medal: '🥇' },
              { model: top3[2], rank: 3, cls: styles.podiumBronze, medal: '🥉' },
            ].filter(e => e.model)

            return (
              <div className={styles.leaderboard}>
                <h4 className={styles.sectionTitle}>模型消耗排名</h4>
                <div className={styles.podium}>
                  {podiumEntries.map(({ model, rank, cls, medal }) => {
                    const pct = grandTotal > 0 ? ((model.total / grandTotal) * 100).toFixed(1) : '0'
                    const hasPrice = model.input_price > 0 || model.output_price > 0
                    const info = modelLookup.get(model.model)
                    return (
                      <div key={model.model} className={`${styles.podiumCol} ${cls}`}>
                        <div className={styles.podiumMedal}>{medal}</div>
                        <div className={styles.podiumModelName} title={model.model}>{model.model}</div>
                        {info && (
                          <div className={styles.podiumBadges}>
                            <span className={`${styles.modelBadge} ${isLocalModel(info.backend) ? styles.badgeLocal : styles.badgeCloud}`}>
                              {isLocalModel(info.backend) ? '本地' : '云端'}
                            </span>
                            <span className={styles.modelBadgeProvider}>
                              {getProviderLabel(info.backend, info.backendLabel)}
                            </span>
                          </div>
                        )}
                        <div className={styles.podiumTokens}>{formatTokens(model.total)}</div>
                        <div className={styles.podiumPct}>{pct}%</div>
                        {hasPrice && <div className={styles.podiumCost}>{formatCost(model.cost)}</div>}
                        <div className={styles.podiumStats}>
                          <span>输入 {formatNumber(model.prompt)}</span>
                          <span>输出 {formatNumber(model.completion)}</span>
                          <span>{model.requests} 次</span>
                        </div>
                      </div>
                    )
                  })}
                </div>
              </div>
            )
          })()}

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
                      const cacheHit = m.cache_hit_tokens ?? 0
                      const cacheMiss = m.cache_miss_tokens ?? (m.prompt - cacheHit || 0)
                      const cacheWrite = m.cache_write_tokens ?? m.cached ?? 0

                      // Ensure the ranked model includes typed cache fields for the table
                      // (they are already part of TokenSummary.by_model — no need for `as any`)
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

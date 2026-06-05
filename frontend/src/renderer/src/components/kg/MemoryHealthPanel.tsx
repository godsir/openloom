import { useState, useEffect, useCallback, useMemo } from 'react'
import { useStore } from '../../stores'
import styles from './MemoryHealthPanel.module.css'

// ── helpers ──

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v))
}

function hashHue(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) h = ((h << 5) - h + s.charCodeAt(i)) | 0
  return Math.abs(h) % 360
}

function entityColor(type: string): string {
  return `hsl(${hashHue(type)}, 55%, 58%)`
}

function formatPct(v: number): string {
  return `${(v * 100).toFixed(0)}%`
}

function formatTs(iso: string): string {
  if (!iso) return ''
  const d = new Date(iso)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

// ── layer color map ──
const LAYER_COLORS: Record<string, string> = {
  working: styles.barFillWorking,
  episodic: styles.barFillEpisodic,
  semantic: styles.barFillSemantic,
  global: styles.barFillGlobal,
}

function layerColor(name: string): string {
  const lower = name.toLowerCase()
  for (const [key, cls] of Object.entries(LAYER_COLORS)) {
    if (lower.includes(key)) return cls
  }
  // fallback: no special color
  return ''
}

// ── SVG gauge ring ──
const GAUGE_R = 50
const GAUGE_CIRC = 2 * Math.PI * GAUGE_R

// ── component ──

export default function MemoryHealthPanel() {
  const memoryHealth = useStore(s => s.memoryHealth)
  const qualityReport = useStore(s => s.qualityReport)
  const kgLoadHealth = useStore(s => s.kgLoadHealth)
  const kgLoadQuality = useStore(s => s.kgLoadQuality)

  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const loadData = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      await Promise.all([kgLoadHealth(), kgLoadQuality()])
      // Give the store a tick to update; if both are still null the RPC failed silently
      // We check via a follow-up read — but since our selectors are reactive, just catch
      // any thrown errors from the RPC layer.
    } catch (e: any) {
      setError(e?.message ?? '未知错误')
      console.error('[MemoryHealthPanel] load failed:', e)
    } finally {
      setLoading(false)
    }
  }, [kgLoadHealth, kgLoadQuality])

  // Load on mount
  useEffect(() => {
    if (!memoryHealth && !qualityReport) {
      loadData()
    }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  // Compute health score from fragmentation: lower fragmentation = higher health
  const healthScore = useMemo(() => {
    if (!memoryHealth) return null
    return Math.round(100 - memoryHealth.fragmentation_score * 100)
  }, [memoryHealth])

  // Gauge color class
  const gaugeColorClass = useMemo(() => {
    if (healthScore === null) return styles.scoreRed
    if (healthScore < 40) return styles.scoreRed
    if (healthScore < 70) return styles.scoreYellow
    return styles.scoreGreen
  }, [healthScore])

  const gaugeFillClass = useMemo(() => {
    if (healthScore === null) return styles.fillRed
    if (healthScore < 40) return styles.fillRed
    if (healthScore < 70) return styles.fillYellow
    return styles.fillGreen
  }, [healthScore])

  // SVG ring dash offset
  const dashOffset = useMemo(() => {
    if (healthScore === null) return GAUGE_CIRC
    const pct = clamp(healthScore / 100, 0, 1)
    return GAUGE_CIRC * (1 - pct)
  }, [healthScore])

  // Status badge
  const statusClass = useMemo(() => {
    if (!memoryHealth?.status) return ''
    switch (memoryHealth.status) {
      case 'critical': return styles.statusCritical
      case 'degraded': return styles.statusDegraded
      default: return styles.statusHealthy
    }
  }, [memoryHealth])

  const statusLabel = useMemo(() => {
    if (!memoryHealth?.status) return ''
    switch (memoryHealth.status) {
      case 'critical': return '危险'
      case 'degraded': return '欠佳'
      default: return '健康'
    }
  }, [memoryHealth])

  // Layer distribution sorted
  const layers = useMemo(() => {
    if (!memoryHealth?.layer_distribution) return []
    return [...memoryHealth.layer_distribution]
  }, [memoryHealth])

  const maxLayerCount = useMemo(() => {
    if (layers.length === 0) return 0
    return Math.max(...layers.map(([, c]) => c))
  }, [layers])

  // Entity type distribution sorted desc
  const typeDistribution = useMemo(() => {
    if (!qualityReport?.entity_types_distribution) return []
    return [...qualityReport.entity_types_distribution].sort((a, b) => b[1] - a[1])
  }, [qualityReport])

  // ── render states ──

  // Loading state (initial)
  if (loading && !memoryHealth && !qualityReport) {
    return (
      <div className={styles.panel}>
        <div className={styles.header}>
          <span className={styles.title}>记忆力健康</span>
        </div>
        <div className={styles.loadingState}>
          <div className={styles.loadingSpinner} />
          <span className={styles.loadingText}>加载健康数据...</span>
        </div>
      </div>
    )
  }

  // Error state (with no data)
  if (error && !memoryHealth && !qualityReport) {
    return (
      <div className={styles.panel}>
        <div className={styles.header}>
          <span className={styles.title}>记忆力健康</span>
        </div>
        <div className={styles.errorState}>
          <span className={styles.errorIcon}>!</span>
          <span className={styles.errorText}>加载失败: {error}</span>
          <button className={styles.retryBtn} onClick={loadData}>
            重试
          </button>
        </div>
      </div>
    )
  }

  // Empty state (no data yet)
  if (!memoryHealth && !qualityReport) {
    return (
      <div className={styles.panel}>
        <div className={styles.header}>
          <span className={styles.title}>记忆力健康</span>
          <button className={styles.refreshBtn} onClick={loadData} disabled={loading}>
            <span className={`${styles.refreshIcon} ${loading ? styles.spinning : ''}`}>
              {loading ? '⟳' : '↻'}
            </span>
            刷新
          </button>
        </div>
        <div className={styles.emptyState}>
          暂无健康数据
          <div className={styles.emptyHint}>
            点击刷新按钮获取最新的记忆系统健康状态
          </div>
        </div>
      </div>
    )
  }

  // ── main render ──

  return (
    <div className={styles.panel}>
      {/* Header */}
      <div className={styles.header}>
        <span className={styles.title}>记忆力健康</span>
        <button className={styles.refreshBtn} onClick={loadData} disabled={loading}>
          <span className={`${styles.refreshIcon} ${loading ? styles.spinning : ''}`}>
            {loading ? '⟳' : '↻'}
          </span>
          刷新
        </button>
      </div>

      {/* Top row: gauge + cards */}
      <div className={styles.topRow}>
        {/* Health gauge */}
        <div className={styles.gaugeSection}>
          <span className={styles.gaugeLabel}>健康指数</span>
          <div className={styles.gaugeRing}>
            <svg
              className={styles.gaugeSvg}
              width="120"
              height="120"
              viewBox="0 0 120 120"
            >
              <circle
                className={styles.gaugeBg}
                cx="60"
                cy="60"
                r={GAUGE_R}
              />
              <circle
                className={`${styles.gaugeFill} ${gaugeFillClass}`}
                cx="60"
                cy="60"
                r={GAUGE_R}
                strokeDasharray={GAUGE_CIRC}
                strokeDashoffset={dashOffset}
              />
            </svg>
            <div className={styles.gaugeCenter}>
              <span className={`${styles.gaugeScore} ${gaugeColorClass}`}>
                {healthScore ?? '--'}
              </span>
              <span className={styles.gaugeUnit}>/ 100</span>
            </div>
          </div>
          {memoryHealth?.status && (
            <span className={`${styles.gaugeStatus} ${statusClass}`}>
              {statusLabel}
            </span>
          )}
        </div>

        {/* Card grid */}
        <div className={styles.cardGrid}>
          <div className={styles.statCard}>
            <span className={styles.cardLabel}>实体总数</span>
            <span className={styles.cardValue}>
              {memoryHealth?.total_nodes?.toLocaleString() ?? '--'}
            </span>
            <span className={styles.cardSubtext}>
              {memoryHealth?.total_edges != null
                ? `${memoryHealth.total_edges} 条关系`
                : ''}
            </span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.cardLabel}>平均置信度</span>
            <span className={styles.cardValue}>
              {qualityReport?.avg_confidence != null
                ? formatPct(qualityReport.avg_confidence)
                : '--'}
            </span>
            <span className={styles.cardSubtext}>
              {qualityReport?.total_entities != null
                ? `共 ${qualityReport.total_entities} 实体`
                : ''}
            </span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.cardLabel}>孤立节点</span>
            <span className={styles.cardValue}>
              {memoryHealth?.orphan_nodes?.toLocaleString() ?? '--'}
            </span>
            <span className={styles.cardSubtext}>无关系连接的实体</span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.cardLabel}>陈旧节点</span>
            <span className={styles.cardValue}>
              {memoryHealth?.stale_nodes?.toLocaleString() ?? '--'}
            </span>
            <span className={styles.cardSubtext}>超过 90 天未更新</span>
          </div>
        </div>
      </div>

      {/* Layer distribution bar chart */}
      {layers.length > 0 && (
        <div className={styles.section}>
          <span className={styles.sectionHeader}>层级分布</span>
          <div className={styles.barChart}>
            {layers.map(([name, count]) => {
              const pct = maxLayerCount > 0 ? (count / maxLayerCount) * 100 : 0
              const fillCls = layerColor(name)
              return (
                <div key={name} className={styles.barRow}>
                  <span className={styles.barLabel}>{name}</span>
                  <div className={styles.barTrack}>
                    <div
                      className={`${styles.barFill} ${fillCls}`}
                      style={{ width: `${pct}%` }}
                    >
                      {pct > 15 && (
                        <span className={styles.barValue}>{count}</span>
                      )}
                    </div>
                  </div>
                  <span className={styles.barCount}>{count}</span>
                </div>
              )
            })}
          </div>
        </div>
      )}

      {/* Entity type distribution */}
      {typeDistribution.length > 0 && (
        <div className={styles.section}>
          <span className={styles.sectionHeader}>实体类型分布</span>
          <div className={styles.typeList}>
            {typeDistribution.slice(0, 12).map(([name, count]) => (
              <div key={name} className={styles.typeRow}>
                <div className={styles.typeLeft}>
                  <span
                    className={styles.typeDot}
                    style={{ background: entityColor(name) }}
                  />
                  <span className={styles.typeName}>{name}</span>
                </div>
                <span className={styles.typeCount}>{count.toLocaleString()}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Footer */}
      {memoryHealth?.checked_at && (
        <div className={styles.footer}>
          <span className={styles.footerText}>
            检查时间: {formatTs(memoryHealth.checked_at)}
          </span>
          {error && (
            <span className={styles.footerText} style={{ color: 'var(--red)' }}>
              刷新出错: {error}
            </span>
          )}
        </div>
      )}
    </div>
  )
}

import { useState, useEffect, useCallback, useMemo } from 'react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
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
  return ''
}

// ── SVG gauge ring ──
const GAUGE_R = 50
const GAUGE_CIRC = 2 * Math.PI * GAUGE_R

// ── component ──

export default function MemoryHealthPanel() {
  const { t } = useLocale()
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
    } catch (e: any) {
      setError(e?.message ?? t('kg.health.unknownError'))
      console.error('[MemoryHealthPanel] load failed:', e)
    } finally {
      setLoading(false)
    }
  }, [kgLoadHealth, kgLoadQuality, t])

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
      case 'critical': return t('kg.health.status.critical')
      case 'degraded': return t('kg.health.status.degraded')
      default: return t('kg.health.status.healthy')
    }
  }, [memoryHealth, t])

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
          <span className={styles.title}>{t('kg.health.title')}</span>
        </div>
        <div className={styles.loadingState}>
          <div className={styles.loadingSpinner} />
          <span className={styles.loadingText}>{t('kg.health.loading')}</span>
        </div>
      </div>
    )
  }

  // Error state (with no data)
  if (error && !memoryHealth && !qualityReport) {
    return (
      <div className={styles.panel}>
        <div className={styles.header}>
          <span className={styles.title}>{t('kg.health.title')}</span>
        </div>
        <div className={styles.errorState}>
          <span className={styles.errorIcon}>!</span>
          <span className={styles.errorText}>{t('kg.health.loadFailed', { error })}</span>
          <button className={styles.retryBtn} onClick={loadData}>
            {t('common.retry')}
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
          <span className={styles.title}>{t('kg.health.title')}</span>
          <button className={styles.refreshBtn} onClick={loadData} disabled={loading}>
            <span className={`${styles.refreshIcon} ${loading ? styles.spinning : ''}`}>
              {loading ? '⟳' : '↻'}
            </span>
            {t('common.refresh')}
          </button>
        </div>
        <div className={styles.emptyState}>
          {t('kg.health.noData')}
          <div className={styles.emptyHint}>
            {t('kg.health.refreshHint')}
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
        <span className={styles.title}>{t('kg.health.title')}</span>
        <button className={styles.refreshBtn} onClick={loadData} disabled={loading}>
          <span className={`${styles.refreshIcon} ${loading ? styles.spinning : ''}`}>
            {loading ? '⟳' : '↻'}
          </span>
          {t('common.refresh')}
        </button>
      </div>

      {/* Top row: gauge + cards */}
      <div className={styles.topRow}>
        {/* Health gauge */}
        <div className={styles.gaugeSection}>
          <span className={styles.gaugeLabel}>{t('kg.health.score')}</span>
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
            <span className={styles.cardLabel}>{t('kg.health.totalNodes')}</span>
            <span className={styles.cardValue}>
              {memoryHealth?.total_nodes?.toLocaleString() ?? '--'}
            </span>
            <span className={styles.cardSubtext}>
              {memoryHealth?.total_edges != null
                ? t('kg.health.edgeCount', { n: String(memoryHealth.total_edges) })
                : ''}
            </span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.cardLabel}>{t('kg.health.avgConfidence')}</span>
            <span className={styles.cardValue}>
              {qualityReport?.avg_confidence != null
                ? formatPct(qualityReport.avg_confidence)
                : '--'}
            </span>
            <span className={styles.cardSubtext}>
              {qualityReport?.total_entities != null
                ? t('kg.health.totalEntities', { n: String(qualityReport.total_entities) })
                : ''}
            </span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.cardLabel}>{t('kg.health.orphanNodes')}</span>
            <span className={styles.cardValue}>
              {memoryHealth?.orphan_nodes?.toLocaleString() ?? '--'}
            </span>
            <span className={styles.cardSubtext}>{t('kg.health.orphanHint')}</span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.cardLabel}>{t('kg.health.staleNodes')}</span>
            <span className={styles.cardValue}>
              {memoryHealth?.stale_nodes?.toLocaleString() ?? '--'}
            </span>
            <span className={styles.cardSubtext}>{t('kg.health.staleHint')}</span>
          </div>
        </div>
      </div>

      {/* Layer distribution bar chart */}
      {layers.length > 0 && (
        <div className={styles.section}>
          <span className={styles.sectionHeader}>{t('kg.health.layerDistribution')}</span>
          <div className={styles.barChart}>
            {layers.map(([name, count]) => {
              const pct = maxLayerCount > 0 ? (count / maxLayerCount) * 100 : 0
              const fillCls = layerColor(name)
              const layerNameTranslated = t(`kg.layer.${name.toLowerCase()}`) ?? name
              return (
                <div key={name} className={styles.barRow}>
                  <span className={styles.barLabel}>{layerNameTranslated}</span>
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
          <span className={styles.sectionHeader}>{t('kg.health.entityTypeDistribution')}</span>
          <div className={styles.typeList}>
            {typeDistribution.slice(0, 12).map(([name, count]) => (
              <div key={name} className={styles.typeRow}>
                <div className={styles.typeLeft}>
                  <span
                    className={styles.typeDot}
                    style={{ background: entityColor(name) }}
                  />
                  <span className={styles.typeName}>{t(`kg.entityType.${name}`) ?? name}</span>
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
            {t('kg.health.checkedAt', { time: formatTs(memoryHealth.checked_at) })}
          </span>
          {error && (
            <span className={styles.footerText} style={{ color: 'var(--red)' }}>
              {t('kg.health.refreshError', { error })}
            </span>
          )}
        </div>
      )}
    </div>
  )
}

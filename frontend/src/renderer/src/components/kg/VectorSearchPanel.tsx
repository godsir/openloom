import { useState, useCallback } from 'react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import type { VectorSearchResult } from '../../types/bindings'
import styles from './VectorSearchPanel.module.css'

// Hash entity type → stable hue, so each type gets a distinct color.
function hashHue(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) h = ((h << 5) - h + s.charCodeAt(i)) | 0
  return Math.abs(h) % 360
}

function entityColor(type: string): string {
  return `hsl(${hashHue(type)}, 70%, 62%)`
}

interface VectorSearchPanelProps {
  onEntitySelected?: (name: string) => void
}

export default function VectorSearchPanel({ onEntitySelected }: VectorSearchPanelProps) {
  const { t } = useLocale()
  const vectorResults = useStore(s => s.vectorResults)
  const kgVectorSearch = useStore(s => s.kgVectorSearch)
  const kgWalkFrom = useStore(s => s.kgWalkFrom)

  const [query, setQuery] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleSearch = useCallback(async (overrideQuery?: string) => {
    const q = (overrideQuery ?? query).trim()
    if (!q) return
    setError(null)
    setLoading(true)
    try {
      await kgVectorSearch(q)
      if (overrideQuery) setQuery(overrideQuery)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [query, kgVectorSearch])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') handleSearch()
    },
    [handleSearch],
  )

  const handleResultClick = useCallback(
    (r: VectorSearchResult) => {
      // Navigate to the graph view and load the entity's subgraph.
      // The parent decides how to switch tabs — this avoids
      // destructively discarding the search context.
      kgWalkFrom(r.name, 2)
      onEntitySelected?.(r.name)
    },
    [kgWalkFrom, onEntitySelected],
  )

  return (
    <div className={styles.panel}>
      <div className={styles.searchRow}>
        <input
          className={styles.searchInput}
          value={query}
          onChange={e => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={t('kg.semanticSearch')}
        />
        <button
          className={styles.searchBtn}
          onClick={handleSearch}
          disabled={loading || !query.trim()}
        >
          {loading ? t('kg.searching') : t('kg.search')}
        </button>
      </div>

      <div className={styles.hint}>
        {t('kg.vectorHint')}
      </div>

      {error && (
        <div className={styles.errorState}>
          <span>{t('kg.vectorUnavailable')}</span>
          <span className={styles.emptyStateHint}>{error}</span>
          <button className={styles.retryBtn} onClick={handleSearch}>
            {t('common.retry')}
          </button>
        </div>
      )}

      {!error && loading && (
        <div className={styles.loadingState}>
          <span className={styles.spinner} />
          <span>{t('kg.vectorSearching')}</span>
        </div>
      )}

      {!error && !loading && vectorResults.length > 0 && (
        <div className={styles.results}>
          {vectorResults.map(r => (
            <div
              key={r.node_id}
              className={styles.resultItem}
              onClick={() => handleResultClick(r)}
            >
              <div className={styles.resultHeader}>
                <span className={styles.resultName}>{r.name}</span>
                <span
                  className={styles.resultType}
                  style={{
                    color: entityColor(r.entity_type),
                    background: entityColor(r.entity_type) + '18',
                  }}
                >
                  {t(`kg.entityType.${r.entity_type}`) ?? r.entity_type}
                </span>
                <span className={styles.resultSim}>
                  {(r.similarity * 100).toFixed(0)}%
                </span>
              </div>
              {r.description && (
                <div className={styles.resultDesc}>{r.description}</div>
              )}
            </div>
          ))}
        </div>
      )}

      {!error && !loading && vectorResults.length === 0 && (
        <div className={styles.emptyState}>
          <span className={styles.emptyStateTitle}>{t('kg.semanticSearch')}</span>
          <span className={styles.emptyStateHint}>
            {t('kg.vectorEmptyHint')}
          </span>
          <div className={styles.sampleQueries}>
            <span className={styles.sampleLabel}>{t('kg.tryThese')}</span>
            {[t('kg.samplePython'), t('kg.sampleML'), t('kg.sampleFrontend'), t('kg.samplePerf')].map(q => (
              <button key={q} className={styles.sampleChip} onClick={() => handleSearch(q)}>{q}</button>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

import { useState, useEffect, useCallback, useMemo } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import {
  IconSearch,
  IconDownload,
  IconRefresh,
  IconLoader,
  IconChevronDown,
  IconCheck,
  IconFolder,
  IconMessageSquare,
} from '../../utils/icons'
import { toggleId, allSelected } from './importSelection'
import styles from './ImportConversationsTab.module.css'

interface ConvSummary {
  session_uuid: string
  project_dir: string
  title: string | null
  first_message: string | null
  message_count: number
  model: string | null
  started_at: string
  last_at: string
  already_imported: boolean
}

type FilterKey = 'all' | 'pending' | 'imported'
type SourceKey = 'claude' | 'openclaw' | 'codex'

const SOURCES: { key: SourceKey; labelKey: string; available: boolean }[] = [
  { key: 'claude', labelKey: 'settings.importSourceClaude', available: true },
  { key: 'openclaw', labelKey: 'settings.importSourceOpenclaw', available: false },
  { key: 'codex', labelKey: 'settings.importSourceCodex', available: false },
]

export default function ImportConversationsTab() {
  const { t } = useLocale()
  const loadSessions = useStore((s) => s.loadSessions)
  const [convs, setConvs] = useState<ConvSummary[]>([])
  const [selected, setSelected] = useState<string[]>([])
  const [scanning, setScanning] = useState(false)
  const [importing, setImporting] = useState(false)
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<FilterKey>('all')
  const [source, setSource] = useState<SourceKey>('claude')
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set())
  const [expandedImported, setExpandedImported] = useState<Set<string>>(new Set())

  const scan = useCallback(async () => {
    setScanning(true)
    try {
      const r = await loomRpc<{ conversations: ConvSummary[] }>('claude_import.scan')
      setConvs(r.conversations ?? [])
      setSelected([])
    } catch {
      useStore.getState().addToast({ type: 'error', message: t('settings.scanFailed') })
    } finally {
      setScanning(false)
    }
  }, [t])

  useEffect(() => { scan().catch(() => {}) }, [scan])

  // === filtering ===

  const filtered = useMemo(() => {
    let list = convs
    if (query) {
      const q = query.toLowerCase()
      list = list.filter((c) =>
        (c.title ?? '').toLowerCase().includes(q)
        || (c.first_message ?? '').toLowerCase().includes(q)
        || c.project_dir.toLowerCase().includes(q)
      )
    }
    if (filter === 'pending') list = list.filter((c) => !c.already_imported)
    if (filter === 'imported') list = list.filter((c) => c.already_imported)
    return list
  }, [convs, query, filter])

  // === grouping ===

  const groups = useMemo(() => {
    const g: Record<string, ConvSummary[]> = {}
    for (const c of filtered) {
      (g[c.project_dir] ??= []).push(c)
    }
    // Sort groups: pending first, then imported within each group
    for (const key of Object.keys(g)) {
      g[key].sort((a, b) => {
        if (a.already_imported !== b.already_imported) return a.already_imported ? 1 : -1
        return b.started_at.localeCompare(a.started_at)
      })
    }
    return g
  }, [filtered])

  // Auto-expand groups on first load
  useEffect(() => {
    if (Object.keys(groups).length > 0 && expandedGroups.size === 0) {
      setExpandedGroups(new Set(Object.keys(groups)))
    }
  }, [Object.keys(groups).join(',')])

  // === stats ===

  const stats = useMemo(() => ({
    total: filtered.length,
    pending: filtered.filter((c) => !c.already_imported).length,
    imported: filtered.filter((c) => c.already_imported).length,
    projects: Object.keys(groups).length,
  }), [filtered, groups])

  // === selection ===

  const importable = filtered.filter((c) => !c.already_imported)
  const allSel = allSelected(importable.map((c) => c.session_uuid), selected)

  const importSelected = async () => {
    setImporting(true)
    try {
      await rpc('claude_import.run', { ids: selected }, t('settings.importDone'))
      await scan()
      await loadSessions()
    } finally {
      setImporting(false)
    }
  }

  const toggleGroup = (dir: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev)
      if (next.has(dir)) next.delete(dir)
      else next.add(dir)
      return next
    })
  }

  const toggleImported = (dir: string) => {
    setExpandedImported((prev) => {
      const next = new Set(prev)
      if (next.has(dir)) next.delete(dir)
      else next.add(dir)
      return next
    })
  }

  // === filter tabs ===

  const filterTabs: { key: FilterKey; label: string; count: number }[] = [
    { key: 'all', label: t('settings.filterAll'), count: stats.total },
    { key: 'pending', label: t('settings.filterPending'), count: stats.pending },
    { key: 'imported', label: t('settings.filterImported'), count: stats.imported },
  ]

  // === render ===

  const renderRow = (c: ConvSummary) => {
    const disabled = c.already_imported
    const checked = selected.includes(c.session_uuid)
    return (
      <label
        key={c.session_uuid}
        className={`${styles.row} ${disabled ? styles.rowDisabled : ''}`}
      >
        <input
          type="checkbox"
          disabled={disabled}
          checked={checked}
          onChange={() => setSelected((s) => toggleId(s, c.session_uuid))}
          className={styles.checkbox}
        />
        <div className={styles.rowContent}>
          <span className={styles.rowTitle}>
            {c.title || c.first_message || c.session_uuid}
          </span>
          <span className={styles.rowMeta}>
            <span className={styles.metaItem}>{c.message_count} 条消息</span>
            <span className={styles.metaSep} />
            <span className={styles.metaItem}>{c.model ?? 'unknown'}</span>
            <span className={styles.metaSep} />
            <span className={styles.metaItem}>{c.started_at.slice(0, 10)}</span>
            {disabled && (
              <>
                <span className={styles.metaSep} />
                <span className={styles.importedBadge}>{t('settings.imported')}</span>
              </>
            )}
          </span>
        </div>
      </label>
    )
  }

  return (
    <div className={styles.wrap}>
      {/* ── Source Picker ── */}
      <div className={styles.sourceRow}>
        {SOURCES.map((s) => (
          <button
            key={s.key}
            className={`${styles.sourceTab} ${source === s.key ? styles.sourceTabActive : ''}`}
            disabled={!s.available}
            onClick={() => s.available && setSource(s.key)}
            title={!s.available ? t('settings.comingSoon') : undefined}
          >
            {t(s.labelKey)}
            {!s.available && <span className={styles.comingSoon}>{t('settings.comingSoon')}</span>}
          </button>
        ))}
      </div>

      {source !== 'claude' ? (
        <div className={styles.placeholderBox}>
          <span className={styles.placeholderTitle}>{t(s.labelKey)}</span>
          <span className={styles.placeholderSub}>{t('settings.importComingSoon')}</span>
        </div>
      ) : (
        <>
      {/* ── Stats Cards ── */}
      <div className={styles.statsRow}>
        <div className={styles.statCard}>
          <div className={styles.statIcon}>
            <IconMessageSquare size={14} />
          </div>
          <div className={styles.statBody}>
            <span className={styles.statValue}>{stats.total}</span>
            <span className={styles.statLabel}>{t('settings.importStatsFound')}</span>
          </div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statIcon}>
            <IconDownload size={14} />
          </div>
          <div className={styles.statBody}>
            <span className={styles.statValue}>{stats.pending}</span>
            <span className={styles.statLabel}>{t('settings.importStatsPending')}</span>
          </div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statIcon}>
            <IconCheck size={14} />
          </div>
          <div className={styles.statBody}>
            <span className={styles.statValue}>{stats.imported}</span>
            <span className={styles.statLabel}>{t('settings.importStatsImported')}</span>
          </div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statIcon}>
            <IconFolder size={14} />
          </div>
          <div className={styles.statBody}>
            <span className={styles.statValue}>{stats.projects}</span>
            <span className={styles.statLabel}>{t('settings.importStatsProjects')}</span>
          </div>
        </div>
      </div>

      {/* ── Search Bar ── */}
      <div className={styles.searchWrap}>
        <IconSearch size={14} className={styles.searchIcon} />
        <input
          className={styles.searchInput}
          placeholder={t('settings.searchConversations')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button className={styles.scanBtn} onClick={scan} disabled={scanning}>
          {scanning ? <IconLoader size={13} /> : <IconRefresh size={13} />}
          {t('settings.rescan')}
        </button>
      </div>

      {/* ── Filter Tabs + Import Button ── */}
      <div className={styles.filterRow}>
        <div className={styles.filterTabs}>
          {filterTabs.map((tab) => (
            <button
              key={tab.key}
              className={`${styles.filterTab} ${filter === tab.key ? styles.filterTabActive : ''}`}
              onClick={() => setFilter(tab.key)}
            >
              {tab.label}
              <span className={`${styles.filterCount} ${filter === tab.key ? styles.filterCountActive : ''}`}>
                {tab.count}
              </span>
            </button>
          ))}
        </div>
        {selected.length > 0 && (
          <button
            className={styles.importBtn}
            onClick={importSelected}
            disabled={importing}
          >
            {importing ? <IconLoader size={13} /> : <IconDownload size={13} />}
            {t('settings.importSelected')} ({selected.length})
          </button>
        )}
      </div>

      {/* ── Conversation List ── */}
      {filtered.length === 0 && (
        <div className={styles.empty}>
          {scanning ? t('settings.scanning') : t('settings.noConversations')}
        </div>
      )}

      {Object.entries(groups).map(([dir, items]) => {
        const groupOpen = expandedGroups.has(dir)
        const pendingItems = items.filter((c) => !c.already_imported)
        const importedItems = items.filter((c) => c.already_imported)
        const showImported = importedItems.length > 0
        const importedOpen = expandedImported.has(dir)

        return (
          <div key={dir} className={styles.group}>
            <div className={styles.groupHeader} onClick={() => toggleGroup(dir)}>
              <IconFolder size={13} className={styles.groupIcon} />
              <span className={styles.groupLabel}>{dir}</span>
              <span className={styles.groupCount}>{items.length}</span>
              <IconChevronDown
                size={12}
                className={`${styles.groupChevron} ${groupOpen ? styles.groupChevronOpen : ''}`}
              />
            </div>
            {groupOpen && (
              <div className={styles.groupBody}>
                {pendingItems.map(renderRow)}
                {showImported && (
                  <div className={styles.importedSection}>
                    <button
                      className={styles.importedToggle}
                      onClick={() => toggleImported(dir)}
                    >
                      <IconChevronDown
                        size={10}
                        className={`${styles.importedChevron} ${importedOpen ? styles.importedChevronOpen : ''}`}
                      />
                      {t('settings.importedCollapsed', { n: String(importedItems.length) })}
                    </button>
                    {importedOpen && (
                      <div className={styles.importedBody}>
                        {importedItems.map(renderRow)}
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>
        )
      })}
        </>
      )}
    </div>
  )
}

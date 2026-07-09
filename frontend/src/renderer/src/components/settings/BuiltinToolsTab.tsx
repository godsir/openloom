import { useState, useEffect, useCallback, useMemo } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { useLocale } from '../../i18n'
import { IconChevronDown, IconSearch } from '../../utils/icons'
import Select from '../shared/Select'
import styles from './BuiltinToolsTab.module.css'

interface ToolPrefs {
  shell_default_timeout_secs: number
  shell_max_timeout_secs: number
  file_read_max_output_kb: number
  web_search_engine: string
  web_search_max_results: number
  web_fetch_max_chars: number
  process_wait_max_timeout_secs: number
  monitor_default_timeout_ms: number
}

interface ConfigDef {
  key: string
  labelKey: string
  type: 'slider' | 'select'
  min?: number
  max?: number
  step?: number
  unit?: string
  options?: { value: string; label: string }[]
  msToSec?: boolean
}

interface ToolDef {
  name: string
  descKey: string
  configs?: ConfigDef[]
  category: string
}

interface CategoryDef {
  id: string
  labelKey: string
}

function buildTools(): ToolDef[] {
  return [
    { name: 'shell', descKey: 'bt.shell', category: 'files', configs: [
      { key: 'shell_default_timeout_secs', labelKey: 'bt.shellDefaultTimeout', type: 'slider', min: 10, max: 300, step: 5, unit: 's' },
      { key: 'shell_max_timeout_secs', labelKey: 'bt.shellMaxTimeout', type: 'slider', min: 60, max: 600, step: 10, unit: 's' },
    ]},
    { name: 'file_list', descKey: 'bt.file_list', category: 'files' },
    { name: 'file_read', descKey: 'bt.file_read', category: 'files', configs: [
      { key: 'file_read_max_output_kb', labelKey: 'bt.fileReadMaxKb', type: 'slider', min: 8, max: 512, step: 8, unit: 'KB' },
    ]},
    { name: 'file_write', descKey: 'bt.file_write', category: 'files' },
    { name: 'file_edit', descKey: 'bt.file_edit', category: 'files' },
    { name: 'file_delete', descKey: 'bt.file_delete', category: 'files' },
    { name: 'file_glob', descKey: 'bt.file_glob', category: 'files' },
    { name: 'file_find', descKey: 'bt.file_find', category: 'files' },
    { name: 'content_search', descKey: 'bt.content_search', category: 'files' },
    { name: 'web_search', descKey: 'bt.web_search', category: 'web', configs: [
      { key: 'web_search_engine', labelKey: 'bt.webSearchEngine', type: 'select', options: [
        { value: 'duckduckgo_lite', label: 'DuckDuckGo' },
        { value: 'brave', label: 'Brave Search' },
        { value: 'searxng', label: 'SearXNG' },
      ]},
      { key: 'web_search_max_results', labelKey: 'bt.webSearchMaxResults', type: 'slider', min: 1, max: 10 },
    ]},
    { name: 'web_fetch', descKey: 'bt.web_fetch', category: 'web', configs: [
      { key: 'web_fetch_max_chars', labelKey: 'bt.webFetchMaxChars', type: 'slider', min: 1000, max: 20000, step: 500, unit: 'chars' },
    ]},
    { name: 'process_spawn', descKey: 'bt.process_spawn', category: 'processes' },
    { name: 'process_kill', descKey: 'bt.process_kill', category: 'processes' },
    { name: 'process_stdin', descKey: 'bt.process_stdin', category: 'processes' },
    { name: 'process_list', descKey: 'bt.process_list', category: 'processes' },
    { name: 'process_wait', descKey: 'bt.process_wait', category: 'processes', configs: [
      { key: 'process_wait_max_timeout_secs', labelKey: 'bt.processWaitMaxTimeout', type: 'slider', min: 60, max: 7200, step: 60, unit: 's' },
    ]},
    { name: 'process_peek', descKey: 'bt.process_peek', category: 'processes' },
    { name: 'monitor', descKey: 'bt.monitor', category: 'monitors', configs: [
      { key: 'monitor_default_timeout_ms', labelKey: 'bt.monitorDefaultTimeout', type: 'slider', min: 60, max: 1800, step: 30, unit: 's', msToSec: true },
    ]},
    { name: 'monitor_list', descKey: 'bt.monitor_list', category: 'monitors' },
    { name: 'monitor_kill', descKey: 'bt.monitor_kill', category: 'monitors' },
    { name: 'monitor_wait', descKey: 'bt.monitor_wait', category: 'monitors' },
    { name: 'monitor_peek', descKey: 'bt.monitor_peek', category: 'monitors' },
    { name: 'memory_search', descKey: 'bt.memory_search', category: 'system' },
    { name: 'todo_write', descKey: 'bt.todo_write', category: 'system' },
    { name: 'todo_list', descKey: 'bt.todo_list', category: 'system' },
    { name: 'schedule_reminder', descKey: 'bt.schedule_reminder', category: 'system' },
    { name: 'system_info', descKey: 'bt.system_info', category: 'system' },
    { name: 'token_usage', descKey: 'bt.token_usage', category: 'system' },
    { name: 'use_skill', descKey: 'bt.use_skill', category: 'system' },
    { name: 'ask_user', descKey: 'bt.ask_user', category: 'system' },
  ]
}

function buildCategories(): CategoryDef[] {
  return [
    { id: 'files', labelKey: 'bt.category_files' },
    { id: 'web', labelKey: 'bt.category_web' },
    { id: 'processes', labelKey: 'bt.category_processes' },
    { id: 'monitors', labelKey: 'bt.category_monitors' },
    { id: 'system', labelKey: 'bt.category_system' },
  ]
}

function renderTool(
  tool: ToolDef,
  t: (k: string) => string,
  expandedTools: Set<string>,
  toggleTool: (name: string) => void,
  getPref: (key: string) => number,
  setPref: (key: string, val: string | number) => Promise<void>,
) {
  const open = expandedTools.has(tool.name)
  const hasConfig = tool.configs && tool.configs.length > 0

  return (
    <div key={tool.name} className={styles.toolItem}>
      <div className={styles.toolHeader} onClick={() => toggleTool(tool.name)}>
        <span className={`${styles.configDot} ${hasConfig ? styles.configDotActive : styles.configDotNone}`} />
        <span className={styles.toolName}>{tool.name}</span>
        <span className={styles.toolDesc}>{t(tool.descKey)}</span>
        <span className={`${styles.toolBadge} ${hasConfig ? styles.toolBadgeConfig : styles.toolBadgeDefault}`}>
          {hasConfig ? t('bt.configurable') : t('bt.systemDefault')}
        </span>
        <IconChevronDown size={12} className={`${styles.toolChevron} ${open ? styles.toolChevronOpen : ''}`} />
      </div>
      {open && (
        <div className={styles.toolBody}>
          {!hasConfig && (
            <span className={styles.noConfig}>{t('bt.noConfig')}</span>
          )}
          {tool.configs?.map(cfg => {
            const rawVal = getPref(cfg.key)
            const displayVal = cfg.msToSec ? Math.round(rawVal / 1000) : rawVal
            return (
              <div key={cfg.key} className={styles.configField}>
                <div className={styles.configLabelRow}>
                  <span className={styles.configLabel}>{t(cfg.labelKey)}</span>
                  <span className={styles.configCurrent}>
                    {displayVal}{cfg.unit ? cfg.unit : ''}
                  </span>
                </div>
                <div className={styles.configControl}>
                  {cfg.type === 'select' ? (
                    <Select
                      value={String(rawVal)}
                      options={cfg.options?.map(o => ({ value: o.value, label: o.label })) || []}
                      onChange={(v) => setPref(cfg.key, v)}
                      variant="form"
                    />
                  ) : (
                    <>
                      <input
                        type="range"
                        className={styles.configSlider}
                        value={displayVal}
                        min={cfg.min}
                        max={cfg.max}
                        step={cfg.step ?? 1}
                        onChange={e => setPref(cfg.key, Number(e.target.value))}
                      />
                      <input
                        type="number"
                        className={styles.configInput}
                        value={displayVal}
                        min={cfg.min}
                        max={cfg.max}
                        step={cfg.step ?? 1}
                        onChange={e => {
                          const v = Number(e.target.value)
                          if (!isNaN(v)) setPref(cfg.key, v)
                        }}
                      />
                    </>
                  )}
                </div>
                {cfg.min !== undefined && cfg.max !== undefined && (
                  <div className={styles.configRange}>{cfg.min} — {cfg.max}</div>
                )}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

export default function BuiltinToolsTab() {
  const { t } = useLocale()
  const allTools = useMemo(() => buildTools(), [])
  const categories = useMemo(() => buildCategories(), [])

  const [prefs, setPrefs] = useState<ToolPrefs | null>(null)
  const [expandedTools, setExpandedTools] = useState<Set<string>>(new Set())
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set(categories.map(c => c.id)))
  const [search, setSearch] = useState('')
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    loomRpc<ToolPrefs>('config.get_tool_prefs').then(p => {
      setPrefs(p)
      setLoaded(true)
    }).catch(() => setLoaded(true))
  }, [])

  const toggleTool = (name: string) => {
    setExpandedTools(prev => {
      const next = new Set(prev)
      if (next.has(name)) next.delete(name)
      else next.add(name)
      return next
    })
  }

  const toggleGroup = (id: string) => {
    setExpandedGroups(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const getPref = (key: string): number => {
    if (!prefs) return 0
    return (prefs as any)[key] ?? 0
  }

  const setPref = useCallback(async (key: string, val: string | number) => {
    const next: Partial<ToolPrefs> = {}
    const allConfigs = allTools.flatMap(tl => tl.configs || [])
    const cfg = allConfigs.find(c => c.key === key)
    if (cfg?.msToSec && typeof val === 'number') {
      (next as any)[key] = val * 1000
    } else {
      (next as any)[key] = val
    }
    try {
      await loomRpc('config.set_tool_prefs', next)
      setPrefs(prev => prev ? { ...prev, ...next } : prev)
    } catch {}
  }, [allTools])

  const searchLower = search.toLowerCase().trim()
  const filteredTools = searchLower
    ? allTools.filter(tool =>
      tool.name.toLowerCase().includes(searchLower) ||
      t(tool.descKey).toLowerCase().includes(searchLower)
    )
    : allTools

  if (!loaded) return <p style={{ fontSize: 13, color: 'var(--text-muted)', padding: 12 }}>{t('common.loading')}</p>

  return (
    <div>
      <div className={styles.searchWrap}>
        <IconSearch size={14} className={styles.searchIcon} />
        <input
          className={styles.searchInput}
          placeholder={t('common.search')}
          value={search}
          onChange={e => setSearch(e.target.value)}
        />
      </div>

      {searchLower ? (
        filteredTools.length === 0 ? (
          <p className={styles.noResults}>{t('bt.noResults')}</p>
        ) : (
          filteredTools.map(item => renderTool(item, t, expandedTools, toggleTool, getPref, setPref))
        )
      ) : (
        categories.map(cat => {
          const catTools = filteredTools.filter(tl => tl.category === cat.id)
          if (catTools.length === 0) return null
          const catOpen = expandedGroups.has(cat.id)
          return (
            <div key={cat.id} className={styles.group}>
              <div className={styles.groupHeader} onClick={() => toggleGroup(cat.id)}>
                <span className={styles.groupLabel}>{t(cat.labelKey)}</span>
                <span className={styles.groupCount}>{catTools.length}</span>
                <IconChevronDown size={12} className={`${styles.groupChevron} ${catOpen ? styles.groupChevronOpen : ''}`} />
              </div>
              {catOpen && (
                <div className={styles.groupBody}>
                  {catTools.map(item => renderTool(item, t, expandedTools, toggleTool, getPref, setPref))}
                </div>
              )}
            </div>
          )
        })
      )}
    </div>
  )
}

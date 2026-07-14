import { useState, useEffect, useCallback } from 'react'
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
  searxng_url?: string | null
  web_search_api_key?: string | null
  http_proxy?: string | null
  web_fetch_max_chars: number
  process_wait_max_timeout_secs: number
  monitor_default_timeout_ms: number
}

interface ToolInfo {
  name: string
  description: string
}

interface ConfigDef {
  key: string
  labelKey: string
  type: 'slider' | 'select' | 'text'
  min?: number
  max?: number
  step?: number
  unit?: string
  options?: { value: string; label: string }[]
  msToSec?: boolean
  placeholder?: string
  visibleWhen?: { key: string; values: string[] }
  inline?: boolean
  inlineFor?: string[]
}

interface ToolDef {
  name: string
  description: string
  configs?: ConfigDef[]
  category: string
}

interface CategoryDef {
  id: string
  labelKey: string
}

/** Per-tool config definitions — keys determine which UI controls to render.
 *  Tool names come from the backend via tools.list; this map provides the
 *  editable preference controls for tools that have configurable settings. */
const TOOL_CONFIGS: Record<string, ConfigDef[]> = {
  shell: [
    { key: 'shell_default_timeout_secs', labelKey: 'bt.shellDefaultTimeout', type: 'slider', min: 10, max: 300, step: 5, unit: 's' },
    { key: 'shell_max_timeout_secs', labelKey: 'bt.shellMaxTimeout', type: 'slider', min: 60, max: 600, step: 10, unit: 's' },
  ],
  file_read: [
    { key: 'file_read_max_output_kb', labelKey: 'bt.fileReadMaxKb', type: 'slider', min: 8, max: 512, step: 8, unit: 'KB' },
  ],
  web_search: [
    { key: 'web_search_engine', labelKey: 'bt.webSearchEngine', type: 'select', options: [
      { value: 'duckduckgo_lite', label: 'DuckDuckGo' },
      { value: 'brave', label: 'Brave Search' },
      { value: 'google', label: 'Google' },
      { value: 'bing', label: 'Bing' },
      { value: 'tavily', label: 'Tavily' },
      { value: 'serper', label: 'Serper' },
      { value: 'searxng', label: 'SearXNG' },
    ]},
    { key: 'searxng_url', inline: true, labelKey: 'bt.searxngUrl', type: 'text', placeholder: 'https://searx.example.com', visibleWhen: { key: 'web_search_engine', values: ['searxng'] }, inlineFor: ['searxng'] },
    { key: 'web_search_api_key', inline: true, labelKey: 'bt.webSearchApiKey', type: 'text', placeholder: 'api-key...', visibleWhen: { key: 'web_search_engine', values: ['brave', 'google', 'bing', 'tavily', 'serper'] }, inlineFor: ['brave', 'google', 'bing', 'tavily', 'serper'] },
    { key: 'web_search_max_results', labelKey: 'bt.webSearchMaxResults', type: 'slider', min: 1, max: 10 },
  ],
  web_fetch: [
    { key: 'web_fetch_max_chars', labelKey: 'bt.webFetchMaxChars', type: 'slider', min: 1000, max: 20000, step: 500, unit: 'chars' },
  ],
  process_wait: [
    { key: 'process_wait_max_timeout_secs', labelKey: 'bt.processWaitMaxTimeout', type: 'slider', min: 60, max: 7200, step: 60, unit: 's' },
  ],
  monitor: [
    { key: 'monitor_default_timeout_ms', labelKey: 'bt.monitorDefaultTimeout', type: 'slider', min: 60, max: 1800, step: 30, unit: 's', msToSec: true },
  ],
}

/** Assign a category based on tool name prefix.  Categories match the
 *  i18n keys bt.category_*. New tools fall into `system` unless they
 *  match a known prefix. */
function toolCategory(name: string): string {
  if (name.startsWith('file_')) return 'files'
  if (name === 'shell' || name === 'content_search') return 'files'
  if (name.startsWith('web_')) return 'web'
  if (name.startsWith('process_')) return 'processes'
  if (name.startsWith('monitor')) return 'monitors'
  return 'system'
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

// ── render helpers ──

function renderConfig(
  cfg: ConfigDef,
  rawVal: number | string,
  displayVal: number | string,
  t: (k: string) => string,
  setPref: (key: string, val: string | number) => Promise<void>,
  getPref: (key: string) => number | string,
  inlineConfigs?: ConfigDef[],
) {
  const filteredInlines = inlineConfigs?.filter(ic => {
    if (!ic.visibleWhen) return false
    const depVal = String(getPref(ic.visibleWhen.key))
    return ic.visibleWhen.values.includes(depVal)
  })

  return (
    <div key={cfg.key} className={styles.configField}>
      <div className={styles.configLabelRow}>
        <span className={styles.configLabel}>{t(cfg.labelKey)}</span>
        {(cfg.type !== 'select' || !filteredInlines?.length) && (
          <span className={styles.configCurrent}>
            {displayVal}{cfg.unit ? cfg.unit : ''}
          </span>
        )}
      </div>
      <div className={styles.configControl}>
        {cfg.type === 'select' ? (
          <Select
            value={String(rawVal)}
            options={cfg.options?.map(o => ({ value: o.value, label: o.label })) || []}
            onChange={(v) => setPref(cfg.key, v)}
            variant="form"
          />
        ) : cfg.type === 'text' ? (
          <input
            type="text"
            className={styles.configText}
            value={rawVal as string || ''}
            placeholder={cfg.placeholder}
            onChange={e => setPref(cfg.key, e.target.value)}
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
              onChange={e => { const v = Number(e.target.value); if (!isNaN(v)) setPref(cfg.key, v) }}
            />
          </>
        )}
        {/* inline 字段跟在 select 同行 */}
        {cfg.type === 'select' && filteredInlines?.map(ic => {
          const v = getPref(ic.key)
          return (
            <input
              key={ic.key}
              type="text"
              className={styles.configText}
              value={typeof v === 'string' ? v : ''}
              placeholder={ic.placeholder}
              onChange={e => setPref(ic.key, e.target.value)}
            />
          )
        })}
      </div>
      {cfg.min !== undefined && cfg.max !== undefined && cfg.type !== 'select' && (
        <div className={styles.configRange}>{cfg.min} — {cfg.max}</div>
      )}
    </div>
  )
}

function renderTool(
  tool: ToolDef,
  t: (k: string) => string,
  expandedTools: Set<string>,
  toggleTool: (name: string) => void,
  getPref: (key: string) => number | string,
  setPref: (key: string, val: string | number) => Promise<void>,
) {
  const open = expandedTools.has(tool.name)
  const hasConfig = tool.configs && tool.configs.length > 0
  // 收集 inline 字段
  const inlineConfigs = (tool.configs || []).filter(c => c.inline)
  // 非 inline 的顶级字段
  const topConfigs = (tool.configs || []).filter(c => !c.inline)

  return (
    <div key={tool.name} className={styles.toolItem}>
      <div className={styles.toolHeader} onClick={() => toggleTool(tool.name)}>
        <span className={`${styles.configDot} ${hasConfig ? styles.configDotActive : styles.configDotNone}`} />
        <span className={styles.toolName}>{tool.name}</span>
        <span className={styles.toolDesc}>{tool.description}</span>
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
          {topConfigs.filter(cfg => {
            if (!cfg.visibleWhen) return true
            const depVal = String(getPref(cfg.visibleWhen.key))
            return cfg.visibleWhen.values.includes(depVal)
          }).map(cfg => {
            const rawVal = getPref(cfg.key)
            const displayVal = cfg.msToSec && typeof rawVal === 'number' ? Math.round(rawVal / 1000) : rawVal
            return renderConfig(cfg, rawVal, displayVal, t, setPref, getPref, inlineConfigs)
          })}
        </div>
      )}
    </div>
  )
}

export default function BuiltinToolsTab() {
  const { t } = useLocale()
  const [serverTools, setServerTools] = useState<ToolInfo[]>([])
  const [prefs, setPrefs] = useState<ToolPrefs | null>(null)
  const [expandedTools, setExpandedTools] = useState<Set<string>>(new Set())
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set(['files', 'web', 'processes', 'monitors', 'system']))
  const [search, setSearch] = useState('')
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    Promise.all([
      loomRpc<{ tools: ToolInfo[] }>('tools.list').then(r => setServerTools(r.tools || [])),
      loomRpc<ToolPrefs>('config.get_tool_prefs').then(p => setPrefs(p)),
    ]).catch(() => {}).finally(() => setLoaded(true))
  }, [])

  // Merge backend tool info with local config definitions
  const allTools: ToolDef[] = (serverTools || []).map(ti => ({
    name: ti.name,
    description: ti.description,
    configs: TOOL_CONFIGS[ti.name],
    category: toolCategory(ti.name),
  }))
  const categories = buildCategories()

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

  const getPref = (key: string): number | string => {
    if (!prefs) return 0
    return (prefs as any)[key] ?? 0
  }

  const setPref = useCallback(async (key: string, val: string | number) => {
    const next: Partial<ToolPrefs> = {}
    const allConfigs = Object.values(TOOL_CONFIGS).flat()
    const cfg = allConfigs.find(c => c.key === key)
    if (cfg?.msToSec && typeof val === 'number') {
      (next as any)[key] = val * 1000
    } else if (cfg?.type === 'text' && typeof val === 'string') {
      (next as any)[key] = val || null
    } else {
      (next as any)[key] = val
    }
    try {
      await loomRpc('config.set_tool_prefs', next)
      setPrefs(prev => prev ? { ...prev, ...next } : prev)
    } catch {}
  }, [])

  const searchLower = search.toLowerCase().trim()
  const filteredTools = searchLower
    ? allTools.filter(tool =>
      tool.name.toLowerCase().includes(searchLower) ||
      (tool.description || '').toLowerCase().includes(searchLower)
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

import { useState, useEffect, useCallback } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { useLocale } from '../../i18n'
import { IconChevronDown } from '../../utils/icons'
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

interface ToolDef {
  name: string
  descKey: string
  configs?: ConfigDef[]
}

interface ConfigDef {
  key: string
  labelKey: string
  type: 'number' | 'select'
  min?: number
  max?: number
  options?: { value: string; label: string }[]
  msToSec?: boolean
}

function buildTools(t: (k: string) => string): ToolDef[] {
  return [
    { name: 'shell', descKey: 'bt.shell', configs: [
      { key: 'shell_default_timeout_secs', labelKey: 'bt.shellDefaultTimeout', type: 'number', min: 10, max: 300 },
      { key: 'shell_max_timeout_secs', labelKey: 'bt.shellMaxTimeout', type: 'number', min: 60, max: 600 },
    ]},
    { name: 'file_list', descKey: 'bt.file_list' },
    { name: 'file_read', descKey: 'bt.file_read', configs: [
      { key: 'file_read_max_output_kb', labelKey: 'bt.fileReadMaxKb', type: 'number', min: 8, max: 512 },
    ]},
    { name: 'file_write', descKey: 'bt.file_write' },
    { name: 'file_edit', descKey: 'bt.file_edit' },
    { name: 'file_delete', descKey: 'bt.file_delete' },
    { name: 'file_glob', descKey: 'bt.file_glob' },
    { name: 'file_find', descKey: 'bt.file_find' },
    { name: 'content_search', descKey: 'bt.content_search' },
    { name: 'web_search', descKey: 'bt.web_search', configs: [
      { key: 'web_search_engine', labelKey: 'bt.webSearchEngine', type: 'select', options: [
        { value: 'duckduckgo_lite', label: 'DuckDuckGo' },
        { value: 'brave', label: 'Brave Search' },
        { value: 'searxng', label: 'SearXNG' },
      ]},
      { key: 'web_search_max_results', labelKey: 'bt.webSearchMaxResults', type: 'number', min: 1, max: 10 },
    ]},
    { name: 'web_fetch', descKey: 'bt.web_fetch', configs: [
      { key: 'web_fetch_max_chars', labelKey: 'bt.webFetchMaxChars', type: 'number', min: 1000, max: 20000 },
    ]},
    { name: 'memory_search', descKey: 'bt.memory_search' },
    { name: 'use_skill', descKey: 'bt.use_skill' },
    { name: 'todo_write', descKey: 'bt.todo_write' },
    { name: 'todo_list', descKey: 'bt.todo_list' },
    { name: 'schedule_reminder', descKey: 'bt.schedule_reminder' },
    { name: 'system_info', descKey: 'bt.system_info' },
    { name: 'token_usage', descKey: 'bt.token_usage' },
    { name: 'ask_user', descKey: 'bt.ask_user' },
    { name: 'process_spawn', descKey: 'bt.process_spawn' },
    { name: 'process_kill', descKey: 'bt.process_kill' },
    { name: 'process_stdin', descKey: 'bt.process_stdin' },
    { name: 'process_list', descKey: 'bt.process_list' },
    { name: 'process_wait', descKey: 'bt.process_wait', configs: [
      { key: 'process_wait_max_timeout_secs', labelKey: 'bt.processWaitMaxTimeout', type: 'number', min: 60, max: 7200 },
    ]},
    { name: 'process_peek', descKey: 'bt.process_peek' },
    { name: 'monitor', descKey: 'bt.monitor', configs: [
      { key: 'monitor_default_timeout_ms', labelKey: 'bt.monitorDefaultTimeout', type: 'number', min: 60, max: 1800, msToSec: true },
    ]},
    { name: 'monitor_list', descKey: 'bt.monitor_list' },
    { name: 'monitor_kill', descKey: 'bt.monitor_kill' },
    { name: 'monitor_wait', descKey: 'bt.monitor_wait' },
    { name: 'monitor_peek', descKey: 'bt.monitor_peek' },
  ]
}

export default function BuiltinToolsTab() {
  const { t } = useLocale()
  const tools = buildTools(t)
  const [prefs, setPrefs] = useState<ToolPrefs | null>(null)
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    loomRpc<ToolPrefs>('config.get_tool_prefs').then(p => {
      setPrefs(p)
      setLoaded(true)
    }).catch(() => setLoaded(true))
  }, [])

  const toggleExpand = (name: string) => {
    setExpanded(prev => {
      const next = new Set(prev)
      if (next.has(name)) next.delete(name)
      else next.add(name)
      return next
    })
  }

  const getPref = (key: string): number => {
    if (!prefs) return 0
    return (prefs as any)[key] ?? 0
  }

  const setPref = useCallback(async (key: string, val: string | number) => {
    const next: Partial<ToolPrefs> = {}
    const toolCfg = tools.flatMap(t => t.configs || []).find(c => c.key === key)
    if (toolCfg?.msToSec && typeof val === 'number') {
      (next as any)[key] = val * 1000
    } else {
      (next as any)[key] = val
    }
    try {
      await loomRpc('config.set_tool_prefs', next)
      setPrefs(prev => prev ? { ...prev, ...next } : prev)
    } catch {}
  }, [tools])

  if (!loaded) return <p>{t('common.loading')}</p>

  return (
    <div className={styles.list}>
      {tools.map(tool => {
        const open = expanded.has(tool.name)
        const hasConfig = tool.configs && tool.configs.length > 0
        return (
          <div key={tool.name} className={styles.toolItem}>
            <div className={styles.toolHeader} onClick={() => toggleExpand(tool.name)}>
              <div style={{ display: 'flex', alignItems: 'baseline', flex: 1 }}>
                <span className={styles.toolName}>{tool.name}</span>
                <span className={styles.toolDesc}>{t(tool.descKey)}</span>
              </div>
              <IconChevronDown size={14} className={`${styles.toolChevron} ${open ? styles.toolChevronOpen : ''}`} />
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
                    <div key={cfg.key} className={styles.configRow}>
                      <span className={styles.configLabel}>{t(cfg.labelKey)}</span>
                      <div className={styles.configValue}>
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
                              type="number"
                              className={styles.configInput}
                              value={displayVal}
                              min={cfg.min}
                              max={cfg.max}
                              onChange={e => {
                                const v = Number(e.target.value)
                                if (!isNaN(v)) setPref(cfg.key, v)
                              }}
                            />
                            {cfg.min !== undefined && cfg.max !== undefined && (
                              <span className={styles.configRange}>{cfg.min}—{cfg.max}</span>
                            )}
                          </>
                        )}
                      </div>
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}

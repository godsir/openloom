import { useState, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useLocale, t as _t } from '../../i18n'
import styles from '../shared/SettingsModal.module.css'

interface McpTool {
  name: string
  description?: string
}

interface LspLangInfo {
  language: string
  command: string
  available: boolean
  running: boolean
  install_hint?: { manager: string; command: string }
  uninstall_command?: string
}

interface McpServerConfig {
  name: string
  transport: 'stdio' | 'http'
  command: string
  args: string[]
  url: string | null
  headers: Record<string, string>
  env: Record<string, string>
  cwd: string | null
  startup_timeout_secs: number
  tool_timeout_secs: number
  enabled_tools: string[] | null
  disabled_tools: string[] | null
  autostart: boolean
  connected: boolean
}

const EMPTY_FORM: McpServerConfig = {
  name: '',
  transport: 'stdio',
  command: '',
  args: [],
  url: '',
  headers: {},
  env: {},
  cwd: '',
  startup_timeout_secs: 30,
  tool_timeout_secs: 60,
  enabled_tools: null,
  disabled_tools: null,
  autostart: true,
  connected: false,
}

function parseKvLines(text: string): Record<string, string> {
  const out: Record<string, string> = {}
  for (const raw of text.split('\n')) {
    const line = raw.trim()
    if (!line) continue
    const eq = line.indexOf('=')
    if (eq <= 0) continue
    out[line.slice(0, eq).trim()] = line.slice(eq + 1).trim()
  }
  return out
}
function kvToText(obj: Record<string, string>): string {
  return Object.entries(obj).map(([k, v]) => `${k}=${v}`).join('\n')
}
function parseCsv(text: string): string[] {
  return text.split(/[\n,]/).map((s) => s.trim()).filter(Boolean)
}

/* ─── MCP Tab ─── */

function McpTab() {
  const { t } = useLocale()
  const [configs, setConfigs] = useState<McpServerConfig[]>([])
  const [healthByName, setHealthByName] = useState<Record<string, boolean | null>>({})
  const [toolsByServer, setToolsByServer] = useState<Record<string, McpTool[]>>({})
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  // Editor state — null = closed, {} = new entry, populated = editing existing.
  const [editing, setEditing] = useState<McpServerConfig | null>(null)
  const [editingOriginalName, setEditingOriginalName] = useState<string | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const [busy, setBusy] = useState(false)

  const loadData = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [cfgRes, toolsRes] = await Promise.allSettled([
        loomRpc<{ configs: McpServerConfig[] }>('mcp.config.list'),
        loomRpc<{ tools: (McpTool & { server?: string })[] }>('mcp.list_tools'),
      ])

      let list: McpServerConfig[] = []
      if (cfgRes.status === 'fulfilled') {
        list = cfgRes.value.configs ?? []
        setConfigs(list)
      } else {
        setError(_t('mcp.loadConfigFailed', { reason: cfgRes.reason?.message || cfgRes.reason }))
      }

      // Health for currently connected servers.
      const health: Record<string, boolean | null> = {}
      await Promise.allSettled(
        list.filter((c) => c.connected).map(async (c) => {
          try {
            const res = await loomRpc<{ healthy: boolean }>('mcp.server_health', { name: c.name })
            health[c.name] = res.healthy
          } catch {
            health[c.name] = null
          }
        })
      )
      setHealthByName(health)

      if (toolsRes.status === 'fulfilled') {
        // Tool names are prefixed mcp__<server>__<tool>; bucket by server.
        const grouped: Record<string, McpTool[]> = {}
        for (const t of toolsRes.value.tools ?? []) {
          const m = /^mcp__([^_]+(?:_[^_]+)*?)__(.+)$/.exec(t.name)
          const server = m?.[1]
          const local = m?.[2] ?? t.name
          if (!server) continue
          if (!grouped[server]) grouped[server] = []
          grouped[server].push({ name: local, description: t.description })
        }
        setToolsByServer(grouped)
      }
    } catch (e: any) {
      setError(_t('mcp.loadFailed', { message: e.message || e }))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  const startCreate = () => {
    setEditing({ ...EMPTY_FORM })
    setEditingOriginalName(null)
  }
  const startEdit = (c: McpServerConfig) => {
    setEditing({ ...c, url: c.url ?? '', cwd: c.cwd ?? '' })
    setEditingOriginalName(c.name)
  }
  const cancelEdit = () => {
    setEditing(null)
    setEditingOriginalName(null)
  }

  const buildPayload = (cfg: McpServerConfig): Record<string, unknown> => ({
    name: cfg.name.trim(),
    transport: cfg.transport,
    command: cfg.command,
    args: cfg.args,
    url: cfg.url || null,
    headers: cfg.headers,
    env: cfg.env,
    cwd: cfg.cwd || null,
    startup_timeout_secs: cfg.startup_timeout_secs,
    tool_timeout_secs: cfg.tool_timeout_secs,
    enabled_tools: cfg.enabled_tools,
    disabled_tools: cfg.disabled_tools,
    autostart: cfg.autostart,
  })

  const handleSaveAndConnect = async () => {
    if (!editing || !editing.name.trim()) return
    setBusy(true)
    try {
      // If renaming an existing entry, drop the old row first.
      if (editingOriginalName && editingOriginalName !== editing.name.trim()) {
        await loomRpc('mcp.config.delete', { name: editingOriginalName }).catch(() => {})
      }
      await rpc('mcp.connect', { ...buildPayload(editing), persist: true },
        _t('mcp.connectedToast', { name: editing.name }))
      cancelEdit()
      await loadData()
    } catch (e: any) {
      setError(_t('mcp.connectFailed', { message: e.message || e }))
    } finally {
      setBusy(false)
    }
  }
  const handleSaveOnly = async () => {
    if (!editing || !editing.name.trim()) return
    setBusy(true)
    try {
      if (editingOriginalName && editingOriginalName !== editing.name.trim()) {
        await loomRpc('mcp.config.delete', { name: editingOriginalName }).catch(() => {})
      }
      await rpc('mcp.config.save', buildPayload(editing), _t('mcp.savedToast', { name: editing.name }))
      cancelEdit()
      await loadData()
    } catch (e: any) {
      setError(_t('mcp.saveFailed', { message: e.message || e }))
    } finally {
      setBusy(false)
    }
  }
  const handleConnectExisting = async (cfg: McpServerConfig) => {
    setBusy(true)
    try {
      await rpc('mcp.connect', { ...buildPayload(cfg), persist: true },
        _t('mcp.connectedToast', { name: cfg.name }))
      await loadData()
    } catch (e: any) {
      setError(_t('mcp.connectFailed', { message: e.message || e }))
    } finally {
      setBusy(false)
    }
  }
  const handleDisconnect = async (name: string) => {
    try {
      await rpc('mcp.disconnect', { name }, _t('mcp.disconnectedToast', { name }))
      await loadData()
    } catch { /* toast already shown */ }
  }
  const handleDelete = async (name: string) => {
    const ok = await useStore.getState().showConfirm(t('mcp.deleteConfirmTitle'), _t('mcp.deleteConfirmMessage', { name }), true)
    if (!ok) return
    try {
      await rpc('mcp.config.delete', { name }, _t('mcp.deletedToast', { name }))
      await loadData()
    } catch { /* toast already shown */ }
  }

  const handleRefresh = async () => {
    setRefreshing(true)
    await loadData()
    setRefreshing(false)
  }

  return (
    <>
      <div className={styles.aboutSection}>
        <div className={styles.sectionHeaderRow}>
          <h4 className={styles.sectionSubTitle}>{t('mcp.title')}</h4>
          {!editing && (
            <button className={styles.mcpAddBtn} onClick={startCreate}>
              {t('mcp.addServer')}
            </button>
          )}
        </div>
        {error && <p className={styles.toolsError}>{error}</p>}
        {loading ? (
          <p className={styles.toolsEmpty}>{t('common.loading')}</p>
        ) : (
          <>
            <div className={styles.mcpServerList}>
              {configs.length === 0 ? (
                <p className={styles.toolsEmpty}>{t('mcp.noServerConfig')}</p>
              ) : (
                configs.map((c) => {
                  const healthState = !c.connected
                    ? 'unknown'
                    : healthByName[c.name] === true
                      ? 'true'
                      : healthByName[c.name] === false
                        ? 'false'
                        : 'unknown'
                  const tools = toolsByServer[c.name] ?? []
                  return (
                    <div key={c.name} className={styles.mcpServerItem}>
                      <div className={styles.mcpServerHeader}>
                        <div className={styles.mcpServerNameRow}>
                          <span className={styles.mcpServerStatus} data-healthy={healthState} />
                          <span className={styles.mcpServerName}>{c.name}</span>
                          <span className={styles.mcpServerMeta}>
                            {c.transport.toUpperCase()}
                            {c.autostart && ' · autostart'}
                            {!c.connected && ` · ${t('mcp.disconnected')}`}
                          </span>
                        </div>
                        <div className={styles.mcpServerActions}>
                          {c.connected ? (
                            <button className={styles.mcpDisconnectBtn} onClick={() => handleDisconnect(c.name)}>
                              {t('mcp.disconnect')}
                            </button>
                          ) : (
                            <button className={styles.mcpDisconnectBtn} onClick={() => handleConnectExisting(c)}>
                              {t('mcp.connect')}
                            </button>
                          )}
                          <button className={styles.mcpDisconnectBtn} onClick={() => startEdit(c)}>
                            {t('common.edit')}
                          </button>
                          <button className={styles.mcpDisconnectBtn} onClick={() => handleDelete(c.name)}>
                            {t('common.delete')}
                          </button>
                        </div>
                      </div>
                      <div className={styles.mcpServerCmd}>
                        {c.transport === 'stdio'
                          ? `${c.command} ${c.args.join(' ')}`
                          : c.url || ''}
                      </div>
                      {tools.length > 0 && (
                        <div className={styles.toolsBadgeGrid}>
                          {tools.map((tool) => (
                            <span key={tool.name} className={styles.toolsBadge} title={tool.description}>
                              {tool.name}
                            </span>
                          ))}
                        </div>
                      )}
                    </div>
                  )
                })
              )}
            </div>

            {editing && (
              <McpEditor
                value={editing}
                onChange={setEditing}
                onCancel={cancelEdit}
                onSave={handleSaveOnly}
                onSaveAndConnect={handleSaveAndConnect}
                busy={busy}
                isEdit={editingOriginalName !== null}
              />
            )}
          </>
        )}
      </div>
    </>
  )
}

interface McpEditorProps {
  value: McpServerConfig
  onChange: (next: McpServerConfig) => void
  onCancel: () => void
  onSave: () => void
  onSaveAndConnect: () => void
  busy: boolean
  isEdit: boolean
}

function McpEditor({ value, onChange, onCancel, onSave, onSaveAndConnect, busy, isEdit }: McpEditorProps) {
  const { t } = useLocale()
  const v = value
  const set = (patch: Partial<McpServerConfig>) => onChange({ ...v, ...patch })

  return (
    <div className={styles.mcpAddForm}>
      <div className={styles.mcpFormRow}>
        <label className={styles.mcpFormLabel}>{t('mcp.name')}</label>
        <input
          className={styles.mcpFormInput}
          value={v.name}
          onChange={(e) => set({ name: e.target.value })}
          placeholder="server-name"
        />
      </div>
      <div className={styles.mcpFormRow}>
        <label className={styles.mcpFormLabel}>{t('mcp.transport')}</label>
        <div className={styles.mcpTransportToggle}>
          <button
            className={`${styles.mcpTransportBtn} ${v.transport === 'stdio' ? styles.mcpTransportActive : ''}`}
            onClick={() => set({ transport: 'stdio' })}
          >
            stdio
          </button>
          <button
            className={`${styles.mcpTransportBtn} ${v.transport === 'http' ? styles.mcpTransportActive : ''}`}
            onClick={() => set({ transport: 'http' })}
          >
            HTTP
          </button>
        </div>
      </div>

      {v.transport === 'stdio' ? (
        <>
          <div className={styles.mcpFormRow}>
            <label className={styles.mcpFormLabel}>{t('mcp.command')}</label>
            <input
              className={styles.mcpFormInput}
              value={v.command}
              onChange={(e) => set({ command: e.target.value })}
              placeholder="npx, node, python..."
            />
          </div>
          <div className={styles.mcpFormRow}>
            <label className={styles.mcpFormLabel}>{t('mcp.args')}</label>
            <textarea
              className={`${styles.mcpFormInput} ${styles.mcpFormTextarea}`}
              value={v.args.join('\n')}
              onChange={(e) => set({ args: parseCsv(e.target.value) })}
              placeholder={'-y\n@modelcontextprotocol/server-xxx'}
            />
          </div>
          <div className={styles.mcpFormRow}>
            <label className={styles.mcpFormLabel}>{t('mcp.workDir')}</label>
            <input
              className={styles.mcpFormInput}
              value={v.cwd ?? ''}
              onChange={(e) => set({ cwd: e.target.value })}
              placeholder="/path/to/cwd"
            />
          </div>
        </>
      ) : (
        <>
          <div className={styles.mcpFormRow}>
            <label className={styles.mcpFormLabel}>{t('mcp.url')}</label>
            <input
              className={styles.mcpFormInput}
              value={v.url ?? ''}
              onChange={(e) => set({ url: e.target.value })}
              placeholder="http://localhost:8080/sse"
            />
          </div>
          <div className={styles.mcpFormRow}>
            <label className={styles.mcpFormLabel}>{t('mcp.headers')}</label>
            <textarea
              className={`${styles.mcpFormInput} ${styles.mcpFormTextarea} ${styles.mcpFormTextareaLg}`}
              value={kvToText(v.headers)}
              onChange={(e) => set({ headers: parseKvLines(e.target.value) })}
              placeholder={'Authorization=Bearer xxx\nX-Custom=abc'}
            />
          </div>
        </>
      )}

      <div className={styles.mcpFormRow}>
        <label className={styles.mcpFormLabel}>{t('mcp.env')}</label>
        <textarea
          className={`${styles.mcpFormInput} ${styles.mcpFormTextarea}`}
          value={kvToText(v.env)}
          onChange={(e) => set({ env: parseKvLines(e.target.value) })}
          placeholder={'API_KEY=...'}
        />
      </div>

      <div className={`${styles.mcpFormRow} ${styles.mcpFormRowHorizontal}`}>
        <div className={styles.mcpFormFlexCell}>
          <label className={styles.mcpFormLabel}>{t('mcp.startupTimeout')}</label>
          <input
            className={styles.mcpFormInput}
            type="number"
            min={1}
            value={v.startup_timeout_secs}
            onChange={(e) => set({ startup_timeout_secs: Number(e.target.value) || 30 })}
          />
        </div>
        <div className={styles.mcpFormFlexCell}>
          <label className={styles.mcpFormLabel}>{t('mcp.toolTimeout')}</label>
          <input
            className={styles.mcpFormInput}
            type="number"
            min={1}
            value={v.tool_timeout_secs}
            onChange={(e) => set({ tool_timeout_secs: Number(e.target.value) || 60 })}
          />
        </div>
      </div>

      <div className={styles.mcpFormRow}>
        <label className={styles.mcpFormLabel}>{t('mcp.enabledTools')}</label>
        <textarea
          className={`${styles.mcpFormInput} ${styles.mcpFormTextarea} ${styles.mcpFormTextareaSm}`}
          value={(v.enabled_tools ?? []).join('\n')}
          onChange={(e) => {
            const list = parseCsv(e.target.value)
            set({ enabled_tools: list.length ? list : null })
          }}
          placeholder="tool_a, tool_b"
        />
      </div>
      <div className={styles.mcpFormRow}>
        <label className={styles.mcpFormLabel}>{t('mcp.disabledTools')}</label>
        <textarea
          className={`${styles.mcpFormInput} ${styles.mcpFormTextarea} ${styles.mcpFormTextareaSm}`}
          value={(v.disabled_tools ?? []).join('\n')}
          onChange={(e) => {
            const list = parseCsv(e.target.value)
            set({ disabled_tools: list.length ? list : null })
          }}
          placeholder="dangerous_tool"
        />
      </div>

      <div className={`${styles.mcpFormRow} ${styles.mcpFormRowCheckbox}`}>
        <input
          id="mcp-autostart"
          type="checkbox"
          checked={v.autostart}
          onChange={(e) => set({ autostart: e.target.checked })}
        />
        <label htmlFor="mcp-autostart" className={`${styles.mcpFormLabel} ${styles.mcpFormLabelClickable}`}>
          {t('mcp.autoReconnect')}
        </label>
      </div>

      <div className={styles.mcpFormActions}>
        <button className={styles.mcpCancelBtn} onClick={onCancel}>{t('common.cancel')}</button>
        <button
          className={styles.mcpCancelBtn}
          onClick={onSave}
          disabled={busy || !v.name.trim()}
        >
          {busy ? t('mcp.saving') : t('mcp.saveOnly')}
        </button>
        <button
          className={styles.mcpConnectBtn}
          onClick={onSaveAndConnect}
          disabled={busy || !v.name.trim()}
        >
          {busy ? t('mcp.connecting') : isEdit ? t('mcp.saveAndReconnect') : t('mcp.saveAndConnect')}
        </button>
      </div>
    </div>
  )
}

/* ─── LSP Tab ─── */

/** Simple SVG icon per language — consistent visual weight, no emoji fallback issues. */
function LangIcon({ lang }: { lang: string }) {
  const cls = styles.lspCardIconBox
  switch (lang) {
    case 'rust':       return <span className={cls} style={{background:'#dea584',color:'#000'}}>Rs</span>
    case 'typescript': return <span className={cls} style={{background:'#3178c6',color:'#fff'}}>TS</span>
    case 'javascript': return <span className={cls} style={{background:'#f7df1e',color:'#000'}}>JS</span>
    case 'python':     return <span className={cls} style={{background:'#3776ab',color:'#fff'}}>Py</span>
    case 'go':         return <span className={cls} style={{background:'#00add8',color:'#fff'}}>Go</span>
    case 'c':          return <span className={cls} style={{background:'#555',color:'#fff'}}>C</span>
    case 'cpp':        return <span className={cls} style={{background:'#649ad2',color:'#fff'}}>C++</span>
    case 'java':       return <span className={cls} style={{background:'#ed8b00',color:'#fff'}}>Jv</span>
    case 'csharp':     return <span className={cls} style={{background:'#9b4f96',color:'#fff'}}>C#</span>
    case 'swift':      return <span className={cls} style={{background:'#f05138',color:'#fff'}}>Sw</span>
    case 'kotlin':     return <span className={cls} style={{background:'#7f52ff',color:'#fff'}}>Kt</span>
    case 'scala':      return <span className={cls} style={{background:'#dc322f',color:'#fff'}}>Sc</span>
    case 'ruby':       return <span className={cls} style={{background:'#cc342d',color:'#fff'}}>Rb</span>
    case 'lua':        return <span className={cls} style={{background:'#000080',color:'#fff'}}>Lu</span>
    case 'zig':        return <span className={cls} style={{background:'#f7a41d',color:'#000'}}>Zg</span>
    case 'haskell':    return <span className={cls} style={{background:'#5e5086',color:'#fff'}}>Hs</span>
    case 'dart':       return <span className={cls} style={{background:'#00b4ab',color:'#fff'}}>Da</span>
    case 'vue':        return <span className={cls} style={{background:'#42b883',color:'#fff'}}>Vue</span>
    case 'svelte':     return <span className={cls} style={{background:'#ff3e00',color:'#fff'}}>Sv</span>
    case 'html':       return <span className={cls} style={{background:'#e34c26',color:'#fff'}}>Ht</span>
    case 'css':        return <span className={cls} style={{background:'#264de4',color:'#fff'}}>CS</span>
    case 'json':       return <span className={cls} style={{background:'#5e5e5e',color:'#fff'}}>{`{}`}</span>
    case 'yaml':       return <span className={cls} style={{background:'#cb171e',color:'#fff'}}>Ym</span>
    case 'toml':       return <span className={cls} style={{background:'#9c4221',color:'#fff'}}>Tl</span>
    case 'markdown':   return <span className={cls} style={{background:'#000',color:'#fff'}}>Md</span>
    case 'bash':       return <span className={cls} style={{background:'#4eaa25',color:'#fff'}}>Sh</span>
    case 'dockerfile': return <span className={cls} style={{background:'#2496ed',color:'#fff'}}>Df</span>
    default:           return <span className={cls} style={{background:'#555',color:'#fff'}}>{lang.slice(0,2).toUpperCase()}</span>
  }
}

function LspTab() {
  const { t } = useLocale()
  const [languages, setLanguages] = useState<LspLangInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [starting, setStarting] = useState<string | null>(null)
  const [installing, setInstalling] = useState<string | null>(null)
  const [installLog, setInstallLog] = useState<string[]>([])
  const [installDone, setInstallDone] = useState<boolean>(false)
  const [rescanning, setRescanning] = useState(false)
  const [diagServers, setDiagServers] = useState<Array<{ language: string; total: number; files: Array<{ file: string; count: number }> }>>([])
  const [showDiag, setShowDiag] = useState(false)
  const [filter, setFilter] = useState<'all' | 'installed' | 'missing'>('installed')

  const loadData = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const res = await loomRpc<{ languages: LspLangInfo[] }>('lsp.check')
      setLanguages(res.languages ?? [])
    } catch (e: any) {
      setError(_t('mcp.loadFailed', { message: e.message || e }))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  const handleStart = async (lang: string, cmd: string) => {
    setStarting(lang)
    try {
      await loomRpc('lsp.start', { language: lang, command: cmd, args: [] })
      useStore.getState().addToast({ type: 'success', message: `${lang} LSP started` })
      await loadData()
    } catch (e: any) {
      setError(_t('lsp.startFailed', { message: e.message || e }))
    } finally {
      setStarting(null)
    }
  }

  const handleShutdown = async (lang: string) => {
    try {
      await loomRpc('lsp.shutdown', { language: lang })
      useStore.getState().addToast({ type: 'success', message: _t('lsp.serverStopped', { language: lang }) })
      await loadData()
    } catch (e: any) {
      setError(_t('lsp.stopFailed', { message: e.message || e }))
    }
  }

  const handleShutdownAll = async () => {
    try {
      await loomRpc('lsp.shutdown_all', {})
      useStore.getState().addToast({ type: 'success', message: t('lsp.allStopped') })
      await loadData()
    } catch (e: any) {
      setError(_t('lsp.stopFailed', { message: e.message || e }))
    }
  }

  const runLspTask = async (lang: string, rpcMethod: 'lsp.install' | 'lsp.uninstall', payload: Record<string, unknown>, successMsg: string, failPrefix: string) => {
    setInstalling(lang)
    setInstallLog([])
    setInstallDone(false)
    try {
      const { task_id } = await loomRpc<{ task_id: string }>(rpcMethod, payload)
      const poll = setInterval(async () => {
        try {
          const status = await loomRpc<{
            task_id: string; lines: string[]; done: boolean; ok: boolean; exit_code: number | null
          }>('lsp.install_status', { task_id })
          setInstallLog(status.lines ?? [])
          if (status.done) {
            clearInterval(poll)
            setInstallDone(true)
            if (status.ok) {
              useStore.getState().addToast({ type: 'success', message: successMsg })
              await loadData()
            } else {
              setError(`${failPrefix} (exit ${status.exit_code})`)
            }
            setTimeout(() => { setInstalling(null); setInstallLog([]); setInstallDone(false) }, 3000)
          }
        } catch {
          clearInterval(poll)
          setInstalling(null)
          setInstallLog([])
          setInstallDone(false)
        }
      }, 400)
    } catch (e: any) {
      setError(`${failPrefix}: ${e.message || e}`)
      setInstalling(null)
    }
  }

  const handleInstall = (lang: string, cmd: string) =>
    runLspTask(lang, 'lsp.install', { language: lang, command: cmd }, `${lang} LSP installed`, 'Install failed')

  const handleUninstall = (lang: string) =>
    runLspTask(lang, 'lsp.uninstall', { language: lang }, `${lang} LSP uninstalled`, 'Uninstall failed')

  const handleRescan = async () => {
    setRescanning(true)
    await loadData()
    setRescanning(false)
  }

  const loadDiags = async () => {
    try {
      const res = await loomRpc<{ servers: Array<{ language: string; total: number; files: Array<{ file: string; count: number }> }> }>('lsp.all_diagnostics')
      setDiagServers(res.servers ?? [])
    } catch { /* non-critical */ }
  }

  const installed = languages.filter(l => l.available).length
  const active = languages.filter(l => l.running).length

  return (
    <div className={styles.aboutSection}>
      {/* Header */}
      <div className={styles.sectionHeaderRow}>
        <div>
          <h4 className={styles.sectionSubTitle}>{t('lsp.title')}</h4>
          <p className={styles.pluginsDesc}>
            {t('lsp.statsLine', { installed, active, total: languages.length })}
          </p>
        </div>
        <div className={styles.lspHeaderActions}>
          {active > 0 && (
            <button className={styles.lspStopAllBtn} onClick={handleShutdownAll}>
              {t('lsp.stopAll')}
            </button>
          )}
          <button className={styles.mcpAddBtn} onClick={handleRescan} disabled={rescanning}>
            {t('lsp.scan')}
          </button>
        </div>
      </div>

      {/* Quick filter tabs */}
      <div className={styles.lspFilterRow}>
        {(['installed', 'missing', 'all'] as const).map(f => (
          <button
            key={f}
            className={`${styles.lspFilterTab} ${filter === f ? styles.lspFilterTabActive : ''}`}
            onClick={() => setFilter(f)}
          >
            {f === 'installed' ? t('lsp.filterInstalled', { n: installed })
             : f === 'missing' ? t('lsp.filterMissing', { n: languages.length - installed })
             : t('lsp.filterAll', { n: languages.length })}
          </button>
        ))}
      </div>

      {error && <p className={styles.toolsError}>{error}</p>}

      {/* Language grid */}
      {loading ? (
        <p className={styles.toolsEmpty}>{t('common.loading')}</p>
      ) : (
        <div className={styles.lspGrid}>
          {languages
            .filter(l => filter === 'all' || (filter === 'installed' ? l.available : !l.available))
            .map(l => {
            const isStarting = starting === l.language
            return (
              <div
                key={l.language}
                className={`${styles.lspCard} ${l.running ? styles.lspCardRunning : ''} ${l.available ? '' : styles.lspCardMissing}`}
              >
                {/* Top row: icon, name, status */}
                <div className={styles.lspCardTop}>
                  <LangIcon lang={l.language} />
                  <div className={styles.lspCardInfo}>
                    <span className={styles.lspCardName}>{l.language}</span>
                    <span className={styles.lspCardCmd}>{l.command}</span>
                  </div>
                  <span className={`
                    ${styles.lspCardStatus}
                    ${l.running ? styles.lspCardStatusLive : ''}
                    ${!l.available ? styles.lspCardStatusMissing : ''}
                  `}>
                    {l.running ? t('lsp.statusLive') : l.available ? t('lsp.statusReady') : t('lsp.statusMissing')}
                  </span>
                </div>

                {/* Actions */}
                <div className={styles.lspCardActions}>
                  {l.available ? (
                    <>
                      {l.running ? (
                        <button
                          className={styles.lspStopBtn}
                          onClick={() => handleShutdown(l.language)}
                        >
                          {t('lsp.stop')}
                        </button>
                      ) : (
                        <button
                          className={styles.lspStartBtn}
                          onClick={() => handleStart(l.language, l.command)}
                          disabled={isStarting}
                        >
                          {isStarting ? t('lsp.starting') : t('lsp.start')}
                        </button>
                      )}
                      {l.uninstall_command && (
                        <button
                          className={styles.lspUninstallBtn}
                          onClick={() => handleUninstall(l.language)}
                          disabled={installing === l.language}
                        >
                          {t('lsp.uninstall')}
                        </button>
                      )}
                    </>
                  ) : (
                    <button
                      className={styles.lspInstallBtn}
                      onClick={() => handleInstall(l.language, l.command)}
                      disabled={installing === l.language}
                    >
                      {installing === l.language ? t('lsp.installing') : t('lsp.install')}
                    </button>
                  )}
                </div>

                {/* Install hint / live progress */}
                {!l.available && l.install_hint && (
                  <>
                    <div className={styles.lspInstallHint}>
                      <span className={styles.lspInstallManager}>{l.install_hint.manager}</span>
                      <code className={styles.lspInstallCmd}>{l.install_hint.command}</code>
                    </div>
                    {installing === l.language && (
                      <div className={styles.lspInstallLog}>
                        {installLog.length === 0 && !installDone && (
                          <span className={styles.lspInstallSpinner}>{t('lsp.startingInstall')}</span>
                        )}
                        {installLog.map((line, i) => (
                          <div key={i} className={styles.lspInstallLogLine}>{line}</div>
                        ))}
                        {installDone && (
                          <span className={styles.lspInstallDone}>{t('lsp.doneScanning')}</span>
                        )}
                      </div>
                    )}
                  </>
                )}
              </div>
            )
          })}
        </div>
      )}

      {/* Diagnostics */}
      <div className={styles.lspDiagSection}>
        <div className={styles.sectionHeaderRow}>
          <h4 className={styles.sectionSubTitle}>{t('lsp.diagnostics')}</h4>
          <div style={{ display: 'flex', gap: 8 }}>
            <button className={styles.mcpAddBtn} onClick={loadDiags}>{t('lsp.fetch')}</button>
            {diagServers.length > 0 && (
              <button className={styles.lspStopAllBtn} onClick={() => setShowDiag(!showDiag)}>
                {showDiag ? t('lsp.hide') : t('lsp.showIssues', { n: diagServers.reduce((s, d) => s + d.total, 0) })}
              </button>
            )}
          </div>
        </div>
        {diagServers.length === 0 ? (
          <p className={styles.toolsEmpty}>{t('lsp.noDiagnostics')}</p>
        ) : showDiag && (
          <div className={styles.lspDiagGrid}>
            {diagServers.filter(d => d.total > 0).map(ds => (
              <div key={ds.language} className={styles.lspDiagCard}>
                <div className={styles.lspDiagHeader}>
                  <span className={styles.lspCardName}>{ds.language}</span>
                  <span className={`${styles.lspCardStatus} ${ds.total > 0 ? styles.lspCardStatusMissing : ''}`}>
                    {t('lsp.issuesCount', { n: ds.total })}
                  </span>
                </div>
                {ds.files.filter(f => f.count > 0).map(f => (
                  <div key={f.file} className={styles.lspDiagRow}>
                    <code className={styles.lspDiagFile}>{f.file.replace(/\\/g, '/').split('/').pop() ?? f.file}</code>
                    <span className={styles.lspDiagCount}>{f.count}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        )}
      </div>

      <p className={styles.lspInfoText}>{t('lsp.infoText')}</p>
    </div>
  )
}

export default function McpLspTab() {
  return (
    <>
      <McpTab />
      <hr className={styles.sectionDivider} />
      <LspTab />
    </>
  )
}

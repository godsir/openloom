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

interface LspServerInfo {
  language?: string
  name?: string
  [key: string]: unknown
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

function LspTab() {
  const { t } = useLocale()
  const [servers, setServers] = useState<LspServerInfo[]>([])
  const [supported, setSupported] = useState<{ language: string; command: string }[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [showForm, setShowForm] = useState(false)
  const [formLang, setFormLang] = useState('')
  const [formCmd, setFormCmd] = useState('')
  const [formArgs, setFormArgs] = useState('')
  const [starting, setStarting] = useState(false)

  const loadData = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [serversRes, langRes] = await Promise.allSettled([
        loomRpc<{ servers: LspServerInfo[] }>('lsp.list_servers'),
        loomRpc<{ languages: { language: string; command: string }[] }>('lsp.supported_languages'),
      ])
      if (serversRes.status === 'fulfilled') setServers(serversRes.value.servers ?? [])
      if (langRes.status === 'fulfilled') setSupported(langRes.value.languages ?? [])
    } catch (e: any) {
      setError(_t('mcp.loadFailed', { message: e.message || e }))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  const handleShutdown = async (language: string) => {
    try {
      await loomRpc('lsp.shutdown', { language })
      useStore.getState().addToast({ type: 'success', message: _t('lsp.serverStopped', { language }) })
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

  const handleStart = async () => {
    if (!formLang.trim() || !formCmd.trim()) return
    setStarting(true)
    try {
      const args = formArgs.trim() ? formArgs.trim().split(/\s+/) : []
      await loomRpc('lsp.start', { language: formLang.trim(), command: formCmd.trim(), args })
      useStore.getState().addToast({ type: 'success', message: _t('lsp.serverStopped', { language: formLang.trim() }) })
      setShowForm(false)
      setFormLang('')
      setFormCmd('')
      setFormArgs('')
      await loadData()
    } catch (e: any) {
      setError(_t('lsp.startFailed', { message: e.message || e }))
    } finally {
      setStarting(false)
    }
  }

  const handleSelectPreset = (lang: string) => {
    const preset = supported.find(s => s.language === lang)
    if (preset) {
      setFormLang(preset.language)
      setFormCmd(preset.command)
      setFormArgs('')
      setShowForm(true)
    }
  }

  return (
    <>
      <div className={styles.aboutSection}>
        <div className={styles.sectionHeaderRow}>
          <h4 className={styles.sectionSubTitle}>{t('lsp.title')}</h4>
          {!showForm && (
            <button className={styles.mcpAddBtn} onClick={() => setShowForm(true)}>
              {t('lsp.startServer')}
            </button>
          )}
        </div>
        {error && <p className={styles.toolsError}>{error}</p>}
        {loading ? (
          <p className={styles.toolsEmpty}>{t('common.loading')}</p>
        ) : (
          <>
            {/* Active servers */}
            <div className={styles.lspServerList}>
              {servers.length === 0 ? (
                <p className={styles.toolsEmpty}>{t('lsp.noActiveServers')}</p>
              ) : (
                <>
                  {servers.map((srv, i) => {
                    const lang = srv.language ?? srv.name ?? `Server ${i + 1}`
                    return (
                      <div key={i} className={styles.lspServerItem}>
                        <span className={styles.lspServerName}>{lang}</span>
                        <button
                          className={styles.lspStopBtn}
                          onClick={() => handleShutdown(srv.language ?? srv.name ?? '')}
                        >
                          {t('lsp.stop')}
                        </button>
                      </div>
                    )
                  })}
                  <button className={styles.lspStopAllBtn} onClick={handleShutdownAll}>
                    {t('lsp.stopAll')}
                  </button>
                </>
              )}
            </div>

            {/* Start form */}
            {showForm && (
              <div className={styles.mcpAddForm}>
                <div className={styles.mcpFormRow}>
                  <label className={styles.mcpFormLabel}>{t('lsp.languageId')}</label>
                  <input
                    value={formLang}
                    onChange={e => setFormLang(e.target.value)}
                    placeholder={'rust, python, go'}
                    className={styles.mcpFormInput}
                  />
                </div>
                <div className={styles.mcpFormRow}>
                  <label className={styles.mcpFormLabel}>{t('lsp.command')}</label>
                  <input
                    value={formCmd}
                    onChange={e => setFormCmd(e.target.value)}
                    placeholder={'rust-analyzer, pylsp'}
                    className={styles.mcpFormInput}
                  />
                </div>
                <div className={styles.mcpFormRow}>
                  <label className={styles.mcpFormLabel}>{t('lsp.args')}</label>
                  <input
                    value={formArgs}
                    onChange={e => setFormArgs(e.target.value)}
                    placeholder={t('lsp.args')}
                    className={styles.mcpFormInput}
                  />
                </div>
                <div className={styles.mcpFormActions}>
                  <button className={styles.mcpCancelBtn} onClick={() => setShowForm(false)}>{t('common.cancel')}</button>
                  <button
                    className={styles.mcpConnectBtn}
                    onClick={handleStart}
                    disabled={starting || !formLang.trim() || !formCmd.trim()}
                  >
                    {starting ? t('lsp.starting') : t('lsp.start')}
                  </button>
                </div>
              </div>
            )}

            {/* Supported languages as quick-start pills */}
            {supported.length > 0 && (
              <div className={styles.lspQuickStart}>
                <div className={styles.toolsSectionLabel}>{t('lsp.quickStart')}</div>
                <div className={styles.toolsBadgeGrid}>
                  {supported.map(s => (
                    <button
                      key={s.language}
                      className={styles.toolsBadge}
                      onClick={() => handleSelectPreset(s.language)}
                      title={s.command}
                    >
                      {s.language}
                    </button>
                  ))}
                </div>
              </div>
            )}

            <p className={styles.lspInfoText}>
              {t('lsp.infoText')}
            </p>
          </>
        )}
      </div>
    </>
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

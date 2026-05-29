import { useState, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { IconFolder, IconPackage } from '../../utils/icons'
import Overlay from './Overlay'
import AgentConfigPanel from './AgentConfigPanel'
import ModelConfigPanel from './ModelConfigPanel'
import VisionConfigSection from './VisionConfigSection'
import KnowledgeGraphPanel from '../kg/KnowledgeGraphPanel'
import { type ThemeId } from '../../stores/ui'
import styles from './SettingsModal.module.css'

const THEMES: { id: ThemeId; label: string; bg: string; surface: string; text: string; accent: string }[] = [
  { id: 'dark', label: '暗色', bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' },
  { id: 'light', label: '亮色', bg: '#ffffff', surface: '#f1f5f9', text: '#0f172a', accent: '#0d9488' },
  { id: 'midnight', label: '星夜', bg: '#0b1120', surface: '#0f172a', text: '#e2e8f0', accent: '#a5bff8' },
  { id: 'warm-paper', label: '素笺', bg: '#fdfbf7', surface: '#f5f0e8', text: '#2d2416', accent: '#b05a30' },
  { id: 'neon-pink', label: '紫夜', bg: '#1a1a1d', surface: '#222225', text: '#f0e0e8', accent: '#e6397c' },
  { id: 'ember', label: '熔岩', bg: '#000026', surface: '#060630', text: '#ffe0c0', accent: '#ff770f' },
  { id: 'navy-gold', label: '鎏金', bg: '#050F2E', surface: '#0A1A45', text: '#e2e8f0', accent: '#FFE76F' },
  { id: 'umber-cream', label: '摩卡', bg: '#2D1B14', surface: '#3D271D', text: '#fff8f0', accent: '#D8C7B5' },
]

type Tab = 'appearance' | 'software' | 'agent' | 'models' | 'mcp' | 'lsp' | 'skills' | 'plugins' | 'kg' | 'about'

interface McpTool {
  name: string
  description?: string
}

interface SystemHealth {
  status: string
  version: string
  agent_count: number
  tool_count: number
}

interface LspServerInfo {
  language?: string
  name?: string
  [key: string]: unknown
}

interface SkillInfo {
  name: string
  description?: string
  path?: string
  version?: string
  user_invocable?: boolean
  always_active?: boolean
}

interface PluginInfo {
  name: string
  version?: string
  description?: string
  path?: string
  skill_count?: number
  mcp_server_count?: number
}

export default function SettingsModal({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) {
  const theme = useStore((s) => s.theme)
  const setTheme = useStore((s) => s.setTheme)
  const wsState = useStore((s) => s.wsState)
  const [tab, setTab] = useState<Tab>('appearance')

  const tabs: { id: Tab; label: string }[] = [
    { id: 'appearance', label: '外观' },
    { id: 'software', label: '软件' },
    { id: 'agent', label: 'Agent' },
    { id: 'models', label: '模型' },
    { id: 'mcp', label: 'MCP' },
    { id: 'lsp', label: 'LSP' },
    { id: 'skills', label: 'Skills' },
    { id: 'plugins', label: 'Plugins' },
    { id: 'kg', label: '认知图谱' },
    { id: 'about', label: '关于' },
  ]

  return (
    <Overlay open={open} onClose={onClose} size="lg">
      <div className={styles.layout}>
        <div className={styles.nav}>
          <div className={styles.navLabel}>设置</div>
          {tabs.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={`${styles.navItem} ${tab === t.id ? styles.navActive : ''}`}
            >
              {t.label}
            </button>
          ))}
        </div>

        <div className={styles.content}>
          {tab === 'appearance' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>外观</h3>
                <p className={styles.sectionDesc}>选择主题和界面偏好</p>
              </div>
              <div className={styles.contentBody}>
                <div className={styles.themeSection}>
                  <div className={styles.themeLabel}>主题</div>
                  <div className={styles.themeGrid}>
                    {THEMES.map((t) => (
                      <button
                        key={t.id}
                                                onClick={() => { setTheme(t.id); useStore.getState().addToast({ type: 'success', message: `主题已切换为${t.label}` }) }}
                        className={`${styles.themeCard} ${theme === t.id ? styles.themeCardActive : ''}`}
                      >
                        <div
                          className={styles.themePreview}
                          style={{
                            '--pv-bg': t.bg,
                            '--pv-surface': t.surface,
                            '--pv-accent': t.accent,
                            '--pv-text-13': t.text + '22',
                            '--pv-text-27': t.text + '44',
                          } as React.CSSProperties}
                        >
                          <div className={styles.themePreviewInner}>
                            <div className={styles.themePreviewSidebar}>
                              <div className={styles.themePreviewAccentBar} />
                              <div className={styles.themePreviewBarWide} />
                              <div className={styles.themePreviewBarNarrow} />
                            </div>
                            <div className={styles.themePreviewMain}>
                              <div>
                                <div className={styles.themePreviewBarTitle} />
                                <div className={styles.themePreviewBarBody} />
                              </div>
                              <div className={styles.themePreviewCard} />
                            </div>
                          </div>
                        </div>
                        <span className={`${styles.themeName} ${theme === t.id ? styles.themeNameActive : ''}`}>
                          {t.label}
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
              </div>
            </>
          )}

          {tab === 'software' && <SoftwareTab />}

          {tab === 'agent' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>Agent 配置</h3>
                <p className={styles.sectionDesc}>管理 Agent 角色和行为</p>
              </div>
              <div className={styles.contentBody}>
                <AgentConfigPanel />
              </div>
            </>
          )}

          {tab === 'models' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>模型</h3>
                <p className={styles.sectionDesc}>配置推理模型和 API 密钥</p>
              </div>
              <div className={styles.contentBody}>
                <ModelConfigPanel />
                <VisionConfigSection />
              </div>
            </>
          )}

          {tab === 'mcp' && <McpTab />}
          {tab === 'lsp' && <LspTab />}
          {tab === 'skills' && <SkillsTab />}
          {tab === 'plugins' && <PluginsTab />}
          {tab === 'kg' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>认知图谱</h3>
                <p className={styles.sectionDesc}>浏览和管理 AI 学习到的知识实体与关系</p>
              </div>
              <div className={styles.contentBody}>
                <KnowledgeGraphPanel />
              </div>
            </>
          )}
          {tab === 'about' && <AboutTab wsState={wsState} />}
        </div>
      </div>
    </Overlay>
  )
}

/* ─── Software Tab ─── */

function SoftwareTab() {
  const [autoStart, setAutoStart] = useState(false)
  const [closeToTray, setCloseToTray] = useState(true)
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    Promise.all([
      window.hana.getPreference('autoStart', false),
      window.hana.getPreference('closeToTray', true),
    ]).then(([as, ct]) => {
      setAutoStart(as)
      setCloseToTray(ct)
      setLoaded(true)
    })
  }, [])

  const handleAutoStart = async (val: boolean) => {
    setAutoStart(val)
    await window.hana.setPreference('autoStart', val)
    useStore.getState().addToast({ type: 'success', message: val ? '已开启开机自启动' : '已关闭开机自启动' })
  }

  const handleCloseToTray = async (val: boolean) => {
    setCloseToTray(val)
    await window.hana.setPreference('closeToTray', val)
    useStore.getState().addToast({ type: 'success', message: val ? '关闭按钮将最小化到托盘' : '关闭按钮将退出程序' })
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>软件设置</h3>
        <p className={styles.sectionDesc}>启动行为和关闭方式</p>
      </div>
      <div className={styles.contentBody}>
        {!loaded ? (
          <p className={styles.toolsEmpty}>加载中...</p>
        ) : (
          <div className={styles.aboutSection}>
            <div className={styles.aboutRow}>
              <div>
                <span className={styles.aboutLabel}>关闭按钮行为</span>
                <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>点击标题栏关闭按钮时的操作</p>
              </div>
              <div className={styles.mcpTransportToggle}>
                <button
                  className={`${styles.mcpTransportBtn} ${closeToTray ? styles.mcpTransportActive : ''}`}
                  onClick={() => handleCloseToTray(true)}
                >
                  最小化到托盘
                </button>
                <button
                  className={`${styles.mcpTransportBtn} ${!closeToTray ? styles.mcpTransportActive : ''}`}
                  onClick={() => handleCloseToTray(false)}
                >
                  退出程序
                </button>
              </div>
            </div>
            <div className={styles.aboutRow}>
              <div>
                <span className={styles.aboutLabel}>开机自启动</span>
                <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>系统启动时自动运行 openLoom</p>
              </div>
              <div className={styles.mcpTransportToggle}>
                <button
                  className={`${styles.mcpTransportBtn} ${autoStart ? styles.mcpTransportActive : ''}`}
                  onClick={() => handleAutoStart(true)}
                >
                  开启
                </button>
                <button
                  className={`${styles.mcpTransportBtn} ${!autoStart ? styles.mcpTransportActive : ''}`}
                  onClick={() => handleAutoStart(false)}
                >
                  关闭
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </>
  )
}

/* ─── MCP Tab ─── */

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

function McpTab() {
  const [configs, setConfigs] = useState<McpServerConfig[]>([])
  const [healthByName, setHealthByName] = useState<Record<string, boolean | null>>({})
  const [toolsByServer, setToolsByServer] = useState<Record<string, McpTool[]>>({})
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  // Editor state — null = closed, {} = new entry, populated = editing existing.
  const [editing, setEditing] = useState<McpServerConfig | null>(null)
  const [editingOriginalName, setEditingOriginalName] = useState<string | null>(null)
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
        setError(`加载 MCP 配置失败: ${cfgRes.reason?.message || cfgRes.reason}`)
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
      setError(`加载失败: ${e.message || e}`)
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
        `MCP "${editing.name}" 已连接`)
      cancelEdit()
      await loadData()
    } catch (e: any) {
      setError(`连接失败: ${e.message || e}`)
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
      await rpc('mcp.config.save', buildPayload(editing), `MCP "${editing.name}" 已保存`)
      cancelEdit()
      await loadData()
    } catch (e: any) {
      setError(`保存失败: ${e.message || e}`)
    } finally {
      setBusy(false)
    }
  }
  const handleConnectExisting = async (cfg: McpServerConfig) => {
    setBusy(true)
    try {
      await rpc('mcp.connect', { ...buildPayload(cfg), persist: true },
        `MCP "${cfg.name}" 已连接`)
      await loadData()
    } catch (e: any) {
      setError(`连接失败: ${e.message || e}`)
    } finally {
      setBusy(false)
    }
  }
  const handleDisconnect = async (name: string) => {
    try {
      await rpc('mcp.disconnect', { name }, `MCP "${name}" 已断开`)
      await loadData()
    } catch { /* toast already shown */ }
  }
  const handleDelete = async (name: string) => {
    if (!window.confirm(`确认删除 MCP "${name}" 的配置？这会断开连接并移除保存的参数。`)) return
    try {
      await rpc('mcp.config.delete', { name }, `已删除 "${name}"`)
      await loadData()
    } catch { /* toast already shown */ }
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>MCP 服务</h3>
        <p className={styles.sectionDesc}>管理 Model Context Protocol 服务器连接（配置自动持久化）</p>
      </div>
      <div className={styles.contentBody}>
        {error && <p className={styles.toolsError}>{error}</p>}
        {loading ? (
          <p className={styles.toolsEmpty}>加载中...</p>
        ) : (
          <>
            <div className={styles.mcpServerList}>
              {configs.length === 0 ? (
                <p className={styles.toolsEmpty}>暂无 MCP 服务器配置</p>
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
                            {!c.connected && ' · 已断开'}
                          </span>
                        </div>
                        <div className={styles.mcpServerActions}>
                          {c.connected ? (
                            <button className={styles.mcpDisconnectBtn} onClick={() => handleDisconnect(c.name)}>
                              断开
                            </button>
                          ) : (
                            <button className={styles.mcpDisconnectBtn} onClick={() => handleConnectExisting(c)}>
                              连接
                            </button>
                          )}
                          <button className={styles.mcpDisconnectBtn} onClick={() => startEdit(c)}>
                            编辑
                          </button>
                          <button className={styles.mcpDisconnectBtn} onClick={() => handleDelete(c.name)}>
                            删除
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

            {!editing ? (
              <button className={styles.mcpAddBtn} onClick={startCreate}>
                + 添加服务器
              </button>
            ) : (
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
  const v = value
  const set = (patch: Partial<McpServerConfig>) => onChange({ ...v, ...patch })

  return (
    <div className={styles.mcpAddForm}>
      <div className={styles.mcpFormRow}>
        <label className={styles.mcpFormLabel}>名称</label>
        <input
          className={styles.mcpFormInput}
          value={v.name}
          onChange={(e) => set({ name: e.target.value })}
          placeholder="server-name"
        />
      </div>
      <div className={styles.mcpFormRow}>
        <label className={styles.mcpFormLabel}>传输类型</label>
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
            <label className={styles.mcpFormLabel}>命令</label>
            <input
              className={styles.mcpFormInput}
              value={v.command}
              onChange={(e) => set({ command: e.target.value })}
              placeholder="npx, node, python..."
            />
          </div>
          <div className={styles.mcpFormRow}>
            <label className={styles.mcpFormLabel}>参数（逗号或换行分隔）</label>
            <textarea
              className={`${styles.mcpFormInput} ${styles.mcpFormTextarea}`}
              value={v.args.join('\n')}
              onChange={(e) => set({ args: parseCsv(e.target.value) })}
              placeholder={'-y\n@modelcontextprotocol/server-xxx'}
            />
          </div>
          <div className={styles.mcpFormRow}>
            <label className={styles.mcpFormLabel}>工作目录（可选）</label>
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
            <label className={styles.mcpFormLabel}>URL</label>
            <input
              className={styles.mcpFormInput}
              value={v.url ?? ''}
              onChange={(e) => set({ url: e.target.value })}
              placeholder="http://localhost:8080/sse"
            />
          </div>
          <div className={styles.mcpFormRow}>
            <label className={styles.mcpFormLabel}>请求头（每行 KEY=VALUE）</label>
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
        <label className={styles.mcpFormLabel}>环境变量（每行 KEY=VALUE，可选）</label>
        <textarea
          className={`${styles.mcpFormInput} ${styles.mcpFormTextarea}`}
          value={kvToText(v.env)}
          onChange={(e) => set({ env: parseKvLines(e.target.value) })}
          placeholder={'API_KEY=...'}
        />
      </div>

      <div className={`${styles.mcpFormRow} ${styles.mcpFormRowHorizontal}`}>
        <div className={styles.mcpFormFlexCell}>
          <label className={styles.mcpFormLabel}>启动超时(秒)</label>
          <input
            className={styles.mcpFormInput}
            type="number"
            min={1}
            value={v.startup_timeout_secs}
            onChange={(e) => set({ startup_timeout_secs: Number(e.target.value) || 30 })}
          />
        </div>
        <div className={styles.mcpFormFlexCell}>
          <label className={styles.mcpFormLabel}>工具超时(秒)</label>
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
        <label className={styles.mcpFormLabel}>仅启用工具（逗号或换行，留空=全部）</label>
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
        <label className={styles.mcpFormLabel}>禁用工具（逗号或换行）</label>
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
          引擎启动时自动重连
        </label>
      </div>

      <div className={styles.mcpFormActions}>
        <button className={styles.mcpCancelBtn} onClick={onCancel}>取消</button>
        <button
          className={styles.mcpCancelBtn}
          onClick={onSave}
          disabled={busy || !v.name.trim()}
        >
          {busy ? '保存中...' : '仅保存'}
        </button>
        <button
          className={styles.mcpConnectBtn}
          onClick={onSaveAndConnect}
          disabled={busy || !v.name.trim()}
        >
          {busy ? '连接中...' : isEdit ? '保存并重连' : '保存并连接'}
        </button>
      </div>
    </div>
  )
}

/* ─── LSP Tab ─── */

function LspTab() {
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
      setError(`加载失败: ${e.message || e}`)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  const handleShutdown = async (language: string) => {
    try {
      await loomRpc('lsp.shutdown', { language })
      useStore.getState().addToast({ type: 'success', message: `LSP "${language}" 已停止` })
      await loadData()
    } catch (e: any) {
      setError(`停止失败: ${e.message || e}`)
    }
  }

  const handleShutdownAll = async () => {
    try {
      await loomRpc('lsp.shutdown_all', {})
      useStore.getState().addToast({ type: 'success', message: '所有 LSP 服务已停止' })
      await loadData()
    } catch (e: any) {
      setError(`停止失败: ${e.message || e}`)
    }
  }

  const handleStart = async () => {
    if (!formLang.trim() || !formCmd.trim()) return
    setStarting(true)
    try {
      const args = formArgs.trim() ? formArgs.trim().split(/\s+/) : []
      await loomRpc('lsp.start', { language: formLang.trim(), command: formCmd.trim(), args })
      useStore.getState().addToast({ type: 'success', message: `LSP "${formLang.trim()}" 已启动` })
      setShowForm(false)
      setFormLang('')
      setFormCmd('')
      setFormArgs('')
      await loadData()
    } catch (e: any) {
      setError(`启动失败: ${e.message || e}`)
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
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>LSP 语言服务</h3>
        <p className={styles.sectionDesc}>管理语言服务器 — 启动、停止、自定义配置</p>
      </div>
      <div className={styles.contentBody}>
        {error && <p className={styles.toolsError}>{error}</p>}
        {loading ? (
          <p className={styles.toolsEmpty}>加载中...</p>
        ) : (
          <>
            {/* Active servers */}
            <div className={styles.lspServerList}>
              {servers.length === 0 ? (
                <p className={styles.toolsEmpty}>暂无活跃的语言服务器</p>
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
                          停止
                        </button>
                      </div>
                    )
                  })}
                  <button className={styles.lspStopAllBtn} onClick={handleShutdownAll}>
                    全部停止
                  </button>
                </>
              )}
            </div>

            {/* Start form */}
            {!showForm ? (
              <button className={styles.mcpAddBtn} onClick={() => setShowForm(true)}>
                + 启动语言服务器
              </button>
            ) : (
              <div className={styles.mcpAddForm}>
                <div className={styles.mcpFormRow}>
                  <label className={styles.mcpFormLabel}>语言 ID</label>
                  <input
                    value={formLang}
                    onChange={e => setFormLang(e.target.value)}
                    placeholder="如 rust, python, go"
                    className={styles.mcpFormInput}
                  />
                </div>
                <div className={styles.mcpFormRow}>
                  <label className={styles.mcpFormLabel}>命令</label>
                  <input
                    value={formCmd}
                    onChange={e => setFormCmd(e.target.value)}
                    placeholder="如 rust-analyzer, pylsp"
                    className={styles.mcpFormInput}
                  />
                </div>
                <div className={styles.mcpFormRow}>
                  <label className={styles.mcpFormLabel}>参数</label>
                  <input
                    value={formArgs}
                    onChange={e => setFormArgs(e.target.value)}
                    placeholder="空格分隔，如 --stdio"
                    className={styles.mcpFormInput}
                  />
                </div>
                <div className={styles.mcpFormActions}>
                  <button className={styles.mcpCancelBtn} onClick={() => setShowForm(false)}>取消</button>
                  <button
                    className={styles.mcpConnectBtn}
                    onClick={handleStart}
                    disabled={starting || !formLang.trim() || !formCmd.trim()}
                  >
                    {starting ? '启动中...' : '启动'}
                  </button>
                </div>
              </div>
            )}

            {/* Supported languages as quick-start pills */}
            {supported.length > 0 && (
              <div className={styles.lspQuickStart}>
                <div className={styles.toolsSectionLabel}>快速启动（点击预填）</div>
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
              LSP 服务器也可按需自动启动 — Agent 打开文件时自动激活对应语言服务器。
            </p>
          </>
        )}
      </div>
    </>
  )
}

/* ─── Skills Tab ─── */

function SkillsTab() {
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [selectedSkill, setSelectedSkill] = useState<string | null>(null)
  const [skillContent, setSkillContent] = useState<string | null>(null)
  const [loadingContent, setLoadingContent] = useState(false)
  const [importing, setImporting] = useState(false)

  const loadSkills = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const res = await loomRpc<{ skills: SkillInfo[] }>('skills.list')
      setSkills(res.skills ?? [])
    } catch (e: any) {
      setError(`加载失败: ${e.message || e}`)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { loadSkills() }, [loadSkills])

  const handleSelectSkill = async (name: string) => {
    if (selectedSkill === name) {
      setSelectedSkill(null)
      setSkillContent(null)
      return
    }
    setSelectedSkill(name)
    setLoadingContent(true)
    try {
      const res = await loomRpc<{ content: string }>('skills.get', { name })
      setSkillContent(res.content ?? '')
    } catch (e: any) {
      setSkillContent(`加载失败: ${e.message || e}`)
    } finally {
      setLoadingContent(false)
    }
  }

  const handleImportFolder = async () => {
    try {
      const input = document.createElement('input')
      input.type = 'file'
      input.setAttribute('webkitdirectory', '')
      input.setAttribute('directory', '')
      input.onchange = async () => {
        if (!input.files || input.files.length === 0) return
        setImporting(true)
        try {
          const fileList = input.files
          // Derive skill name from common path prefix (top folder name)
          const firstPath = fileList[0].webkitRelativePath || fileList[0].name
          const skillName = firstPath.split('/')[0]

          const files: { path: string; content: string }[] = []
          for (let i = 0; i < fileList.length; i++) {
            const f = fileList[i]
            const relPath = (f.webkitRelativePath || f.name).replace(`${skillName}/`, '')
            const content = await f.text()
            files.push({ path: relPath, content })
          }

          await rpc('skills.import', { name: skillName, files }, `Skill "${skillName}" 已导入`)
          await loadSkills()
        } catch (e: any) {
          setError(`导入失败: ${e.message || e}`)
        } finally {
          setImporting(false)
        }
      }
      input.click()
    } catch (e: any) {
      setError(`导入失败: ${e.message || e}`)
    }
  }

  const handleImportZip = async () => {
    try {
      const input = document.createElement('input')
      input.type = 'file'
      input.accept = '.zip'
      input.onchange = async () => {
        if (!input.files || input.files.length === 0) return
        setImporting(true)
        try {
          const zipFile = input.files[0]
          const arrayBuffer = await zipFile.arrayBuffer()
          const { readZipEntries } = await import('../../utils/zip-reader')
          const files = readZipEntries(arrayBuffer)
          const skillName = zipFile.name.replace(/\.zip$/i, '')
          await rpc('skills.import', { name: skillName, files }, `Skill "${skillName}" 已导入`)
          await loadSkills()
        } catch (e: any) {
          setError(`ZIP 导入失败: ${e.message || e}`)
        } finally {
          setImporting(false)
        }
      }
      input.click()
    } catch (e: any) {
      setError(`导入失败: ${e.message || e}`)
    }
  }

  const handleDelete = async (name: string) => {
    const ok = await useStore.getState().showConfirm('删除 Skill', `确定删除 Skill "${name}"？`, true)
    if (!ok) return
    try {
      await rpc('skills.delete', { name }, `Skill "${name}" 已删除`)
      await loadSkills()
    } catch { /* toast already shown */ }
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>Skills</h3>
        <p className={styles.sectionDesc}>管理技能定义 — 支持文件夹或 ZIP 导入</p>
      </div>
      <div className={styles.contentBody}>
        {error && <p className={styles.toolsError}>{error}</p>}

        <div className={styles.skillActions}>
          <button onClick={handleImportFolder} disabled={importing} className={styles.mcpAddBtn}>
            {importing ? '导入中...' : <><IconFolder size={14} /> 导入文件夹</>}
          </button>
          <button onClick={handleImportZip} disabled={importing} className={styles.mcpAddBtn}>
            {importing ? '导入中...' : <><IconPackage size={14} /> 导入 ZIP</>}
          </button>
        </div>

        {loading ? (
          <p className={styles.toolsEmpty}>加载中...</p>
        ) : (
          <>
            <div className={styles.skillList}>
              {skills.length === 0 ? (
                <p className={styles.toolsEmpty}>暂无已发现的 Skill</p>
              ) : (
                skills.map((skill, i) => (
                  <div key={skill.path || `${skill.name}-${i}`}>
                    <div
                      className={`${styles.skillCard} ${selectedSkill === skill.name ? styles.skillCardActive : ''}`}
                      onClick={() => handleSelectSkill(skill.name)}
                    >
                      <div className={styles.skillCardHeader}>
                        <span className={styles.skillCardName}>{skill.name}</span>
                        <div className={styles.skillBadges}>
                          {skill.version && (
                            <span className={styles.skillBadge}>{skill.version}</span>
                          )}
                          {skill.user_invocable && (
                            <span className={`${styles.skillBadge} ${styles.skillBadgeAccent}`}>user</span>
                          )}
                          {skill.always_active && (
                            <span className={`${styles.skillBadge} ${styles.skillBadgeGreen}`}>active</span>
                          )}
                          <button
                            className={styles.mcpDisconnectBtn}
                            onClick={(e) => { e.stopPropagation(); handleDelete(skill.name) }}
                          >
                            删除
                          </button>
                        </div>
                      </div>
                      {skill.description && (
                        <p className={styles.skillCardDesc}>{skill.description}</p>
                      )}
                    </div>
                    {selectedSkill === skill.name && (
                      <div className={styles.skillDetail}>
                        {loadingContent ? (
                          <p className={styles.toolsEmpty}>加载中...</p>
                        ) : (
                          <pre className={styles.skillDetailContent}>{skillContent}</pre>
                        )}
                      </div>
                    )}
                  </div>
                ))
              )}
            </div>
            <p className={styles.lspInfoText}>
              Skills 从 ~/.loom/skills/ 和插件目录自动发现。点击查看完整定义。
            </p>
          </>
        )}
      </div>
    </>
  )
}

/* ─── Plugins Tab ─── */

function PluginsTab() {
  const [plugins, setPlugins] = useState<PluginInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    async function load() {
      setLoading(true)
      setError(null)
      try {
        const res = await loomRpc<{ plugins: PluginInfo[] }>('plugins.list')
        if (!cancelled) setPlugins(res.plugins ?? [])
      } catch (e: any) {
        if (!cancelled) setError(`加载失败: ${e.message || e}`)
      } finally {
        if (!cancelled) setLoading(false)
      }
    }
    load()
    return () => { cancelled = true }
  }, [])

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>Plugins</h3>
        <p className={styles.sectionDesc}>已发现的插件包</p>
      </div>
      <div className={styles.contentBody}>
        {error && <p className={styles.toolsError}>{error}</p>}
        {loading ? (
          <p className={styles.toolsEmpty}>加载中...</p>
        ) : (
          <>
            <div className={styles.pluginList}>
              {plugins.length === 0 ? (
                <p className={styles.toolsEmpty}>暂无已发现的插件</p>
              ) : (
                plugins.map((plugin) => (
                  <div key={plugin.name} className={styles.pluginCard}>
                    <div className={styles.pluginCardHeader}>
                      <span className={styles.pluginCardName}>{plugin.name}</span>
                      {plugin.version && (
                        <span className={styles.skillBadge}>{plugin.version}</span>
                      )}
                    </div>
                    {plugin.description && (
                      <p className={styles.pluginCardDesc}>{plugin.description}</p>
                    )}
                    <div className={styles.pluginCardMeta}>
                      {plugin.skill_count != null && (
                        <span className={styles.pluginMetaItem}>Skills: {plugin.skill_count}</span>
                      )}
                      {plugin.mcp_server_count != null && (
                        <span className={styles.pluginMetaItem}>MCP: {plugin.mcp_server_count}</span>
                      )}
                    </div>
                    {plugin.path && (
                      <div className={styles.pluginPath}>{plugin.path}</div>
                    )}
                  </div>
                ))
              )}
            </div>
            <p className={styles.lspInfoText}>
              插件从 ~/.loom/skills/ 目录递归发现（最深 4 层）。支持 Claude Code 和 OpenClaw SKILL.md 格式。
            </p>
          </>
        )}
      </div>
    </>
  )
}

/* ─── About Tab ─── */

function AboutTab({ wsState }: { wsState: string }) {
  const [health, setHealth] = useState<SystemHealth | null>(null)
  const [healthError, setHealthError] = useState(false)
  const [appVersion, setAppVersion] = useState('...')
  const [updateStatus, setUpdateStatus] = useState<'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'no-update' | 'error'>('idle')
  const [updateVersion, setUpdateVersion] = useState<string | null>(null)
  const [updateError, setUpdateError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false

    window.hana.getAppVersion().then((v) => { if (!cancelled) setAppVersion(v) })

    loomRpc<SystemHealth>('system.health')
      .then((data) => { if (!cancelled) setHealth(data) })
      .catch(() => { if (!cancelled) setHealthError(true) })

    // Listen for update events from main process
    window.hana.onUpdateAvailable((info: any) => {
      if (!cancelled) {
        setUpdateStatus('available')
        setUpdateVersion(info?.version ?? null)
      }
    })
    window.hana.onUpdateNotAvailable(() => {
      if (!cancelled) setUpdateStatus('no-update')
    })
    window.hana.onUpdateDownloaded(() => {
      if (!cancelled) setUpdateStatus('downloaded')
    })
    window.hana.onUpdateError((msg: string) => {
      if (!cancelled) {
        setUpdateStatus('error')
        setUpdateError(msg)
      }
    })

    return () => { cancelled = true }
  }, [])

  const handleCheckUpdate = async () => {
    setUpdateStatus('checking')
    setUpdateError(null)
    try {
      await window.hana.checkForUpdates()
    } catch {
      setUpdateStatus('error')
      setUpdateError('检查更新失败')
    }
  }

  const handleDownload = async () => {
    setUpdateStatus('downloading')
    try {
      await window.hana.downloadUpdate()
    } catch {
      setUpdateStatus('error')
      setUpdateError('下载失败')
    }
  }

  const handleInstall = () => {
    window.hana.installUpdate()
  }

  const updateLabel = () => {
    switch (updateStatus) {
      case 'checking': return '正在检查更新...'
      case 'available': return updateVersion ? `发现新版本 ${updateVersion}` : '发现新版本'
      case 'downloading': return '正在下载更新...'
      case 'downloaded': return '更新已就绪，重启后生效'
      case 'no-update': return '已是最新版本'
      case 'error': return updateError ?? '检查更新失败'
      default: return ''
    }
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>关于</h3>
        <p className={styles.sectionDesc}>版本、更新和连接信息</p>
      </div>
      <div className={styles.contentBody}>
        <div className={styles.aboutSection}>
          <div className={styles.aboutRow}>
            <span className={styles.aboutLabel}>版本</span>
            <span className={styles.aboutValue}>{appVersion}</span>
          </div>
          <div className={styles.aboutRow}>
            <span className={styles.aboutLabel}>状态</span>
            <span className={`${styles.aboutValue} ${
              (health?.status === 'ok' || wsState === 'connected') ? styles.aboutValueGreen : styles.aboutValueAmber
            }`}>
              {health ? health.status : wsState === 'connected' ? '已连接' : wsState}
            </span>
          </div>
          {health && (
            <>
              <div className={styles.aboutRow}>
                <span className={styles.aboutLabel}>Agent 数量</span>
                <span className={styles.aboutValue}>{health.agent_count}</span>
              </div>
              <div className={styles.aboutRow}>
                <span className={styles.aboutLabel}>工具数量</span>
                <span className={styles.aboutValue}>{health.tool_count}</span>
              </div>
            </>
          )}
          <div className={styles.aboutRow}>
            <span className={styles.aboutLabel}>连接状态</span>
            <span className={`${styles.aboutValue} ${wsState === 'connected' ? styles.aboutValueGreen : styles.aboutValueAmber}`}>
              {wsState === 'connected' ? '已连接' : wsState}
            </span>
          </div>

          {/* Auto-update section */}
          <div className={styles.aboutRow}>
            <div>
              <span className={styles.aboutLabel}>自动更新</span>
              {updateLabel() && (
                <p style={{ fontSize: 11, color: updateStatus === 'available' || updateStatus === 'downloaded' ? 'var(--accent)' : updateStatus === 'error' ? 'var(--red)' : 'var(--text-muted)', margin: '2px 0 0' }}>
                  {updateLabel()}
                </p>
              )}
            </div>
            <div style={{ display: 'flex', gap: 6 }}>
              {(updateStatus === 'idle' || updateStatus === 'no-update' || updateStatus === 'error') && (
                <button className={styles.mcpDisconnectBtn} onClick={handleCheckUpdate}>
                  检查更新
                </button>
              )}
              {updateStatus === 'available' && (
                <button className={styles.mcpConnectBtn} onClick={handleDownload}>
                  下载更新
                </button>
              )}
              {updateStatus === 'downloaded' && (
                <button className={styles.mcpConnectBtn} onClick={handleInstall}>
                  立即重启
                </button>
              )}
            </div>
          </div>

          {healthError && (
            <p className={styles.toolsError}>系统信息加载失败</p>
          )}
          <p className={styles.aboutFooter}>
            本地优先的私人 AI 助理。所有数据存储在本地。
          </p>
        </div>
      </div>
    </>
  )
}

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
  { id: 'midnight', label: '午夜蓝', bg: '#0b1120', surface: '#0f172a', text: '#e2e8f0', accent: '#a5bff8' },
  { id: 'warm-paper', label: '暖纸', bg: '#fdfbf7', surface: '#f5f0e8', text: '#2d2416', accent: '#b05a30' },
  { id: 'neon-pink', label: '霓虹粉', bg: '#1a1a1d', surface: '#222225', text: '#f0e0e8', accent: '#e6397c' },
  { id: 'ember', label: '余烬', bg: '#000026', surface: '#060630', text: '#ffe0c0', accent: '#ff770f' },
]

type Tab = 'appearance' | 'agent' | 'models' | 'mcp' | 'lsp' | 'skills' | 'plugins' | 'kg' | 'about'

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
                        onClick={() => setTheme(t.id)}
                        className={`${styles.themeCard} ${theme === t.id ? styles.themeCardActive : ''}`}
                      >
                        <div className={styles.themePreview} style={{ background: t.bg }}>
                          <div className={styles.themePreviewInner}>
                            <div className={styles.themePreviewSidebar} style={{ background: t.surface, borderRight: '1px solid rgba(128,128,128,0.1)' }}>
                              <div style={{ width: '70%', height: 3, background: t.accent, borderRadius: 2, marginBottom: 4 }} />
                              <div style={{ width: '90%', height: 2, background: `${t.text}22`, borderRadius: 1, marginBottom: 3 }} />
                              <div style={{ width: '60%', height: 2, background: `${t.text}22`, borderRadius: 1 }} />
                            </div>
                            <div className={styles.themePreviewMain} style={{ background: t.bg }}>
                              <div>
                                <div style={{ width: '50%', height: 2, background: `${t.text}44`, borderRadius: 1, marginBottom: 3 }} />
                                <div style={{ width: '80%', height: 2, background: `${t.text}22`, borderRadius: 1 }} />
                              </div>
                              <div style={{ width: '70%', height: 10, background: t.surface, borderRadius: 4, border: '1px solid rgba(128,128,128,0.1)' }} />
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

/* ─── MCP Tab ─── */

function McpTab() {
  const [servers, setServers] = useState<string[]>([])
  const [serverHealth, setServerHealth] = useState<Record<string, boolean | null>>({})
  const [mcpTools, setMcpTools] = useState<McpTool[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [showForm, setShowForm] = useState(false)

  // Form state
  const [formName, setFormName] = useState('')
  const [formTransport, setFormTransport] = useState<'stdio' | 'http'>('stdio')
  const [formCommand, setFormCommand] = useState('')
  const [formArgs, setFormArgs] = useState('')
  const [formUrl, setFormUrl] = useState('')
  const [connecting, setConnecting] = useState(false)

  const loadData = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [serversRes, toolsRes] = await Promise.allSettled([
        loomRpc<{ servers: string[] }>('mcp.list_servers'),
        loomRpc<{ tools: McpTool[] }>('mcp.list_tools'),
      ])

      if (serversRes.status === 'fulfilled') {
        const srvList = serversRes.value.servers ?? []
        setServers(srvList)
        // Check health for each server
        const healthMap: Record<string, boolean | null> = {}
        await Promise.allSettled(
          srvList.map(async (name) => {
            try {
              const res = await loomRpc<{ healthy: boolean }>('mcp.server_health', { name })
              healthMap[name] = res.healthy
            } catch {
              healthMap[name] = null
            }
          })
        )
        setServerHealth(healthMap)
      } else {
        setError(`加载 MCP 服务列表失败: ${serversRes.reason?.message || serversRes.reason}`)
      }

      if (toolsRes.status === 'fulfilled') {
        setMcpTools(toolsRes.value.tools ?? [])
      }
    } catch (e: any) {
      setError(`加载失败: ${e.message || e}`)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  const handleConnect = async () => {
    if (!formName.trim()) return
    setConnecting(true)
    try {
      const params: Record<string, unknown> = {
        name: formName.trim(),
        transport: formTransport,
      }
      if (formTransport === 'stdio') {
        params.command = formCommand.trim()
        params.args = formArgs.trim() ? formArgs.split(',').map((s) => s.trim()) : []
      } else {
        params.url = formUrl.trim()
      }
      await rpc('mcp.connect', params, `MCP "${formName}" 已连接`)
      setShowForm(false)
      setFormName('')
      setFormCommand('')
      setFormArgs('')
      setFormUrl('')
      await loadData()
    } catch (e: any) {
      setError(`连接失败: ${e.message || e}`)
    } finally {
      setConnecting(false)
    }
  }

  const handleDisconnect = async (name: string) => {
    try {
      await rpc('mcp.disconnect', { name }, `MCP "${name}" 已断开`)
      await loadData()
    } catch { /* toast already shown */ }
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>MCP 服务</h3>
        <p className={styles.sectionDesc}>管理 Model Context Protocol 服务器连接</p>
      </div>
      <div className={styles.contentBody}>
        {error && <p className={styles.toolsError}>{error}</p>}
        {loading ? (
          <p className={styles.toolsEmpty}>加载中...</p>
        ) : (
          <>
            {/* Server list */}
            <div className={styles.mcpServerList}>
              {servers.length === 0 ? (
                <p className={styles.toolsEmpty}>暂无已连接的 MCP 服务器</p>
              ) : (
                servers.map((name) => (
                  <div key={name} className={styles.mcpServerItem}>
                    <div className={styles.mcpServerHeader}>
                      <div className={styles.mcpServerNameRow}>
                        <span className={styles.mcpServerStatus} data-healthy={serverHealth[name] === true ? 'true' : serverHealth[name] === false ? 'false' : 'unknown'} />
                        <span className={styles.mcpServerName}>{name}</span>
                      </div>
                      <button className={styles.mcpDisconnectBtn} onClick={() => handleDisconnect(name)}>
                        断开
                      </button>
                    </div>
                    {mcpTools.length > 0 && (
                      <div className={styles.toolsBadgeGrid}>
                        {mcpTools.map((tool) => (
                          <span key={tool.name} className={styles.toolsBadge} title={tool.description}>
                            {tool.name}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                ))
              )}
            </div>

            {/* Add form toggle */}
            {!showForm ? (
              <button className={styles.mcpAddBtn} onClick={() => setShowForm(true)}>
                + 添加服务器
              </button>
            ) : (
              <div className={styles.mcpAddForm}>
                <div className={styles.mcpFormRow}>
                  <label className={styles.mcpFormLabel}>名称</label>
                  <input
                    className={styles.mcpFormInput}
                    value={formName}
                    onChange={(e) => setFormName(e.target.value)}
                    placeholder="server-name"
                  />
                </div>
                <div className={styles.mcpFormRow}>
                  <label className={styles.mcpFormLabel}>传输类型</label>
                  <div className={styles.mcpTransportToggle}>
                    <button
                      className={`${styles.mcpTransportBtn} ${formTransport === 'stdio' ? styles.mcpTransportActive : ''}`}
                      onClick={() => setFormTransport('stdio')}
                    >
                      stdio
                    </button>
                    <button
                      className={`${styles.mcpTransportBtn} ${formTransport === 'http' ? styles.mcpTransportActive : ''}`}
                      onClick={() => setFormTransport('http')}
                    >
                      HTTP
                    </button>
                  </div>
                </div>
                {formTransport === 'stdio' ? (
                  <>
                    <div className={styles.mcpFormRow}>
                      <label className={styles.mcpFormLabel}>命令</label>
                      <input
                        className={styles.mcpFormInput}
                        value={formCommand}
                        onChange={(e) => setFormCommand(e.target.value)}
                        placeholder="npx, node, python..."
                      />
                    </div>
                    <div className={styles.mcpFormRow}>
                      <label className={styles.mcpFormLabel}>参数 (逗号分隔)</label>
                      <input
                        className={styles.mcpFormInput}
                        value={formArgs}
                        onChange={(e) => setFormArgs(e.target.value)}
                        placeholder="-y, @modelcontextprotocol/server-xxx"
                      />
                    </div>
                  </>
                ) : (
                  <div className={styles.mcpFormRow}>
                    <label className={styles.mcpFormLabel}>URL</label>
                    <input
                      className={styles.mcpFormInput}
                      value={formUrl}
                      onChange={(e) => setFormUrl(e.target.value)}
                      placeholder="http://localhost:8080/sse"
                    />
                  </div>
                )}
                <div className={styles.mcpFormActions}>
                  <button className={styles.mcpCancelBtn} onClick={() => setShowForm(false)}>
                    取消
                  </button>
                  <button
                    className={styles.mcpConnectBtn}
                    onClick={handleConnect}
                    disabled={connecting || !formName.trim()}
                  >
                    {connecting ? '连接中...' : '连接'}
                  </button>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </>
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
      await loadData()
    } catch (e: any) {
      setError(`停止失败: ${e.message || e}`)
    }
  }

  const handleShutdownAll = async () => {
    try {
      await loomRpc('lsp.shutdown_all', {})
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
              <div style={{ marginTop: 16 }}>
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

  useEffect(() => {
    let cancelled = false

    loomRpc<SystemHealth>('system.health')
      .then((data) => { if (!cancelled) setHealth(data) })
      .catch(() => { if (!cancelled) setHealthError(true) })

    return () => { cancelled = true }
  }, [])

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>关于</h3>
        <p className={styles.sectionDesc}>版本和连接信息</p>
      </div>
      <div className={styles.contentBody}>
        <div className={styles.aboutSection}>
          <div className={styles.aboutRow}>
            <span className={styles.aboutLabel}>版本</span>
            <span className={styles.aboutValue}>
              {health ? health.version : healthError ? 'v0.2.0' : '...'}
            </span>
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

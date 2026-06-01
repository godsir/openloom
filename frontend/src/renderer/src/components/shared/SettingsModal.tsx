import { useState, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { IconFolder, IconPackage, IconRefresh, IconSettings, IconBot, IconBox, IconBrain, IconBarChart, IconTerminal, IconSparkles, IconPawPrint, IconInfo, IconSearch, IconChevronRight, IconChevronDown } from '../../utils/icons'
import Overlay from './Overlay'
import Select from './Select'
import AgentConfigPanel from './AgentConfigPanel'
import ModelConfigPanel from './ModelConfigPanel'
import VisionConfigSection from './VisionConfigSection'
import AuxiliaryConfigSection from './AuxiliaryConfigSection'
import WorkspaceTab from './WorkspaceTab'
import PetTab from './PetTab'
import KnowledgeGraphPanel from '../kg/KnowledgeGraphPanel'
import TokenUsagePanel from './TokenUsagePanel'
import { type ThemeId, type FontSizeId, FONT_SIZE_MAP } from '../../stores/ui'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'
import styles from './SettingsModal.module.css'
import logoDev from '../../assets/loom_logo_dev.png'
import logoRelease from '../../assets/loom_logo.png'

const THEMES: { id: ThemeId; label: string; bg: string; surface: string; text: string; accent: string }[] = [
  { id: 'dark', label: '暗色', bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' },
  { id: 'light', label: '亮色', bg: '#ffffff', surface: '#f1f5f9', text: '#0f172a', accent: '#0d9488' },
  { id: 'midnight', label: '星夜', bg: '#0b1120', surface: '#0f172a', text: '#e2e8f0', accent: '#a5bff8' },
  { id: 'warm-paper', label: '素笺', bg: '#fdfbf7', surface: '#f5f0e8', text: '#2d2416', accent: '#b05a30' },
  { id: 'neon-pink', label: '紫夜', bg: '#1a1a1d', surface: '#222225', text: '#f0e0e8', accent: '#e6397c' },
  { id: 'ember', label: '熔岩', bg: '#000026', surface: '#060630', text: '#ffe0c0', accent: '#ff770f' },
  { id: 'navy-gold', label: '鎏金', bg: '#050F2E', surface: '#0A1A45', text: '#e2e8f0', accent: '#FFE76F' },
  { id: 'umber-cream', label: '摩卡', bg: '#2D1B14', surface: '#3D271D', text: '#fff8f0', accent: '#D8C7B5' },
  { id: 'custom', label: '自定义', bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' },
]

function hexToRgb(hex: string): [number, number, number] {
  const v = parseInt(hex.replace('#', ''), 16)
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255]
}

function applyCustomTheme(c: { bg: string; surface: string; text: string; accent: string }) {
  const root = document.documentElement
  const [ar, ag, ab] = hexToRgb(c.accent)
  const isLight = c.bg > '#888'
  const textAlpha = isLight ? 0.08 : 0.08
  const borderBase = isLight ? `rgba(0,0,0,0.06)` : `rgba(255,255,255,0.06)`

  root.style.setProperty('--bg', c.bg)
  root.style.setProperty('--bg-surface', c.surface)
  root.style.setProperty('--bg-card', c.surface)
  root.style.setProperty('--bg-active', isLight ? 'rgba(0,0,0,0.04)' : 'rgba(255,255,255,0.04)')
  root.style.setProperty('--bg-overlay', isLight ? 'rgba(255,255,255,0.70)' : 'rgba(0,0,0,0.72)')
  root.style.setProperty('--bg-input', c.surface)
  root.style.setProperty('--bg-tooltip', c.surface)
  root.style.setProperty('--text', c.text)
  root.style.setProperty('--text-secondary', isLight ? 'rgba(0,0,0,0.55)' : 'rgba(255,255,255,0.55)')
  root.style.setProperty('--text-muted', isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.28)')
  root.style.setProperty('--text-light', isLight ? 'rgba(0,0,0,0.55)' : 'rgba(255,255,255,0.55)')
  root.style.setProperty('--border', borderBase)
  root.style.setProperty('--border-default', isLight ? 'rgba(0,0,0,0.10)' : 'rgba(255,255,255,0.10)')
  root.style.setProperty('--border-accent', `rgba(${ar},${ag},${ab},0.28)`)
  root.style.setProperty('--border-light', `rgba(${ar},${ag},${ab},0.06)`)
  root.style.setProperty('--accent', c.accent)
  root.style.setProperty('--accent-hover', c.accent)
  root.style.setProperty('--accent-rgb', `${ar},${ag},${ab}`)
  root.style.setProperty('--accent-subtle', `rgba(${ar},${ag},${ab},0.10)`)
  root.style.setProperty('--accent-medium', `rgba(${ar},${ag},${ab},0.16)`)
  root.style.setProperty('--accent-glow', `rgba(${ar},${ag},${ab},0.22)`)
  root.style.setProperty('--accent-light', `rgba(${ar},${ag},${ab},0.12)`)
  root.style.setProperty('--accent-strong', `rgba(${ar},${ag},${ab},0.22)`)
  root.style.setProperty('--shadow', isLight ? '0 1px 3px rgba(0,0,0,0.08)' : '0 1px 3px rgba(0,0,0,0.5)')
  root.style.setProperty('--shadow-md', isLight ? '0 4px 16px rgba(0,0,0,0.08)' : '0 4px 16px rgba(0,0,0,0.6)')
  root.style.setProperty('--shadow-lg', isLight ? '0 8px 32px rgba(0,0,0,0.10)' : '0 8px 32px rgba(0,0,0,0.7)')
  root.style.setProperty('--shadow-glass', isLight ? '0 8px 32px rgba(0,0,0,0.06)' : '0 8px 32px rgba(0,0,0,0.5)')
}

type Tab = 'software' | 'agent' | 'models' | 'workspace' | 'mcp' | 'skills' | 'plugins' | 'pet' | 'kg' | 'token' | 'about'

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
  const [tab, setTab] = useState<Tab>('software')

  const tabs: { id: Tab; label: string; icon: React.ReactNode }[] = [
    { id: 'software', label: '通用', icon: <IconSettings size={14} /> },
    { id: 'agent', label: '智能体', icon: <IconBot size={14} /> },
    { id: 'models', label: '模型', icon: <IconBox size={14} /> },
    { id: 'kg', label: '记忆系统', icon: <IconBrain size={14} /> },
    { id: 'token', label: 'Token 用量', icon: <IconBarChart size={14} /> },
    { id: 'workspace', label: '工作空间', icon: <IconFolder size={14} /> },
    { id: 'mcp', label: 'MCP / LSP', icon: <IconTerminal size={14} /> },
    { id: 'skills', label: '技能', icon: <IconSparkles size={14} /> },
    { id: 'plugins', label: '插件', icon: <IconPackage size={14} /> },
    { id: 'pet', label: '桌宠', icon: <IconPawPrint size={14} /> },
    { id: 'about', label: '关于', icon: <IconInfo size={14} /> },
  ]

  return (
    <Overlay open={open} onClose={onClose} size="lg">
      <div className={styles.layout}>
        <div className={styles.nav}>
          {tabs.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={`${styles.navItem} ${tab === t.id ? styles.navActive : ''}`}
            >
              <span className={styles.navIcon}>{t.icon}</span>
              {t.label}
            </button>
          ))}
          <div className={styles.navFooter}>设置</div>
        </div>

        <div className={styles.content}>
          {tab === 'software' && <SoftwareTab theme={theme} setTheme={setTheme} />}

          {tab === 'agent' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>智能体配置</h3>
                <p className={styles.sectionDesc}>管理智能体角色和行为</p>
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
                <AuxiliaryConfigSection />
              </div>
            </>
          )}

          {tab === 'workspace' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>工作空间</h3>
                <p className={styles.sectionDesc}>配置文件操作的默认工作目录</p>
              </div>
              <div className={styles.contentBody}>
                <WorkspaceTab />
              </div>
            </>
          )}

          {tab === 'mcp' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>MCP / LSP</h3>
                <p className={styles.sectionDesc}>外部工具协议和语言服务器</p>
              </div>
              <div className={styles.contentBody}>
                <McpTab />
                <div style={{ marginTop: 24 }} />
                <LspTab />
              </div>
            </>
          )}
          {tab === 'skills' && <SkillsTab />}
          {tab === 'plugins' && <PluginsTab />}
          {tab === 'pet' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>桌宠</h3>
                <p className={styles.sectionDesc}>桌面宠物伙伴设置</p>
              </div>
              <div className={styles.contentBody}>
                <PetTab />
              </div>
            </>
          )}
          {tab === 'kg' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>记忆系统</h3>
                <p className={styles.sectionDesc}>浏览和管理 AI 的知识图谱与认知记录</p>
              </div>
              <div className={styles.contentBody}>
                <KnowledgeGraphPanel />
              </div>
            </>
          )}
          {tab === 'token' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>Token 用量</h3>
                <p className={styles.sectionDesc}>查看 Token 消耗统计和历史趋势</p>
              </div>
              <div className={styles.contentBody}>
                <TokenUsagePanel />
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

const UI_FONTS = [
  { value: '', label: '系统默认' },
  { value: '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif', label: 'Inter' },
  { value: '"Microsoft YaHei", "微软雅黑", sans-serif', label: '微软雅黑' },
  { value: '"Microsoft YaHei UI", "微软雅黑 UI", sans-serif', label: '微软雅黑 UI' },
  { value: '"Noto Sans SC", "思源黑体", sans-serif', label: '思源黑体' },
  { value: '"Noto Serif SC", "思源宋体", serif', label: '思源宋体' },
  { value: '"PingFang SC", "苹方", -apple-system, sans-serif', label: 'PingFang SC' },
  { value: '"HarmonyOS Sans SC", "鸿蒙字体", sans-serif', label: '鸿蒙字体' },
  { value: '"Segoe UI", system-ui, sans-serif', label: 'Segoe UI' },
  { value: '"SF Pro", -apple-system, BlinkMacSystemFont, sans-serif', label: 'SF Pro' },
  { value: 'system-ui, -apple-system, sans-serif', label: '系统 UI 字体' },
]

const CODE_FONTS = [
  { value: '', label: '系统默认' },
  { value: '"JetBrains Mono", "Cascadia Code", "Fira Code", "Consolas", monospace', label: 'JetBrains Mono' },
  { value: '"Cascadia Code", "JetBrains Mono", "Fira Code", "Consolas", monospace', label: 'Cascadia Code' },
  { value: '"Fira Code", "JetBrains Mono", "Cascadia Code", "Consolas", monospace', label: 'Fira Code' },
  { value: '"Consolas", "JetBrains Mono", "Cascadia Code", monospace', label: 'Consolas' },
  { value: '"Source Code Pro", "JetBrains Mono", "Consolas", monospace', label: 'Source Code Pro' },
  { value: '"SF Mono", "JetBrains Mono", "Consolas", monospace', label: 'SF Mono' },
  { value: '"Sarasa Mono SC", "更纱黑体等宽", "JetBrains Mono", monospace', label: '更纱黑体等宽' },
]

function SoftwareTab({ theme, setTheme }: { theme: string; setTheme: (t: any) => void }) {
  const fontSize = useStore((s) => s.fontSize)
  const setFontSize = useStore((s) => s.setFontSize)
  const [autoStart, setAutoStart] = useState(false)
  const [closeToTray, setCloseToTray] = useState(true)
  const [autoTitle, setAutoTitle] = useState(false)
  const [uiFont, setUiFont] = useState('')
  const [codeFont, setCodeFont] = useState('')
  const [loaded, setLoaded] = useState(false)
  const [customColors, setCustomColors] = useState({ bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' })

  useEffect(() => {
    Promise.all([
      window.loom.getPreference('autoStart', false),
      window.loom.getPreference('closeToTray', true),
      window.loom.getPreference('autoTitle', false),
      window.loom.getPreference('uiFont', ''),
      window.loom.getPreference('codeFont', ''),
      window.loom.getPreference('customTheme', { bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' }),
    ]).then(([as, ct, at, uf, cf, cc]) => {
      setAutoStart(as)
      setCloseToTray(ct)
      setAutoTitle(at)
      if (uf) document.documentElement.style.setProperty('--font', uf as string)
      if (cf) document.documentElement.style.setProperty('--font-mono', cf as string)
      setUiFont(uf as string)
      setCodeFont(cf as string)
      setCustomColors(cc as typeof customColors)
      setLoaded(true)
    })
  }, [])

  const handleCustomColor = async (key: keyof typeof customColors, val: string) => {
    const next = { ...customColors, [key]: val }
    setCustomColors(next)
    applyCustomTheme(next)
    await window.loom.setPreference('customTheme', next)
  }

  const handleAutoTitle = async (val: boolean) => {
    setAutoTitle(val)
    await window.loom.setPreference('autoTitle', val)
    useStore.getState().addToast({ type: 'success', message: val ? '已开启 AI 自动命名' : '已关闭 AI 自动命名' })
  }

  const handleAutoStart = async (val: boolean) => {
    setAutoStart(val)
    await window.loom.setPreference('autoStart', val)
    useStore.getState().addToast({ type: 'success', message: val ? '已开启开机自启动' : '已关闭开机自启动' })
  }

  const handleCloseToTray = async (val: boolean) => {
    setCloseToTray(val)
    await window.loom.setPreference('closeToTray', val)
    useStore.getState().addToast({ type: 'success', message: val ? '关闭按钮将最小化到托盘' : '关闭按钮将退出程序' })
  }

  const handleUiFont = async (val: string) => {
    setUiFont(val)
    if (val) {
      document.documentElement.style.setProperty('--font', val)
    } else {
      document.documentElement.style.removeProperty('--font')
    }
    await window.loom.setPreference('uiFont', val)
    useStore.getState().addToast({ type: 'success', message: val ? '界面字体已更新' : '已恢复系统默认字体' })
  }

  const handleCodeFont = async (val: string) => {
    setCodeFont(val)
    if (val) {
      document.documentElement.style.setProperty('--font-mono', val)
    } else {
      document.documentElement.style.removeProperty('--font-mono')
    }
    await window.loom.setPreference('codeFont', val)
    useStore.getState().addToast({ type: 'success', message: val ? '代码字体已更新' : '已恢复系统默认等宽字体' })
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>通用</h3>
        <p className={styles.sectionDesc}>外观、字体与软件行为</p>
      </div>
      <div className={styles.contentBody}>
        {/* ── 外观 ── */}
        <div className={styles.aboutSection}>
          <div className={styles.themeLabel}>外观</div>

          {/* Theme */}
          <div className={styles.themeGrid}>
            {THEMES.map((t) => {
              const previewColors = t.id === 'custom' ? customColors : { bg: t.bg, surface: t.surface, text: t.text, accent: t.accent }
              return (
              <button
                key={t.id}
                onClick={() => {
                  setTheme(t.id)
                  if (t.id === 'custom') applyCustomTheme(customColors)
                  useStore.getState().addToast({ type: 'success', message: `主题已切换为${t.label}` })
                }}
                className={`${styles.themeCard} ${theme === t.id ? styles.themeCardActive : ''}`}
              >
                <div
                  className={styles.themePreview}
                  style={{
                    '--pv-bg': previewColors.bg,
                    '--pv-surface': previewColors.surface,
                    '--pv-accent': previewColors.accent,
                    '--pv-text-13': previewColors.text + '22',
                    '--pv-text-27': previewColors.text + '44',
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
            )})}
          </div>

          {theme === 'custom' && (
            <div className={styles.customColors}>
              <div className={styles.customColorRow}>
                <label className={styles.customColorLabel}>
                  <span className={styles.customColorSwatch} style={{ background: customColors.bg }} />
                  背景色
                </label>
                <input
                  type="color"
                  value={customColors.bg}
                  onChange={e => handleCustomColor('bg', e.target.value)}
                  className={styles.customColorInput}
                />
              </div>
              <div className={styles.customColorRow}>
                <label className={styles.customColorLabel}>
                  <span className={styles.customColorSwatch} style={{ background: customColors.surface }} />
                  表面色
                </label>
                <input
                  type="color"
                  value={customColors.surface}
                  onChange={e => handleCustomColor('surface', e.target.value)}
                  className={styles.customColorInput}
                />
              </div>
              <div className={styles.customColorRow}>
                <label className={styles.customColorLabel}>
                  <span className={styles.customColorSwatch} style={{ background: customColors.text }} />
                  文字色
                </label>
                <input
                  type="color"
                  value={customColors.text}
                  onChange={e => handleCustomColor('text', e.target.value)}
                  className={styles.customColorInput}
                />
              </div>
              <div className={styles.customColorRow}>
                <label className={styles.customColorLabel}>
                  <span className={styles.customColorSwatch} style={{ background: customColors.accent }} />
                  强调色
                </label>
                <input
                  type="color"
                  value={customColors.accent}
                  onChange={e => handleCustomColor('accent', e.target.value)}
                  className={styles.customColorInput}
                />
              </div>
            </div>
          )}

          {loaded && (
            <>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>字体大小</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>调整对话和输入区域的文字大小</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  {(Object.entries(FONT_SIZE_MAP) as [FontSizeId, { label: string; px: number }][]).map(([id, { label }]) => (
                    <button
                      key={id}
                      className={`${styles.mcpTransportBtn} ${fontSize === id ? styles.mcpTransportActive : ''}`}
                      onClick={() => setFontSize(id)}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>
            </>
          )}
        </div>

        <hr className={styles.sectionDivider} />

        {/* ── 字体 ── */}
        <div className={styles.aboutSection}>
          <div className={styles.themeLabel}>字体</div>
          {!loaded ? (
            <p className={styles.toolsEmpty}>加载中...</p>
          ) : (
            <>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>界面字体</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>菜单、对话和通用文本的字体</p>
                </div>
                <div style={{ width: 170 }}>
                  <Select
                    value={uiFont}
                    options={UI_FONTS}
                    onChange={(v) => handleUiFont(v)}
                    variant="form"
                  />
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>代码字体</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>代码块、Shell 输出等位置的等宽字体</p>
                </div>
                <div style={{ width: 170 }}>
                  <Select
                    value={codeFont}
                    options={CODE_FONTS}
                    onChange={(v) => handleCodeFont(v)}
                    variant="form"
                  />
                </div>
              </div>
              <div className={styles.fontPreview}>
                <div className={styles.fontPreviewLabel}>预览</div>
                <pre className={styles.fontPreviewCode} style={{ fontFamily: codeFont || undefined }}>
                  {`fn main() {\n  println!("你好，openLoom");\n}`}
                </pre>
              </div>
            </>
          )}
        </div>

        <hr className={styles.sectionDivider} />

        {/* ── 行为 ── */}
        <div className={styles.aboutSection}>
          <div className={styles.themeLabel}>行为</div>
          {!loaded ? (
            <p className={styles.toolsEmpty}>加载中...</p>
          ) : (
            <>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>AI 自动命名会话</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>首次对话后由 AI 提取 2-7 字标题</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    onClick={() => handleAutoTitle(true)}
                    className={`${styles.mcpTransportBtn} ${autoTitle ? styles.mcpTransportActive : ''}`}
                  >
                    开启
                  </button>
                  <button
                    onClick={() => handleAutoTitle(false)}
                    className={`${styles.mcpTransportBtn} ${!autoTitle ? styles.mcpTransportActive : ''}`}
                  >
                    关闭
                  </button>
                </div>
              </div>
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
            </>
          )}
        </div>

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

  const handleRefresh = async () => {
    setRefreshing(true)
    await loadData()
    setRefreshing(false)
  }

  return (
    <>
      <div className={styles.contentBody}>
        <div className={styles.sectionHeaderRow}>
          <h4 className={styles.sectionSubTitle}>MCP 服务</h4>
          {!editing && (
            <button className={styles.mcpAddBtn} onClick={startCreate}>
              + 添加服务器
            </button>
          )}
        </div>
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
      <div className={styles.contentBody}>
        <div className={styles.sectionHeaderRow}>
          <h4 className={styles.sectionSubTitle}>LSP 服务</h4>
          {!showForm && (
            <button className={styles.mcpAddBtn} onClick={() => setShowForm(true)}>
              + 启动语言服务器
            </button>
          )}
        </div>
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
            {showForm && (
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

/** Derive a source category from a skill's on-disk path. */
function skillSource(path: string | undefined): { group: string; icon: string } {
  if (!path) return { group: '其他', icon: 'M' }
  const p = path.replace(/\\/g, '/')
  if (p.includes('.claude/plugins') || p.includes('.claude/skills')) return { group: 'Claude Code', icon: 'C' }
  if (p.includes('.openclaw')) return { group: 'OpenClaw', icon: 'O' }
  if (p.includes('.codex')) return { group: 'Codex', icon: 'X' }
  if (p.includes('.loom/skills')) return { group: 'openLoom 用户', icon: 'L' }
  if (p.includes('.loom/plugins')) return { group: 'openLoom 插件', icon: 'P' }
  return { group: '其他', icon: 'M' }
}

function SkillsTab() {
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [selectedSkill, setSelectedSkill] = useState<string | null>(null)
  const [skillContent, setSkillContent] = useState<string | null>(null)
  const [loadingContent, setLoadingContent] = useState(false)
  const [importing, setImporting] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())

  const toggleGroup = (group: string) => {
    setCollapsedGroups(prev => {
      const next = new Set(prev)
      if (next.has(group)) next.delete(group)
      else next.add(group)
      return next
    })
  }

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

  // Filter + Group
  const q = searchQuery.toLowerCase().trim()
  const filtered = q
    ? skills.filter(s => s.name.toLowerCase().includes(q) || (s.description ?? '').toLowerCase().includes(q))
    : skills
  const grouped: Record<string, { icon: string; skills: SkillInfo[] }> = {}
  for (const s of filtered) {
    const { group, icon } = skillSource(s.path)
    if (!grouped[group]) grouped[group] = { icon, skills: [] }
    grouped[group].skills.push(s)
  }

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

  const renderBody = (raw: string) => {
    const cleaned = raw
      .replace(/^## Skill: [^\n]*\n\n?/, '')
      .replace(/^### Skill: [^\n]*\n\n?/, '')
    return sanitizeHtml(renderMarkdown(cleaned || raw))
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <div className={styles.sectionHeaderRow}>
          <h3 className={styles.sectionTitle}>技能</h3>
          <span className={styles.skillCountBadge}>{skills.length} 个</span>
          <button
            onClick={async () => { setRefreshing(true); await loadSkills(); setRefreshing(false) }}
            disabled={refreshing || loading}
            className={styles.refreshBtn}
            title="重新扫描技能"
          >
            <IconRefresh size={14} />
          </button>
        </div>
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

        {/* Search */}
        <div className={styles.skillSearchWrap}>
          <IconSearch size={13} className={styles.skillSearchIcon} />
          <input
            type="text"
            className={styles.skillSearchInput}
            placeholder="搜索技能名称或描述..."
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
          />
          {searchQuery && (
            <button className={styles.skillSearchClear} onClick={() => setSearchQuery('')}>&times;</button>
          )}
        </div>

        {loading ? (
          <p className={styles.toolsEmpty}>加载中...</p>
        ) : (
          <>
            <div className={styles.skillList}>
              {filtered.length === 0 ? (
                <p className={styles.toolsEmpty}>{searchQuery ? '无匹配结果' : '暂无已发现的 Skill'}</p>
              ) : (
                Object.entries(grouped).map(([group, { icon, skills: groupSkills }]) => (
                  <div key={group} className={styles.skillGroup}>
                    <div
                      className={styles.skillGroupHeader}
                      onClick={() => toggleGroup(group)}
                    >
                      {collapsedGroups.has(group)
                        ? <IconChevronRight size={10} className={styles.skillGroupChevron} />
                        : <IconChevronDown size={10} className={styles.skillGroupChevron} />
                      }
                      <span className={styles.skillGroupIcon}>{icon}</span>
                      <span className={styles.skillGroupName}>{group}</span>
                      <span className={styles.skillGroupCount}>{groupSkills.length}</span>
                    </div>
                    <div className={`${styles.skillGroupBody} ${collapsedGroups.has(group) ? styles.skillGroupBodyCollapsed : ''}`}>
                      <div className={styles.skillGroupBodyInner}>
                    {groupSkills.map((skill, i) => {
                    const isSelected = selectedSkill === skill.name
                    return (
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
                    {isSelected && (
                      <div className={styles.skillDetail}>
                        {loadingContent ? (
                          <p className={styles.toolsEmpty}>加载中...</p>
                        ) : (
                          <div className={styles.skillDetailRendered} dangerouslySetInnerHTML={{ __html: renderBody(skillContent!) }} />
                        )}
                      </div>
                    )}
                  </div>
                )})}
                      </div>
                    </div>
              </div>
              )))}
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
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const res = await loomRpc<{ plugins: PluginInfo[] }>('plugins.list')
      setPlugins(res.plugins ?? [])
    } catch (e: any) {
      setError(`加载失败: ${e.message || e}`)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { load() }, [load])

  const handleRefresh = async () => {
    setRefreshing(true)
    try {
      await loomRpc('plugins.reload')
      await load()
    } catch { /* ignore */ }
    setRefreshing(false)
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <div className={styles.sectionHeaderRow}>
          <h3 className={styles.sectionTitle}>插件</h3>
          <button
            onClick={handleRefresh}
            disabled={refreshing || loading}
            className={styles.refreshBtn}
            title="重新扫描插件"
          >
            <IconRefresh size={14} />
          </button>
        </div>
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
  const [dataDir, setDataDir] = useState('')
  const update = useStore((s) => s.update)
  const currentModel = useStore((s) => s.currentModel)
  const port = useStore((s) => s.port)
  const checkUpdate = useStore((s) => s.checkUpdate)
  const downloadUpdate = useStore((s) => s.downloadUpdate)
  const installUpdate = useStore((s) => s.installUpdate)
  const simulateUpdateFlow = useStore((s) => s.simulateUpdateFlow)
  const isDev = !(window.__isPackaged__ ?? true)
  const connected = wsState === 'connected'

  useEffect(() => {
    let cancelled = false
    window.loom.getAppVersion().then((v) => { if (!cancelled) setAppVersion(v) })
    window.loom.getLoomDir().then((d) => { if (!cancelled) setDataDir(d) })
    loomRpc<SystemHealth>('system.health')
      .then((data) => { if (!cancelled) setHealth(data) })
      .catch(() => { if (!cancelled) setHealthError(true) })
    return () => { cancelled = true }
  }, [])

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>关于</h3>
        <p className={styles.sectionDesc}>版本、更新和连接信息</p>
      </div>
      <div className={styles.contentBody}>
        <div className={styles.aboutSection}>
          {/* App info card */}
          <div className={styles.aboutCard}>
            <div className={styles.aboutAppRow}>
              <img
                src={isDev ? logoDev : logoRelease}
                alt="openLoom"
                className={styles.aboutAppIcon}
              />
              <div>
                <h4 className={styles.aboutAppName}>openLoom</h4>
                <p className={styles.aboutAppVer}>v{appVersion}</p>
              </div>
            </div>
            <p className={styles.aboutAppTag}>
              本地优先的私人 AI 助理。所有数据存储在本地。
            </p>
            <a
              className={styles.aboutGitLink}
              href="https://github.com/godsir/openloom"
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => { e.preventDefault(); window.loom.openExternal('https://github.com/godsir/openloom') }}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
              </svg>
              github.com/godsir/openloom
            </a>
            {dataDir && (
              <div className={styles.aboutDataRow}>
                <span className={styles.aboutDataLabel}>数据目录</span>
                <span className={styles.aboutDataPath}>{dataDir}</span>
              </div>
            )}
          </div>


          {/* System status */}
          <div className={styles.aboutCard}>
            <h4 className={styles.aboutCardTitle}>系统状态</h4>
            <div className={styles.aboutStatGrid}>
              <div className={styles.aboutStatItem}>
                <span className={styles.aboutStatLabel}>后端连接</span>
                <span className={`${styles.aboutStatValue} ${connected ? styles.aboutStatOk : styles.aboutStatWarn}`}>
                  {connected ? `localhost:${port}` : wsState}
                </span>
              </div>
              <div className={styles.aboutStatItem}>
                <span className={styles.aboutStatLabel}>当前模型</span>
                <span className={styles.aboutStatValue}>{currentModel || '未选择'}</span>
              </div>
              {health && (
                <>
                  <div className={styles.aboutStatItem}>
                    <span className={styles.aboutStatLabel}>Agent 数量</span>
                    <span className={styles.aboutStatValue}>{health.agent_count}</span>
                  </div>
                  <div className={styles.aboutStatItem}>
                    <span className={styles.aboutStatLabel}>工具数量</span>
                    <span className={styles.aboutStatValue}>{health.tool_count}</span>
                  </div>
                </>
              )}
            </div>
            {healthError && <p className={styles.toolsError}>系统信息加载失败</p>}
          </div>

          {/* Auto-update */}
          <div className={styles.aboutCard}>
            <h4 className={styles.aboutCardTitle}>自动更新</h4>
            <div className={styles.aboutUpdateBody}>
              {update.status === 'checking' && (
                <p className={styles.updateStatusText}>正在检查更新...</p>
              )}
              {update.status === 'available' && (
                <p className={styles.updateStatusAccent}>
                  {update.version ? `发现新版本 ${update.version}` : '发现新版本'}
                </p>
              )}
              {update.status === 'downloading' && (
                <>
                  <p className={styles.updateStatusAccent}>{update.progress.toFixed(0)}% 下载中</p>
                  <div className={styles.updateProgressBar}>
                    <div className={styles.updateProgressFill} style={{ width: `${update.progress}%` }} />
                  </div>
                </>
              )}
              {update.status === 'downloaded' && (
                <p className={styles.updateStatusAccent}>更新已就绪，重启后生效</p>
              )}
              {(update.status === 'no-update' || update.status === 'idle') && (
                <p className={styles.updateStatusText}>已是最新版本</p>
              )}
              {update.status === 'error' && (
                <p className={styles.updateStatusError}>{update.error ?? '检查更新失败'}</p>
              )}
            </div>
            <div className={styles.aboutUpdateActions}>
              {(update.status === 'idle' || update.status === 'no-update' || update.status === 'error') && (
                <>
                  <button className={styles.mcpConnectBtn} onClick={checkUpdate}>
                    检查更新
                  </button>
                  {isDev && (
                    <button className={styles.mcpDisconnectBtn} onClick={simulateUpdateFlow}>
                      测试更新
                    </button>
                  )}
                </>
              )}
              {update.status === 'available' && (
                <button className={styles.mcpConnectBtn} onClick={downloadUpdate}>
                  下载更新
                </button>
              )}
              {update.status === 'downloaded' && (
                <button className={styles.mcpConnectBtn} onClick={installUpdate}>
                  立即重启
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </>
  )
}

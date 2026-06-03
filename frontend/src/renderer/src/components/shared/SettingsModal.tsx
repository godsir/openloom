import { useState, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import { IconFolder, IconSettings, IconBot, IconBox, IconBrain, IconBarChart, IconTerminal, IconSparkles, IconPawPrint, IconInfo, IconPackage, IconStore } from '../../utils/icons'
import Overlay from './Overlay'
import AgentConfigPanel from './AgentConfigPanel'
import { loomRpc } from '../../services/jsonrpc'
import WorkspaceTab from './WorkspaceTab'
import PetTab from './PetTab'
import styles from './SettingsModal.module.css'
import SoftwareTab from '../settings/SoftwareTab'
import ModelsTab from '../settings/ModelsTab'
import McpLspTab from '../settings/McpTab'
import SkillsTab from '../settings/SkillsTab'
import PluginsTab from '../settings/PluginsTab'
import MarketplaceTab from '../settings/MarketplaceTab'
import SkillMarketTab from '../settings/SkillMarketTab'
import AboutTab from '../settings/AboutTab'
import TokenTab from '../settings/TokenTab'
import KgTab from '../settings/KgTab'

type Tab = 'software' | 'agent' | 'models' | 'workspace' | 'mcp' | 'skills' | 'plugins' | 'pluginMarket' | 'skillMarket' | 'pet' | 'kg' | 'token' | 'about'

function GlobalDefaultsSection() {
  const [maxIterations, setMaxIterations] = useState(30)
  const [maxPromptBudget, setMaxPromptBudget] = useState(0)
  const [loaded, setLoaded] = useState(false)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    loomRpc<{ max_iterations: number; max_prompt_budget: number }>('config.get_defaults')
      .then((r) => {
        setMaxIterations(r.max_iterations || 30)
        setMaxPromptBudget(r.max_prompt_budget || 0)
        setLoaded(true)
      })
      .catch(() => setLoaded(true))
  }, [])

  const save = useCallback(async () => {
    setSaving(true)
    try {
      await loomRpc('config.set_defaults', {
        max_iterations: maxIterations,
        max_prompt_budget: maxPromptBudget,
      })
    } finally {
      setSaving(false)
    }
  }, [maxIterations, maxPromptBudget])

  if (!loaded) return null

  return (
    <div style={{ marginBottom: 24, padding: '12px 16px', background: 'var(--bg-card)', borderRadius: 'var(--r-md)', border: '1px solid var(--border)' }}>
      <h4 style={{ margin: '0 0 4px', fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>全局默认值</h4>
      <p style={{ margin: '0 0 12px', fontSize: 11, color: 'var(--text-muted)' }}>对所有智能体生效的全局默认值</p>
      <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap', alignItems: 'flex-end' }}>
        <div>
          <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>最大迭代次数</label>
          <input
            type="number" min={1} max={200}
            value={maxIterations}
            onChange={(e) => setMaxIterations(Number(e.target.value) || 30)}
            style={{ width: 80, height: 28, padding: '0 8px', fontSize: 12, color: 'var(--text)', background: 'var(--bg)', border: '1px solid var(--border)', borderRadius: 'var(--r-sm)' }}
          />
          <span style={{ fontSize: 10, color: 'var(--text-muted)', marginLeft: 6 }}>单轮最多 LLM 调用次数</span>
        </div>
        <div>
          <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Token 预算上限</label>
          <input
            type="number" min={0} step={1000}
            value={maxPromptBudget}
            onChange={(e) => setMaxPromptBudget(Number(e.target.value) || 0)}
            style={{ width: 100, height: 28, padding: '0 8px', fontSize: 12, color: 'var(--text)', background: 'var(--bg)', border: '1px solid var(--border)', borderRadius: 'var(--r-sm)' }}
          />
          <span style={{ fontSize: 10, color: 'var(--text-muted)', marginLeft: 6 }}>累计 prompt token 上限（0=不限）</span>
        </div>
        <button
          onClick={save}
          disabled={saving}
          style={{ height: 28, padding: '0 14px', fontSize: 12, fontWeight: 500, color: '#fff', background: 'var(--accent)', border: 'none', borderRadius: 'var(--r-sm)', cursor: 'pointer' }}
        >
          {saving ? '保存中...' : '保存'}
        </button>
      </div>
    </div>
  )
}

const tabGroups: { label: string; items: { id: Tab; label: string; icon: React.ReactNode }[] }[] = [
  {
    label: '助手',
    items: [
      { id: 'agent', label: '智能体', icon: <IconBot size={14} /> },
      { id: 'models', label: '模型', icon: <IconBox size={14} /> },
      { id: 'kg', label: '记忆系统', icon: <IconBrain size={14} /> },
      { id: 'token', label: 'Token 用量', icon: <IconBarChart size={14} /> },
    ],
  },
  {
    label: '工具与扩展',
    items: [
      { id: 'workspace', label: '工作空间', icon: <IconFolder size={14} /> },
      { id: 'mcp', label: 'MCP / LSP', icon: <IconTerminal size={14} /> },
      { id: 'skills', label: '本地技能', icon: <IconSparkles size={14} /> },
      { id: 'plugins', label: '本地插件', icon: <IconPackage size={14} /> },
    ],
  },
  {
    label: '市场',
    items: [
      { id: 'pluginMarket', label: '插件市场', icon: <IconPackage size={14} /> },
      { id: 'skillMarket', label: '技能市场', icon: <IconStore size={14} /> },
    ],
  },
  {
    label: '系统',
    items: [
      { id: 'software', label: '通用', icon: <IconSettings size={14} /> },
      { id: 'pet', label: '桌宠', icon: <IconPawPrint size={14} /> },
      { id: 'about', label: '关于', icon: <IconInfo size={14} /> },
    ],
  },
]

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

  return (
    <Overlay open={open} onClose={onClose} size="lg">
      <div className={styles.layout}>
        <div className={styles.nav}>
          {tabGroups.map((group, gi) => (
            <div key={gi} className={styles.navGroup}>
              <div className={styles.navLabel}>{group.label}</div>
              {group.items.map((t) => (
                <button
                  key={t.id}
                  onClick={() => setTab(t.id)}
                  className={`${styles.navItem} ${tab === t.id ? styles.navActive : ''}`}
                >
                  <span className={styles.navIcon}>{t.icon}</span>
                  {t.label}
                </button>
              ))}
            </div>
          ))}
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
                <GlobalDefaultsSection />
                <AgentConfigPanel />
              </div>
            </>
          )}

          {tab === 'models' && <ModelsTab />}

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
                <McpLspTab />
              </div>
            </>
          )}

          {tab === 'skills' && <SkillsTab />}
          {tab === 'plugins' && <PluginsTab />}
          {tab === 'pluginMarket' && <MarketplaceTab mode="plugin" />}
          {tab === 'skillMarket' && <SkillMarketTab />}

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

          {tab === 'kg' && <KgTab />}
          {tab === 'token' && <TokenTab />}
          {tab === 'about' && <AboutTab wsState={wsState} />}
        </div>
      </div>
    </Overlay>
  )
}

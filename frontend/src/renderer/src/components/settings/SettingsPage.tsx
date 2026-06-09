import { useState, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import { IconFolder, IconSettings, IconBot, IconBox, IconBrain, IconBarChart, IconTerminal, IconSparkles, IconPawPrint, IconInfo, IconPackage } from '../../utils/icons'
import AgentConfigPanel from '../shared/AgentConfigPanel'
import { loomRpc } from '../../services/jsonrpc'
import WorkspaceTab from '../shared/WorkspaceTab'
import PetTab from '../shared/PetTab'
import styles from '../shared/SettingsModal.module.css'
import SoftwareTab from './SoftwareTab'
import ModelsTab from './ModelsTab'
import McpLspTab from './McpTab'
import SkillsTab from './SkillsTab'
import PluginsTab from './PluginsTab'
import AboutTab from './AboutTab'
import TokenTab from './TokenTab'
import KgTab from './KgTab'

type Tab = 'software' | 'agent' | 'models' | 'workspace' | 'mcp' | 'skills' | 'plugins' | 'pet' | 'kg' | 'token' | 'about'

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
    <div className={styles.globalDefaultsCard}>
      <h4 className={styles.globalDefaultsTitle}>全局默认值</h4>
      <p className={styles.globalDefaultsDesc}>对所有智能体生效的全局默认值</p>
      <div className={styles.globalDefaultsRow}>
        <div>
          <label className={styles.globalDefaultsFieldLabel}>最大迭代次数</label>
          <input
            type="number" min={1} max={200}
            value={maxIterations}
            onChange={(e) => setMaxIterations(Number(e.target.value) || 30)}
            className={styles.globalDefaultsInput}
          />
          <span className={styles.globalDefaultsHint}>单轮最多 LLM 调用次数</span>
        </div>
        <div>
          <label className={styles.globalDefaultsFieldLabel}>Token 预算上限</label>
          <input
            type="number" min={0} step={1000}
            value={maxPromptBudget}
            onChange={(e) => setMaxPromptBudget(Number(e.target.value) || 0)}
            className={`${styles.globalDefaultsInput} ${styles.globalDefaultsInputWide}`}
          />
          <span className={styles.globalDefaultsHint}>累计 prompt token 上限（0=不限）</span>
        </div>
        <button
          onClick={save}
          disabled={saving}
          className={styles.globalDefaultsSaveBtn}
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
      { id: 'plugins', label: '插件', icon: <IconPackage size={14} /> },
      { id: 'skills', label: '技能', icon: <IconSparkles size={14} /> },
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

export default function SettingsPage() {
  const theme = useStore((s) => s.theme)
  const setTheme = useStore((s) => s.setTheme)
  const wsState = useStore((s) => s.wsState)
  const [tab, setTab] = useState<Tab>('software')

  return (
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
              <p className={styles.sectionDesc}>知识图谱 · 用户画像 · 模式洞察 · 记忆健康</p>
            </div>
            <div className={`${styles.contentBody} ${styles.contentBodyFlush}`}>
              <KgTab />
            </div>
          </>
        )}
        {tab === 'token' && <TokenTab />}
        {tab === 'about' && <AboutTab wsState={wsState} />}
      </div>
    </div>
  )
}

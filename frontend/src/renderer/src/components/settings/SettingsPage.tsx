import { useState, useEffect, useCallback, useRef } from 'react'
import { useStore } from '../../stores'
import { IconFolder, IconSettings, IconBot, IconBox, IconBrain, IconBarChart, IconTerminal, IconSparkles, IconPawPrint, IconInfo, IconBug, IconChevronUp, IconCommand, IconEdit, IconMessageSquare, IconUsers, IconSearch } from '../../utils/icons'
import AgentConfigPanel from '../shared/AgentConfigPanel'
import LoomMdSection from '../shared/LoomMdSection'
import { loomRpc } from '../../services/jsonrpc'
import { useLocale } from '../../i18n'
import WorkspaceTab from '../shared/WorkspaceTab'
import PetTab from '../shared/PetTab'
import styles from '../shared/SettingsModal.module.css'
import SoftwareTab from './SoftwareTab'
import ModelsTab from './ModelsTab'
import McpLspTab from './McpTab'
import SkillsTab from './SkillsTab'
import BuiltinToolsTab from './BuiltinToolsTab'
import AboutTab from './AboutTab'
import DevTestTab from './DevTestTab'
import ShortcutsTab from './ShortcutsTab'
import TokenTab from './TokenTab'
import KgTab from './KgTab'
import ImTab from './ImTab'
import TeamTab from './TeamTab'
import ImportConversationsTab from './ImportConversationsTab'
import { WriteSettingsSection } from '../write/WriteSettingsSection'

type Tab = 'software' | 'agent' | 'other_tools' | 'models' | 'workspace' | 'mcp' | 'skills' | 'pet' | 'kg' | 'token' | 'shortcuts' | 'devtest' | 'write' | 'about' | 'im' | 'builtin_tools'

function GlobalDefaultsSection() {
  const { t } = useLocale()
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
      <h4 className={styles.globalDefaultsTitle}>{t('settings.globalDefaults')}</h4>
      <p className={styles.globalDefaultsDesc}>{t('settings.globalDefaultsDesc')}</p>
      <div className={styles.globalDefaultsRow}>
        <div>
          <label className={styles.globalDefaultsFieldLabel}>{t('settings.maxIterations')}</label>
          <input
            type="number" min={1} max={200}
            value={maxIterations}
            onChange={(e) => setMaxIterations(Number(e.target.value) || 30)}
            className={styles.globalDefaultsInput}
          />
          <span className={styles.globalDefaultsHint}>{t('settings.maxIterationsHint')}</span>
        </div>
        <div>
          <label className={styles.globalDefaultsFieldLabel}>{t('settings.tokenBudget')}</label>
          <input
            type="number" min={0} step={1000}
            value={maxPromptBudget}
            onChange={(e) => setMaxPromptBudget(Number(e.target.value) || 0)}
            className={`${styles.globalDefaultsInput} ${styles.globalDefaultsInputWide}`}
          />
          <span className={styles.globalDefaultsHint}>{t('settings.tokenBudgetHint')}</span>
        </div>
        <button
          onClick={save}
          disabled={saving}
          className={styles.globalDefaultsSaveBtn}
        >
          {saving ? t('settings.saving') : t('common.save')}
        </button>
      </div>
    </div>
  )
}

function useSettingsTabs() {
  const { t } = useLocale()
  return [
    {
      label: t('settings.assistantGroup'),
      items: [
        { id: 'agent' as Tab, label: t('settings.agentAndTeam'), icon: <IconBot size={14} />, searchTerms: [t('settings.globalDefaults'), t('settings.maxIterations'), t('settings.tokenBudget'), 'agent team 智能体 团队'] },
        { id: 'models' as Tab, label: t('settings.models'), icon: <IconBox size={14} />, searchTerms: ['model provider api 模型 服务商'] },
        { id: 'kg' as Tab, label: t('settings.memorySystem'), icon: <IconBrain size={14} />, searchTerms: [t('settings.kgDesc'), 'memory knowledge graph 记忆 知识图谱'] },
        { id: 'token' as Tab, label: t('settings.tokenUsage'), icon: <IconBarChart size={14} />, searchTerms: ['token usage 用量 统计'] },
      ],
    },
    {
      label: t('settings.toolsGroup'),
      items: [
        { id: 'workspace' as Tab, label: t('settings.workspace'), icon: <IconFolder size={14} />, searchTerms: [t('settings.workspaceDesc'), 'workspace sandbox 工作区 沙箱'] },
        { id: 'mcp' as Tab, label: t('settings.mcpLsp'), icon: <IconTerminal size={14} />, searchTerms: [t('settings.mcpLspDesc'), 'server protocol 工具 协议'] },
        { id: 'skills' as Tab, label: t('settings.skills'), icon: <IconSparkles size={14} />, searchTerms: ['skill marketplace 技能 市场'] },
        { id: 'write' as Tab, label: t('write.settings', '写作设置'), icon: <IconEdit size={14} />, searchTerms: ['writing editor completion 写作 编辑器 补全'] },
        { id: 'im' as Tab, label: t('settings.im', 'IM 接入'), icon: <IconMessageSquare size={14} />, searchTerms: ['wechat feishu telegram discord 微信 飞书'] },
        { id: 'builtin_tools' as Tab, label: t('settings.builtinTools'), icon: <IconSettings size={14} />, searchTerms: [t('settings.builtinToolsDesc'), 'tools 内置 工具'] },
        { id: 'other_tools' as Tab, label: t('settings.otherTools'), icon: <IconSettings size={14} />, searchTerms: [t('settings.loomMd'), t('settings.loomMdDesc'), t('settings.importConversations'), t('settings.importConversationsDesc'), 'rules loom markdown claude codex openclaw 规则 导入 会话'] },
      ],
    },
    {
      label: t('settings.systemGroup'),
      items: [
        { id: 'software' as Tab, label: t('settings.software'), icon: <IconSettings size={14} />, searchTerms: ['theme font update appearance 主题 字体 更新 外观'] },
        { id: 'shortcuts' as Tab, label: t('keybindings.title'), icon: <IconCommand size={14} />, searchTerms: ['shortcut hotkey 快捷键 热键'] },
        { id: 'pet' as Tab, label: t('settings.pet'), icon: <IconPawPrint size={14} />, searchTerms: [t('settings.petDesc'), 'avatar desktop pet 宠物 形象'] },
        { id: 'about' as Tab, label: t('settings.about'), icon: <IconInfo size={14} />, searchTerms: ['version update about 版本 更新 关于'] },
      ],
    },
  ]
}

function useSettingsSearchItems() {
  const { t } = useLocale()
  return [
    { id: 'agent-defaults', label: t('settings.globalDefaults'), tab: 'agent' as Tab, group: t('settings.agentAndTeam'), terms: [t('settings.maxIterations'), t('settings.tokenBudget'), 'iterations token 迭代 预算'] },
    { id: 'agent-config', label: t('settings.agentSection'), tab: 'agent' as Tab, group: t('settings.agentAndTeam'), terms: ['agent 智能体'] },
    { id: 'team-config', label: t('settings.teamSection'), tab: 'agent' as Tab, group: t('settings.agentAndTeam'), terms: ['team 团队 专家'] },
    { id: 'models', label: t('settings.models'), tab: 'models' as Tab, group: t('settings.models'), terms: ['model provider api 模型 服务商'] },
    { id: 'memory', label: t('settings.memorySystem'), tab: 'kg' as Tab, group: t('settings.memorySystem'), terms: [t('settings.kgDesc'), 'memory knowledge graph 记忆 知识图谱'] },
    { id: 'token-usage', label: t('settings.tokenUsage'), tab: 'token' as Tab, group: t('settings.tokenUsage'), terms: ['token usage 用量 统计'] },
    { id: 'rules', label: t('settings.loomMd'), tab: 'other_tools' as Tab, group: t('settings.otherTools'), terms: [t('settings.loomMdDesc'), 'rules loom markdown 规则 文件'] },
    { id: 'import-conversations', label: t('settings.importConversations'), tab: 'other_tools' as Tab, group: t('settings.otherTools'), terms: [t('settings.importConversationsDesc'), 'claude codex openclaw 导入 会话'] },
    { id: 'workspace', label: t('settings.workspace'), tab: 'workspace' as Tab, group: t('settings.workspace'), terms: [t('settings.workspaceDesc'), 'workspace sandbox 工作区 沙箱'] },
    { id: 'mcp-lsp', label: t('settings.mcpLsp'), tab: 'mcp' as Tab, group: t('settings.mcpLsp'), terms: [t('settings.mcpLspDesc'), 'server protocol 工具 协议'] },
    { id: 'skills', label: t('settings.skills'), tab: 'skills' as Tab, group: t('settings.skills'), terms: ['skill marketplace 技能 市场'] },
    { id: 'writing', label: t('write.settings', '写作设置'), tab: 'write' as Tab, group: t('write.settings', '写作设置'), terms: ['writing editor completion 写作 编辑器 补全'] },
    { id: 'im', label: t('settings.im', 'IM 接入'), tab: 'im' as Tab, group: t('settings.im', 'IM 接入'), terms: ['wechat feishu telegram discord 微信 飞书'] },
    { id: 'builtin-tools', label: t('settings.builtinTools'), tab: 'builtin_tools' as Tab, group: t('settings.builtinTools'), terms: [t('settings.builtinToolsDesc'), 'tools 内置 工具'] },
    { id: 'theme', label: t('software.appearance'), tab: 'software' as Tab, group: t('settings.software'), terms: ['theme color 主题 颜色 外观'] },
    { id: 'font', label: t('software.font'), tab: 'software' as Tab, group: t('settings.software'), terms: ['font size 字体 字号'] },
    { id: 'software-interaction', label: t('software.interaction'), tab: 'software' as Tab, group: t('settings.software'), terms: ['zoom interaction 交互 缩放'] },
    { id: 'software-behavior', label: t('software.behavior'), tab: 'software' as Tab, group: t('settings.software'), terms: ['startup update behavior 启动 更新 行为'] },
    { id: 'shortcuts', label: t('keybindings.title'), tab: 'shortcuts' as Tab, group: t('keybindings.title'), terms: ['shortcut hotkey 快捷键 热键'] },
    { id: 'pet', label: t('settings.pet'), tab: 'pet' as Tab, group: t('settings.pet'), terms: [t('settings.petDesc'), 'avatar desktop pet 宠物 形象'] },
    { id: 'about', label: t('settings.about'), tab: 'about' as Tab, group: t('settings.about'), terms: ['version update about 版本 更新 关于'] },
  ]
}

export default function SettingsPage() {
  const { t } = useLocale()
  const theme = useStore((s) => s.theme)
  const setTheme = useStore((s) => s.setTheme)
  const wsState = useStore((s) => s.wsState)
  const [tab, setTab] = useState<Tab>('software')
  const [searchQuery, setSearchQuery] = useState('')
  const isDev = !(window.__isPackaged__ ?? true)
  const [showScrollTop, setShowScrollTop] = useState(false)
  const contentRef = useRef<HTMLDivElement>(null)
  const tabGroups = useSettingsTabs()
  const searchItems = useSettingsSearchItems()

  // Scroll-to-top: track scroll position of the visible contentBody
  useEffect(() => {
    const el = contentRef.current
    if (!el) return
    const body = el.querySelector<HTMLElement>('[class*="contentBody"]')
    if (!body) return
    const onScroll = () => setShowScrollTop(body.scrollTop > 200)
    body.addEventListener('scroll', onScroll, { passive: true })
    onScroll()
    return () => body.removeEventListener('scroll', onScroll)
  }, [tab])

  const scrollToTop = () => {
    const el = contentRef.current
    const body = el?.querySelector<HTMLElement>('[class*="contentBody"]')
    body?.scrollTo({ top: 0, behavior: 'smooth' })
  }

  // Inject dev test tab into the system group when in dev mode
  const displayGroups = isDev
    ? tabGroups.map((group) => {
        if (group.label === t('settings.systemGroup')) {
          return {
            ...group,
            items: [
              ...group.items,
              { id: 'devtest' as Tab, label: t('settings.devTest'), icon: <IconBug size={14} /> },
            ],
          }
        }
        return group
      })
    : tabGroups
  const normalizedQuery = searchQuery.trim().toLocaleLowerCase()
  const searchResults = normalizedQuery
    ? searchItems.filter((item) => [item.label, item.group, ...item.terms].join(' ').toLocaleLowerCase().includes(normalizedQuery))
    : []
  const noSearchResults = normalizedQuery.length > 0 && searchResults.length === 0

  useEffect(() => {
    if (normalizedQuery && searchResults.length > 0 && !searchResults.some((item) => item.tab === tab)) {
      setTab(searchResults[0].tab)
    }
  }, [normalizedQuery, tab, searchResults.map((item) => item.id).join('|')])

  return (
    <div className={styles.layout}>
      <div className={styles.nav}>
        <label className={styles.settingsSearch}>
          <IconSearch size={14} />
          <input
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder={t('settings.searchPlaceholder')}
            aria-label={t('settings.searchPlaceholder')}
          />
        </label>
        {normalizedQuery ? (
          <div className={styles.navGroup}>
            <div className={styles.navLabel}>{t('settings.searchResults')}</div>
            {searchResults.map((item) => (
              <button key={item.id} onClick={() => setTab(item.tab)} className={styles.navItem}>
                <span className={styles.navIcon}><IconSettings size={14} /></span>
                <span className={styles.settingsSearchResultLabel}>
                  {item.label}
                  <span>{item.group}</span>
                </span>
              </button>
            ))}
          </div>
        ) : displayGroups.map((group, gi) => (
          <div key={gi} className={styles.navGroup}>
            <div className={styles.navLabel}>{group.label}</div>
            {group.items.map((item) => (
              <button
                key={item.id}
                onClick={() => setTab(item.id)}
                className={`${styles.navItem} ${tab === item.id ? styles.navActive : ''}`}
              >
                <span className={styles.navIcon}>{item.icon}</span>
                {item.label}
              </button>
            ))}
          </div>
        ))}
      </div>

      <div className={styles.content} ref={contentRef}>
        {noSearchResults ? (
          <div className={styles.settingsSearchEmpty}>{t('settings.noSearchResults')}</div>
        ) : (
        <div key={tab} className={styles.tabView}>
        {tab === 'software' && <SoftwareTab theme={theme} setTheme={setTheme} />}

        {tab === 'shortcuts' && <ShortcutsTab />}

        {tab === 'agent' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.agentAndTeam')}</h3>
              <p className={styles.sectionDesc}>{t('settings.agentAndTeamDesc')}</p>
            </div>
            <div className={styles.contentBody}>
              <GlobalDefaultsSection />
              <AgentConfigPanel sectionTitle={t('settings.agentSection')} />
              <hr className={styles.sectionDivider} />
              <TeamTab sectionTitle={t('settings.teamSection')} />
            </div>
          </>
        )}

        {tab === 'other_tools' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.otherTools')}</h3>
              <p className={styles.sectionDesc}>{t('settings.otherToolsDesc')}</p>
            </div>
            <div className={styles.contentBody}>
              <div className={styles.otherToolsSectionHeader}>
                <h4 className={styles.otherToolsSectionTitle}>{t('settings.loomMd')}</h4>
                <p className={styles.otherToolsSectionDesc}>{t('settings.loomMdDesc')}</p>
              </div>
              <LoomMdSection />
              <hr className={styles.sectionDivider} />
              <div className={styles.otherToolsSectionHeader}>
                <h4 className={styles.otherToolsSectionTitle}>{t('settings.importConversations')}</h4>
                <p className={styles.otherToolsSectionDesc}>{t('settings.importConversationsDesc')}</p>
              </div>
              <ImportConversationsTab />
            </div>
          </>
        )}

        {tab === 'models' && <ModelsTab />}

        {tab === 'workspace' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.workspace')}</h3>
              <p className={styles.sectionDesc}>{t('settings.workspaceDesc')}</p>
            </div>
            <div className={styles.contentBody}>
              <WorkspaceTab />
            </div>
          </>
        )}

        {tab === 'mcp' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.mcpLsp')}</h3>
              <p className={styles.sectionDesc}>{t('settings.mcpLspDesc')}</p>
            </div>
            <div className={styles.contentBody}>
              <McpLspTab />
            </div>
          </>
        )}

        {tab === 'skills' && <SkillsTab />}
        {tab === 'im' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.im', 'IM 接入')}</h3>
              <p className={styles.sectionDesc}>{t('settings.imDesc', '连接手机 IM 平台，让 Agent 在微信/飞书等渠道收发消息')}</p>
            </div>
            <div className={styles.contentBody}>
              <ImTab />
            </div>
          </>
        )}
        {tab === 'write' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('write.settings', '写作设置')}</h3>
              <p className={styles.sectionDesc}>{t('write.settingsDesc', '配置编辑器、AI 补全和工作区选项')}</p>
            </div>
            <div className={styles.contentBody}>
              <WriteSettingsSection />
            </div>
          </>
        )}

        {tab === 'builtin_tools' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.builtinTools')}</h3>
              <p className={styles.sectionDesc}>{t('settings.builtinToolsDesc')}</p>
            </div>
            <div className={styles.contentBody}>
              <BuiltinToolsTab />
            </div>
          </>
        )}

        {tab === 'pet' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.pet')}</h3>
              <p className={styles.sectionDesc}>{t('settings.petDesc')}</p>
            </div>
            <div className={styles.contentBody}>
              <PetTab />
            </div>
          </>
        )}

        {tab === 'kg' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.memorySystem')}</h3>
              <p className={styles.sectionDesc}>{t('settings.kgDesc')}</p>
            </div>
            <div className={`${styles.contentBody} ${styles.contentBodyFlush}`}>
              <KgTab />
            </div>
          </>
        )}
        {tab === 'token' && <TokenTab />}
        {tab === 'devtest' && <DevTestTab />}
        {tab === 'about' && (
          <div className={styles.contentBody}>
            <AboutTab wsState={wsState} />
          </div>
        )}
        </div>
        )}

        {/* Scroll-to-top */}
        {showScrollTop && (
          <button onClick={scrollToTop} className={styles.scrollTopBtn} title={t('common.backToTop')}>
            <IconChevronUp size={16} />
          </button>
        )}
      </div>
    </div>
  )
}

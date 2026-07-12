import { useState, useEffect, useCallback, useRef } from 'react'
import { useStore } from '../../stores'
import { IconFolder, IconSettings, IconBot, IconBox, IconBrain, IconBarChart, IconTerminal, IconSparkles, IconPawPrint, IconInfo, IconBug, IconChevronUp, IconCommand, IconEdit, IconFileText, IconMessageSquare, IconUsers, IconDownload } from '../../utils/icons'
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

type Tab = 'software' | 'agent' | 'team' | 'loom' | 'models' | 'workspace' | 'mcp' | 'skills' | 'pet' | 'kg' | 'token' | 'shortcuts' | 'devtest' | 'write' | 'about' | 'im' | 'builtin_tools' | 'import'

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
        { id: 'agent' as Tab, label: t('settings.agent'), icon: <IconBot size={14} /> },
        { id: 'team' as Tab, label: t('settings.team'), icon: <IconUsers size={14} /> },
        { id: 'loom' as Tab, label: t('settings.loomMd'), icon: <IconFileText size={14} /> },
        { id: 'models' as Tab, label: t('settings.models'), icon: <IconBox size={14} /> },
        { id: 'kg' as Tab, label: t('settings.memorySystem'), icon: <IconBrain size={14} /> },
        { id: 'token' as Tab, label: t('settings.tokenUsage'), icon: <IconBarChart size={14} /> },
      ],
    },
    {
      label: t('settings.toolsGroup'),
      items: [
        { id: 'workspace' as Tab, label: t('settings.workspace'), icon: <IconFolder size={14} /> },
        { id: 'mcp' as Tab, label: t('settings.mcpLsp'), icon: <IconTerminal size={14} /> },
        { id: 'skills' as Tab, label: t('settings.skills'), icon: <IconSparkles size={14} /> },
        { id: 'write' as Tab, label: t('write.settings', '写作设置'), icon: <IconEdit size={14} /> },
        { id: 'im' as Tab, label: t('settings.im', 'IM 接入'), icon: <IconMessageSquare size={14} /> },
        { id: 'builtin_tools' as Tab, label: t('settings.builtinTools'), icon: <IconSettings size={14} /> },
        { id: 'import' as Tab, label: t('settings.importConversations'), icon: <IconDownload size={14} /> },
      ],
    },
    {
      label: t('settings.systemGroup'),
      items: [
        { id: 'software' as Tab, label: t('settings.software'), icon: <IconSettings size={14} /> },
        { id: 'shortcuts' as Tab, label: t('keybindings.title'), icon: <IconCommand size={14} /> },
        { id: 'pet' as Tab, label: t('settings.pet'), icon: <IconPawPrint size={14} /> },
        { id: 'about' as Tab, label: t('settings.about'), icon: <IconInfo size={14} /> },
      ],
    },
  ]
}

export default function SettingsPage() {
  const { t } = useLocale()
  const theme = useStore((s) => s.theme)
  const setTheme = useStore((s) => s.setTheme)
  const wsState = useStore((s) => s.wsState)
  const [tab, setTab] = useState<Tab>('software')
  const isDev = !(window.__isPackaged__ ?? true)
  const [showScrollTop, setShowScrollTop] = useState(false)
  const contentRef = useRef<HTMLDivElement>(null)
  const tabGroups = useSettingsTabs()

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

  return (
    <div className={styles.layout}>
      <div className={styles.nav}>
        {displayGroups.map((group, gi) => (
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
        <div key={tab} className={styles.tabView}>
        {tab === 'software' && <SoftwareTab theme={theme} setTheme={setTheme} />}

        {tab === 'shortcuts' && <ShortcutsTab />}

        {tab === 'agent' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.agentConfig')}</h3>
              <p className={styles.sectionDesc}>{t('settings.agentConfigDesc')}</p>
            </div>
            <div className={styles.contentBody}>
              <GlobalDefaultsSection />
              <AgentConfigPanel />
            </div>
          </>
        )}

        {tab === 'team' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.teamConfig')}</h3>
              <p className={styles.sectionDesc}>{t('settings.teamConfigDesc')}</p>
            </div>
            <div className={styles.contentBody}>
              <TeamTab />
            </div>
          </>
        )}

        {tab === 'loom' && (
          <>
            <div className={styles.contentHeader}>
              <h3 className={styles.sectionTitle}>{t('settings.loomMd')}</h3>
              <p className={styles.sectionDesc}>{t('settings.loomMdDesc')}</p>
            </div>
            <div className={styles.contentBody}>
              <LoomMdSection />
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

        {tab === 'import' && <ImportConversationsTab />}

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
        {tab === 'about' && <AboutTab wsState={wsState} />}
        </div>

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

import { useState, useEffect, useCallback, useMemo } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { IconBot, IconSearch, IconChevronDown, IconPuzzle, IconGlobe, IconFile, IconServer, IconZap, IconCommand, IconMessageSquare, IconClock, IconRefresh, IconPackage, IconSettings, IconStore } from '../../utils/icons'
import styles from '../shared/SettingsModal.module.css'
import MarketplaceTab from './MarketplaceTab'

interface HookHandlerInfo {
  type: string  // 'command' | 'prompt' | 'agent'
  command?: string
  prompt?: string
  timeout: number
  matcher?: string
}

interface HookEventDetail {
  event: string
  handler_count: number
  handlers: HookHandlerInfo[]
}

interface PluginInfo {
  name: string
  version?: string
  description?: string
  path?: string
  source: string
  skill_count?: number
  hook_count?: number
  mcp_server_count?: number
  has_settings: boolean
  skills?: Array<{name: string, path?: string}>
  mcp_servers?: Array<{name: string, transport: string}>
  hooks?: HookEventDetail[]
}

function getSourceColor(source: string): string {
  if (source.startsWith('claude')) return 'blue'
  if (source === 'openclaw') return 'green'
  if (source === 'loom') return 'purple'
  return 'gray'
}

function getGroupKey(source: string): string {
  if (source.startsWith('claude')) return 'claude'
  if (source === 'openclaw') return 'openclaw'
  if (source === 'loom') return 'loom'
  return source || 'other'
}

const GROUP_CONFIG: Record<string, { label: string; icon: React.ReactNode }> = {
  loom: { label: 'openLoom', icon: <IconPuzzle size={14} /> },
  openclaw: { label: 'OpenClaw (兼容)', icon: <IconGlobe size={14} /> },
  claude: { label: 'Claude Code (兼容)', icon: <IconBot size={14} /> },
}

function hasDetails(plugin: PluginInfo): boolean {
  return (plugin.skills && plugin.skills.length > 0) ||
    (plugin.mcp_servers && plugin.mcp_servers.length > 0) ||
    (plugin.hooks && plugin.hooks.length > 0)
}

export default function PluginsTab() {
  const [plugins, setPlugins] = useState<PluginInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [expandedPlugin, setExpandedPlugin] = useState<string | null>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  const [view, setView] = useState<'installed' | 'market'>('installed')

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

  const toggleExpand = (name: string) => {
    setExpandedPlugin(prev => prev === name ? null : name)
  }

  const filteredPlugins = useMemo(() => {
    if (!searchQuery.trim()) return plugins
    const q = searchQuery.trim().toLowerCase()
    return plugins.filter(p =>
      p.name.toLowerCase().includes(q) ||
      (p.description && p.description.toLowerCase().includes(q))
    )
  }, [plugins, searchQuery])

  const grouped = useMemo(() => {
    const g: Record<string, PluginInfo[]> = {}
    for (const p of filteredPlugins) {
      const key = getGroupKey(p.source)
      if (!g[key]) g[key] = []
      g[key].push(p)
    }
    return g
  }, [filteredPlugins])

  const toggleGroup = (groupKey: string) => {
    setCollapsedGroups(prev => {
      const next = new Set(prev)
      if (next.has(groupKey)) next.delete(groupKey)
      else next.add(groupKey)
      return next
    })
  }

  const groupOrder = ['loom', 'openclaw', 'claude']

  return (
    <>
      <div className={styles.contentHeader}>
        <div className={styles.sectionHeaderRow}>
          <h3 className={styles.sectionTitle}>
            插件
            <span className={styles.pluginsCountBadge}>{plugins.length} 个</span>
          </h3>
          <div className={styles.marketplaceKindToggle}>
            <button
              className={`${styles.marketplaceKindBtn} ${view === 'installed' ? styles.marketplaceKindActive : ''}`}
              onClick={() => setView('installed')}
            >
              <IconPackage size={13} />
              已安装
            </button>
            <button
              className={`${styles.marketplaceKindBtn} ${view === 'market' ? styles.marketplaceKindActive : ''}`}
              onClick={() => setView('market')}
            >
              <IconStore size={13} />
              市场
            </button>
          </div>
          <button
            onClick={handleRefresh}
            disabled={refreshing || loading}
            className={styles.refreshBtn}
            title="重新扫描插件"
          >
            <IconRefresh size={14} />
          </button>
        </div>
        <div className={styles.pluginsSearchWrap}>
          <IconSearch size={14} className={styles.pluginsSearchIcon} />
          <input
            className={styles.pluginsSearchInput}
            type="text"
            placeholder="搜索插件..."
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
          />
        </div>
        <p className={styles.sectionDesc}>
          从插件目录自动发现，点击展开查看详情
        </p>
      </div>
      {view === 'market' ? (
        <MarketplaceTab mode="plugin" hideHeader />
      ) : (
        <div className={styles.contentBody}>
          {error && <p className={styles.toolsError}>{error}</p>}
        {loading ? (
          <p className={styles.toolsEmpty}>加载中...</p>
        ) : filteredPlugins.length === 0 ? (
          <div className={styles.pluginsEmptyState}>
            <p className={styles.pluginsEmptyTitle}>未发现插件</p>
            <p className={styles.pluginsEmptyHelp}>
              将插件放入 ~/.loom/plugins/、~/.claude/plugins/ 或 ~/.openclaw/plugins/ 目录
            </p>
          </div>
        ) : (
          <>
            {groupOrder.map(groupKey => {
              const groupPlugins = grouped[groupKey]
              if (!groupPlugins || groupPlugins.length === 0) return null
              const cfg = GROUP_CONFIG[groupKey]
              const colorClass = groupKey === 'claude' ? styles.dotBlue :
                groupKey === 'openclaw' ? styles.dotGreen :
                groupKey === 'loom' ? styles.dotPurple : styles.dotGray

              return (
                <div key={groupKey} className={styles.pluginsGroup}>
                  <div
                    className={styles.pluginsGroupHeader}
                    onClick={() => toggleGroup(groupKey)}
                  >
                    <span className={`${styles.pluginsGroupDot} ${colorClass}`} />
                    {cfg && <span className={styles.pluginsGroupIcon}>{cfg.icon}</span>}
                    <span className={styles.pluginsGroupLabel}>{cfg ? cfg.label : groupKey}</span>
                    <span className={styles.pluginsGroupCount}>{groupPlugins.length}</span>
                    <span className={`${styles.pluginsGroupChevron} ${collapsedGroups.has(groupKey) ? '' : styles.pluginsGroupChevronOpen}`}>
                      <IconChevronDown size={14} />
                    </span>
                  </div>
                  {!collapsedGroups.has(groupKey) && (
                  <div className={styles.pluginList}>
                    {groupPlugins.map(plugin => {
                      const pluginKey = `${groupKey}-${plugin.name}-${plugin.path || ''}`
                      const isExpanded = expandedPlugin === pluginKey
                      const expandable = hasDetails(plugin)
                      const dotColor = getSourceColor(plugin.source)
                      const dotClass = dotColor === 'blue' ? styles.dotBlue :
                        dotColor === 'green' ? styles.dotGreen :
                        dotColor === 'purple' ? styles.dotPurple : styles.dotGray

                      return (
                        <div
                          key={pluginKey}
                          className={`${styles.pluginCard} ${isExpanded ? styles.pluginCardExpanded : ''}`}
                          onClick={() => toggleExpand(pluginKey)}
                        >
                          <div className={styles.pluginCardMain}>
                            <div className={styles.pluginCardTop}>
                              <span className={`${styles.pluginCardDot} ${dotClass}`} />
                              <div className={styles.pluginCardInfo}>
                                <div className={styles.pluginCardNameRow}>
                                  <span className={styles.pluginCardName}>{plugin.name}</span>
                                  {plugin.version && (
                                    <span className={styles.pluginVersionBadge}>{plugin.version}</span>
                                  )}
                                </div>
                                {plugin.description && (
                                  <p className={styles.pluginCardDesc}>{plugin.description}</p>
                                )}
                              </div>
                              <div className={styles.pluginCardRight}>
                                {plugin.has_settings && (
                                  <span className={styles.pluginCardSettingsIcon} title="含设置项">
                                    <IconSettings size={12} />
                                  </span>
                                )}
                                <span className={`${styles.pluginCardChevron} ${isExpanded ? styles.pluginCardChevronOpen : ''}`}>
                                  <IconChevronDown size={16} />
                                </span>
                              </div>
                            </div>
                            <div className={styles.pluginCardStats}>
                              {plugin.skill_count != null && (
                                <span className={styles.pluginCardStat}>
                                  <IconFile size={10} />
                                  {plugin.skill_count} skills
                                </span>
                              )}
                              {plugin.hook_count != null && (
                                <span className={styles.pluginCardStat}>
                                  <IconZap size={10} />
                                  {plugin.hook_count} hooks
                                </span>
                              )}
                              {plugin.mcp_server_count != null && (
                                <span className={styles.pluginCardStat}>
                                  <IconServer size={10} />
                                  {plugin.mcp_server_count} MCP
                                </span>
                              )}
                            </div>
                          </div>
                          {expandable && (
                            <>
                              <div className={styles.pluginCardDivider} />
                              <div className={`${styles.pluginCardExpand} ${isExpanded ? styles.pluginCardExpandOpen : ''}`}>
                                <div className={styles.pluginDetailBody}>
                                  {plugin.skills && plugin.skills.length > 0 && (
                                    <div className={styles.pluginDetailSection}>
                                      <div className={styles.pluginDetailLabel}>
                                        <IconFile size={11} />
                                        Skills ({plugin.skills.length})
                                      </div>
                                      <div className={styles.pluginDetailItems}>
                                        {plugin.skills.map(skill => (
                                          <div key={skill.name} className={styles.pluginDetailItem}>
                                            <IconFile size={10} className={styles.pluginDetailItemIcon} />
                                            {skill.name}
                                            {skill.path && (
                                              <span className={styles.pluginDetailItemMeta}>{skill.path}</span>
                                            )}
                                          </div>
                                        ))}
                                      </div>
                                    </div>
                                  )}
                                  {plugin.mcp_servers && plugin.mcp_servers.length > 0 && (
                                    <div className={styles.pluginDetailSection}>
                                      <div className={styles.pluginDetailLabel}>
                                        <IconServer size={11} />
                                        MCP Servers ({plugin.mcp_servers.length})
                                      </div>
                                      <div className={styles.pluginDetailItems}>
                                        {plugin.mcp_servers.map(srv => (
                                          <div key={srv.name} className={styles.pluginDetailItem}>
                                            <IconServer size={10} className={styles.pluginDetailItemIcon} />
                                            {srv.name}
                                            <span className={styles.pluginDetailItemMeta}>{srv.transport}</span>
                                          </div>
                                        ))}
                                      </div>
                                    </div>
                                  )}
                                  {plugin.hooks && plugin.hooks.length > 0 && (
                                    <div className={styles.pluginDetailSection}>
                                      <div className={styles.pluginDetailLabel}>
                                        <IconZap size={11} />
                                        Hooks ({plugin.hooks.length})
                                      </div>
                                      <div className={styles.pluginDetailItems}>
                                        {plugin.hooks.map(hook => (
                                          <div key={hook.event} className={styles.pluginDetailItem}>
                                            <IconZap size={10} className={styles.pluginDetailItemIcon} />
                                            <span className={styles.pluginDetailItemName}>{hook.event}</span>
                                            <span className={styles.pluginDetailItemMeta}>{hook.handler_count} handlers</span>
                                            {hook.handlers && hook.handlers.length > 0 && (
                                              <div className={styles.hookHandlerList}>
                                                {hook.handlers.map((h, hi) => (
                                                  <div key={hi} className={styles.hookHandlerItem}>
                                                    <span className={`${styles.hookHandlerBadge} ${h.type === 'command' ? styles.hookHandlerBadgeCmd : h.type === 'prompt' ? styles.hookHandlerBadgePrompt : styles.hookHandlerBadgeAgent}`}>
                                                      {h.type === 'command' ? <IconCommand size={9} /> : h.type === 'prompt' ? <IconMessageSquare size={9} /> : <IconBot size={9} />}
                                                      {h.type}
                                                    </span>
                                                    {h.matcher && (
                                                      <span className={styles.hookHandlerMatcher} title="Matcher regex">
                                                        {h.matcher}
                                                      </span>
                                                    )}
                                                    {h.command && (
                                                      <code className={styles.hookHandlerCode}>{h.command}</code>
                                                    )}
                                                    {h.prompt && (
                                                      <code className={styles.hookHandlerCode}>{h.prompt.length > 60 ? h.prompt.slice(0, 60) + '...' : h.prompt}</code>
                                                    )}
                                                    <span className={styles.hookHandlerTimeout}>
                                                      <IconClock size={9} />
                                                      {h.timeout}s
                                                    </span>
                                                  </div>
                                                ))}
                                              </div>
                                            )}
                                          </div>
                                        ))}
                                      </div>
                                    </div>
                                  )}
                                </div>
                              </div>
                            </>
                          )}
                        </div>
                      )
                    })}
                  </div>
                  )}
                </div>
              )
            })}
            {/* Other groups (non-standard sources) */}
            {Object.keys(grouped).filter(k => !groupOrder.includes(k)).map(groupKey => {
              const groupPlugins = grouped[groupKey]
              if (!groupPlugins || groupPlugins.length === 0) return null

              return (
                <div key={groupKey} className={styles.pluginsGroup}>
                  <div
                    className={styles.pluginsGroupHeader}
                    onClick={() => toggleGroup(groupKey)}
                  >
                    <span className={`${styles.pluginsGroupDot} ${styles.dotGray}`} />
                    <span className={styles.pluginsGroupIcon}><IconPackage size={14} /></span>
                    <span className={styles.pluginsGroupLabel}>{groupKey}</span>
                    <span className={styles.pluginsGroupCount}>{groupPlugins.length}</span>
                    <span className={`${styles.pluginsGroupChevron} ${collapsedGroups.has(groupKey) ? '' : styles.pluginsGroupChevronOpen}`}>
                      <IconChevronDown size={14} />
                    </span>
                  </div>
                  {!collapsedGroups.has(groupKey) && (
                  <div className={styles.pluginList}>
                    {groupPlugins.map(plugin => {
                      const pluginKey = `${groupKey}-${plugin.name}-${plugin.path || ''}`
                      const isExpanded = expandedPlugin === pluginKey
                      const expandable = hasDetails(plugin)

                      return (
                        <div
                          key={pluginKey}
                          className={`${styles.pluginCard} ${isExpanded ? styles.pluginCardExpanded : ''}`}
                          onClick={() => toggleExpand(pluginKey)}
                        >
                          <div className={styles.pluginCardMain}>
                            <div className={styles.pluginCardTop}>
                              <span className={`${styles.pluginCardDot} ${styles.dotGray}`} />
                              <div className={styles.pluginCardInfo}>
                                <div className={styles.pluginCardNameRow}>
                                  <span className={styles.pluginCardName}>{plugin.name}</span>
                                  {plugin.version && (
                                    <span className={styles.pluginVersionBadge}>{plugin.version}</span>
                                  )}
                                </div>
                                {plugin.description && (
                                  <p className={styles.pluginCardDesc}>{plugin.description}</p>
                                )}
                              </div>
                              <div className={styles.pluginCardRight}>
                                {plugin.has_settings && (
                                  <span className={styles.pluginCardSettingsIcon} title="含设置项">
                                    <IconSettings size={12} />
                                  </span>
                                )}
                                <span className={`${styles.pluginCardChevron} ${isExpanded ? styles.pluginCardChevronOpen : ''}`}>
                                  <IconChevronDown size={16} />
                                </span>
                              </div>
                            </div>
                            <div className={styles.pluginCardStats}>
                              {plugin.skill_count != null && (
                                <span className={styles.pluginCardStat}>
                                  <IconFile size={10} />
                                  {plugin.skill_count} skills
                                </span>
                              )}
                              {plugin.hook_count != null && (
                                <span className={styles.pluginCardStat}>
                                  <IconZap size={10} />
                                  {plugin.hook_count} hooks
                                </span>
                              )}
                              {plugin.mcp_server_count != null && (
                                <span className={styles.pluginCardStat}>
                                  <IconServer size={10} />
                                  {plugin.mcp_server_count} MCP
                                </span>
                              )}
                            </div>
                          </div>
                          {expandable && (
                            <>
                              <div className={styles.pluginCardDivider} />
                              <div className={`${styles.pluginCardExpand} ${isExpanded ? styles.pluginCardExpandOpen : ''}`}>
                                <div className={styles.pluginDetailBody}>
                                  {plugin.skills && plugin.skills.length > 0 && (
                                    <div className={styles.pluginDetailSection}>
                                      <div className={styles.pluginDetailLabel}>
                                        <IconFile size={11} />
                                        Skills ({plugin.skills.length})
                                      </div>
                                      <div className={styles.pluginDetailItems}>
                                        {plugin.skills.map(skill => (
                                          <div key={skill.name} className={styles.pluginDetailItem}>
                                            <IconFile size={10} className={styles.pluginDetailItemIcon} />
                                            {skill.name}
                                            {skill.path && (
                                              <span className={styles.pluginDetailItemMeta}>{skill.path}</span>
                                            )}
                                          </div>
                                        ))}
                                      </div>
                                    </div>
                                  )}
                                  {plugin.mcp_servers && plugin.mcp_servers.length > 0 && (
                                    <div className={styles.pluginDetailSection}>
                                      <div className={styles.pluginDetailLabel}>
                                        <IconServer size={11} />
                                        MCP Servers ({plugin.mcp_servers.length})
                                      </div>
                                      <div className={styles.pluginDetailItems}>
                                        {plugin.mcp_servers.map(srv => (
                                          <div key={srv.name} className={styles.pluginDetailItem}>
                                            <IconServer size={10} className={styles.pluginDetailItemIcon} />
                                            {srv.name}
                                            <span className={styles.pluginDetailItemMeta}>{srv.transport}</span>
                                          </div>
                                        ))}
                                      </div>
                                    </div>
                                  )}
                                  {plugin.hooks && plugin.hooks.length > 0 && (
                                    <div className={styles.pluginDetailSection}>
                                      <div className={styles.pluginDetailLabel}>
                                        <IconZap size={11} />
                                        Hooks ({plugin.hooks.length})
                                      </div>
                                      <div className={styles.pluginDetailItems}>
                                        {plugin.hooks.map(hook => (
                                          <div key={hook.event} className={styles.pluginDetailItem}>
                                            <IconZap size={10} className={styles.pluginDetailItemIcon} />
                                            <span className={styles.pluginDetailItemName}>{hook.event}</span>
                                            <span className={styles.pluginDetailItemMeta}>{hook.handler_count} handlers</span>
                                            {hook.handlers && hook.handlers.length > 0 && (
                                              <div className={styles.hookHandlerList}>
                                                {hook.handlers.map((h, hi) => (
                                                  <div key={hi} className={styles.hookHandlerItem}>
                                                    <span className={`${styles.hookHandlerBadge} ${h.type === 'command' ? styles.hookHandlerBadgeCmd : h.type === 'prompt' ? styles.hookHandlerBadgePrompt : styles.hookHandlerBadgeAgent}`}>
                                                      {h.type === 'command' ? <IconCommand size={9} /> : h.type === 'prompt' ? <IconMessageSquare size={9} /> : <IconBot size={9} />}
                                                      {h.type}
                                                    </span>
                                                    {h.matcher && (
                                                      <span className={styles.hookHandlerMatcher} title="Matcher regex">
                                                        {h.matcher}
                                                      </span>
                                                    )}
                                                    {h.command && (
                                                      <code className={styles.hookHandlerCode}>{h.command}</code>
                                                    )}
                                                    {h.prompt && (
                                                      <code className={styles.hookHandlerCode}>{h.prompt.length > 60 ? h.prompt.slice(0, 60) + '...' : h.prompt}</code>
                                                    )}
                                                    <span className={styles.hookHandlerTimeout}>
                                                      <IconClock size={9} />
                                                      {h.timeout}s
                                                    </span>
                                                  </div>
                                                ))}
                                              </div>
                                            )}
                                          </div>
                                        ))}
                                      </div>
                                    </div>
                                  )}
                                </div>
                              </div>
                            </>
                          )}
                        </div>
                      )
                    })}
                  </div>
                  )}
                </div>
              )
            })}
          </>
        )}
        </div>
      )}
    </>
  )
}

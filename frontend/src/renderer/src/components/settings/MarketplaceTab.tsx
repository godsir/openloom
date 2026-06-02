import { useState, useEffect, useCallback, useMemo } from 'react'
import { useStore } from '../../stores'
import { IconRefresh, IconSearch, IconPackage, IconSparkles, IconStore, IconGlobe, IconCheck, IconZap, IconTrash, IconExternalLink } from '../../utils/icons'
import { listMarketplace, installMarketPlugin, uninstallMarketPlugin, updateMarketPlugin, type MarketPlugin } from '../../services/marketplace'
import styles from '../shared/SettingsModal.module.css'

const MARKETPLACE_CATEGORIES = ['全部', 'Security', 'Development', 'Productivity', 'Workflow', 'Research', 'Design']

export default function MarketplaceTab() {
  const [plugins, setPlugins] = useState<MarketPlugin[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [activeCategory, setActiveCategory] = useState<string>('全部')
  const [activeKind, setActiveKind] = useState<string>('plugin')
  const [busyIds, setBusyIds] = useState<Set<string>>(new Set())
  const [refreshing, setRefreshing] = useState(false)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const res = await listMarketplace()
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
    await load()
    setRefreshing(false)
  }

  const addBusy = (id: string) => setBusyIds(prev => new Set(prev).add(id))
  const removeBusy = (id: string) => setBusyIds(prev => { const n = new Set(prev); n.delete(id); return n })

  const handleInstall = async (pluginId: string) => {
    addBusy(pluginId)
    try {
      await installMarketPlugin(pluginId)
      await load()
      useStore.getState().addToast({ type: 'success', message: `${pluginId} 安装成功` })
    } catch (e: any) {
      useStore.getState().addToast({ type: 'error', message: `安装失败: ${e.message || e}` })
    } finally {
      removeBusy(pluginId)
    }
  }

  const handleUninstall = async (pluginId: string) => {
    const ok = await useStore.getState().showConfirm('卸载', `确认卸载 "${pluginId}"？此操作将删除目录。`, true)
    if (!ok) return
    addBusy(pluginId)
    try {
      await uninstallMarketPlugin(pluginId)
      await load()
      useStore.getState().addToast({ type: 'success', message: `${pluginId} 已卸载` })
    } catch (e: any) {
      useStore.getState().addToast({ type: 'error', message: `卸载失败: ${e.message || e}` })
    } finally {
      removeBusy(pluginId)
    }
  }

  const handleUpdate = async (pluginId: string) => {
    addBusy(pluginId)
    try {
      await updateMarketPlugin(pluginId)
      await load()
      useStore.getState().addToast({ type: 'success', message: `${pluginId} 已更新` })
    } catch (e: any) {
      useStore.getState().addToast({ type: 'error', message: `更新失败: ${e.message || e}` })
    } finally {
      removeBusy(pluginId)
    }
  }

  const filtered = useMemo(() => {
    const q = searchQuery.toLowerCase().trim()
    return plugins.filter(p => {
      if (p.kind !== activeKind) return false
      if (activeCategory !== '全部' && p.category !== activeCategory) return false
      if (!q) return true
      return (
        p.name.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q) ||
        p.author.toLowerCase().includes(q) ||
        p.tags.some(t => t.toLowerCase().includes(q))
      )
    })
  }, [plugins, searchQuery, activeCategory, activeKind])

  const installedCount = useMemo(() => plugins.filter(p => p.installed).length, [plugins])
  const kindCounts = useMemo(() => ({
    plugin: plugins.filter(p => p.kind === 'plugin').length,
    skill: plugins.filter(p => p.kind === 'skill').length,
  }), [plugins])

  return (
    <>
      <div className={styles.contentHeader}>
        <div className={styles.pluginsHeader}>
          <div className={styles.sectionHeaderRow}>
            <h3 className={styles.sectionTitle}>
              市场
              <span className={styles.pluginsCountBadge}>{plugins.length} 个</span>
              {installedCount > 0 && (
                <span className={styles.marketplaceInstalledSummary}>
                  {installedCount} 已安装
                </span>
              )}
            </h3>
            <button
              onClick={handleRefresh}
              disabled={refreshing || loading}
              className={styles.refreshBtn}
              title="刷新市场"
            >
              <IconRefresh size={14} />
            </button>
          </div>
          <div className={styles.pluginsSearchWrap}>
            <IconSearch size={14} className={styles.pluginsSearchIcon} />
            <input
              className={styles.pluginsSearchInput}
              type="text"
              placeholder="搜索插件名称、描述、标签或作者..."
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
            />
          </div>
          <p className={styles.pluginsDesc}>
            浏览和安装社区插件，扩展 AI 助手的能力
          </p>
        </div>
      </div>
      <div className={styles.contentBody}>
        {error && <p className={styles.toolsError}>{error}</p>}

        {/* Kind toggle: Plugins / Skills */}
        <div className={styles.marketplaceKindToggle}>
          <button
            className={`${styles.marketplaceKindBtn} ${activeKind === 'plugin' ? styles.marketplaceKindActive : ''}`}
            onClick={() => { setActiveKind('plugin'); setActiveCategory('全部') }}
          >
            <IconPackage size={13} />
            插件
            <span className={styles.marketplaceKindCount}>{kindCounts.plugin}</span>
          </button>
          <button
            className={`${styles.marketplaceKindBtn} ${activeKind === 'skill' ? styles.marketplaceKindActive : ''}`}
            onClick={() => { setActiveKind('skill'); setActiveCategory('全部') }}
          >
            <IconSparkles size={13} />
            技能
            <span className={styles.marketplaceKindCount}>{kindCounts.skill}</span>
          </button>
        </div>

        {/* Category filter pills */}
        <div className={styles.marketplaceCategories}>
          {MARKETPLACE_CATEGORIES.map(cat => (
            <button
              key={cat}
              className={`${styles.marketplaceCategoryBtn} ${activeCategory === cat ? styles.marketplaceCategoryActive : ''}`}
              onClick={() => setActiveCategory(cat)}
            >
              {cat}
            </button>
          ))}
        </div>

        {loading ? (
          <p className={styles.toolsEmpty}>加载中...</p>
        ) : filtered.length === 0 ? (
          <div className={styles.marketplaceEmptyState}>
            <div className={styles.marketplaceEmptyIcon}>
              <IconStore size={32} />
            </div>
            <p className={styles.pluginsEmptyTitle}>
              {searchQuery || activeCategory !== '全部' ? '无匹配结果' : '暂无可用插件'}
            </p>
            <p className={styles.pluginsEmptyHelp}>
              {searchQuery || activeCategory !== '全部'
                ? '尝试其他关键词或清除筛选条件'
                : '市场目录加载为空，请刷新重试'}
            </p>
          </div>
        ) : (
          <div className={styles.marketplaceGrid}>
            {filtered.map(plugin => {
              const isBusy = busyIds.has(plugin.id)
              return (
                <div key={plugin.id} className={`${styles.marketplaceCard} ${plugin.installed ? styles.marketplaceCardInstalled : ''}`}>
                  <div className={styles.marketplaceCardHeader}>
                    <div className={styles.marketplaceCardTitleRow}>
                      <span className={styles.marketplaceCardName}>{plugin.name}</span>
                      <span className={styles.marketplaceCardVersion}>v{plugin.version}</span>
                    </div>
                    <div className={styles.marketplaceCardAuthor}>
                      <IconGlobe size={11} />
                      {plugin.author}
                    </div>
                  </div>
                  <p className={styles.marketplaceCardDesc}>{plugin.description}</p>
                  <div className={styles.marketplaceCardTags}>
                    <span className={styles.marketplaceCardCategory}>{plugin.category}</span>
                    {plugin.tags.slice(0, 4).map(tag => (
                      <span key={tag} className={styles.marketplaceCardTag}>{tag}</span>
                    ))}
                    {plugin.tags.length > 4 && (
                      <span className={styles.marketplaceCardTag}>+{plugin.tags.length - 4}</span>
                    )}
                  </div>
                  <div className={styles.marketplaceCardFooter}>
                    <div className={styles.marketplaceCardMeta}>
                      {plugin.installed ? (
                        <>
                          <span className={styles.marketplaceInstalledBadge}>
                            <IconCheck size={11} />
                            已安装{plugin.installed_version ? ` v${plugin.installed_version}` : ''}
                          </span>
                          {plugin.has_update && (
                            <span className={styles.marketplaceUpdateBadge}>
                              <IconZap size={10} />
                              更新可用 v{plugin.version}
                            </span>
                          )}
                        </>
                      ) : (
                        <span className={styles.marketplaceNotInstalled}>未安装</span>
                      )}
                    </div>
                    <div className={styles.marketplaceCardActions}>
                      {plugin.homepage && (
                        <button
                          className={styles.marketplaceCardLinkBtn}
                          title="查看主页"
                          onClick={() => window.loom.openExternal(plugin.homepage!)}
                        >
                          <IconExternalLink size={14} />
                        </button>
                      )}
                      {plugin.installed ? (
                        <>
                          {plugin.has_update && (
                            <button
                              className={styles.marketplaceInstallBtn}
                              onClick={() => handleUpdate(plugin.id)}
                              disabled={isBusy}
                            >
                              {isBusy ? '更新中...' : <><IconRefresh size={11} /> 更新</>}
                            </button>
                          )}
                          <button
                            className={styles.marketplaceUninstallBtn}
                            onClick={() => handleUninstall(plugin.id)}
                            disabled={isBusy}
                          >
                            {isBusy ? '卸载中...' : <><IconTrash size={11} /> 卸载</>}
                          </button>
                        </>
                      ) : (
                        <button
                          className={styles.marketplaceInstallBtn}
                          onClick={() => handleInstall(plugin.id)}
                          disabled={isBusy}
                        >
                          {isBusy ? '安装中...' : <><IconPackage size={11} /> 安装</>}
                        </button>
                      )}
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </div>
    </>
  )
}

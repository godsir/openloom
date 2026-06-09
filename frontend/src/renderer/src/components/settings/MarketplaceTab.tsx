import { useState, useEffect, useCallback, useMemo } from 'react'
import { useStore } from '../../stores'
import { IconRefresh, IconSearch, IconPackage, IconSparkles, IconStore, IconGlobe, IconCheck, IconZap, IconTrash, IconExternalLink, IconSettings } from '../../utils/icons'
import { listMarketplace, installMarketPlugin, uninstallMarketPlugin, updateMarketPlugin, type MarketPlugin } from '../../services/marketplace'
import { useLocale, t as _t } from '../../i18n'
import styles from '../shared/SettingsModal.module.css'

const ALL_CATEGORY = '__all__'
const MARKETPLACE_CATEGORIES = [ALL_CATEGORY, 'Security', 'Development', 'Productivity', 'Workflow', 'Research', 'Design']
const PREF_KEY_CATALOG_URL = 'marketplaceCatalogUrl'

export default function MarketplaceTab({ mode, hideHeader }: { mode?: 'plugin'; hideHeader?: boolean }) {
  const { t } = useLocale()
  const [plugins, setPlugins] = useState<MarketPlugin[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [activeCategory, setActiveCategory] = useState<string>(ALL_CATEGORY)
  const [activeKind, setActiveKind] = useState<string>(mode === 'plugin' ? 'plugin' : 'plugin')
  const [busyIds, setBusyIds] = useState<Set<string>>(new Set())
  const [refreshing, setRefreshing] = useState(false)
  const [catalogUrl, setCatalogUrl] = useState('')
  const [showSourceConfig, setShowSourceConfig] = useState(false)

  const load = useCallback(async (url?: string) => {
    setLoading(true)
    setError(null)
    try {
      const res = await listMarketplace(url || undefined)
      setPlugins(res.plugins ?? [])
    } catch (e: any) {
      setError(_t('marketplace.loadFailed', { message: e.message || e }))
    } finally {
      setLoading(false)
    }
  }, [])

  // Load custom catalog URL from preferences on mount
  useEffect(() => {
    window.loom.getPreference(PREF_KEY_CATALOG_URL, '').then((url: string) => {
      if (url) setCatalogUrl(url)
      load(url || undefined)
    }).catch(() => load())
  }, [load])

  const saveCatalogUrl = async (url: string) => {
    setCatalogUrl(url)
    await window.loom.setPreference(PREF_KEY_CATALOG_URL, url)
    setRefreshing(true)
    await load(url || undefined)
    setRefreshing(false)
  }

  const handleRefresh = async () => {
    setRefreshing(true)
    await load(catalogUrl || undefined)
    setRefreshing(false)
  }

  const addBusy = (id: string) => setBusyIds(prev => new Set(prev).add(id))
  const removeBusy = (id: string) => setBusyIds(prev => { const n = new Set(prev); n.delete(id); return n })

  const handleInstall = async (pluginId: string) => {
    addBusy(pluginId)
    try {
      await installMarketPlugin(pluginId)
      await load()
      useStore.getState().addToast({ type: 'success', message: _t('marketplace.installSuccess', { id: pluginId }) })
    } catch (e: any) {
      useStore.getState().addToast({ type: 'error', message: _t('marketplace.installFailed', { message: e.message || e }) })
    } finally {
      removeBusy(pluginId)
    }
  }

  const handleUninstall = async (pluginId: string) => {
    const ok = await useStore.getState().showConfirm(t('marketplace.uninstallConfirmTitle'), _t('marketplace.uninstallConfirmMessage', { id: pluginId }), true)
    if (!ok) return
    addBusy(pluginId)
    try {
      await uninstallMarketPlugin(pluginId)
      await load()
      useStore.getState().addToast({ type: 'success', message: _t('marketplace.uninstallSuccess', { id: pluginId }) })
    } catch (e: any) {
      useStore.getState().addToast({ type: 'error', message: _t('marketplace.uninstallFailed', { message: e.message || e }) })
    } finally {
      removeBusy(pluginId)
    }
  }

  const handleUpdate = async (pluginId: string) => {
    addBusy(pluginId)
    try {
      await updateMarketPlugin(pluginId)
      await load()
      useStore.getState().addToast({ type: 'success', message: _t('marketplace.updateSuccess', { id: pluginId }) })
    } catch (e: any) {
      useStore.getState().addToast({ type: 'error', message: _t('marketplace.updateFailed', { message: e.message || e }) })
    } finally {
      removeBusy(pluginId)
    }
  }

  const filtered = useMemo(() => {
    const q = searchQuery.toLowerCase().trim()
    return plugins.filter(p => {
      if (p.kind !== activeKind) return false
      if (activeCategory !== ALL_CATEGORY && p.category !== activeCategory) return false
      if (!q) return true
      return (
        p.name.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q) ||
        p.author.toLowerCase().includes(q) ||
        p.tags.some(tag => tag.toLowerCase().includes(q))
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
      {!hideHeader && (
        <div className={styles.contentHeader}>
          <div className={styles.pluginsHeader}>
            <div className={styles.sectionHeaderRow}>
              <h3 className={styles.sectionTitle}>
                {t('marketplace.title')}
                <span className={styles.pluginsCountBadge}>{t('marketplace.countBadge', { n: plugins.length })}</span>
                {installedCount > 0 && (
                  <span className={styles.marketplaceInstalledSummary}>
                    {t('marketplace.installedSummary', { n: installedCount })}
                  </span>
                )}
              </h3>
              <button
                onClick={handleRefresh}
                disabled={refreshing || loading}
                className={styles.refreshBtn}
                title={t('marketplace.refresh')}
              >
                <IconRefresh size={14} />
              </button>
            </div>
            <div className={styles.pluginsSearchWrap}>
              <IconSearch size={14} className={styles.pluginsSearchIcon} />
              <input
                className={styles.pluginsSearchInput}
                type="text"
                placeholder={t('marketplace.searchPlaceholder')}
                value={searchQuery}
                onChange={e => setSearchQuery(e.target.value)}
              />
            </div>
            <p className={styles.pluginsDesc}>
              {t('marketplace.description')}
            </p>
          </div>
        </div>
      )}
      <div className={styles.contentBody}>
        {error && <p className={styles.toolsError}>{error}</p>}

        {/* Custom market source */}
        <div className={styles.marketplaceSourceRow}>
          <button
            className={styles.marketplaceCategoryBtn}
            onClick={() => setShowSourceConfig(!showSourceConfig)}
          >
            <IconSettings size={11} />
            {showSourceConfig ? t('marketplace.hideSource') : t('marketplace.customSource')}
          </button>
          {showSourceConfig && (
            <div className={styles.marketplaceSourceForm}>
              <input
                className={styles.pluginsSearchInput}
                type="text"
                placeholder={t('marketplace.urlPlaceholder')}
                value={catalogUrl}
                onChange={e => setCatalogUrl(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') saveCatalogUrl(catalogUrl) }}
              />
              <button
                className={styles.mcpConnectBtn}
                onClick={() => saveCatalogUrl(catalogUrl)}
              >
                {t('marketplace.load')}
              </button>
              {catalogUrl && (
                <button
                  className={styles.mcpCancelBtn}
                  onClick={() => saveCatalogUrl('')}
                >
                  {t('marketplace.reset')}
                </button>
              )}
            </div>
          )}
        </div>

        {!mode && (
          <div className={styles.marketplaceKindToggle}>
            <button
              className={`${styles.marketplaceKindBtn} ${activeKind === 'plugin' ? styles.marketplaceKindActive : ''}`}
              onClick={() => { setActiveKind('plugin'); setActiveCategory(ALL_CATEGORY) }}
            >
              <IconPackage size={13} />
              {t('marketplace.plugins')}
              <span className={styles.marketplaceKindCount}>{kindCounts.plugin}</span>
            </button>
            <button
              className={`${styles.marketplaceKindBtn} ${activeKind === 'skill' ? styles.marketplaceKindActive : ''}`}
              onClick={() => { setActiveKind('skill'); setActiveCategory(ALL_CATEGORY) }}
            >
              <IconSparkles size={13} />
              {t('marketplace.skills')}
              <span className={styles.marketplaceKindCount}>{kindCounts.skill}</span>
            </button>
          </div>
        )}

        {/* Category filter pills */}
        <div className={styles.marketplaceCategories}>
          {MARKETPLACE_CATEGORIES.map(cat => (
            <button
              key={cat}
              className={`${styles.marketplaceCategoryBtn} ${activeCategory === cat ? styles.marketplaceCategoryActive : ''}`}
              onClick={() => setActiveCategory(cat)}
            >
              {cat === ALL_CATEGORY ? t('common.selectAll') : cat}
            </button>
          ))}
        </div>

        {loading ? (
          <p className={styles.toolsEmpty}>{t('common.loading')}</p>
        ) : filtered.length === 0 ? (
          <div className={styles.marketplaceEmptyState}>
            <div className={styles.marketplaceEmptyIcon}>
              <IconStore size={32} />
            </div>
            <p className={styles.pluginsEmptyTitle}>
              {searchQuery || activeCategory !== ALL_CATEGORY ? t('marketplace.noResults') : t('marketplace.noPlugins')}
            </p>
            <p className={styles.pluginsEmptyHelp}>
              {searchQuery || activeCategory !== ALL_CATEGORY
                ? t('marketplace.tryOtherFilters')
                : t('marketplace.catalogEmpty')}
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
                            {t('marketplace.installed')}{plugin.installed_version ? ` v${plugin.installed_version}` : ''}
                          </span>
                          {plugin.has_update && (
                            <span className={styles.marketplaceUpdateBadge}>
                              <IconZap size={10} />
                              {t('marketplace.updateAvailable', { version: plugin.version })}
                            </span>
                          )}
                        </>
                      ) : (
                        <span className={styles.marketplaceNotInstalled}>{t('marketplace.notInstalled')}</span>
                      )}
                    </div>
                    <div className={styles.marketplaceCardActions}>
                      {plugin.homepage && (
                        <button
                          className={styles.marketplaceCardLinkBtn}
                          title={t('marketplace.viewHomepage')}
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
                              {isBusy ? t('marketplace.updating') : <><IconRefresh size={11} /> {t('marketplace.update')}</>}
                            </button>
                          )}
                          <button
                            className={styles.marketplaceUninstallBtn}
                            onClick={() => handleUninstall(plugin.id)}
                            disabled={isBusy}
                          >
                            {isBusy ? t('marketplace.uninstalling') : <><IconTrash size={11} /> {t('marketplace.uninstall')}</>}
                          </button>
                        </>
                      ) : (
                        <button
                          className={styles.marketplaceInstallBtn}
                          onClick={() => handleInstall(plugin.id)}
                          disabled={isBusy}
                        >
                          {isBusy ? t('marketplace.installing') : <><IconPackage size={11} /> {t('marketplace.install')}</>}
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

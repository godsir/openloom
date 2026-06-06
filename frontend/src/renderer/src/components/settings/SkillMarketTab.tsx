import { useState, useEffect, useCallback, useMemo } from 'react'
import { useStore } from '../../stores'
import { IconRefresh, IconSearch, IconSparkles, IconCheck, IconTrash, IconExternalLink, IconSettings } from '../../utils/icons'
import { loomRpc } from '../../services/jsonrpc'
import styles from '../shared/SettingsModal.module.css'

const PREF_KEY_SKILL_MARKET_URL = 'skillMarketBaseUrl'
const DEFAULT_SKILL_MARKET_URL = 'https://clawhub.ai'

interface ClawhubSkill {
  id: string
  name: string
  description: string
  version: string
  author: string
  downloads: number
  stars: number
  tags: Record<string, string> | null
  installed: boolean
  installed_version: string | null
  has_update: boolean
  source: string
}

export default function SkillMarketTab({ hideHeader }: { hideHeader?: boolean }) {
  const [skills, setSkills] = useState<ClawhubSkill[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [busyIds, setBusyIds] = useState<Set<string>>(new Set())
  const [refreshing, setRefreshing] = useState(false)
  const [currentPage, setCurrentPage] = useState(1)
  const [pageSize, setPageSize] = useState(25)
  const [baseUrl, setBaseUrl] = useState(DEFAULT_SKILL_MARKET_URL)
  const [showSourceConfig, setShowSourceConfig] = useState(false)

  const load = useCallback(async (search?: string, force = false, url?: string) => {
    setLoading(true)
    setError(null)
    try {
      const res = await loomRpc<{ skills: ClawhubSkill[]; cached?: boolean }>(
        'clawhub.list',
        { ...(search ? { search } : {}), force, base_url: url || baseUrl }
      )
      setSkills(res.skills ?? [])
      setCurrentPage(1)
    } catch (e: any) {
      setError(`加载失败: ${e.message || e}`)
    } finally {
      setLoading(false)
    }
  }, [baseUrl])

  // Load custom base URL from preferences on mount
  useEffect(() => {
    window.loom.getPreference(PREF_KEY_SKILL_MARKET_URL, DEFAULT_SKILL_MARKET_URL).then((url: string) => {
      if (url && url !== baseUrl) setBaseUrl(url)
      load(undefined, false, url || DEFAULT_SKILL_MARKET_URL)
    }).catch(() => load())
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const saveBaseUrl = async (url: string) => {
    setBaseUrl(url)
    await window.loom.setPreference(PREF_KEY_SKILL_MARKET_URL, url)
    setRefreshing(true)
    await load(undefined, true, url)
    setRefreshing(false)
  }

  // Debounced search
  useEffect(() => {
    const timer = setTimeout(() => load(searchQuery || undefined), 400)
    return () => clearTimeout(timer)
  }, [searchQuery, load])

  const handleRefresh = async () => {
    setRefreshing(true)
    await load(undefined, true)
    setRefreshing(false)
  }

  const handleInstall = async (slug: string) => {
    setBusyIds(prev => new Set(prev).add(slug))
    try {
      await loomRpc('clawhub.install', { slug })
      useStore.getState().addToast({ type: 'success', message: `技能 "${slug}" 安装成功` })
      await load(searchQuery || undefined)
    } catch (e: any) {
      useStore.getState().addToast({ type: 'error', message: `安装失败: ${e.message || e}` })
    } finally {
      setBusyIds(prev => { const next = new Set(prev); next.delete(slug); return next })
    }
  }

  const handleUninstall = async (slug: string) => {
    setBusyIds(prev => new Set(prev).add(slug))
    try {
      await loomRpc('clawhub.uninstall', { slug })
      useStore.getState().addToast({ type: 'success', message: `技能 "${slug}" 已卸载` })
      await load(searchQuery || undefined)
    } catch (e: any) {
      useStore.getState().addToast({ type: 'error', message: `卸载失败: ${e.message || e}` })
    } finally {
      setBusyIds(prev => { const next = new Set(prev); next.delete(slug); return next })
    }
  }

  const filtered = useMemo(() => {
    if (!searchQuery) return skills
    const q = searchQuery.toLowerCase()
    return skills.filter(s =>
      s.name.toLowerCase().includes(q) ||
      s.description.toLowerCase().includes(q) ||
      s.id.toLowerCase().includes(q) ||
      (s.tags && Object.keys(s.tags).some(t => t.toLowerCase().includes(q)))
    )
  }, [skills, searchQuery])

  // Reset to page 1 when search query changes
  useEffect(() => {
    setCurrentPage(1)
  }, [searchQuery])

  // Pagination
  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSize))
  const safePage = Math.min(currentPage, totalPages)
  const paginatedSkills = useMemo(() => {
    const start = (safePage - 1) * pageSize
    return filtered.slice(start, start + pageSize)
  }, [filtered, safePage, pageSize])

  // Generate page numbers with ellipsis
  const pageNumbers = useMemo(() => {
    const pages: (number | 'ellipsis')[] = []
    const total = totalPages
    if (total <= 7) {
      for (let i = 1; i <= total; i++) pages.push(i)
    } else {
      pages.push(1)
      if (safePage > 3) pages.push('ellipsis')
      const start = Math.max(2, safePage - 1)
      const end = Math.min(total - 1, safePage + 1)
      for (let i = start; i <= end; i++) pages.push(i)
      if (safePage < total - 2) pages.push('ellipsis')
      pages.push(total)
    }
    return pages
  }, [totalPages, safePage])

  const handlePageSizeChange = (size: number) => {
    setPageSize(size)
    setCurrentPage(1)
  }

  const startItem = filtered.length === 0 ? 0 : (safePage - 1) * pageSize + 1
  const endItem = Math.min(safePage * pageSize, filtered.length)

  const installedCount = useMemo(() => skills.filter(s => s.installed).length, [skills])

  return (
    <>
      {!hideHeader && (
        <div className={styles.contentHeader}>
          <div className={styles.sectionHeaderRow}>
            <div>
              <h3 className={styles.sectionTitle}>技能市场</h3>
              <p className={styles.sectionDesc}>
                Clawhub 社区技能注册表 · {skills.length} 个技能
                {installedCount > 0 && (
                  <span className={styles.marketplaceInstalledSummary}>
                    {installedCount} 已安装
                  </span>
                )}
              </p>
            </div>
            <button onClick={handleRefresh} disabled={refreshing} className={styles.refreshBtn} title="刷新">
              <IconRefresh size={14} />
            </button>
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
            {showSourceConfig ? '收起' : '自定义源'}
          </button>
          {showSourceConfig && (
            <div className={styles.marketplaceSourceForm}>
              <input
                className={styles.pluginsSearchInput}
                type="text"
                placeholder="输入技能市场 API 地址..."
                value={baseUrl}
                onChange={e => setBaseUrl(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') saveBaseUrl(baseUrl) }}
              />
              <button
                className={styles.mcpConnectBtn}
                onClick={() => saveBaseUrl(baseUrl)}
              >
                加载
              </button>
              {baseUrl !== DEFAULT_SKILL_MARKET_URL && (
                <button
                  className={styles.mcpCancelBtn}
                  onClick={() => saveBaseUrl(DEFAULT_SKILL_MARKET_URL)}
                >
                  重置
                </button>
              )}
            </div>
          )}
        </div>

        {/* Search */}
        <div className={styles.pluginsSearchWrap}>
          <span className={styles.pluginsSearchIcon}><IconSearch size={14} /></span>
          <input
            type="text"
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
            placeholder="搜索 Clawhub 技能..."
            className={styles.pluginsSearchInput}
          />
        </div>

        {loading && !refreshing && <p className={styles.toolsEmpty}>加载中...</p>}

        {!loading && filtered.length === 0 && (
          <div className={styles.marketplaceEmptyState}>
            <div className={styles.marketplaceEmptyIcon}><IconSparkles size={28} /></div>
            <p className={styles.pluginsEmptyTitle}>
              {searchQuery ? '未找到匹配的技能' : '暂无技能'}
            </p>
            <p className={styles.pluginsEmptyHelp}>
              {searchQuery ? '尝试其他关键词搜索' : '点击刷新按钮重新加载'}
            </p>
          </div>
        )}

        {/* Skill list */}
        <div className={styles.marketplaceGrid}>
          {paginatedSkills.map(skill => (
            <div key={skill.id} className={`${styles.marketplaceCard} ${skill.installed ? styles.marketplaceCardInstalled : ''}`}>
              <div className={styles.marketplaceCardHeader}>
                <div className={styles.marketplaceCardTitleRow}>
                  <span className={styles.marketplaceCardName}>{skill.name}</span>
                  {skill.version && <span className={styles.marketplaceCardVersion}>v{skill.version}</span>}
                </div>
                {skill.description && (
                  <p className={styles.marketplaceCardDesc}>{skill.description.slice(0, 160)}</p>
                )}
              </div>

              {/* Tags */}
              {skill.tags && Object.keys(skill.tags).length > 0 && (
                <div className={styles.marketplaceCardTags}>
                  {Object.entries(skill.tags).slice(0, 5).map(([tag, ver]) => (
                    <span key={tag} className={styles.marketplaceCardTag}>{tag}{ver !== 'latest' ? ` ${ver}` : ''}</span>
                  ))}
                </div>
              )}

              {/* Stats & actions */}
              <div className={styles.marketplaceCardFooter}>
                <div className={styles.marketplaceCardMeta}>
                  {skill.downloads > 0 && (
                    <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>↓ {skill.downloads.toLocaleString()}</span>
                  )}
                  {skill.stars > 0 && (
                    <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>★ {skill.stars}</span>
                  )}
                  {skill.installed && (
                    <span className={styles.marketplaceInstalledBadge}>
                      <IconCheck size={11} />
                      已安装{skill.installed_version ? ` v${skill.installed_version}` : ''}
                    </span>
                  )}
                  {skill.has_update && (
                    <span className={styles.marketplaceUpdateBadge}>有更新</span>
                  )}
                </div>
                <div className={styles.marketplaceCardActions}>
                  <a
                    href={`https://clawhub.ai/skills/${skill.id}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className={styles.marketplaceCardLinkBtn}
                    title="在 Clawhub 查看"
                  >
                    <IconExternalLink size={13} />
                  </a>
                  {skill.installed ? (
                    <button
                      onClick={() => handleUninstall(skill.id)}
                      disabled={busyIds.has(skill.id)}
                      className={styles.marketplaceUninstallBtn}
                    >
                      <IconTrash size={12} />
                      卸载
                    </button>
                  ) : (
                    <button
                      onClick={() => handleInstall(skill.id)}
                      disabled={busyIds.has(skill.id)}
                      className={styles.marketplaceInstallBtn}
                    >
                      {busyIds.has(skill.id) ? '安装中...' : '安装'}
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>

        {/* Pagination */}
        {!loading && filtered.length > 0 && (
          <div className={styles.marketplacePagination}>
            <div className={styles.marketplacePaginationInfo}>
              {startItem}-{endItem} / {filtered.length} 个技能
            </div>
            <div className={styles.marketplacePaginationControls}>
              <button
                className={styles.marketplacePageBtn}
                disabled={safePage <= 1}
                onClick={() => setCurrentPage(p => Math.max(1, p - 1))}
              >
                &lt;
              </button>
              {pageNumbers.map((p, i) =>
                p === 'ellipsis' ? (
                  <span key={`ellipsis-${i}`} className={styles.marketplacePageEllipsis}>...</span>
                ) : (
                  <button
                    key={p}
                    className={`${styles.marketplacePageBtn} ${p === safePage ? styles.marketplacePageBtnActive : ''}`}
                    onClick={() => setCurrentPage(p)}
                  >
                    {p}
                  </button>
                )
              )}
              <button
                className={styles.marketplacePageBtn}
                disabled={safePage >= totalPages}
                onClick={() => setCurrentPage(p => Math.min(totalPages, p + 1))}
              >
                &gt;
              </button>
              <select
                className={styles.marketplacePageSizeSelect}
                value={pageSize}
                onChange={e => handlePageSizeChange(Number(e.target.value))}
              >
                <option value={25}>25/页</option>
                <option value={50}>50/页</option>
                <option value={100}>100/页</option>
              </select>
            </div>
          </div>
        )}
      </div>
    </>
  )
}

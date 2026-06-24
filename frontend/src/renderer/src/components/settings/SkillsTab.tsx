import { useState, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { IconFolder, IconPackage, IconRefresh, IconSearch } from '../../utils/icons'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'
import { useLocale, t as _t } from '../../i18n'
import styles from '../shared/SettingsModal.module.css'

interface SkillInfo {
  name: string
  description?: string
  path?: string
  version?: string
  user_invocable?: boolean
  always_active?: boolean
}

/** Derive a source category from a skill's on-disk path.
 *  `group` is the full i18n label, `short` is the compact chip/card label,
 *  `icon` is the single-letter avatar. */
function skillSource(path: string | undefined): { group: string; short: string; icon: string } {
  if (!path) return { group: _t('skills.sourceOther'), short: _t('skills.sourceOther'), icon: 'M' }
  const p = path.replace(/\\/g, '/')
  if (p.includes('.claude/skills')) return { group: _t('skills.sourceClaudeCode'), short: 'Claude', icon: 'C' }
  if (p.includes('.openclaw')) return { group: _t('skills.sourceOpenclaw'), short: 'OpenClaw', icon: 'O' }
  if (p.includes('.codex')) return { group: _t('skills.sourceCodex'), short: 'Codex', icon: 'X' }
  if (p.includes('.loom/skills')) return { group: _t('skills.sourceLoomUser'), short: 'Loom', icon: 'L' }
  return { group: _t('skills.sourceOther'), short: _t('skills.sourceOther'), icon: 'M' }
}

const ALL = '__all__'

export default function SkillsTab() {
  const { t } = useLocale()
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [selectedSkill, setSelectedSkill] = useState<string | null>(null)
  const [skillContent, setSkillContent] = useState<string | null>(null)
  const [loadingContent, setLoadingContent] = useState(false)
  const [importing, setImporting] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [activeSource, setActiveSource] = useState<string>(ALL)
  const [copied, setCopied] = useState(false)
  const [page, setPage] = useState(0)

  const PAGE_SIZE = 12

  const loadSkills = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const res = await loomRpc<{ skills: SkillInfo[] }>('skills.list')
      setSkills(res.skills ?? [])
    } catch (e: any) {
      setError(_t('skills.loadFailed', { message: e.message || e }))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { loadSkills() }, [loadSkills])

  // Reset to first page whenever the source filter or search changes
  useEffect(() => { setPage(0) }, [activeSource, searchQuery])

  // Search filter
  const q = searchQuery.toLowerCase().trim()
  const searched = q
    ? skills.filter(s => s.name.toLowerCase().includes(q) || (s.description ?? '').toLowerCase().includes(q))
    : skills

  // Source chips with counts (derived from the full set so counts stay stable
  // while typing in search — chips reflect what exists, not what's filtered).
  const sourceOrder: { short: string; group: string }[] = []
  const sourceCounts: Record<string, number> = {}
  for (const s of skills) {
    const { group, short } = skillSource(s.path)
    if (!(group in sourceCounts)) {
      sourceCounts[group] = 0
      sourceOrder.push({ short, group })
    }
    sourceCounts[group]++
  }

  // Apply source-chip filter on top of search
  const filtered = activeSource === ALL
    ? searched
    : searched.filter(s => skillSource(s.path).group === activeSource)

  // Pagination: 12 per page (3 rows × 4 cols)
  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE))
  const safePage = Math.min(page, totalPages - 1)
  const paged = filtered.slice(safePage * PAGE_SIZE, safePage * PAGE_SIZE + PAGE_SIZE)

  const selected = selectedSkill ? skills.find(s => s.name === selectedSkill) ?? null : null

  const handleSelectSkill = async (name: string) => {
    if (selectedSkill === name) {
      // Toggle closed
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
      setSkillContent(_t('skills.loadFailed', { message: e.message || e }))
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
          const firstPath = fileList[0].webkitRelativePath || fileList[0].name
          const skillName = firstPath.split('/')[0]
          const files: { path: string; content: string }[] = []
          for (let i = 0; i < fileList.length; i++) {
            const f = fileList[i]
            const relPath = (f.webkitRelativePath || f.name).replace(`${skillName}/`, '')
            const content = await f.text()
            files.push({ path: relPath, content })
          }
          await rpc('skills.import', { name: skillName, files }, _t('skills.importSuccess', { name: skillName }))
          await loadSkills()
        } catch (e: any) {
          setError(_t('skills.importFailed', { message: e.message || e }))
        } finally {
          setImporting(false)
        }
      }
      input.click()
    } catch (e: any) {
      setError(_t('skills.importFailed', { message: e.message || e }))
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
          await rpc('skills.import', { name: skillName, files }, _t('skills.importSuccess', { name: skillName }))
          await loadSkills()
        } catch (e: any) {
          setError(_t('skills.importFailed', { message: e.message || e }))
        } finally {
          setImporting(false)
        }
      }
      input.click()
    } catch (e: any) {
      setError(_t('skills.importFailed', { message: e.message || e }))
    }
  }

  const handleDelete = async (name: string) => {
    const ok = await useStore.getState().showConfirm(t('skills.deleteConfirmTitle'), _t('skills.deleteConfirmMessage', { name }), true)
    if (!ok) return
    try {
      await rpc('skills.delete', { name }, _t('skills.deleteSuccess', { name }))
      setSelectedSkill(null)
      setSkillContent(null)
      await loadSkills()
    } catch { /* toast already shown */ }
  }

  const handleCopyPath = async (path?: string) => {
    if (!path) return
    try {
      await navigator.clipboard.writeText(path)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch { /* clipboard unavailable */ }
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
          <h3 className={styles.sectionTitle}>
            {t('skills.title')}
            <span className={styles.pluginsCountBadge}>{t('skills.countBadge', { n: skills.length })}</span>
          </h3>
          <button
            onClick={async () => {
              setRefreshing(true)
              try {
                await loomRpc('skills.reload')
                await loadSkills()
              } catch { /* ignore reload errors */ }
              setRefreshing(false)
            }}
            disabled={refreshing || loading}
            className={styles.refreshBtn}
            title={t('skills.rescan')}
          >
            <IconRefresh size={14} />
          </button>
        </div>
        <div className={styles.pluginsSearchWrap}>
          <IconSearch size={13} className={styles.pluginsSearchIcon} />
          <input
            type="text"
            className={styles.pluginsSearchInput}
            placeholder={t('skills.searchPlaceholder')}
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
          />
          {searchQuery && (
            <button className={styles.skillSearchClear} onClick={() => setSearchQuery('')}>&times;</button>
          )}
        </div>
        <p className={styles.sectionDesc}>{t('skills.description')}</p>
      </div>

      <div className={styles.skillLayout}>
        {/* ── Main: chips + grid ── */}
        <div className={styles.skillMain}>
          {error && <p className={styles.toolsError}>{error}</p>}

          <div className={styles.skillActions}>
            <button onClick={handleImportFolder} disabled={importing} className={styles.mcpAddBtn}>
              {importing ? t('skills.importing') : <><IconFolder size={14} /> {t('skills.importFolder')}</>}
            </button>
            <button onClick={handleImportZip} disabled={importing} className={styles.mcpAddBtn}>
              {importing ? t('skills.importing') : <><IconPackage size={14} /> {t('skills.importZip')}</>}
            </button>
          </div>

          {/* Source filter chips */}
          {sourceOrder.length > 0 && (
            <div className={styles.skillChips}>
              <button
                className={`${styles.skillChip} ${activeSource === ALL ? styles.skillChipActive : ''}`}
                onClick={() => setActiveSource(ALL)}
              >
                {t('skills.filterAll')} <span className={styles.skillChipCount}>{skills.length}</span>
              </button>
              {sourceOrder.map(({ short, group }) => (
                <button
                  key={group}
                  className={`${styles.skillChip} ${activeSource === group ? styles.skillChipActive : ''}`}
                  onClick={() => setActiveSource(group)}
                >
                  {short} <span className={styles.skillChipCount}>{sourceCounts[group]}</span>
                </button>
              ))}
            </div>
          )}

          {loading ? (
            <p className={styles.toolsEmpty}>{t('common.loading')}</p>
          ) : filtered.length === 0 ? (
            <p className={styles.toolsEmpty}>{searchQuery ? t('skills.noResults') : t('skills.noDiscovered')}</p>
          ) : (
            <>
              <div className={styles.skillGrid}>
                {paged.map((skill, i) => {
                  const isSelected = selectedSkill === skill.name
                  const { icon } = skillSource(skill.path)
                  return (
                    <div
                      key={skill.path || `${skill.name}-${i}`}
                      className={`${styles.skillGridCard} ${isSelected ? styles.skillGridCardActive : ''}`}
                      onClick={() => handleSelectSkill(skill.name)}
                    >
                      <div className={styles.skillGridCardTop}>
                        <span className={styles.skillCardIcon}>{icon}</span>
                        <span className={styles.skillGridCardName}>{skill.name}</span>
                      </div>
                      <p className={styles.skillGridCardDesc}>{skill.description}</p>
                      {(skill.version || skill.always_active) && (
                        <div className={styles.skillGridCardStatus}>
                          {skill.version && <span className={styles.skillGridCardVer}>{skill.version}</span>}
                          {skill.version && skill.always_active && <span className={styles.skillGridCardStatusDot} />}
                          {skill.always_active && (
                            <span className={styles.skillGridCardResident}>
                              <span className={styles.skillGridCardResidentDot} />
                              {t('skills.resident')}
                            </span>
                          )}
                        </div>
                      )}
                    </div>
                  )
                })}
              </div>

              {filtered.length > PAGE_SIZE && (
                <div className={styles.skillPagination}>
                  <span className={styles.skillPageInfo}>
                    {t('skills.pageInfo', { total: String(filtered.length), current: String(safePage + 1), pages: String(totalPages) })}
                  </span>
                  <div className={styles.skillPageControls}>
                    <button className={styles.skillPageBtn} disabled={safePage === 0} onClick={() => setPage(0)}>{t('skills.firstPage')}</button>
                    <button className={styles.skillPageBtn} disabled={safePage === 0} onClick={() => setPage(safePage - 1)}>{t('skills.previousPage')}</button>
                    <button className={styles.skillPageBtn} disabled={safePage >= totalPages - 1} onClick={() => setPage(safePage + 1)}>{t('skills.nextPage')}</button>
                    <button className={styles.skillPageBtn} disabled={safePage >= totalPages - 1} onClick={() => setPage(totalPages - 1)}>{t('skills.lastPage')}</button>
                  </div>
                </div>
              )}
            </>
          )}

          <p className={styles.lspInfoText}>{t('skills.infoText')}</p>
        </div>

        {/* ── Drawer: floating detail overlay ── */}
        {selected && (
          <>
            <div
              className={styles.skillDrawerBackdrop}
              onClick={() => { setSelectedSkill(null); setSkillContent(null) }}
            />
            <div className={styles.skillDrawer}>
            <div className={styles.skillDrawerHead}>
              <div className={styles.skillDrawerTitle}>
                <span className={styles.skillCardIcon}>{skillSource(selected.path).icon}</span>
                <span className={styles.skillDrawerName}>{selected.name}</span>
              </div>
              <button
                className={styles.skillDrawerClose}
                onClick={() => { setSelectedSkill(null); setSkillContent(null) }}
                title={t('skills.closeDetail')}
              >✕</button>
            </div>

            {(selected.version || selected.user_invocable || selected.always_active || selected.path) && (
              <div className={styles.skillDrawerBadges}>
                {selected.version && <span className={styles.skillBadge}>{selected.version}</span>}
                {selected.user_invocable && <span className={`${styles.skillBadge} ${styles.skillBadgeAccent}`}>user</span>}
                {selected.always_active && <span className={`${styles.skillBadge} ${styles.skillBadgeGreen}`}>active</span>}
              </div>
            )}

            <div className={styles.skillDrawerBody}>
              {loadingContent ? (
                <p className={styles.toolsEmpty}>{t('common.loading')}</p>
              ) : (
                <div className={styles.skillDetailRendered} dangerouslySetInnerHTML={{ __html: renderBody(skillContent!) }} />
              )}
            </div>

            <div className={styles.skillDrawerFoot}>
              <button
                className={styles.skillDrawerBtn}
                onClick={() => handleCopyPath(selected.path)}
                disabled={!selected.path}
              >
                {copied ? t('skills.pathCopied') : t('skills.copyPath')}
              </button>
              <button
                className={`${styles.skillDrawerBtn} ${styles.skillDrawerBtnDanger}`}
                onClick={() => handleDelete(selected.name)}
              >
                {t('common.delete')}
              </button>
            </div>
          </div>
          </>
        )}
      </div>
    </>
  )
}

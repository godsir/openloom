import { useState, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { IconFolder, IconPackage, IconRefresh, IconSearch, IconChevronRight, IconChevronDown, IconStore } from '../../utils/icons'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'
import styles from '../shared/SettingsModal.module.css'
import SkillMarketTab from './SkillMarketTab'

interface SkillInfo {
  name: string
  description?: string
  path?: string
  version?: string
  user_invocable?: boolean
  always_active?: boolean
}

/** Derive a source category from a skill's on-disk path. */
function skillSource(path: string | undefined): { group: string; icon: string } {
  if (!path) return { group: '其他', icon: 'M' }
  const p = path.replace(/\\/g, '/')
  if (p.includes('.claude/plugins') || p.includes('.claude/skills')) return { group: 'Claude Code (兼容)', icon: 'C' }
  if (p.includes('.openclaw')) return { group: 'OpenClaw (兼容)', icon: 'O' }
  if (p.includes('.codex')) return { group: 'Codex (兼容)', icon: 'X' }
  if (p.includes('.loom/skills')) return { group: 'openLoom 用户', icon: 'L' }
  if (p.includes('.loom/plugins')) return { group: 'openLoom 插件', icon: 'P' }
  return { group: '其他', icon: 'M' }
}

export default function SkillsTab() {
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
  const [view, setView] = useState<'installed' | 'market'>('installed')

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
          <h3 className={styles.sectionTitle}>
            技能
            <span className={styles.pluginsCountBadge}>{skills.length} 个</span>
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
            onClick={async () => { setRefreshing(true); await loadSkills(); setRefreshing(false) }}
            disabled={refreshing || loading}
            className={styles.refreshBtn}
            title="重新扫描技能"
          >
            <IconRefresh size={14} />
          </button>
        </div>
        <div className={styles.pluginsSearchWrap}>
          <IconSearch size={13} className={styles.pluginsSearchIcon} />
          <input
            type="text"
            className={styles.pluginsSearchInput}
            placeholder="搜索技能名称或描述..."
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
          />
          {searchQuery && (
            <button className={styles.skillSearchClear} onClick={() => setSearchQuery('')}>&times;</button>
          )}
        </div>
        <p className={styles.sectionDesc}>管理技能定义 — 支持文件夹或 ZIP 导入</p>
      </div>
      {view === 'market' ? (
        <SkillMarketTab hideHeader />
      ) : (
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
      )}
    </>
  )
}

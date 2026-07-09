import { useState, useRef, useEffect, useCallback, useMemo, type ReactElement } from 'react'
import { IconGitBranch, IconChevronDown, IconPlus, IconCheck, IconAlertCircle } from '../../utils/icons'
import { useLocale } from '../../i18n'
import styles from './GitBranchPicker.module.css'

interface GitBranchRow {
  name: string
  current: boolean
}

type GitBranchesResult = {
  ok: true
  repositoryRoot: string
  currentBranch: string | null
  branches: GitBranchRow[]
  remoteUrl: string | null
} | {
  ok: false
  reason: string
  message: string
}

interface Props {
  workspaceRoot: string
}

export function GitBranchPicker({ workspaceRoot }: Props): ReactElement | null {
  const { t } = useLocale()
  const root = workspaceRoot.trim()
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [result, setResult] = useState<GitBranchesResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [actingBranch, setActingBranch] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const wrapRef = useRef<HTMLDivElement | null>(null)
  const inputRef = useRef<HTMLInputElement | null>(null)

  const load = useCallback(async () => {
    if (!root) return
    setLoading(true)
    setError(null)
    try {
      const next = await window.loom.getGitBranches(root)
      setResult(next)
      if (!next.ok) setError(next.message)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [root])

  useEffect(() => { void load() }, [load])
  useEffect(() => { setOpen(false); setQuery('') }, [root])

  useEffect(() => {
    if (!open) return
    void load()
    setTimeout(() => inputRef.current?.focus(), 0)
  }, [load, open])

  useEffect(() => {
    if (!open) return
    const onPointerDown = (event: PointerEvent) => {
      if (event.target instanceof Node && wrapRef.current?.contains(event.target)) return
      setOpen(false)
    }
    window.addEventListener('pointerdown', onPointerDown)
    return () => window.removeEventListener('pointerdown', onPointerDown)
  }, [open])

  const branches = useMemo(() => (result?.ok ? result.branches : []), [result])
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return branches
    return branches.filter(b => b.name.toLowerCase().includes(q))
  }, [branches, query])

  const trimmedQuery = query.trim()
  const canCreate = trimmedQuery.length > 0 && !branches.find(b => b.name === trimmedQuery)
  const currentBranch = result?.ok ? result.currentBranch : null
  const remoteUrl = result?.ok ? result.remoteUrl : null
  const label = currentBranch || (result?.ok ? '...' : t('git.branchUnavailable'))

  const switchBranch = async (branch: string) => {
    if (!root || !branch) return
    setActingBranch(branch)
    setError(null)
    try {
      const next = await window.loom.switchGitBranch(root, branch)
      setResult(next)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setActingBranch(null)
    }
  }

  const createBranch = async () => {
    if (!root || !trimmedQuery) return
    setActingBranch(trimmedQuery)
    setError(null)
    try {
      const next = await window.loom.createAndSwitchGitBranch(root, trimmedQuery)
      if (next.ok) {
        setResult(next)
        setOpen(false)
        setQuery('')
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setActingBranch(null)
    }
  }

  const handleSelect = (branch: GitBranchRow) => {
    if (branch.current) { setOpen(false); return }
    void switchBranch(branch.name)
  }

  if (!root) return null

  return (
    <div ref={wrapRef} className={styles.wrap}>
      {remoteUrl && (
        <button
          className={styles.githubBtn}
          title={remoteUrl}
          onClick={() => window.loom.openExternal(remoteUrl)}
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>
          </svg>
        </button>
      )}
      <button
        type="button"
        className={`${styles.trigger} ${open ? styles.triggerOpen : ''}`}
        onClick={() => setOpen(v => !v)}
      >
        <IconGitBranch size={13} />
        <span className={styles.triggerLabel}>{label}</span>
        <IconChevronDown size={10} />
      </button>

      {open && (
        <div className={styles.popup}>
          <div className={styles.searchBox}>
            <input
              ref={inputRef}
              value={query}
              onChange={e => setQuery(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Escape') { e.preventDefault(); setOpen(false) }
                if (e.key === 'Enter') {
                  if (canCreate) { e.preventDefault(); void createBranch() }
                  else if (trimmedQuery) {
                    const match = branches.find(b => b.name === trimmedQuery || b.name.includes(trimmedQuery))
                    if (match) { e.preventDefault(); handleSelect(match) }
                  }
                }
              }}
              placeholder={t('git.searchBranches')}
              className={styles.searchInput}
            />
          </div>

          <div className={styles.list}>
            {error && (
              <div className={styles.error}>
                <IconAlertCircle size={14} style={{ marginTop: 1, flexShrink: 0 }} />
                <span>{error}</span>
              </div>
            )}

            {loading && !result && (
              <div className={styles.loading}>{t('common.loading')}</div>
            )}

            {filtered.map(branch => (
              <button
                key={branch.name}
                type="button"
                disabled={actingBranch !== null}
                onClick={() => handleSelect(branch)}
                className={`${styles.branchItem} ${branch.current ? styles.branchItemCurrent : ''}`}
              >
                <IconGitBranch size={13} style={{ flexShrink: 0, color: 'var(--text-muted)' }} />
                <span className={styles.branchName}>{branch.name}</span>
                {branch.current && <IconCheck size={14} style={{ flexShrink: 0 }} />}
              </button>
            ))}

            {!loading && filtered.length === 0 && !error && (
              <div className={styles.empty}>{t('git.noBranches')}</div>
            )}
          </div>

          {canCreate && (
            <div className={styles.footer}>
              <button
                type="button"
                disabled={actingBranch !== null}
                onClick={() => void createBranch()}
                className={styles.createBtn}
              >
                <IconPlus size={13} style={{ flexShrink: 0, color: 'var(--text-muted)' }} />
                <span className={styles.branchName}>{t('git.createBranch')} {trimmedQuery}</span>
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

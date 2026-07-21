import { useState, useMemo, useCallback, useEffect } from 'react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import { sendMessage } from '../../services/sendMessage'
import { loomRpc } from '../../services/jsonrpc'
import { IconChevronDown, IconAlertTriangle, IconAlertCircle, IconInfo, IconX } from '../../utils/icons'
import styles from './ReviewPanel.module.css'

interface Finding { severity: string; file: string; line?: number; summary: string; suggestion?: string }
interface ReviewData { findings: Finding[]; total: number; critical: number; important: number; minor: number }
interface GitFile { status: string; file: string; adds: number; dels: number }
interface UnpushedCommit { hash: string; subject: string }
interface GitChanges { files: GitFile[]; diff: string; repoRoot: string; error?: string; unpushedCommits: number; ahead: number; behind: number; unpushedLog: UnpushedCommit[] }

function extractReviewData(messages: any[]): ReviewData | null {
  for (let i = messages.length - 1; i >= 0; i--) {
    const msg = messages[i]
    if (msg.role !== 'assistant' || !msg.blocks) continue
    for (const block of msg.blocks) {
      if (block.type !== 'shell' || block.toolName !== 'report_findings') continue
      const sc = block.structured_content || block.details?.structured_content
      if (!sc) continue
      const findings = (sc.findings || []) as Finding[]
      if (findings.length === 0) continue
      return { findings, total: findings.length,
        critical: findings.filter((f: Finding) => f.severity === 'critical').length,
        important: findings.filter((f: Finding) => f.severity === 'important').length,
        minor: findings.filter((f: Finding) => f.severity === 'minor').length }
    }
  }
  return null
}

function splitDiffByFile(diff: string): Record<string, string> {
  const result: Record<string, string> = {}
  let currentFile = ''
  let currentChunk = ''
  for (const line of diff.split('\n')) {
    if (line.startsWith('diff --git ')) {
      if (currentFile && currentChunk) result[currentFile] = currentChunk.trim()
      currentFile = ''
      currentChunk = line + '\n'
    } else if (line.startsWith('+++ b/')) {
      currentFile = line.replace('+++ b/', '').trim()
      currentChunk += line + '\n'
    } else if (currentFile) {
      currentChunk += line + '\n'
    }
  }
  if (currentFile && currentChunk) result[currentFile] = currentChunk.trim()
  return result
}

const SEV_ICON: Record<string, (s: number) => JSX.Element> = { critical: (s) => <IconAlertTriangle size={s} />, important: (s) => <IconAlertCircle size={s} />, minor: (s) => <IconInfo size={s} /> }

// Build colorized diff HTML (rendered via dangerouslySetInnerHTML).
// All user content is HTML-escaped before color spans are applied.
function colorizeDiff(diff: string): string {
  const lines = diff.split('\n')
  let lineNum = 0
  return lines.map((line) => {
    const escaped = line.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    if (line.startsWith('@@')) {
      const m = line.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/)
      if (m) lineNum = parseInt(m[2]) - 1
      return `<span class="dh">${escaped}</span>`
    }
    if (line.startsWith('diff --git') || line.startsWith('--- ') || line.startsWith('+++ ')) return `<span class="dh">${escaped}</span>`
    if (line.startsWith('index ')) return `<span class="dm">${escaped}</span>`
    let cls: string
    if (line.startsWith('-')) { cls = 'dd' }
    else if (line.startsWith('+')) { lineNum++; cls = 'da' }
    else { lineNum++; cls = 'dc' }
    return `<span class="${cls}"><span class="ln">${line.startsWith('-') ? '' : String(lineNum)}</span>${escaped}</span>`
  }).join('\n')
}

export default function ReviewPanel() {
  const { t } = useLocale()
  const reviewPanelOpen = useStore(s => s.reviewPanelOpen)
  const toggleReviewPanel = useStore(s => s.toggleReviewPanel)
  const [expandedFiles, setExpandedFiles] = useState<Set<string>>(new Set())
  const [expandedFindings, setExpandedFindings] = useState<Set<number>>(new Set())
  const [gitData, setGitData] = useState<GitChanges | null>(null)
  const [loadingGit, setLoadingGit] = useState(false)
  const [reviewing, setReviewing] = useState(false)
  const [commitMsg, setCommitMsg] = useState('')
  const [committing, setCommitting] = useState(false)
  const [pushing, setPushing] = useState(false)
  const [generatingCommit, setGeneratingCommit] = useState(false)
  const [committed, setCommitted] = useState(false)

  const totalAdds = useMemo(() => gitData?.files.reduce((s, f) => s + f.adds, 0) ?? 0, [gitData])
  const totalDels = useMemo(() => gitData?.files.reduce((s, f) => s + f.dels, 0) ?? 0, [gitData])
  const sessionId = useStore(s => s.currentSessionId)
  const messagesBySession = useStore(s => s.messagesBySession)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)
  const defaultWorkspace = useStore(s => s.defaultWorkspace)
  const workspaceRoot = sessionId ? (sessionWorkspaces[sessionId] || defaultWorkspace || '') : (defaultWorkspace || '')
  const messages = sessionId ? (messagesBySession.get(sessionId) ?? []) : []
  const reviewData = useMemo(() => extractReviewData(messages), [messages])
  const permissionMode = useStore(s => s.permissionMode)

  const fallback: GitChanges = { files: [], diff: '', repoRoot: '', unpushedCommits: 0, ahead: 0, behind: 0, unpushedLog: [] }

  useEffect(() => { setGitData(null); setLoadingGit(false); setExpandedFiles(new Set()); setCommitted(false) }, [workspaceRoot])

  const perFileDiff = useMemo(() => gitData ? splitDiffByFile(gitData.diff) : {}, [gitData])

  useEffect(() => {
    if (!reviewPanelOpen || !workspaceRoot) return
    setGitData(null); setLoadingGit(true)
    ;(window as any).loom?.getUncommittedChanges?.(workspaceRoot).then((d: GitChanges) => { setGitData(d || fallback); setLoadingGit(false) }).catch(() => { setGitData(fallback); setLoadingGit(false) })
  }, [reviewPanelOpen, workspaceRoot])

  const refreshGit = useCallback(async () => {
    try {
      const d = await (window as any).loom?.getUncommittedChanges?.(workspaceRoot)
      setGitData(d || fallback)
    } catch {
      // refresh 失败时静默降级、保留现有数据：否则 commit/push 成功后 await
      // refreshGit() 抛错，会被外层 catch 误报成"提交/推送失败"（B6）
    }
  }, [workspaceRoot])

  const toggleFile = useCallback((f: string) => setExpandedFiles(p => { const n = new Set(p); if (n.has(f)) n.delete(f); else n.add(f); return n }), [])
  const toggleFinding = useCallback((i: number) => setExpandedFindings(p => { const n = new Set(p); if (n.has(i)) n.delete(i); else n.add(i); return n }), [])
  const openFile = useCallback((relPath: string) => {
    const root = gitData?.repoRoot
    window.loom?.openFile?.(root && !root.endsWith('/') && !root.endsWith('\\') ? root + '/' + relPath : root ? root + relPath : relPath)
  }, [gitData?.repoRoot])

  const handleCommit = useCallback(async () => {
    if (!commitMsg.trim() || committing) return
    setCommitting(true)
    try {
      const res = await (window as any).loom?.gitCommit?.(workspaceRoot, commitMsg.trim())
      if (res?.ok) { setCommitMsg(''); setCommitted(true); await refreshGit() }
      else useStore.getState().addToast({ type: 'error', message: res?.message || t('review.commitFailed') })
    } catch { useStore.getState().addToast({ type: 'error', message: t('review.commitFailed') }) }
    finally { setCommitting(false) }
  }, [commitMsg, committing, workspaceRoot, t, refreshGit])

  const handlePush = useCallback(async () => {
    if (pushing) return
    setPushing(true)
    try {
      const res = await (window as any).loom?.gitPush?.(workspaceRoot)
      if (res?.ok) { useStore.getState().addToast({ type: 'success', message: t('review.pushOk') }); setCommitted(false); await refreshGit() }
      else useStore.getState().addToast({ type: 'error', message: res?.message || t('review.pushFailed') })
    } catch { useStore.getState().addToast({ type: 'error', message: t('review.pushFailed') }) }
    finally { setPushing(false) }
  }, [pushing, workspaceRoot, t, refreshGit])

  const handleAiGenCommit = useCallback(async () => {
    if (generatingCommit || !gitData?.diff) return
    setGeneratingCommit(true)
    // Show running state on dynamic island (duration=0 keeps it visible until cleared)
    useStore.getState().showIslandTransient(t('review.aiGenRunning'), 300_000)
    try {
      const diffSnippet = gitData.diff.slice(0, 4000)
      const r: any = await loomRpc('completion.chat', { messages: [{ role: 'user', content: `Generate a concise git commit message (under 72 chars). Output ONLY the message:\n\n${diffSnippet}` }], max_tokens: 2048, temperature: 0.0 })
      if (r?.ok && r?.content) {
        setCommitMsg(r.content.trim())
        useStore.getState().showIslandTransient(t('review.aiGenOk'), 2500)
      } else {
        useStore.getState().addToast({ type: 'warning', message: t('review.aiGenFailed') + ': ' + (r?.message || 'no content') })
        useStore.getState().clearIslandTransient()
      }
    } catch (e: any) {
      useStore.getState().addToast({ type: 'warning', message: t('review.aiGenFailed') + ': ' + (e?.message || '') })
      useStore.getState().clearIslandTransient()
    } finally { setGeneratingCommit(false) }
  }, [generatingCommit, gitData, t])

  const handleStartReview = useCallback(async () => {
    if (!sessionId || reviewing) return
    setReviewing(true)
    let prompt = t('review.promptDefault')
    if (gitData?.files.length) {
      prompt = t('review.promptWithDiff', { files: gitData.files.map(f => f.file).join('\n'), diff: gitData.diff.slice(0, 8000) })
    }
    try {
      await sendMessage({ sessionId, content: prompt, permissionMode })
    } catch {
      // 审查发起失败时给出反馈，而非留下未处理的 rejection（B7）
      useStore.getState().addToast({ type: 'error', message: t('review.startFailed') })
    } finally { setReviewing(false) }
  }, [sessionId, reviewing, gitData, t, permissionMode])

  if (!reviewPanelOpen) return null

  const hasChanges = (gitData?.files.length ?? 0) > 0
  const unpushedCount = gitData?.unpushedCommits ?? 0

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <span className={styles.title}>{t('review.title')}</span>
        {gitData && (gitData.ahead > 0 || gitData.behind > 0) && (
          <span className={styles.syncBadge}>
            {gitData.ahead > 0 && <span className={styles.syncUp}>↑{gitData.ahead}</span>}
            {gitData.behind > 0 && <span className={styles.syncDown}>↓{gitData.behind}</span>}
          </span>
        )}
        <span className={styles.headerSpacer} />
        <button className={styles.closeBtn} onClick={toggleReviewPanel} title={t('review.close')}><IconX size={15} /></button>
      </div>

      <div className={styles.body}>
        {loadingGit && <div className={styles.emptyHint}>{t('review.loading')}</div>}
        {/* git 加载失败（非仓库 / git 不可用等）时独立展示错误，而非误报"无改动"（B3） */}
        {!loadingGit && gitData?.error && <div className={styles.gitErrorHint}>{gitData.error}</div>}
        {!loadingGit && !gitData?.error && !hasChanges && !committed && <div className={styles.emptyHint}>{t('review.noChanges')}</div>}

        {/* ---- Changed files ---- */}
        {hasChanges && (
          <div className={styles.block}>
            <div className={styles.blockLabel}>
              {t('review.uncommitted')}
              <span className={styles.blockCount}>
                <span className={styles.statAdd}>+{totalAdds}</span>
                <span className={styles.statDel}>-{totalDels}</span>
              </span>
            </div>
            <div className={styles.fileList}>
              {gitData!.files.map(f => (
                <div key={f.file}>
                  <div className={styles.fileRow} onClick={() => toggleFile(f.file)}>
                    <span className={styles.fileName} title={f.file}>{f.file.replace(/\\/g, '/').split('/').pop()}</span>
                    <span className={styles.filePath}>{f.file}</span>
                    <span className={styles.fileStats}>
                      {f.adds > 0 && <span className={styles.statAdd}>+{f.adds}</span>}
                      {f.dels > 0 && <span className={styles.statDel}>-{f.dels}</span>}
                    </span>
                    <IconChevronDown size={10} className={`${styles.chevron} ${expandedFiles.has(f.file) ? styles.chevronOpen : ''}`} />
                  </div>
                  {expandedFiles.has(f.file) && perFileDiff[f.file] && (
                    <pre className={styles.diffPreview} dangerouslySetInnerHTML={{ __html: colorizeDiff(perFileDiff[f.file].slice(0, 2000)) }} />
                  )}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* ---- Review button ---- */}
        {hasChanges && (
          <button className={styles.reviewBtn} onClick={handleStartReview} disabled={reviewing}>
            {reviewing ? t('review.reviewing') : t('review.review')}
          </button>
        )}

        {/* ---- Commit ---- */}
        {hasChanges && (
          <div className={styles.block}>
            <div className={styles.blockLabel}>{t('review.commit')}</div>
            <div className={styles.commitRow}>
              <input className={styles.cmtInput} value={commitMsg} onChange={e => setCommitMsg(e.target.value)} placeholder={t('review.commitPlaceholder')}
                onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleCommit() } }} />
              <button className={styles.cmtAiBtn} onClick={handleAiGenCommit} disabled={generatingCommit}>{generatingCommit ? '...' : t('review.aiGenCommit')}</button>
            </div>
            <button className={styles.cmtBtn} onClick={handleCommit} disabled={committing || !commitMsg.trim()}>
              {committing ? '...' : t('review.commit')}
            </button>
          </div>
        )}

        {/* ---- Push (after commit or existing unpushed) ---- */}
        {(committed || unpushedCount > 0) && (
          <div className={styles.block}>
            <div className={styles.blockLabel}>
              {t('review.unpushedCommits', { count: unpushedCount })}
              <button className={styles.pushBtn} onClick={handlePush} disabled={pushing}>
                {pushing ? '...' : t('review.push')}
              </button>
            </div>
            {unpushedCount > 0 && (
              <div className={styles.unpushedList}>
                {gitData!.unpushedLog.slice(0, 20).map(c => (
                  <div key={c.hash} className={styles.unpushedItem}>
                    <span className={styles.unpushedHash}>{c.hash.slice(0, 7)}</span>
                    <span className={styles.unpushedSubject}>{c.subject}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* ---- Findings ---- */}
        {reviewData && (
          <div className={styles.block}>
            <div className={styles.blockLabel}>
              {t('review.findings')}
              <span className={styles.findingCounts}>
                {reviewData.critical > 0 && <span className={styles.fcCritical}>{reviewData.critical}</span>}
                {reviewData.important > 0 && <span className={styles.fcImportant}>{reviewData.important}</span>}
                {reviewData.minor > 0 && <span className={styles.fcMinor}>{reviewData.minor}</span>}
              </span>
            </div>
            {reviewData.findings.map((f, i) => (
              <div key={i} className={styles.findingCard}>
                <div className={styles.findingHeader} onClick={() => toggleFinding(i)}>
                  <span className={styles.findingIcon} style={{ color: f.severity === 'critical' ? 'var(--red)' : f.severity === 'important' ? 'var(--amber)' : 'var(--text-muted)' }}>
                    {SEV_ICON[f.severity]?.(13) || SEV_ICON.minor(13)}
                  </span>
                  <span className={styles.findingFile} onClick={e => { e.stopPropagation(); openFile(f.file) }}>{f.file}{f.line ? ':' + f.line : ''}</span>
                  <IconChevronDown size={10} className={`${styles.chevron} ${expandedFindings.has(i) ? styles.chevronOpen : ''}`} />
                </div>
                <div className={styles.findingSummary}>{f.summary}</div>
                {expandedFindings.has(i) && f.suggestion && (
                  <div className={styles.findingSuggestion}><span className={styles.findingSuggestionLabel}>{t('review.fix')}:</span> {f.suggestion}</div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

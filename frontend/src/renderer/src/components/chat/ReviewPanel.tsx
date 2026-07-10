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
interface GitChanges { files: GitFile[]; diff: string; repoRoot: string; error?: string }

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
const SEV_COLOR: Record<string, string> = { critical: 'var(--red)', important: 'var(--amber)', minor: 'var(--text-muted)' }

// Build colorized diff HTML (rendered via dangerouslySetInnerHTML in <pre>).
// All user content is html-escaped before color spans are applied — safe XSS-wise.
function colorizeDiff(diff: string): string {
  const lines = diff.split('\n')
  let lineNum = 0

  return lines.map((line) => {
    let cls = ''
    const escaped = line.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')

    if (line.startsWith('@@')) {
      const m = line.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/)
      if (m) lineNum = parseInt(m[2]) - 1
      return `<span class="diff-hunk">${escaped}</span>`
    }
    if (line.startsWith('diff --git') || line.startsWith('--- ') || line.startsWith('+++ ')) {
      return `<span class="diff-header">${escaped}</span>`
    }
    if (line.startsWith('index ')) {
      return `<span class="diff-meta">${escaped}</span>`
    }
    if (line.startsWith('-')) {
      cls = 'diff-del'
    } else if (line.startsWith('+')) {
      lineNum++
      cls = 'diff-add'
    } else {
      lineNum++
      cls = 'diff-context'
    }
    const num = line.startsWith('-') ? '' : String(lineNum)
    return `<span class="${cls}"><span class="diff-ln">${num}</span>${escaped}</span>`
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

  const totalAdds = useMemo(() => gitData?.files.reduce((s, f) => s + f.adds, 0) ?? 0, [gitData])
  const totalDels = useMemo(() => gitData?.files.reduce((s, f) => s + f.dels, 0) ?? 0, [gitData])
  const sessionId = useStore(s => s.currentSessionId)
  const messagesBySession = useStore(s => s.messagesBySession)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)
  const defaultWorkspace = useStore(s => s.defaultWorkspace)
  const workspaceRoot = sessionId ? (sessionWorkspaces[sessionId] || defaultWorkspace || '') : (defaultWorkspace || '')
  const messages = sessionId ? (messagesBySession.get(sessionId) ?? []) : []
  const reviewData = useMemo(() => extractReviewData(messages), [messages])

  // Clear stale git data when workspace changes
  useEffect(() => { setGitData(null); setLoadingGit(false); setExpandedFiles(new Set()) }, [workspaceRoot])

  const perFileDiff = useMemo(() => gitData ? splitDiffByFile(gitData.diff) : {}, [gitData])

  // Fetch git data when panel opens or workspace changes
  useEffect(() => {
    if (!reviewPanelOpen) return
    if (!workspaceRoot) return
    setGitData(null)
    setLoadingGit(true)
    ;(window as any).loom?.getUncommittedChanges?.(workspaceRoot).then((d: GitChanges) => {
      setGitData(d || { files: [], diff: '', repoRoot: '' })
      setLoadingGit(false)
    }).catch(() => { setGitData({ files: [], diff: '', repoRoot: '' }); setLoadingGit(false) })
  }, [reviewPanelOpen, workspaceRoot])

  const toggleFile = useCallback((f: string) => { setExpandedFiles(prev => { const n = new Set(prev); if (n.has(f)) n.delete(f); else n.add(f); return n }) }, [])
  const toggleFinding = useCallback((idx: number) => { setExpandedFindings(prev => { const n = new Set(prev); if (n.has(idx)) n.delete(idx); else n.add(idx); return n }) }, [])
  const openFile = useCallback((relPath: string, _line?: number) => {
    const root = gitData?.repoRoot
    if (!root) { window.loom?.openFile?.(relPath); return }
    window.loom?.openFile?.(root.endsWith('/') || root.endsWith('\\') ? root + relPath : root + '/' + relPath)
  }, [gitData?.repoRoot])
  const handleCommit = useCallback(async () => {
    if (!commitMsg.trim() || committing) return
    setCommitting(true)
    try {
      const res = await (window as any).loom?.gitCommit?.(workspaceRoot, commitMsg.trim())
      if (res?.ok) {
        setCommitMsg('')
        // Refresh git data
        const d = await (window as any).loom?.getUncommittedChanges?.(workspaceRoot)
        setGitData(d || { files: [], diff: '', repoRoot: '' })
      } else {
        useStore.getState().addToast({ type: 'error', message: res?.message || t('review.commitFailed') })
      }
    } catch { useStore.getState().addToast({ type: 'error', message: t('review.commitFailed') }) }
    finally { setCommitting(false) }
  }, [commitMsg, committing, workspaceRoot, t])

  const handlePush = useCallback(async () => {
    if (pushing) return
    setPushing(true)
    try {
      const res = await (window as any).loom?.gitPush?.(workspaceRoot)
      if (res?.ok) {
        useStore.getState().addToast({ type: 'success', message: t('review.pushOk') })
      } else {
        useStore.getState().addToast({ type: 'error', message: res?.message || t('review.pushFailed') })
      }
    } catch { useStore.getState().addToast({ type: 'error', message: t('review.pushFailed') }) }
    finally { setPushing(false) }
  }, [pushing, workspaceRoot, t])

  const handleAiGenCommit = useCallback(async () => {
    if (generatingCommit || !gitData?.diff) return
    setGeneratingCommit(true)
    try {
      const diffSnippet = gitData.diff.length > 4000 ? gitData.diff.slice(0, 4000) : gitData.diff
      const prompt = `Generate a concise git commit message (under 72 chars) based on the diff. Output ONLY the commit message, nothing else:\n\n${diffSnippet}`

      const result: any = await loomRpc('completion.chat', {
        messages: [{ role: 'user', content: prompt }],
        max_tokens: 512,
        temperature: 0.0,
      })
      console.log('[aiGenCommit] result:', result)
      if (result?.ok && result?.content) {
        setCommitMsg(result.content.trim())
      } else {
        const reason = result?.message || 'RPC returned no content'
        useStore.getState().addToast({ type: 'warning', message: t('review.aiGenFailed') + ': ' + reason })
        console.error('[aiGenCommit] failed:', result)
      }
    } catch (e: any) {
      const reason = e?.message || String(e)
      useStore.getState().addToast({ type: 'warning', message: t('review.aiGenFailed') + ': ' + reason })
      console.error('[aiGenCommit]', e)
    } finally { setGeneratingCommit(false) }
  }, [generatingCommit, gitData])

  const hasChanges = gitData && gitData.files.length > 0

  const permissionMode = useStore(s => s.permissionMode)

  const handleStartReview = useCallback(async () => {
    if (!sessionId || reviewing) return
    setReviewing(true)
    let prompt = t('review.promptDefault')
    if (gitData && gitData.files.length > 0) {
      const fileList = gitData.files.map(f => f.file).join('\n')
      const diffSnippet = gitData.diff.slice(0, 8000)
      prompt = t('review.promptWithDiff', { files: fileList, diff: diffSnippet })
    }
    try {
      await sendMessage({ sessionId, content: prompt, permissionMode })
    } finally { setReviewing(false) }
  }, [sessionId, reviewing, gitData, t, permissionMode])

  if (!reviewPanelOpen) return null

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <span className={styles.title}>{t('review.title')}</span>
        {reviewData && (<>
          <div className={styles.badges}>
            {reviewData.critical > 0 && <span className={styles.badge} style={{ background: 'var(--red)', color: '#fff' }}>{reviewData.critical}</span>}
            {reviewData.important > 0 && <span className={styles.badge} style={{ background: 'var(--amber)', color: '#000' }}>{reviewData.important}</span>}
            {reviewData.minor > 0 && <span className={styles.badge} style={{ background: 'var(--bg-active)', color: 'var(--text-muted)' }}>{reviewData.minor}</span>}
          </div>
          <span className={styles.total}>{t('review.totalFindings', { count: reviewData.total })}</span>
        </>)}
        <button className={styles.closeBtn} onClick={toggleReviewPanel} title={t('review.close')}><IconX size={16} /></button>
      </div>
      <div className={styles.body}>
        <div className={styles.sectionTitle}>{t('review.uncommitted')}</div>
        {loadingGit && <div className={styles.emptyHint}>{t('review.loading')}</div>}
        {!loadingGit && !hasChanges && <div className={styles.emptyHint}>{t('review.noChanges')}</div>}
        {hasChanges && (<>
          <button className={styles.reviewBtn} onClick={handleStartReview} disabled={reviewing}>
            {reviewing ? t('review.reviewing') : t('review.review')}
          </button>
          {gitData.files.map((f: GitFile) => (<div key={f.file}>
            <div className={styles.fileRow} onClick={() => toggleFile(f.file)}>
              <span className={styles.fileName} title={f.file}>{f.file.replace(/\\/g, '/').split('/').pop()}</span>
              <span className={styles.fileSpacer} />
              <span className={styles.fileStats}>
                {f.adds > 0 && <span className={styles.statAdd}>+{f.adds}</span>}
                {f.dels > 0 && <span className={styles.statDel}>-{f.dels}</span>}
              </span>
              <span className={styles.expandHint}>{expandedFiles.has(f.file) ? <IconChevronDown size={12} /> : '+'}</span>
            </div>
            {expandedFiles.has(f.file) && perFileDiff[f.file] && (
              <pre className={styles.diffPreview} dangerouslySetInnerHTML={{ __html: colorizeDiff(perFileDiff[f.file].slice(0, 2000)) }} />
            )}
          </div>))}
          <div className={styles.actionBar}>
            <div className={styles.statsSummary}>
              <span className={styles.statAdd}>+{totalAdds}</span>
              <span className={styles.statDel}>-{totalDels}</span>
            </div>
            <input
              className={styles.commitInput}
              value={commitMsg}
              onChange={e => setCommitMsg(e.target.value)}
              placeholder={t('review.commitPlaceholder')}
              onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleCommit() } }}
            />
            <div className={styles.commitRow}>
              <button className={styles.commitBtn} onClick={handleAiGenCommit} disabled={generatingCommit}>
                {generatingCommit ? '...' : t('review.aiGenCommit')}
              </button>
              <button className={styles.commitBtn} onClick={handleCommit} disabled={committing || !commitMsg.trim()}>
                {committing ? '...' : t('review.commit')}
              </button>
              <button className={styles.pushBtn} onClick={handlePush} disabled={pushing}>
                {pushing ? '...' : t('review.push')}
              </button>
            </div>
          </div>
        </>)}
        {reviewData && (<>
          <div className={styles.sectionTitle}>{t('review.findings')}</div>
          {reviewData.findings.map((f: Finding, idx: number) => (
            <div key={idx} className={styles.findingCard}>
              <div className={styles.findingHeader} onClick={() => toggleFinding(idx)}>
                <span className={styles.findingIcon} style={{ color: SEV_COLOR[f.severity] || 'var(--text-muted)' }}>{SEV_ICON[f.severity]?.(16) || SEV_ICON.minor(16)}</span>
                <span className={styles.findingSeverity} style={{ color: SEV_COLOR[f.severity] || 'var(--text-muted)' }}>{f.severity}</span>
                <span className={styles.findingFile} onClick={(e) => { e.stopPropagation(); openFile(f.file, f.line) }}>{f.file}{f.line ? ':' + f.line : ''}</span>
                {expandedFindings.has(idx) ? <IconChevronDown size={12} /> : <span className={styles.expandHint}>+</span>}
              </div>
              <div className={styles.findingSummary}>{f.summary}</div>
              {expandedFindings.has(idx) && f.suggestion && (
                <div className={styles.findingSuggestion}><strong>{t('review.fix')}:</strong> {f.suggestion}</div>
              )}
            </div>
          ))}
        </>)}
        {!reviewData && hasChanges && !reviewing && (
          <div className={styles.emptyHint}>{t('review.noFindings')}</div>
        )}
      </div>
    </div>
  )
}

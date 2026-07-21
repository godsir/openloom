import { useState, useEffect, useCallback } from 'react'
import { loomRpc, loomSubscribe } from '../../services/jsonrpc'
import { useStore } from '../../stores'
import {
  IconPlus, IconPlay, IconPause, IconTrash, IconRefresh,
  IconClock, IconCalendarClock, IconChevronDown, IconEdit,
  IconSearch, IconCopy, IconChevronRight, IconX,
} from '../../utils/icons'
import { useLocale, t as _t } from '../../i18n'
import type { CronJobSummary, CronRunHistory } from '../../stores/cron'
import Select from '../shared/Select'
import sharedStyles from '../shared/SettingsModal.module.css'
import styles from './CronTab.module.css'

// ── Presets ──────────────────────────────────────────────────────────────────
const PRESETS = [
  { label: 'cron.presetEveryMinute', expr: '0 * * * * * *' },
  { label: 'cron.presetEvery5Min', expr: '0 */5 * * * * *' },
  { label: 'cron.presetEvery30Min', expr: '0 */30 * * * * *' },
  { label: 'cron.presetEveryHour', expr: '0 0 * * * * *' },
  { label: 'cron.presetDaily9am', expr: '0 0 9 * * * *' },
  { label: 'cron.presetDaily6pm', expr: '0 0 18 * * * *' },
  { label: 'cron.presetMon9am', expr: '0 0 9 * * 1 *' },
  { label: 'cron.preset1stMidnight', expr: '0 0 0 1 * * *' },
]

// ── Helpers ──────────────────────────────────────────────────────────────────

function formatTime(ts: number | null, locale: string): string {
  if (!ts) return '--'
  const d = new Date(ts * 1000)
  return d.toLocaleString(locale, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

function formatFull(ts: number, locale: string): string {
  const d = new Date(ts * 1000)
  return d.toLocaleString(locale, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function relativeTime(ts: number | null): string {
  if (!ts) return ''
  const now = Math.floor(Date.now() / 1000)
  const diff = ts - now
  if (diff < 0) return ''
  if (diff < 60) return _t('cron.inSeconds', { n: diff })
  if (diff < 3600) return _t('cron.inMinutes', { n: Math.floor(diff / 60) })
  if (diff < 86400) return _t('cron.inHours', { n: Math.floor(diff / 3600) })
  return _t('cron.inDays', { n: Math.floor(diff / 86400) })
}

function describeCron(expr: string): string {
  const p = expr.trim().split(/\s+/)
  if (p.length !== 7) return expr
  if (p[0] === '0' && p[1] === '*' && p.slice(2).every(f => f === '*')) return _t('cron.everyMinute')
  if (p[0] === '0' && p[1].startsWith('*/') && p.slice(2).every(f => f === '*')) return _t('cron.everyNMinutes', { n: p[1].slice(2) })
  if (p[0] === '0' && p[1] === '0' && p.slice(2).every(f => f === '*')) return _t('cron.hourly')
  if (p[0] === '0' && p[1] === '0' && /^\d+$/.test(p[2]) && p[3] === '*' && p.slice(4).every(f => f === '*')) return _t('sidebar.everyDayAt', { hour: p[2] })
  if (p[0] === '0' && /^\d+$/.test(p[1]) && /^\d+$/.test(p[2]) && p[3] === '*' && p[4] === '*' && p.slice(5).every(f => f === '*')) return _t('cron.everyDayAtTime', { hour: p[2], minute: p[1].padStart(2, '0') })
  if (p[0] === '0' && p[5] !== '*' && /^\d+$/.test(p[5])) {
    const dowKeys = ['sidebar.dowSun', 'sidebar.dowMon', 'sidebar.dowTue', 'sidebar.dowWed', 'sidebar.dowThu', 'sidebar.dowFri', 'sidebar.dowSat']
    const dow = _t(dowKeys[parseInt(p[5])])
    return _t('sidebar.everyWeekAt', { dow, hour: p[2], minute: p[1].padStart(2, '0') })
  }
  return expr
}

type JobFilter = 'all' | 'enabled' | 'running' | 'paused' | 'failed'
const FILTERS: JobFilter[] = ['all', 'enabled', 'running', 'paused', 'failed']

function filterJobs(jobs: CronJobSummary[], f: JobFilter): CronJobSummary[] {
  if (f === 'enabled') return jobs.filter(j => j.enabled)
  if (f === 'running') return jobs.filter(j => j.last_status === 'running')
  if (f === 'paused') return jobs.filter(j => !j.enabled)
  if (f === 'failed') return jobs.filter(j => j.error_count > 0)
  return jobs
}

// ── Component ────────────────────────────────────────────────────────────────

export default function CronTab() {
  const { t } = useLocale()
  const locale = (() => { try { return localStorage.getItem('loom-locale') || 'zh-CN' } catch { return 'zh-CN' } })()
  const models = useStore((s) => s.models)
  const currentModel = useStore((s) => s.currentModel)

  const [jobs, setJobs] = useState<CronJobSummary[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [filter, setFilter] = useState<JobFilter>('all')
  const [search, setSearch] = useState('')

  // Dialog state
  const [dialog, setDialog] = useState<{
    open: boolean; editJob?: CronJobSummary
    name: string; cron_expression: string; prompt: string; timeout_secs: number; model: string
  }>({ open: false, name: '', cron_expression: '', prompt: '', timeout_secs: 300, model: '' })
  const [dialogErr, setDialogErr] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  // Inline result state
  const [expandedJobs, setExpandedJobs] = useState<Set<string>>(new Set())
  const [jobResults, setJobResults] = useState<Map<string, CronRunHistory>>(new Map())

  // ── Fetch ──────────────────────────────────────────────────────────────
  const loadJobs = useCallback(async () => {
    try {
      const res = await loomRpc<CronJobSummary[]>('cron.list')
      setJobs(Array.isArray(res) ? res : [])
      setError(null)
    } catch (e: any) { setError(e.message) }
    finally { setLoading(false) }
  }, [])

  useEffect(() => { loadJobs() }, [loadJobs])

  // ── WebSocket live refresh ────────────────────────────────────────────
  useEffect(() => {
    const unsub = loomSubscribe((method) => {
      if (method.startsWith('cron.')) loadJobs()
    })
    return unsub
  }, [loadJobs])

  // ── Dialog ──────────────────────────────────────────────────────────────
  const openCreate = () => setDialog({ open: true, name: '', cron_expression: '', prompt: '', timeout_secs: 300, model: currentModel || '' })
  const openEdit = (job: CronJobSummary) => setDialog({
    open: true, editJob: job,
    name: job.name, cron_expression: job.cron_expression, prompt: job.prompt,
    timeout_secs: 300, model: job.model || currentModel || '',
  })
  const closeDialog = () => { setDialog(d => ({ ...d, open: false })); setDialogErr(null) }

  // ESC to close dialog
  useEffect(() => {
    if (!dialog.open) return
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') closeDialog() }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [dialog.open])

  const saveDialog = async () => {
    const name = dialog.name.trim()
    const expr = dialog.cron_expression.trim()
    const prompt = dialog.prompt.trim()
    if (!name) { setDialogErr(t('cron.nameRequired')); return }
    if (!expr) { setDialogErr(t('cron.cronRequired')); return }
    if (!prompt) { setDialogErr(t('cron.promptRequired')); return }
    setSaving(true)
    try {
      // model 一并提交，避免用户选择的模型被丢弃（A3）。
      // 注：后端当前尚未按任务持久化/应用 model，此为前端侧不丢参，后端支持后自动生效。
      const model = (dialog.model || currentModel || '').trim()
      const params: Record<string, unknown> = { name, cron_expression: expr, prompt, session_mode: 'isolated', timeout_secs: Math.max(1, Math.min(3600, dialog.timeout_secs)) }
      if (model) params.model = model
      if (dialog.editJob) {
        await loomRpc('cron.update', { id: dialog.editJob.id, ...params })
      } else {
        await loomRpc('cron.create', params)
      }
      closeDialog()
      await loadJobs()
    } catch (e: any) { setDialogErr(e.message) }
    finally { setSaving(false) }
  }

  // ── Actions ────────────────────────────────────────────────────────────
  const doAction = async (method: string, id: string) => {
    if (method === 'cron.delete') {
      const ok = await useStore.getState().showConfirm(t('cron.confirmDeleteTitle'), t('cron.confirmDeleteMessage'), true)
      if (!ok) return
    }
    setBusy(id)
    try {
      await loomRpc(method, { id })
      if (method === 'cron.run_now') await new Promise(r => setTimeout(r, 500))
      await loadJobs()
    } catch (e: any) { setError(e.message) }
    finally { setBusy(null) }
  }

  // ── Inline result ──────────────────────────────────────────────────────
  const loadLastResult = async (jobId: string) => {
    try {
      const res = await loomRpc<CronRunHistory[]>('cron.history', { id: jobId, limit: 1 })
      if (Array.isArray(res) && res.length > 0) {
        setJobResults(prev => { const m = new Map(prev); m.set(jobId, res[0]); return m })
      }
    } catch { /* ignore */ }
  }

  const toggleExpand = (jobId: string) => {
    setExpandedJobs(prev => {
      const next = new Set(prev)
      if (next.has(jobId)) next.delete(jobId)
      else { next.add(jobId); loadLastResult(jobId) }
      return next
    })
  }

  // ── Filter + Search ────────────────────────────────────────────────────
  let visible = filterJobs(jobs, filter)
  if (search) {
    const q = search.toLowerCase()
    visible = visible.filter(j => j.name.toLowerCase().includes(q) || j.prompt.toLowerCase().includes(q))
  }

  // ── Render ─────────────────────────────────────────────────────────────
  return (
    <>
      {/* === Header === */}
      <div className={sharedStyles.contentHeader}>
        <div className={sharedStyles.sectionHeaderRow}>
          <h3 className={sharedStyles.sectionTitle}>
            {t('cron.title')}
            <span className={styles.countBadge}>{jobs.length}</span>
          </h3>
          <div className={styles.headerRight}>
            <button className={sharedStyles.refreshBtn} onClick={loadJobs} disabled={loading} title={t('common.refresh')}><IconRefresh size={14} /></button>
            <button className={styles.btnAdd} onClick={openCreate}><IconPlus size={13} />{t('common.create')}</button>
          </div>
        </div>
        <p className={sharedStyles.sectionDesc}>{t('cron.description')}</p>

        {/* Filter row */}
        <div className={styles.filterRow}>
          <div className={styles.filterPills}>
            {FILTERS.map(f => (
              <button
                key={f}
                className={`${styles.filterPill} ${filter === f ? styles.filterPillActive : ''}`}
                onClick={() => setFilter(f)}
              >{t(`cron.filter_${f}`)}</button>
            ))}
          </div>
          {jobs.length > 0 && (
            <div className={styles.searchRow}>
              <IconSearch size={12} className={styles.searchIcon} />
              <input className={styles.searchInput} value={search} onChange={e => setSearch(e.target.value)} placeholder={t('cron.searchPlaceholder')} />
            </div>
          )}
        </div>
      </div>

      {/* === Body === */}
      <div className={sharedStyles.contentBody}>
        {error && <p className={styles.errorText}>{error}</p>}
        {loading ? (
          <p className={sharedStyles.toolsEmpty}>{t('common.loading')}</p>
        ) : visible.length === 0 ? (
          <div className={styles.empty}>
            <div className={styles.emptyIcon}><IconCalendarClock size={32} /></div>
            <p>{search || filter !== 'all' ? t('cron.noResults') : t('cron.empty')}</p>
          </div>
        ) : (
          <div className={styles.cardList}>
            {visible.map(job => {
              const b = busy === job.id
              const isRunning = job.last_status === 'running'
              const nextRel = relativeTime(job.next_run)
              const result = jobResults.get(job.id)
              const isExpanded = expandedJobs.has(job.id)
              const hasResult = result && (result.response || result.error_message)
              const resultText = result
                ? (result.status === 'completed' ? result.response : result.error_message) || ''
                : ''
              const resultLong = resultText.length > 200 || (resultText.match(/\n/g) || []).length > 4

              return (
                <div key={job.id} className={styles.card}>
                  {/* Card top: info + actions */}
                  <div className={styles.cardTop}>
                    <div className={styles.cardInfo}>
                      <div className={styles.cardNameRow}>
                        <h2 className={styles.cardName}>{job.name}</h2>
                        <span className={`${styles.badge} ${isRunning ? styles.badgeRunning : job.enabled ? styles.badgeEnabled : styles.badgeDisabled}`}>
                          {isRunning ? t('cron.running') : job.enabled ? t('cron.filter_enabled') : t('cron.paused')}
                        </span>
                      </div>
                      <div className={styles.cardMeta}>
                        <span className={styles.cardSchedule}>
                          <IconCalendarClock size={11} className={styles.inlineIcon} />
                          {describeCron(job.cron_expression)}
                          <span className={styles.cardCronRaw}>{job.cron_expression}</span>
                        </span>
                      </div>
                      <div className={styles.cardMeta2}>
                        <span><IconClock size={11} className={styles.inlineIcon} />{t('cron.lastRun')} {formatTime(job.last_run, locale)}</span>
                        {job.next_run && nextRel && <span className={styles.nextRun}>{t('cron.nextRun')} {nextRel}</span>}
                        <span>{t('cron.executions', { n: job.run_count })}</span>
                        {job.error_count > 0 && <span className={styles.errorCount}>{t('cron.failures', { n: job.error_count })}</span>}
                      </div>
                    </div>
                    {/* Actions */}
                    <div className={styles.cardActions}>
                      <button className={styles.iconBtn} title={t('cron.runNow')} disabled={b || isRunning} onClick={() => doAction('cron.run_now', job.id)}><IconPlay size={14} /></button>
                      <button className={styles.iconBtn} title={t('common.edit')} disabled={b} onClick={() => openEdit(job)}><IconEdit size={14} /></button>
                      <button className={styles.iconBtn} title={t('common.delete')} disabled={b} onClick={() => doAction('cron.delete', job.id)}><IconTrash size={14} /></button>
                      {/* Enable/disable toggle */}
                      <label className={styles.toggle} title={job.enabled ? t('cron.pause') : t('cron.resume')}>
                        <input type="checkbox" checked={job.enabled} disabled={b} onChange={() => doAction(job.enabled ? 'cron.pause' : 'cron.resume', job.id)} className="sr-only" />
                        <span className={`${styles.toggleTrack} ${job.enabled ? styles.toggleOn : styles.toggleOff}`}>
                          <span className={styles.toggleThumb} />
                        </span>
                      </label>
                    </div>
                  </div>

                  {/* Inline result preview */}
                  {hasResult ? (
                    <div className={styles.resultBox}>
                      <div className={styles.resultHeader} onClick={() => toggleExpand(job.id)}>
                        <span className={styles.resultLabel}>
                          {result.status === 'failed' || result.status === 'timed_out' ? t('cron.lastError') : t('cron.lastResult')}
                        </span>
                        {resultLong && (
                          <button className={styles.resultToggle}>
                            {isExpanded ? <IconChevronDown size={12} /> : <IconChevronRight size={12} />}
                            {isExpanded ? t('cron.collapseResult') : t('cron.expandResult')}
                          </button>
                        )}
                      </div>
                      <div className={`${styles.resultContent} ${isExpanded ? styles.resultExpanded : styles.resultCollapsed}`}>
                        <span className={styles.resultText}>{resultText}</span>
                        {isExpanded && (
                          <button className={styles.resultCopyBtn} onClick={e => { e.stopPropagation(); navigator.clipboard.writeText(resultText) }} title={t('common.copy')}>
                            <IconCopy size={10} /> {t('common.copy')}
                          </button>
                        )}
                      </div>
                    </div>
                  ) : (job.run_count > 0 || job.error_count > 0) ? (
                    <div className={styles.resultBox} onClick={() => toggleExpand(job.id)}>
                      <div className={styles.resultHeader}>
                        <span className={styles.resultLabel}>{t('cron.viewLastResult')}</span>
                        <button className={styles.resultToggle}>
                          <IconChevronRight size={12} />
                          {t('cron.expandResult')}
                        </button>
                      </div>
                    </div>
                  ) : null}
                </div>
              )
            })}
          </div>
        )}
      </div>

      {/* === Dialog (Modal) === */}
      {dialog.open && (
        <div className={styles.dialogOverlay}>
          <div className={styles.dialogBackdrop} onClick={closeDialog} />
          <div className={styles.dialog}>
            <div className={styles.dialogHeader}>
              <h3 className={styles.dialogTitle}>{dialog.editJob ? t('cron.editTask') : t('cron.newTask')}</h3>
              <button className={styles.dialogClose} onClick={closeDialog}><IconX size={16} /></button>
            </div>
            <div className={styles.dialogBody}>
              {dialogErr && <p className={styles.errorText}>{dialogErr}</p>}

              <div className={styles.formRow}>
                <label className={styles.formLabel}>{t('cron.taskName')}</label>
                <input className={styles.formInput} value={dialog.name} onChange={e => setDialog(d => ({ ...d, name: e.target.value }))} placeholder={t('cron.taskNamePlaceholder')} />
              </div>

              <div className={styles.formRow}>
                <label className={styles.formLabel}>{t('cron.cronLabel')}</label>
                <div className={styles.presetRow}>
                  {PRESETS.map(p => (
                    <button key={p.label} type="button" className={`${styles.presetTag} ${dialog.cron_expression === p.expr ? styles.presetTagActive : ''}`} onClick={() => setDialog(d => ({ ...d, cron_expression: p.expr }))}>
                      {t(p.label)}
                    </button>
                  ))}
                </div>
                <input className={styles.formInput} value={dialog.cron_expression} onChange={e => setDialog(d => ({ ...d, cron_expression: e.target.value }))} placeholder="0 0 9 * * * *" />
                {dialog.cron_expression && <p className={styles.formHint}>{describeCron(dialog.cron_expression)}</p>}
              </div>

              <div className={styles.formRow}>
                <label className={styles.formLabel}>{t('cron.promptLabel')}</label>
                <textarea className={styles.formTextarea} value={dialog.prompt} onChange={e => setDialog(d => ({ ...d, prompt: e.target.value }))} placeholder={t('cron.promptPlaceholder')} rows={4} />
              </div>

              {models.length > 0 && (
                <div className={styles.formRow}>
                  <label className={styles.formLabel}>{t('cron.model')}</label>
                  <Select
                    value={dialog.model || currentModel || ''}
                    options={models.map((m: any) => ({ value: m.name, label: m.name }))}
                    onChange={(v) => setDialog(d => ({ ...d, model: v }))}
                    variant="form"
                  />
                  <p className={styles.formHint}>{t('cron.modelHint')}</p>
                </div>
              )}

              <div className={styles.formRow}>
                <label className={styles.formLabel}>{t('cron.timeout')}</label>
                <input className={`${styles.formInput} ${styles.formInputSm}`} type="number" min={1} max={3600} value={dialog.timeout_secs} onChange={e => setDialog(d => ({ ...d, timeout_secs: Math.max(1, Number(e.target.value) || 300) }))} />
                <p className={styles.formHint}>{t('cron.timeoutHint')}</p>
            </div>
            <div className={styles.dialogFooter}>
              <button className={styles.btnCancel} onClick={closeDialog}>{t('common.cancel')}</button>
              <button className={styles.btnSave} onClick={saveDialog} disabled={saving}>{saving ? t('cron.saving') : t('common.save')}</button>
            </div>
          </div>
          </div>
        </div>
      )}
    </>
  )
}

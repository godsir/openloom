import { useState, useEffect, useCallback } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { useStore } from '../../stores'
import {
  IconPlus, IconPlay, IconPause, IconTrash, IconRefresh,
  IconClock, IconHistory, IconCalendarClock, IconChevronDown, IconEdit,
} from '../../utils/icons'
import { useLocale, t as _t } from '../../i18n'
import type { CronJobSummary, CronRunHistory } from '../../stores/cron'
import sharedStyles from '../shared/SettingsModal.module.css'
import styles from './CronTab.module.css'

// ── Presets ──────────────────────────────────────────────────────────────────

function useCronPresets(): { label: string; expr: string }[] {
  const { t } = useLocale()
  return [
    { label: t('cron.presetEveryMinute'), expr: '0 * * * * * *' },
    { label: t('cron.presetEvery5Min'), expr: '0 */5 * * * * *' },
    { label: t('cron.presetEvery30Min'), expr: '0 */30 * * * * *' },
    { label: t('cron.presetEveryHour'), expr: '0 0 * * * * *' },
    { label: t('cron.presetDaily9am'), expr: '0 0 9 * * * *' },
    { label: t('cron.presetDaily6pm'), expr: '0 0 18 * * * *' },
    { label: t('cron.presetMon9am'), expr: '0 0 9 * * 1 *' },
    { label: t('cron.preset1stMidnight'), expr: '0 0 0 1 * * *' },
  ]
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function getLocale(): string {
  try { return localStorage.getItem('loom-locale') || 'zh-CN' } catch { return 'zh-CN' }
}

function formatTime(ts: number | null): string {
  if (!ts) return '--'
  const d = new Date(ts * 1000)
  return d.toLocaleString(getLocale(), { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

function formatFull(ts: number): string {
  const d = new Date(ts * 1000)
  return d.toLocaleString(getLocale(), { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function describeCron(expr: string): string {
  const p = expr.trim().split(/\s+/)
  if (p.length !== 7) return expr
  if (p[0] === '0' && p[1] === '*' && p.slice(2).every(f => f === '*')) return _t('cron.everyMinute')
  if (p[0] === '0' && p[1].startsWith('*/') && p.slice(2).every(f => f === '*')) return _t('cron.everyNMinutes', { n: p[1].slice(2) })
  if (p[0] === '0' && p[1] === '0' && p.slice(2).every(f => f === '*')) return _t('cron.hourly')
  if (p[0] === '0' && p[1] === '0' && /^\d+$/.test(p[2]) && p.slice(3).every(f => f === '*')) return _t('sidebar.everyDayAt', { hour: p[2] })
  if (p[0] === '0' && p[1] === '0' && /^\d+$/.test(p[2]) && /^\d+$/.test(p[3]) && p.slice(4).every(f => f === '*')) return _t('cron.everyDayAtTime', { hour: p[2], minute: p[3].padStart(2, '0') })
  if (p[0] === '0' && p[5] !== '*' && /^\d+$/.test(p[5])) {
    const dowKeys = ['sidebar.dowSun', 'sidebar.dowMon', 'sidebar.dowTue', 'sidebar.dowWed', 'sidebar.dowThu', 'sidebar.dowFri', 'sidebar.dowSat']
    const dow = _t(dowKeys[parseInt(p[5])])
    return _t('sidebar.everyWeekAt', { dow, hour: p[2], minute: p[1].padStart(2, '0') })
  }
  return expr
}

function statusClass(status: string | null, enabled: boolean): string {
  if (status === 'running') return `${styles.badge} ${styles.badgeRunning}`
  if (!enabled) return `${styles.badge} ${styles.badgeDisabled}`
  return `${styles.badge} ${styles.badgeEnabled}`
}

function statusText(job: CronJobSummary): string {
  if (job.last_status === 'running') return _t('cron.running')
  if (!job.enabled) return _t('cron.paused')
  return _t('cron.running')
}

function runBadge(s: string): string {
  if (s === 'completed') return `${styles.badge} ${styles.badgeSuccess}`
  if (s === 'failed') return `${styles.badge} ${styles.badgeFailed}`
  if (s === 'timed_out') return `${styles.badge} ${styles.badgeTimeout}`
  return `${styles.badge} ${styles.badgeRunning}`
}

function runText(s: string): string {
  if (s === 'completed') return _t('cron.statusCompleted')
  if (s === 'failed') return _t('cron.statusFailed')
  if (s === 'timed_out') return _t('cron.statusTimeout')
  return _t('cron.running')
}

// ── Types ────────────────────────────────────────────────────────────────────

type View =
  | { kind: 'list' }
  | { kind: 'form'; editJob?: CronJobSummary }
  | { kind: 'history'; job: CronJobSummary }

interface FormData {
  name: string
  cron_expression: string
  prompt: string
  timeout_secs: number
  session_mode: 'isolated' | 'current'
}

const EMPTY_FORM: FormData = { name: '', cron_expression: '', prompt: '', timeout_secs: 300, session_mode: 'isolated' }

// ── Component ────────────────────────────────────────────────────────────────

export default function CronTab() {
  const { t } = useLocale()
  const cronEditJobId = useStore((s) => s.cronEditJobId)
  const setCronEditJobId = useStore((s) => s.setCronEditJobId)
  const [jobs, setJobs] = useState<CronJobSummary[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [view, setView] = useState<View>({ kind: 'list' })
  const [form, setForm] = useState(EMPTY_FORM)
  const [formErr, setFormErr] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [busy, setBusy] = useState<string | null>(null)

  // History
  const [history, setHistory] = useState<CronRunHistory[]>([])
  const [histLoading, setHistLoading] = useState(false)

  // ── Fetch ──────────────────────────────────────────────────────────────

  const loadJobs = useCallback(async () => {
    setLoading(true)
    try {
      const res = await loomRpc<CronJobSummary[]>('cron.list')
      setJobs(Array.isArray(res) ? res : [])
    } catch (e: any) { setError(e.message) }
    finally { setLoading(false) }
  }, [])

  useEffect(() => { loadJobs() }, [loadJobs])

  // ── Form ───────────────────────────────────────────────────────────────

  const openForm = (job?: CronJobSummary) => {
    setForm(job ? { name: job.name, cron_expression: job.cron_expression, prompt: job.prompt, timeout_secs: 300, session_mode: job.session_mode } : EMPTY_FORM)
    setFormErr(null)
    setView({ kind: 'form', editJob: job })
  }

  // Open editor when triggered from sidebar
  useEffect(() => {
    if (!cronEditJobId || jobs.length === 0) return
    const job = jobs.find(j => j.id === cronEditJobId)
    if (job) {
      openForm(job)
      setCronEditJobId(null)
    }
  }, [cronEditJobId, jobs])

  const loadHist = useCallback(async (jobId: string) => {
    setHistLoading(true)
    try {
      const res = await loomRpc<CronRunHistory[]>('cron.history', { id: jobId, limit: 50 })
      setHistory(Array.isArray(res) ? res : [])
    } catch { /* ignore */ }
    finally { setHistLoading(false) }
  }, [])

  // ── Actions ────────────────────────────────────────────────────────────

  const doAction = async (method: string, id: string) => {
    setBusy(id)
    try { await loomRpc(method, { id }) }
    catch (e: any) { setError(e.message) }
    finally { setBusy(null); await loadJobs() }
  }

  // ── Save Form ───────────────────────────────────────────────────────────

  const saveForm = async () => {
    const f = { name: form.name.trim(), expr: form.cron_expression.trim(), prompt: form.prompt.trim() }
    if (!f.name) { setFormErr(t('cron.nameRequired')); return }
    if (!f.expr) { setFormErr(t('cron.cronRequired')); return }
    if (!f.prompt) { setFormErr(t('cron.promptRequired')); return }
    setSaving(true)
    try {
      const params = { name: f.name, cron_expression: f.expr, prompt: f.prompt, session_mode: form.session_mode, timeout_secs: Math.max(1, Math.min(3600, form.timeout_secs)) }
      if (view.kind === 'form' && view.editJob) {
        await loomRpc('cron.update', { id: view.editJob.id, ...params })
      } else {
        await loomRpc('cron.create', params)
      }
      await loadJobs()
      setView({ kind: 'list' })
    } catch (e: any) { setFormErr(e.message) }
    finally { setSaving(false) }
  }

  // ── View: Form ─────────────────────────────────────────────────────────

  if (view.kind === 'form') {
    const edit = view.editJob
    return (
      <>
        <div className={sharedStyles.contentHeader}>
          <div className={sharedStyles.sectionHeaderRow}>
            <h3 className={sharedStyles.sectionTitle}>{edit ? t('cron.editTask') : t('cron.newTask')}</h3>
          </div>
        </div>
        <div className={sharedStyles.contentBody}>
          {formErr && <p className={styles.errorText}>{formErr}</p>}

          <div className={styles.formRow}>
            <label className={styles.formLabel}>{t('cron.taskName')}</label>
            <input className={styles.formInput} value={form.name} onChange={e => setForm({ ...form, name: e.target.value })} placeholder={t('cron.taskNamePlaceholder')} />
          </div>

          <div className={styles.formRow}>
            <label className={styles.formLabel}>{t('cron.cronLabel')}</label>
            <div className={styles.presetRow}>
              {useCronPresets().map(p => (
                <button key={p.label} type="button" className={`${styles.presetTag} ${form.cron_expression === p.expr ? styles.presetTagActive : ''}`} onClick={() => setForm({ ...form, cron_expression: p.expr })}>
                  {p.label}
                </button>
              ))}
            </div>
            <input className={styles.formInput} value={form.cron_expression} onChange={e => setForm({ ...form, cron_expression: e.target.value })} placeholder="0 0 9 * * * *" />
            {form.cron_expression && <p className={styles.formHint}>{describeCron(form.cron_expression)}</p>}
          </div>

          <div className={styles.formRow}>
            <label className={styles.formLabel}>{t('cron.promptLabel')}</label>
            <textarea className={styles.formTextarea} value={form.prompt} onChange={e => setForm({ ...form, prompt: e.target.value })} placeholder={t('cron.promptPlaceholder')} rows={4} />
          </div>

          <div className={styles.formRow}>
            <label className={styles.formLabel}>{t('cron.executionMode')}</label>
            <div className={styles.modeRow}>
              <button type="button" className={`${styles.modeBtn} ${form.session_mode === 'isolated' ? styles.modeBtnActive : ''}`} onClick={() => setForm({ ...form, session_mode: 'isolated' })}>{t('cron.isolatedSession')}</button>
              <button type="button" className={`${styles.modeBtn} ${form.session_mode === 'current' ? styles.modeBtnActive : ''}`} onClick={() => setForm({ ...form, session_mode: 'current' })}>{t('cron.currentSession')}</button>
            </div>
          </div>

          <div className={styles.formRow}>
            <label className={styles.formLabel}>{t('cron.timeout')}</label>
            <input className={styles.formInput} type="number" min={1} max={3600} value={form.timeout_secs} onChange={e => setForm({ ...form, timeout_secs: Math.max(1, Number(e.target.value) || 300) })} style={{ width: 100 }} />
          </div>

          <div className={styles.formActions}>
            <button className={styles.btnCancel} onClick={() => { setView({ kind: 'list' }); setFormErr(null) }}>{t('common.cancel')}</button>
            <button className={styles.btnSave} onClick={saveForm} disabled={saving}>{saving ? t('cron.saving') : t('common.save')}</button>
          </div>
        </div>
      </>
    )
  }

  // ── View: History ──────────────────────────────────────────────────────

  if (view.kind === 'history') {
    const job = view.job
    return (
      <>
        <div className={sharedStyles.contentHeader}>
          <div className={sharedStyles.sectionHeaderRow}>
            <h3 className={sharedStyles.sectionTitle}>{t('cron.executionHistory')}{job.name}</h3>
            <button className={sharedStyles.refreshBtn} onClick={() => loadHist(job.id)} disabled={histLoading} title={t('common.refresh')}><IconRefresh size={14} /></button>
          </div>
        </div>
        <div className={sharedStyles.contentBody}>
          <button className={styles.btnBack} onClick={() => setView({ kind: 'list' })}><IconChevronDown size={14} className={styles.backIcon} />{t('common.back')}</button>
          {histLoading ? <p className={sharedStyles.toolsEmpty}>{t('common.loading')}</p> : history.length === 0 ? <div className={styles.empty}><p>{t('cron.noHistory')}</p></div> : (
            <div>
              <div className={styles.histHeader}>
                <span className={styles.histTime}>{t('cron.time')}</span>
                <span className={styles.histStatus}>{t('cron.status')}</span>
                <span className={styles.histStdout}>{t('cron.response')}</span>
              </div>
              {history.map(r => (
                <div key={r.id} className={styles.histRow}>
                  <span className={styles.histTime}>{formatFull(r.started_at)}</span>
                  <span className={styles.histStatus}><span className={runBadge(r.status)}>{runText(r.status)}</span></span>
                  <span className={styles.histStdout}>
                    {r.status === 'completed'
                      ? (r.response ? r.response.slice(0, 200) : '--')
                      : (r.error_message ? r.error_message.slice(0, 200) : '--')}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </>
    )
  }

  // ── View: Job List ─────────────────────────────────────────────────────

  return (
    <>
      <div className={sharedStyles.contentHeader}>
        <div className={sharedStyles.sectionHeaderRow}>
          <h3 className={sharedStyles.sectionTitle}>
            {t('cron.title')}
            <span className={styles.countBadge}>{jobs.length}</span>
          </h3>
          <div className={styles.headerRight}>
            <button className={sharedStyles.refreshBtn} onClick={loadJobs} disabled={loading} title={t('common.refresh')}><IconRefresh size={14} /></button>
            <button className={styles.btnAdd} onClick={() => openForm()}><IconPlus size={13} />{t('common.create')}</button>
          </div>
        </div>
        <p className={sharedStyles.sectionDesc}>{t('cron.description')}</p>
      </div>

      <div className={sharedStyles.contentBody}>
        {error && <p className={styles.errorText}>{error}</p>}
        {loading ? <p className={sharedStyles.toolsEmpty}>{t('common.loading')}</p> : jobs.length === 0 ? (
          <div className={styles.empty}>
            <div className={styles.emptyIcon}><IconCalendarClock size={32} /></div>
            <p>{t('cron.empty')}</p>
          </div>
        ) : (
          jobs.map(job => {
            const b = busy === job.id
            return (
              <div key={job.id} className={styles.card}>
                <div className={styles.cardTop}>
                  <div className={styles.cardInfo}>
                    <div className={styles.cardNameRow}>
                      <span className={styles.cardName}>{job.name}</span>
                      <span className={statusClass(job.last_status, job.enabled)}>{statusText(job)}</span>
                    </div>
                    <div className={styles.cardCron}>
                      <IconCalendarClock size={12} className={styles.inlineIcon} />
                      {describeCron(job.cron_expression)}
                      <span className={styles.cardCronRaw}>{job.cron_expression}</span>
                    </div>
                    <div className={styles.cardCmd}>{job.prompt.slice(0, 80)}{job.prompt.length > 80 ? '...' : ''}</div>
                  </div>
                  <div className={styles.cardActions}>
                    <button className={styles.iconBtn} title={t('cron.runNow')} disabled={b} onClick={() => doAction('cron.run_now', job.id)}><IconPlay size={14} /></button>
                    {job.enabled
                      ? <button className={styles.iconBtn} title={t('cron.pause')} disabled={b} onClick={() => doAction('cron.pause', job.id)}><IconPause size={14} /></button>
                      : <button className={styles.iconBtn} title={t('cron.resume')} disabled={b} onClick={() => doAction('cron.resume', job.id)}><IconPlay size={14} /></button>
                    }
                    <button className={styles.iconBtn} title={t('common.edit')} disabled={b} onClick={() => openForm(job)}><IconEdit size={14} /></button>
                    <button className={styles.iconBtn} title={t('common.delete')} disabled={b} onClick={() => doAction('cron.delete', job.id)}><IconTrash size={14} /></button>
                    <button className={styles.iconBtn} title={t('cron.history')} disabled={b} onClick={() => { setView({ kind: 'history', job }); loadHist(job.id) }}><IconHistory size={14} /></button>
                  </div>
                </div>
                <div className={styles.cardMeta}>
                  <span><IconClock size={11} className={styles.inlineIcon} />{t('cron.lastRun')} {formatTime(job.last_run)}</span>
                  <span>{t('cron.executions', { n: job.run_count })}</span>
                  {job.error_count > 0 && <span className={styles.errorCount}>{t('cron.failures', { n: job.error_count })}</span>}
                </div>
              </div>
            )
          })
        )}
      </div>
    </>
  )
}

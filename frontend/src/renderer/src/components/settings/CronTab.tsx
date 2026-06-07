import { useState, useEffect, useCallback } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import {
  IconPlus, IconPlay, IconPause, IconTrash, IconRefresh,
  IconClock, IconHistory, IconCalendarClock, IconChevronDown,
} from '../../utils/icons'
import type { CronJobSummary, CronRunHistory } from '../../stores/cron'
import sharedStyles from '../shared/SettingsModal.module.css'
import styles from './CronTab.module.css'

// ── Presets ──────────────────────────────────────────────────────────────────

const CRON_PRESETS = [
  { label: '每分钟', expr: '0 * * * * * *' },
  { label: '每5分钟', expr: '0 */5 * * * * *' },
  { label: '每30分钟', expr: '0 */30 * * * * *' },
  { label: '每小时', expr: '0 0 * * * * *' },
  { label: '每天9:00', expr: '0 0 9 * * * *' },
  { label: '每天18:00', expr: '0 0 18 * * * *' },
  { label: '每周一9:00', expr: '0 0 9 * * 1 *' },
  { label: '每月1日0:00', expr: '0 0 0 1 * * *' },
]

// ── Helpers ──────────────────────────────────────────────────────────────────

function formatTime(ts: number | null): string {
  if (!ts) return '--'
  const d = new Date(ts * 1000)
  return d.toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

function formatFull(ts: number): string {
  const d = new Date(ts * 1000)
  return d.toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function describeCron(expr: string): string {
  const p = expr.trim().split(/\s+/)
  if (p.length !== 7) return expr
  if (p[0] === '0' && p[1] === '*' && p.slice(2).every(f => f === '*')) return '每分钟'
  if (p[0] === '0' && p[1].startsWith('*/') && p.slice(2).every(f => f === '*')) return `每${p[1].slice(2)}分钟`
  if (p[0] === '0' && p[1] === '0' && p.slice(2).every(f => f === '*')) return '每小时整点'
  if (p[0] === '0' && p[1] === '0' && /^\d+$/.test(p[2]) && p.slice(3).every(f => f === '*')) return `每天 ${p[2]}:00`
  if (p[0] === '0' && p[1] === '0' && /^\d+$/.test(p[2]) && /^\d+$/.test(p[3]) && p.slice(4).every(f => f === '*')) return `每天 ${p[2]}:${p[3].padStart(2, '0')}`
  if (p[0] === '0' && p[5] !== '*' && /^\d+$/.test(p[5])) {
    const dow = ['日', '一', '二', '三', '四', '五', '六'][parseInt(p[5])]
    return `每周${dow} ${p[2]}:${p[1].padStart(2, '0')}`
  }
  return expr
}

function statusClass(status: string | null, enabled: boolean): string {
  if (status === 'running') return `${styles.badge} ${styles.badgeRunning}`
  if (!enabled) return `${styles.badge} ${styles.badgeDisabled}`
  return `${styles.badge} ${styles.badgeEnabled}`
}

function statusText(job: CronJobSummary): string {
  if (job.last_status === 'running') return '运行中'
  if (!job.enabled) return '已暂停'
  return '运行中'
}

function runBadge(s: string): string {
  if (s === 'completed') return `${styles.badge} ${styles.badgeSuccess}`
  if (s === 'failed') return `${styles.badge} ${styles.badgeFailed}`
  if (s === 'timed_out') return `${styles.badge} ${styles.badgeTimeout}`
  return `${styles.badge} ${styles.badgeRunning}`
}

function runText(s: string): string {
  if (s === 'completed') return '成功'
  if (s === 'failed') return '失败'
  if (s === 'timed_out') return '超时'
  return '运行中'
}

// ── Types ────────────────────────────────────────────────────────────────────

type View =
  | { kind: 'list' }
  | { kind: 'form'; editJob?: CronJobSummary }
  | { kind: 'history'; job: CronJobSummary }

interface FormData {
  name: string
  cron_expression: string
  command: string
  timeout_secs: number
  session_mode: 'isolated' | 'current'
}

const EMPTY_FORM: FormData = { name: '', cron_expression: '', command: '', timeout_secs: 300, session_mode: 'isolated' }

// ── Component ────────────────────────────────────────────────────────────────

export default function CronTab() {
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

  // ── Form ───────────────────────────────────────────────────────────────

  const openForm = (job?: CronJobSummary) => {
    setForm(job ? { name: job.name, cron_expression: job.cron_expression, command: job.command, timeout_secs: 300, session_mode: job.session_mode } : EMPTY_FORM)
    setFormErr(null)
    setView({ kind: 'form', editJob: job })
  }

  const saveForm = async () => {
    const f = { name: form.name.trim(), expr: form.cron_expression.trim(), cmd: form.command.trim() }
    if (!f.name) { setFormErr('名称不能为空'); return }
    if (!f.expr) { setFormErr('Cron 表达式不能为空'); return }
    if (!f.cmd) { setFormErr('命令不能为空'); return }
    setSaving(true)
    try {
      const params = { name: f.name, cron_expression: f.expr, command: f.cmd, session_mode: form.session_mode, timeout_secs: Math.max(1, Math.min(3600, form.timeout_secs)) }
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
            <h3 className={sharedStyles.sectionTitle}>{edit ? '编辑任务' : '新建任务'}</h3>
          </div>
        </div>
        <div className={sharedStyles.contentBody}>
          {formErr && <p className={styles.errorText}>{formErr}</p>}

          <div className={styles.formRow}>
            <label className={styles.formLabel}>任务名称</label>
            <input className={styles.formInput} value={form.name} onChange={e => setForm({ ...form, name: e.target.value })} placeholder="我的定时任务" />
          </div>

          <div className={styles.formRow}>
            <label className={styles.formLabel}>Cron 表达式（7 字段：秒 分 时 日 月 周 年）</label>
            <div className={styles.presetRow}>
              {CRON_PRESETS.map(p => (
                <button key={p.label} type="button" className={`${styles.presetTag} ${form.cron_expression === p.expr ? styles.presetTagActive : ''}`} onClick={() => setForm({ ...form, cron_expression: p.expr })}>
                  {p.label}
                </button>
              ))}
            </div>
            <input className={styles.formInput} value={form.cron_expression} onChange={e => setForm({ ...form, cron_expression: e.target.value })} placeholder="0 0 9 * * * *" />
            {form.cron_expression && <p className={styles.formHint}>{describeCron(form.cron_expression)}</p>}
          </div>

          <div className={styles.formRow}>
            <label className={styles.formLabel}>Shell 命令</label>
            <textarea className={styles.formTextarea} value={form.command} onChange={e => setForm({ ...form, command: e.target.value })} placeholder="echo 'hello'" rows={3} />
          </div>

          <div className={styles.formRow}>
            <label className={styles.formLabel}>执行模式</label>
            <div className={styles.modeRow}>
              <button type="button" className={`${styles.modeBtn} ${form.session_mode === 'isolated' ? styles.modeBtnActive : ''}`} onClick={() => setForm({ ...form, session_mode: 'isolated' })}>独立会话</button>
              <button type="button" className={`${styles.modeBtn} ${form.session_mode === 'current' ? styles.modeBtnActive : ''}`} onClick={() => setForm({ ...form, session_mode: 'current' })}>当前会话</button>
            </div>
          </div>

          <div className={styles.formRow}>
            <label className={styles.formLabel}>超时（秒）</label>
            <input className={styles.formInput} type="number" min={1} max={3600} value={form.timeout_secs} onChange={e => setForm({ ...form, timeout_secs: Math.max(1, Number(e.target.value) || 300) })} style={{ width: 100 }} />
          </div>

          <div className={styles.formActions}>
            <button className={styles.btnCancel} onClick={() => { setView({ kind: 'list' }); setFormErr(null) }}>取消</button>
            <button className={styles.btnSave} onClick={saveForm} disabled={saving}>{saving ? '保存中...' : '保存'}</button>
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
            <h3 className={sharedStyles.sectionTitle}>执行历史：{job.name}</h3>
            <button className={sharedStyles.refreshBtn} onClick={() => loadHist(job.id)} disabled={histLoading} title="刷新"><IconRefresh size={14} /></button>
          </div>
        </div>
        <div className={sharedStyles.contentBody}>
          <button className={styles.btnBack} onClick={() => setView({ kind: 'list' })}><IconChevronDown size={14} className={styles.backIcon} />返回</button>
          {histLoading ? <p className={sharedStyles.toolsEmpty}>加载中...</p> : history.length === 0 ? <div className={styles.empty}><p>暂无执行记录</p></div> : (
            <div>
              <div className={styles.histHeader}>
                <span className={styles.histTime}>时间</span>
                <span className={styles.histStatus}>状态</span>
                <span className={styles.histStdout}>输出</span>
                <span className={styles.histExit}>退出码</span>
              </div>
              {history.map(r => (
                <div key={r.id} className={styles.histRow}>
                  <span className={styles.histTime}>{formatFull(r.started_at)}</span>
                  <span className={styles.histStatus}><span className={runBadge(r.status)}>{runText(r.status)}</span></span>
                  <span className={styles.histStdout}>{r.stdout ? r.stdout.slice(0, 100) : '--'}</span>
                  <span className={styles.histExit}>{r.exit_code ?? '--'}</span>
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
            定时任务
            <span className={styles.countBadge}>{jobs.length}</span>
          </h3>
          <div className={styles.headerRight}>
            <button className={sharedStyles.refreshBtn} onClick={loadJobs} disabled={loading} title="刷新"><IconRefresh size={14} /></button>
            <button className={styles.btnAdd} onClick={() => openForm()}><IconPlus size={13} />新建</button>
          </div>
        </div>
        <p className={sharedStyles.sectionDesc}>管理服务端定时执行的 Shell 命令</p>
      </div>

      <div className={sharedStyles.contentBody}>
        {error && <p className={styles.errorText}>{error}</p>}
        {loading ? <p className={sharedStyles.toolsEmpty}>加载中...</p> : jobs.length === 0 ? (
          <div className={styles.empty}>
            <div className={styles.emptyIcon}><IconCalendarClock size={32} /></div>
            <p>暂无定时任务，点击上方「新建」创建</p>
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
                    <div className={styles.cardCmd}>{job.command}</div>
                  </div>
                  <div className={styles.cardActions}>
                    <button className={styles.iconBtn} title="立即执行" disabled={b} onClick={() => doAction('cron.run_now', job.id)}><IconPlay size={14} /></button>
                    {job.enabled
                      ? <button className={styles.iconBtn} title="暂停" disabled={b} onClick={() => doAction('cron.pause', job.id)}><IconPause size={14} /></button>
                      : <button className={styles.iconBtn} title="恢复" disabled={b} onClick={() => doAction('cron.resume', job.id)}><IconPlay size={14} /></button>
                    }
                    <button className={styles.iconBtn} title="删除" disabled={b} onClick={() => doAction('cron.delete', job.id)}><IconTrash size={14} /></button>
                    <button className={styles.iconBtn} title="历史" disabled={b} onClick={() => { setView({ kind: 'history', job }); loadHist(job.id) }}><IconHistory size={14} /></button>
                  </div>
                </div>
                <div className={styles.cardMeta}>
                  <span><IconClock size={11} className={styles.inlineIcon} />上次 {formatTime(job.last_run)}</span>
                  <span>执行 {job.run_count} 次</span>
                  {job.error_count > 0 && <span className={styles.errorCount}>失败 {job.error_count}</span>}
                </div>
              </div>
            )
          })
        )}
      </div>
    </>
  )
}

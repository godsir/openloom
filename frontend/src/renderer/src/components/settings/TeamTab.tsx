import { useState, useEffect, useMemo, useRef } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import Select from '../shared/Select'
import { useLocale } from '../../i18n'
import { IconSparkles, IconCheck, IconLoader, IconChevronDown } from '../../utils/icons'
import styles from '../shared/ConfigPanel.module.css'

interface TeamConfig {
  id: string
  name: string
  description: string
  strategy: 'synthesize' | 'debate'
  captain: { model?: string; system_prompt_override?: string }
  members: TeamMember[]
}

type TeamMember =
  | { name: string; source: { persona: string; model?: string; temperature?: number } }
  | { name: string; source: string }

const genSteps = [
  { key: 'analyze', label: '分析团队目标与策略' },
  { key: 'design', label: '设计成员角色分工' },
  { key: 'persona', label: '生成成员人格描述' },
  { key: 'done', label: '生成完成' },
]

export default function TeamTab() {
  const { t } = useLocale()
  const teams = useStore((s) => s.teams)
  const agents = useStore((s) => s.agents)
  const models = useStore((s) => s.models)

  const [showForm, setShowForm] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [nameDraft, setNameDraft] = useState('')
  const [descDraft, setDescDraft] = useState('')
  const [strategyDraft, setStrategyDraft] = useState<'synthesize' | 'debate'>('synthesize')
  const [captainModelDraft, setCaptainModelDraft] = useState('')
  const [membersDraft, setMembersDraft] = useState<TeamMember[]>([])
  const [loading, setLoading] = useState(true)
  const [expandedMemberIdx, setExpandedMemberIdx] = useState<number | null>(null)

  // Member add controls
  const [addMode, setAddMode] = useState<'agent' | 'custom' | null>(null)
  const [agentSelect, setAgentSelect] = useState('')
  const [customName, setCustomName] = useState('')
  const [customPersona, setCustomPersona] = useState('')
  const [customModel, setCustomModel] = useState('')

  // Member inline edit
  const [editMemberIdx, setEditMemberIdx] = useState<number | null>(null)
  const [editMemberName, setEditMemberName] = useState('')
  const [editMemberPersona, setEditMemberPersona] = useState('')
  const [editMemberModel, setEditMemberModel] = useState('')

  // AI generate state
  const [generating, setGenerating] = useState(false)
  const [genStepIdx, setGenStepIdx] = useState(-1)
  const [genError, setGenError] = useState('')
  const stepTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const refresh = async () => {
    try {
      const r = await loomRpc<{ teams: TeamConfig[] }>('team.config.list')
      useStore.getState().setTeams(r.teams || [])
    } catch {
      /* nothing */
    }
    setLoading(false)
  }

  useEffect(() => { refresh() }, [])

  useEffect(() => {
    return () => {
      if (stepTimerRef.current) clearInterval(stepTimerRef.current)
    }
  }, [])

  const modelOpts = useMemo(
    () => [
      { value: '', label: t('agent.useDefaultModel') },
      ...models.map((m) => ({
        value: m.name,
        label: m.name,
        group: m.backend_label || m.backend,
      })),
    ],
    [models, t],
  )

  const agentOpts = useMemo(
    () =>
      agents
        .filter((a) => a.name)
        .map((a) => ({ value: a.name, label: a.name })),
    [agents],
  )

  const strategyOpts = useMemo(
    () => [
      { value: 'synthesize' as const, label: t('team.strategySynthesize') },
      { value: 'debate' as const, label: t('team.strategyDebate') },
    ],
    [t],
  )

  const strategyLabel = (s: string) =>
    s === 'debate' ? t('team.strategyDebate') : t('team.strategySynthesize')

  const captainModelLabel = (m?: string) => m || t('team.captainDefaultModel')

  const buildPayload = () => ({
    name: nameDraft.trim(),
    description: descDraft.trim(),
    strategy: strategyDraft,
    captain: {
      model: captainModelDraft || undefined,
      system_prompt_override: undefined,
    },
    members: membersDraft,
  })

  const valid = nameDraft.trim().length > 0

  const resetForm = () => {
    setShowForm(false)
    setEditingId(null)
    setNameDraft('')
    setDescDraft('')
    setStrategyDraft('synthesize')
    setCaptainModelDraft('')
    setMembersDraft([])
    setAddMode(null)
    setAgentSelect('')
    setCustomName('')
    setCustomPersona('')
    setCustomModel('')
    setGenerating(false)
    setGenStepIdx(-1)
    setGenError('')
    setExpandedMemberIdx(null)
    setEditMemberIdx(null)
    if (stepTimerRef.current) {
      clearInterval(stepTimerRef.current)
      stepTimerRef.current = null
    }
  }

  const startEdit = (team: TeamConfig) => {
    setEditingId(team.id)
    setNameDraft(team.name)
    setDescDraft(team.description || '')
    setStrategyDraft(team.strategy || 'synthesize')
    setCaptainModelDraft(team.captain?.model || '')
    setMembersDraft(team.members || [])
  }

  const handleCreate = async () => {
    if (!valid) return
    try {
      await rpc('team.config.create', buildPayload(), t('team.created'))
      await refresh()
      resetForm()
    } catch { /* done */ }
  }

  const handleUpdate = async () => {
    if (!editingId || !valid) return
    try {
      await rpc('team.config.update', { ...buildPayload(), id: editingId }, t('team.updated'))
      await refresh()
      resetForm()
    } catch { /* done */ }
  }

  const handleDelete = async (id: string, name: string) => {
    const ok = await useStore
      .getState()
      .showConfirm(t('team.deleteConfirmTitle'), t('team.deleteConfirmMsg', { name }), true)
    if (!ok) return
    try {
      await rpc('team.config.delete', { id }, t('team.deleted'))
      await refresh()
    } catch { /* done */ }
  }

  // --- AI Generate Members ---
  const handleGenerateMembers = async () => {
    setGenerating(true)
    setGenStepIdx(0)
    setGenError('')
    let current = 0
    stepTimerRef.current = setInterval(() => {
      current++
      if (current < genSteps.length - 1) setGenStepIdx(current)
    }, 1200)

    try {
      const result = await loomRpc<TeamMember[]>('team.config.generate_members', {
        name: nameDraft.trim(),
        description: descDraft.trim(),
        strategy: strategyDraft,
        captain_model: captainModelDraft || null,
      })
      if (stepTimerRef.current) { clearInterval(stepTimerRef.current); stepTimerRef.current = null }
      setGenStepIdx(genSteps.length - 1)
      await new Promise((r) => setTimeout(r, 600))
      if (Array.isArray(result)) {
        setMembersDraft(result)
        useStore.getState().addToast({
          type: 'success',
          message: t('team.membersGenerated', { count: result.length }),
        })
      }
    } catch (e: any) {
      if (stepTimerRef.current) { clearInterval(stepTimerRef.current); stepTimerRef.current = null }
      setGenError(e.message || String(e))
      useStore.getState().addToast({
        type: 'error',
        message: t('team.generateFailed', { reason: e.message || String(e) }),
      })
    }
    setGenerating(false)
  }

  // --- Member management ---
  const addAgentMember = () => {
    if (!agentSelect) return
    setMembersDraft((prev) => [...prev, { name: agentSelect, source: agentSelect }])
    setAgentSelect('')
    setAddMode(null)
  }

  const addCustomMember = () => {
    if (!customName.trim() || !customPersona.trim()) return
    setMembersDraft((prev) => [
      ...prev,
      { name: customName.trim(), source: { persona: customPersona.trim(), model: customModel || undefined } },
    ])
    setCustomName('')
    setCustomPersona('')
    setCustomModel('')
    setAddMode(null)
  }

  const removeMember = (idx: number) => {
    setMembersDraft((prev) => prev.filter((_, i) => i !== idx))
    if (expandedMemberIdx === idx) setExpandedMemberIdx(null)
    if (editMemberIdx === idx) setEditMemberIdx(null)
  }

  const startEditMember = (idx: number, m: TeamMember) => {
    setEditMemberIdx(idx)
    setEditMemberName(m.name)
    if (typeof m.source === 'object') {
      setEditMemberPersona(m.source.persona || '')
      setEditMemberModel(m.source.model || '')
    } else {
      setEditMemberPersona('')
      setEditMemberModel('')
    }
  }

  const saveEditMember = () => {
    if (editMemberIdx === null) return
    setMembersDraft((prev) => {
      const next = [...prev]
      const m = next[editMemberIdx]
      if (!m) return prev
      const newName = editMemberName.trim()
      if (!newName) return prev
      if (typeof m.source === 'string') {
        next[editMemberIdx] = { name: newName, source: m.source }
      } else {
        next[editMemberIdx] = {
          name: newName,
          source: {
            persona: editMemberPersona.trim(),
            model: editMemberModel || undefined,
          },
        }
      }
      return next
    })
    setEditMemberIdx(null)
  }

  const cancelEditMember = () => {
    setEditMemberIdx(null)
  }

  const toggleMemberExpand = (idx: number) => {
    setExpandedMemberIdx((prev) => (prev === idx ? null : idx))
    if (editMemberIdx !== null && editMemberIdx !== idx) setEditMemberIdx(null)
  }

  const memberCount = (m: TeamMember[]) => m.length

  const memberTypeLabel = (m: TeamMember) =>
    typeof m.source === 'string' ? t('team.fromAgent') : t('team.customMember')

  const memberPersona = (m: TeamMember) => {
    if (typeof m.source === 'string') return t('team.refAgent', { agent: m.source })
    return m.source.persona || ''
  }

  const memberModel = (m: TeamMember) => {
    if (typeof m.source === 'string') return ''
    return m.source.model || ''
  }

  const isEditing = editingId !== null
  const isCreating = showForm && !isEditing

  const renderForm = (isEdit: boolean) => (
    <div className={isEdit ? styles.inlineForm : styles.form}>
      {/* Name */}
      <div className={styles.formRow}>
        <label className={styles.formLabel}>{t('team.name')}</label>
        <input
          value={nameDraft}
          onChange={(e) => setNameDraft(e.target.value)}
          placeholder={t('team.namePlaceholder')}
          className={styles.formInput}
        />
      </div>

      {/* Description */}
      <div className={styles.formRow}>
        <label className={styles.formLabel}>{t('team.description')}</label>
        <input
          value={descDraft}
          onChange={(e) => setDescDraft(e.target.value)}
          placeholder={t('team.descriptionPlaceholder')}
          className={styles.formInput}
        />
      </div>

      {/* Strategy */}
      <div className={styles.formRow}>
        <label className={styles.formLabel}>{t('team.strategy')}</label>
        <Select
          value={strategyDraft}
          options={strategyOpts}
          onChange={(v: 'synthesize' | 'debate') => setStrategyDraft(v)}
        />
      </div>

      {/* Captain */}
      <div className={styles.formRow}>
        <label className={styles.formLabel}>{t('team.captain')}</label>
        <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: '0 0 6px' }}>
          {t('team.captainDesc')}
        </p>
        <div style={{ padding: '10px 12px', borderRadius: 'var(--r-sm)', border: '1px solid var(--border)', background: 'var(--bg-card)' }}>
          <div className={styles.formRow} style={{ marginBottom: 0 }}>
            <label className={styles.formLabel}>{t('team.captainModel')}</label>
            <Select
              value={captainModelDraft}
              options={modelOpts}
              onChange={setCaptainModelDraft}
            />
          </div>
        </div>
      </div>

      {/* Members */}
      <div className={styles.formRow}>
        <label className={styles.formLabel}>
          {t('team.members')} ({membersDraft.length})
        </label>

        {/* AI Generate progress */}
        {generating && genStepIdx >= 0 && (
          <div
            style={{
              marginTop: 4,
              padding: '12px 14px',
              borderRadius: 'var(--r-md)',
              border: '1px solid rgba(99,102,241,0.20)',
              background: 'rgba(99,102,241,0.06)',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
              <IconSparkles size={12} style={{ color: 'rgba(168,130,255,0.9)' }} />
              <span style={{ fontSize: 11, fontWeight: 600, color: 'rgba(168,130,255,0.9)' }}>
                {t('team.generating')}
              </span>
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              {genSteps.map((step, idx) => {
                const done = idx < genStepIdx
                const active = idx === genStepIdx
                const pending = idx > genStepIdx
                return (
                  <div
                    key={step.key}
                    style={{
                      display: 'flex', alignItems: 'center', gap: 10, padding: '4px 0',
                      opacity: pending ? 0.35 : 1, transition: 'opacity 0.3s ease',
                    }}
                  >
                    <div
                      style={{
                        width: 18, height: 18, borderRadius: '50%', display: 'flex',
                        alignItems: 'center', justifyContent: 'center', flexShrink: 0,
                        border: done ? '1px solid var(--green)' : active ? '1px solid rgba(99,102,241,0.40)' : '1px solid var(--border)',
                        background: done ? 'rgba(74,222,128,0.12)' : active ? 'rgba(99,102,241,0.12)' : 'transparent',
                        transition: 'all 0.4s ease',
                      }}
                    >
                      {done ? (
                        <IconCheck size={10} style={{ color: 'var(--green)' }} />
                      ) : active ? (
                        <IconLoader size={10} style={{ color: 'rgba(99,102,241,0.9)', animation: 'spin 1s linear infinite' }} />
                      ) : (
                        <span style={{ fontSize: 9, color: 'var(--text-muted)', lineHeight: 1 }}>{idx + 1}</span>
                      )}
                    </div>
                    <span style={{ fontSize: 11, color: done || active ? 'var(--text)' : 'var(--text-muted)', fontWeight: active ? 500 : 400, transition: 'color 0.3s ease' }}>
                      {done ? step.label.replace('中', '') : step.label}
                    </span>
                  </div>
                )
              })}
            </div>
            {genError && (
              <div style={{ marginTop: 8, padding: '6px 10px', borderRadius: 'var(--r-sm)', background: 'rgba(239,68,68,0.10)', fontSize: 11, color: 'var(--red)' }}>
                {genError}
              </div>
            )}
          </div>
        )}

        {/* Members list with expand */}
        {membersDraft.length > 0 && !generating && (
          <div className={styles.list}>
            {membersDraft.map((m, idx) => {
              const isExpanded = expandedMemberIdx === idx
              const isEditingMember = editMemberIdx === idx
              const isAgentRef = typeof m.source === 'string'

              return (
                <div key={idx}>
                  {/* Member row */}
                  <div
                    className={styles.modelItem}
                    style={{ cursor: 'pointer' }}
                    onClick={() => toggleMemberExpand(idx)}
                  >
                    <div className={styles.modelInfo}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                        <span style={{ display: 'flex', transition: 'transform 0.2s ease', transform: isExpanded ? 'rotate(180deg)' : undefined }}>
                          <IconChevronDown size={10} style={{ color: 'var(--text-muted)' }} />
                        </span>
                        <span className={styles.modelName}>{m.name}</span>
                      </div>
                      <span className={styles.modelId}>{memberTypeLabel(m)}</span>
                    </div>
                    <button
                      onClick={(e) => { e.stopPropagation(); removeMember(idx) }}
                      className={styles.deleteBtn}
                    >
                      {t('common.delete')}
                    </button>
                  </div>

                  {/* Expanded detail */}
                  {isExpanded && !isEditingMember && (
                    <div
                      style={{
                        padding: '10px 14px 10px 28px',
                        marginTop: -1,
                        border: '1px solid var(--border)',
                        borderTop: 'none',
                        borderRadius: '0 0 var(--r-md) var(--r-md)',
                        background: 'var(--bg-card)',
                      }}
                    >
                      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                          <span style={{ fontSize: 10, fontWeight: 600, color: 'var(--text-muted)', minWidth: 40 }}>{t('team.memberType')}</span>
                          <span style={{ fontSize: 11, color: 'var(--text)' }}>{memberTypeLabel(m)}</span>
                        </div>
                        {isAgentRef ? (
                          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                            <span style={{ fontSize: 10, fontWeight: 600, color: 'var(--text-muted)', minWidth: 40 }}>{t('team.refAgentName')}</span>
                            <span style={{ fontSize: 11, color: 'var(--text)', fontFamily: 'var(--font-mono)' }}>{typeof m.source === 'string' ? m.source : ''}</span>
                          </div>
                        ) : (
                          <>
                            <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
                              <span style={{ fontSize: 10, fontWeight: 600, color: 'var(--text-muted)', minWidth: 40, marginTop: 2 }}>Persona</span>
                              <span style={{ fontSize: 11, color: 'var(--text)', lineHeight: 1.5 }}>{memberPersona(m)}</span>
                            </div>
                            {memberModel(m) && (
                              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                                <span style={{ fontSize: 10, fontWeight: 600, color: 'var(--text-muted)', minWidth: 40 }}>{t('team.memberModel')}</span>
                                <span style={{ fontSize: 11, color: 'var(--text)', fontFamily: 'var(--font-mono)' }}>{memberModel(m)}</span>
                              </div>
                            )}
                          </>
                        )}
                        <div style={{ marginTop: 4 }}>
                          <button
                            onClick={(e) => { e.stopPropagation(); startEditMember(idx, m) }}
                            className={styles.editBtn}
                          >
                            {t('common.edit')}
                          </button>
                        </div>
                      </div>
                    </div>
                  )}

                  {/* Inline member edit */}
                  {isExpanded && isEditingMember && (
                    <div
                      style={{
                        padding: '10px 14px 10px 14px',
                        marginTop: -1,
                        border: '1px solid var(--border-accent)',
                        borderTop: 'none',
                        borderRadius: '0 0 var(--r-md) var(--r-md)',
                        background: 'var(--bg-surface)',
                        display: 'flex',
                        flexDirection: 'column',
                        gap: 8,
                      }}
                    >
                      <div className={styles.formRow}>
                        <label className={styles.formLabel}>{t('team.memberName')}</label>
                        <input
                          value={editMemberName}
                          onChange={(e) => setEditMemberName(e.target.value)}
                          className={styles.formInput}
                          onClick={(e) => e.stopPropagation()}
                        />
                      </div>
                      {!isAgentRef && (
                        <>
                          <div className={styles.formRow}>
                            <label className={styles.formLabel}>Persona</label>
                            <textarea
                              value={editMemberPersona}
                              onChange={(e) => setEditMemberPersona(e.target.value)}
                              className={styles.formTextarea}
                              style={{ minHeight: 80 }}
                              onClick={(e) => e.stopPropagation()}
                            />
                          </div>
                          <div className={styles.formRow}>
                            <label className={styles.formLabel}>{t('team.memberModel')}</label>
                            <Select
                              value={editMemberModel}
                              options={modelOpts}
                              onChange={setEditMemberModel}
                            />
                          </div>
                        </>
                      )}
                      <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
                        <button onClick={(e) => { e.stopPropagation(); cancelEditMember() }} className={styles.cancelBtn}>
                          {t('common.cancel')}
                        </button>
                        <button onClick={(e) => { e.stopPropagation(); saveEditMember() }} className={styles.submitBtn}>
                          {t('common.save')}
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        )}

        {/* Add member controls */}
        {!generating && addMode === null && (
          <div style={{ display: 'flex', gap: 8, marginTop: 8, flexWrap: 'wrap' }}>
            <button onClick={() => setAddMode('agent')} className={styles.addBtn}>
              {t('team.addFromAgent')}
            </button>
            <button onClick={() => setAddMode('custom')} className={styles.addBtn}>
              {t('team.addCustom')}
            </button>
            <button onClick={handleGenerateMembers} className={styles.aiCreateBtn}>
              <IconSparkles size={12} /> {t('team.aiGenerateMembers')}
            </button>
          </div>
        )}

        {/* Add from Agent */}
        {addMode === 'agent' && (
          <div
            style={{
              display: 'flex', flexDirection: 'column', gap: 8, marginTop: 8, padding: 12,
              border: '1px solid var(--border)', borderRadius: 'var(--r-sm)', background: 'var(--bg-card)',
            }}
          >
            <Select value={agentSelect} options={agentOpts} onChange={setAgentSelect} placeholder={t('team.selectAgent')} />
            <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
              <button onClick={() => setAddMode(null)} className={styles.cancelBtn}>{t('common.cancel')}</button>
              <button onClick={addAgentMember} disabled={!agentSelect} className={styles.submitBtn}>{t('common.confirm')}</button>
            </div>
          </div>
        )}

        {/* Add custom member */}
        {addMode === 'custom' && (
          <div
            style={{
              display: 'flex', flexDirection: 'column', gap: 8, marginTop: 8, padding: 12,
              border: '1px solid var(--border)', borderRadius: 'var(--r-sm)', background: 'var(--bg-card)',
            }}
          >
            <input value={customName} onChange={(e) => setCustomName(e.target.value)} placeholder={t('team.memberNamePlaceholder')} className={styles.formInput} />
            <textarea value={customPersona} onChange={(e) => setCustomPersona(e.target.value)} placeholder={t('team.memberPersonaPlaceholder')} className={styles.formTextarea} style={{ minHeight: 60 }} />
            <Select value={customModel} options={modelOpts} onChange={setCustomModel} placeholder={t('team.memberModel')} />
            <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
              <button onClick={() => setAddMode(null)} className={styles.cancelBtn}>{t('common.cancel')}</button>
              <button onClick={addCustomMember} disabled={!customName.trim() || !customPersona.trim()} className={styles.submitBtn}>{t('common.confirm')}</button>
            </div>
          </div>
        )}
      </div>

      {/* Actions */}
      <div className={styles.formActions}>
        <button onClick={resetForm} className={styles.cancelBtn}>{t('common.cancel')}</button>
        {isEdit ? (
          <button onClick={handleUpdate} disabled={!valid} className={styles.submitBtn}>{t('common.save')}</button>
        ) : (
          <button onClick={handleCreate} disabled={!valid} className={styles.submitBtn}>{t('common.create')}</button>
        )}
      </div>
    </div>
  )

  if (loading) {
    return (
      <div className={styles.panel}>
        <p className={styles.empty}>{t('common.loading')}</p>
      </div>
    )
  }

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        {!isEditing && !isCreating && (
          <div className={styles.headerButtons}>
            <button onClick={() => setShowForm(true)} className={styles.addBtn}>{t('team.new')}</button>
          </div>
        )}
      </div>

      {isCreating && renderForm(false)}

      {teams.length === 0 && !isEditing && !isCreating && (
        <p className={styles.empty}>{t('team.empty')}</p>
      )}

      <div className={styles.list}>
        {teams.map((team: TeamConfig) => {
          const isActive = editingId === team.id
          return (
            <div key={team.id} className={isActive ? styles.editGroup : ''}>
              <div className={`${styles.agentCard} ${isActive ? styles.agentCardActive : ''}`}>
                <div className={styles.agentAvatar}>
                  <span className={styles.agentAvatarLetter}>{team.name[0]?.toUpperCase() || '?'}</span>
                </div>
                <div className={styles.agentCardBody}>
                  <div className={styles.agentCardHeader}>
                    <span className={styles.agentName}>{team.name}</span>
                    <div className={styles.agentBadges}>
                      <span className={styles.modelBadge}>{strategyLabel(team.strategy)}</span>
                      {team.captain?.model && (
                        <span className={styles.modelBadge}>
                          {t('team.captainLabel')}: {captainModelLabel(team.captain.model)}
                        </span>
                      )}
                      <span className={styles.modelBadge}>
                        {t('team.membersCount', { count: memberCount(team.members) })}
                      </span>
                    </div>
                  </div>
                  {team.description && (
                    <p className={styles.agentDesc}>
                      {team.description.slice(0, 80)}
                      {team.description.length > 80 ? '...' : ''}
                    </p>
                  )}
                  {/* Team inline info: captain + member summary */}
                  {(team.captain?.model || team.members?.length > 0) && (
                    <div style={{ display: 'flex', gap: 12, marginTop: 4, fontSize: 10, color: 'var(--text-muted)' }}>
                      {!team.captain?.model && (
                        <span>{t('team.captainLabel')}: {t('team.captainDefaultModel')}</span>
                      )}
                      {team.members?.length > 0 && (
                        <span>{team.members.map((m) => m.name).join(', ')}</span>
                      )}
                    </div>
                  )}
                </div>
                <div className={styles.agentActions}>
                  <button onClick={() => startEdit(team)} className={styles.editBtn}>{t('common.edit')}</button>
                  <button onClick={() => handleDelete(team.id, team.name)} className={styles.deleteBtn}>{t('common.delete')}</button>
                </div>
              </div>
              {isActive && renderForm(true)}
            </div>
          )
        })}
      </div>
    </div>
  )
}

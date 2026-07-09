import { useState, useEffect, useMemo, useRef } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import Select from '../shared/Select'
import { useLocale } from '../../i18n'
import { IconSparkles, IconCheck, IconLoader } from '../../utils/icons'
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

  // Member add controls
  const [addMode, setAddMode] = useState<'agent' | 'custom' | null>(null)
  const [agentSelect, setAgentSelect] = useState('')
  const [customName, setCustomName] = useState('')
  const [customPersona, setCustomPersona] = useState('')
  const [customModel, setCustomModel] = useState('')

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
      /* rpc handles toast */
    }
    setLoading(false)
  }

  useEffect(() => {
    refresh()
  }, [])

  // Cleanup step timer on unmount
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
    } catch {
      /* done */
    }
  }

  const handleUpdate = async () => {
    if (!editingId || !valid) return
    try {
      await rpc(
        'team.config.update',
        { ...buildPayload(), id: editingId },
        t('team.updated'),
      )
      await refresh()
      resetForm()
    } catch {
      /* done */
    }
  }

  const handleDelete = async (id: string, name: string) => {
    const ok = await useStore
      .getState()
      .showConfirm(t('team.deleteConfirmTitle'), t('team.deleteConfirmMsg', { name }), true)
    if (!ok) return
    try {
      await rpc('team.config.delete', { id }, t('team.deleted'))
      await refresh()
    } catch {
      /* done */
    }
  }

  // --- AI Generate Members ---
  const handleGenerateMembers = async () => {
    setGenerating(true)
    setGenStepIdx(0)
    setGenError('')

    // Animate steps
    let current = 0
    stepTimerRef.current = setInterval(() => {
      current++
      if (current < genSteps.length - 1) {
        setGenStepIdx(current)
      }
    }, 1200)

    try {
      const result = await loomRpc<TeamMember[]>('team.config.generate_members', {
        name: nameDraft.trim(),
        description: descDraft.trim(),
        strategy: strategyDraft,
        captain_model: captainModelDraft || null,
      })

      if (stepTimerRef.current) {
        clearInterval(stepTimerRef.current)
        stepTimerRef.current = null
      }
      setGenStepIdx(genSteps.length - 1)

      // Brief delay so user sees the "done" step
      await new Promise((r) => setTimeout(r, 600))

      if (Array.isArray(result)) {
        setMembersDraft(result)
        useStore.getState().addToast({
          type: 'success',
          message: t('team.membersGenerated', { count: result.length }),
        })
      }
    } catch (e: any) {
      if (stepTimerRef.current) {
        clearInterval(stepTimerRef.current)
        stepTimerRef.current = null
      }
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
      {
        name: customName.trim(),
        source: {
          persona: customPersona.trim(),
          model: customModel || undefined,
        },
      },
    ])
    setCustomName('')
    setCustomPersona('')
    setCustomModel('')
    setAddMode(null)
  }

  const removeMember = (idx: number) => {
    setMembersDraft((prev) => prev.filter((_, i) => i !== idx))
  }

  const memberCount = (m: TeamMember[]) => m.length

  const memberTypeLabel = (m: TeamMember) =>
    typeof m.source === 'string' ? t('team.fromAgent') : t('team.customMember')

  const memberSourceName = (m: TeamMember) => {
    if (typeof m.source === 'string') return m.source
    const p = m.source.persona || ''
    return p.length > 40 ? p.slice(0, 40) + '...' : p
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

      {/* Captain model */}
      <div className={styles.formRow}>
        <label className={styles.formLabel}>{t('team.captainModel')}</label>
        <Select
          value={captainModelDraft}
          options={modelOpts}
          onChange={setCaptainModelDraft}
        />
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

            {/* Step indicators */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              {genSteps.map((step, idx) => {
                const isDone = idx < genStepIdx
                const isActive = idx === genStepIdx
                const isPending = idx > genStepIdx

                return (
                  <div
                    key={step.key}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: 10,
                      padding: '4px 0',
                      opacity: isPending ? 0.35 : 1,
                      transition: 'opacity 0.3s ease',
                    }}
                  >
                    {/* Step indicator dot */}
                    <div
                      style={{
                        width: 18,
                        height: 18,
                        borderRadius: '50%',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        flexShrink: 0,
                        border: isDone
                          ? '1px solid var(--green)'
                          : isActive
                            ? '1px solid rgba(99,102,241,0.40)'
                            : '1px solid var(--border)',
                        background: isDone
                          ? 'rgba(74,222,128,0.12)'
                          : isActive
                            ? 'rgba(99,102,241,0.12)'
                            : 'transparent',
                        transition: 'all 0.4s ease',
                      }}
                    >
                      {isDone ? (
                        <IconCheck size={10} style={{ color: 'var(--green)' }} />
                      ) : isActive ? (
                        <IconLoader
                          size={10}
                          style={{ color: 'rgba(99,102,241,0.9)', animation: 'spin 1s linear infinite' }}
                        />
                      ) : (
                        <span style={{ fontSize: 9, color: 'var(--text-muted)', lineHeight: 1 }}>
                          {idx + 1}
                        </span>
                      )}
                    </div>
                    {/* Step label */}
                    <span
                      style={{
                        fontSize: 11,
                        color: isDone ? 'var(--text)' : isActive ? 'var(--text)' : 'var(--text-muted)',
                        fontWeight: isActive ? 500 : 400,
                        transition: 'color 0.3s ease',
                      }}
                    >
                      {isDone ? step.label.replace('中', '') : step.label}
                    </span>
                  </div>
                )
              })}
            </div>

            {/* Error */}
            {genError && (
              <div
                style={{
                  marginTop: 8,
                  padding: '6px 10px',
                  borderRadius: 'var(--r-sm)',
                  background: 'rgba(239,68,68,0.10)',
                  fontSize: 11,
                  color: 'var(--red)',
                }}
              >
                {genError}
              </div>
            )}
          </div>
        )}

        {/* Existing members list */}
        {membersDraft.length > 0 && !generating && (
          <div className={styles.list}>
            {membersDraft.map((m, idx) => (
              <div key={idx} className={styles.modelItem}>
                <div className={styles.modelInfo}>
                  <span className={styles.modelName}>{m.name}</span>
                  <span className={styles.modelId}>
                    {memberTypeLabel(m)} {memberSourceName(m) && `| ${memberSourceName(m)}`}
                  </span>
                </div>
                <button onClick={() => removeMember(idx)} className={styles.deleteBtn}>
                  {t('common.delete')}
                </button>
              </div>
            ))}
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
            <button
              onClick={handleGenerateMembers}
              className={styles.aiCreateBtn}
            >
              <IconSparkles size={12} /> {t('team.aiGenerateMembers')}
            </button>
          </div>
        )}

        {/* Add from Agent */}
        {addMode === 'agent' && (
          <div
            style={{
              display: 'flex',
              flexDirection: 'column',
              gap: 8,
              marginTop: 8,
              padding: 12,
              border: '1px solid var(--border)',
              borderRadius: 'var(--r-sm)',
              background: 'var(--bg-card)',
            }}
          >
            <Select
              value={agentSelect}
              options={agentOpts}
              onChange={setAgentSelect}
              placeholder={t('team.selectAgent')}
            />
            <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
              <button onClick={() => setAddMode(null)} className={styles.cancelBtn}>
                {t('common.cancel')}
              </button>
              <button
                onClick={addAgentMember}
                disabled={!agentSelect}
                className={styles.submitBtn}
              >
                {t('common.confirm')}
              </button>
            </div>
          </div>
        )}

        {/* Add custom member */}
        {addMode === 'custom' && (
          <div
            style={{
              display: 'flex',
              flexDirection: 'column',
              gap: 8,
              marginTop: 8,
              padding: 12,
              border: '1px solid var(--border)',
              borderRadius: 'var(--r-sm)',
              background: 'var(--bg-card)',
            }}
          >
            <input
              value={customName}
              onChange={(e) => setCustomName(e.target.value)}
              placeholder={t('team.memberNamePlaceholder')}
              className={styles.formInput}
            />
            <textarea
              value={customPersona}
              onChange={(e) => setCustomPersona(e.target.value)}
              placeholder={t('team.memberPersonaPlaceholder')}
              className={styles.formTextarea}
              style={{ minHeight: 60 }}
            />
            <Select
              value={customModel}
              options={modelOpts}
              onChange={setCustomModel}
              placeholder={t('team.memberModel')}
            />
            <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
              <button onClick={() => setAddMode(null)} className={styles.cancelBtn}>
                {t('common.cancel')}
              </button>
              <button
                onClick={addCustomMember}
                disabled={!customName.trim() || !customPersona.trim()}
                className={styles.submitBtn}
              >
                {t('common.confirm')}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Actions */}
      <div className={styles.formActions}>
        <button onClick={resetForm} className={styles.cancelBtn}>
          {t('common.cancel')}
        </button>
        {isEdit ? (
          <button onClick={handleUpdate} disabled={!valid} className={styles.submitBtn}>
            {t('common.save')}
          </button>
        ) : (
          <button onClick={handleCreate} disabled={!valid} className={styles.submitBtn}>
            {t('common.create')}
          </button>
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
            <button onClick={() => setShowForm(true)} className={styles.addBtn}>
              {t('team.new')}
            </button>
          </div>
        )}
      </div>

      {/* Create form */}
      {isCreating && renderForm(false)}

      {/* Empty state */}
      {teams.length === 0 && !isEditing && !isCreating && (
        <p className={styles.empty}>{t('team.empty')}</p>
      )}

      {/* Team list */}
      <div className={styles.list}>
        {teams.map((team: TeamConfig) => {
          const isActive = editingId === team.id
          return (
            <div key={team.id} className={isActive ? styles.editGroup : ''}>
              <div
                className={`${styles.agentCard} ${isActive ? styles.agentCardActive : ''}`}
              >
                <div className={styles.agentAvatar}>
                  <span className={styles.agentAvatarLetter}>
                    {team.name[0]?.toUpperCase() || '?'}
                  </span>
                </div>
                <div className={styles.agentCardBody}>
                  <div className={styles.agentCardHeader}>
                    <span className={styles.agentName}>{team.name}</span>
                    <div className={styles.agentBadges}>
                      <span className={styles.modelBadge}>
                        {strategyLabel(team.strategy)}
                      </span>
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
                </div>
                <div className={styles.agentActions}>
                  <button onClick={() => startEdit(team)} className={styles.editBtn}>
                    {t('common.edit')}
                  </button>
                  <button
                    onClick={() => handleDelete(team.id, team.name)}
                    className={styles.deleteBtn}
                  >
                    {t('common.delete')}
                  </button>
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

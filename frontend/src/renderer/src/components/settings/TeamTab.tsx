import { useState, useEffect, useMemo } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import Select from '../shared/Select'
import { useLocale } from '../../i18n'
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

        {membersDraft.length > 0 && (
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
        {addMode === null && (
          <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
            <button onClick={() => setAddMode('agent')} className={styles.addBtn}>
              {t('team.addFromAgent')}
            </button>
            <button onClick={() => setAddMode('custom')} className={styles.addBtn}>
              {t('team.addCustom')}
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

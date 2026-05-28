import { useState, useMemo, useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import Select from './Select'
import styles from './ConfigPanel.module.css'

export default function AgentConfigPanel() {
  const agents = useStore((s) => s.agents)
  const models = useStore((s) => s.models)
  const [showForm, setShowForm] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [nameDraft, setNameDraft] = useState('')
  const [personaDraft, setPersonaDraft] = useState('')
  const [modelDraft, setModelDraft] = useState('')
  const [systemPromptDraft, setSystemPromptDraft] = useState('')

  const refreshAgents = async () => {
    const result = await loomRpc<{ configs: unknown[] }>('agent.config.list')
    useStore.getState().setAgents(result.configs as any[] || [])
  }

  // Load agents on mount — store may have stale data from before a server restart
  useEffect(() => { refreshAgents() }, [])

  const handleCreate = async () => {
    if (!nameDraft.trim()) return
    try {
      await rpc('agent.config.create', {
        name: nameDraft.trim(),
        persona: personaDraft.trim(),
        model: modelDraft.trim() || null,
        system_prompt_override: systemPromptDraft.trim() || null,
      }, 'Agent 已创建')
      await refreshAgents()
      setShowForm(false)
      setNameDraft('')
      setPersonaDraft('')
      setModelDraft('')
      setSystemPromptDraft('')
    } catch { /* toast already shown */ }
  }

  const startEdit = (agent: any) => {
    setEditingId(agent.name || agent.id)
    setNameDraft(agent.name || '')
    setPersonaDraft(agent.persona || '')
    setModelDraft(agent.model || '')
    setSystemPromptDraft(agent.system_prompt_override || '')
  }

  const handleUpdate = async () => {
    if (!editingId || !nameDraft.trim()) return
    try {
      await rpc('agent.config.update', {
        name: nameDraft.trim(),
        prev_name: editingId,
        persona: personaDraft.trim(),
        model: modelDraft.trim() || null,
        system_prompt_override: systemPromptDraft.trim() || null,
      }, 'Agent 已更新')
      await refreshAgents()
      setEditingId(null)
      setNameDraft('')
      setPersonaDraft('')
      setModelDraft('')
      setSystemPromptDraft('')
    } catch { /* toast already shown */ }
  }

  const handleDelete = async (name: string) => {
    const ok = await useStore.getState().showConfirm('删除 Agent', `确定删除 Agent 配置 "${name}"？`, true)
    if (!ok) return
    try {
      await rpc('agent.config.delete', { name }, 'Agent 已删除')
      await refreshAgents()
    } catch { /* toast already shown */ }
  }

  const cancelForm = () => {
    setShowForm(false)
    setEditingId(null)
    setNameDraft('')
    setPersonaDraft('')
    setModelDraft('')
    setSystemPromptDraft('')
  }

  const isEditing = showForm || editingId !== null

  const modelOptions = useMemo(
    () => [{ value: '', label: '使用默认模型' }, ...models.map((m) => ({ value: m.name, label: m.name }))],
    [models],
  )

  const filteredAgents = agents.filter((a) => a.name && a.name !== 'default')

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        {!isEditing && (
          <button onClick={() => setShowForm(true)} className={styles.addBtn}>+ 新建</button>
        )}
      </div>

      {/* Create / Edit form */}
      {isEditing && (
        <div className={styles.form}>
          <div className={styles.formRow}>
            <label className={styles.formLabel}>名称 *</label>
            <input
              value={nameDraft}
              onChange={(e) => setNameDraft(e.target.value)}
              placeholder="输入 Agent 名称"
              className={styles.formInput}
            />
          </div>
          <div className={styles.formRow}>
            <label className={styles.formLabel}>模型</label>
            <Select
              value={modelDraft}
              options={modelOptions}
              onChange={setModelDraft}
            />
          </div>
          <div className={styles.formRow}>
            <label className={styles.formLabel}>Persona</label>
            <textarea
              value={personaDraft}
              onChange={(e) => setPersonaDraft(e.target.value)}
              placeholder="描述 Agent 的核心身份，如：「你是 openLoom，一个注重隐私的本地 AI 助理。你讲中文，风格简洁直接。」"
              className={styles.formTextarea}
            />
          </div>
          <div className={styles.formRow}>
            <label className={styles.formLabel}>系统提示词</label>
            <textarea
              value={systemPromptDraft}
              onChange={(e) => setSystemPromptDraft(e.target.value)}
              placeholder="自定义系统指令。留空使用默认。例如：「每次回答前先确认用户环境，优先使用本地工具而非远程 API。」"
              className={styles.formTextarea}
            />
          </div>
          <div className={styles.formActions}>
            <button onClick={cancelForm} className={styles.cancelBtn}>取消</button>
            <button
              onClick={editingId ? handleUpdate : handleCreate}
              disabled={!nameDraft.trim()}
              className={styles.submitBtn}
            >
              {editingId ? '保存' : '创建'}
            </button>
          </div>
        </div>
      )}

      {/* Agent list */}
      {filteredAgents.length === 0 && !isEditing && (
        <p className={styles.empty}>暂无 Agent 配置，点击"新建"添加</p>
      )}

      <div className={styles.list}>
        {filteredAgents.map((a) => (
          <div key={a.name} className={`${styles.modelItem} ${editingId === a.name ? styles.modelItemActive : ''}`}>
            <span className={`${styles.dot} ${styles.dotActive}`} />
            <div className={styles.modelInfo}>
              <span className={styles.modelName}>{a.name}</span>
              {a.persona && <span className={styles.modelId}>{a.persona.slice(0, 40)}{a.persona.length > 40 ? '...' : ''}</span>}
              {a.system_prompt_override && <span className={styles.systemPromptBadge}>自定义提示词</span>}
            </div>
            <span className={styles.providerBadge}>{a.model || 'default'}</span>
            <div className={styles.actions}>
              <button onClick={() => startEdit(a)} className={styles.actionBtn}>编辑</button>
              <button onClick={() => handleDelete(a.name)} className={styles.deleteBtn}>删除</button>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

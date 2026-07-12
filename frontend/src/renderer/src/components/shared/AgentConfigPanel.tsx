import { useState, useMemo, useEffect, useRef, useCallback } from 'react'
import ReactCrop, { type Crop, type PixelCrop, centerCrop, makeAspectCrop } from 'react-image-crop'
import 'react-image-crop/dist/ReactCrop.css'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import Select from './Select'
import { IconSparkles } from '../../utils/icons'
import { useLocale } from '../../i18n'
import styles from './ConfigPanel.module.css'

function cropImage(img: HTMLImageElement, crop: PixelCrop): Promise<string> {
  const canvas = document.createElement('canvas')
  const scaleX = img.naturalWidth / img.width
  const scaleY = img.naturalHeight / img.height
  canvas.width = Math.round(crop.width * scaleX)
  canvas.height = Math.round(crop.height * scaleY)
  const ctx = canvas.getContext('2d')!
  ctx.drawImage(
    img,
    Math.round(crop.x * scaleX),
    Math.round(crop.y * scaleY),
    Math.round(crop.width * scaleX),
    Math.round(crop.height * scaleY),
    0, 0,
    canvas.width,
    canvas.height,
  )
  return Promise.resolve(canvas.toDataURL('image/png', 0.85))
}

// System prompt — kept as-is (LLM-facing, not user UI)
const DEFAULT_PERSONA = `你是 Loom，openLoom 的默认助手。你是一个本地优先的私人 AI 助理内核。

核心能力：
- 认知图谱记忆：通过 事件→模式→认知→人格演化 的方式存储和回忆信息，对话具有长期连续性
- 分层路由：80% 的请求无需大模型参与，关键词快速路径 + 本地模型兜底，高效节能
- 事件驱动架构：空闲时零 Token 消耗，不轮询不浪费
- 多模型接入：支持 Anthropic Claude、OpenAI GPT、DeepSeek、Ollama、LM Studio 等
- MCP 工具集成、LSP 代码理解（40+ 语言）、Skills 技能系统

你讲中文，风格简洁直接。优先使用本地工具和本地模型，注重用户隐私。回答前先确认用户环境。`

export default function AgentConfigPanel({ embedded = false }: { embedded?: boolean }) {
  const { t } = useLocale()
  const agents = useStore((s) => s.agents)
  const models = useStore((s) => s.models)
  const [showForm, setShowForm] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [nameDraft, setNameDraft] = useState('')
  const [personaDraft, setPersonaDraft] = useState('')
  const [modelDraft, setModelDraft] = useState('')
  const [systemPromptDraft, setSystemPromptDraft] = useState('')
  const [avatarDraft, setAvatarDraft] = useState<string | null>(null)

  // AI-assisted creation state
  const [showAiForm, setShowAiForm] = useState(false)
  const [aiDescription, setAiDescription] = useState('')
  const [aiGenerating, setAiGenerating] = useState(false)
  const [aiOptimizing, setAiOptimizing] = useState(false)
  const [aiGeneratedConfig, setAiGeneratedConfig] = useState<any | null>(null)

  // Avatar crop state
  const [cropSrc, setCropSrc] = useState<string | null>(null)
  const [crop, setCrop] = useState<Crop>()
  const [completedCrop, setCompletedCrop] = useState<PixelCrop | null>(null)
  const imgRef = useRef<HTMLImageElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)

  const refreshAgents = async () => {
    const result = await loomRpc<{ configs: unknown[] }>('agent.config.list')
    useStore.getState().setAgents(result.configs as any[] || [])
  }

  useEffect(() => { refreshAgents() }, [])

  const onSelectFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => setCropSrc(reader.result as string)
    reader.readAsDataURL(file)
    e.target.value = ''
  }

  const onImageLoad = useCallback((e: React.SyntheticEvent<HTMLImageElement>) => {
    const { width, height } = e.currentTarget
    const c = centerCrop(
      makeAspectCrop({ unit: '%', width: 100 }, 1, width, height),
      width,
      height,
    )
    setCrop(c)
    // Set initial completedCrop so preview shows immediately
    const pixelCrop: PixelCrop = {
      unit: 'px',
      x: Math.round((c.x / 100) * width),
      y: Math.round((c.y / 100) * height),
      width: Math.round((c.width / 100) * width),
      height: Math.round((c.height / 100) * height),
    }
    setCompletedCrop(pixelCrop)
  }, [])

  const confirmCrop = async () => {
    if (!imgRef.current || !completedCrop) return
    const dataUrl = await cropImage(imgRef.current, completedCrop)
    setAvatarDraft(dataUrl)
    setCropSrc(null)
    setCrop(undefined)
    setCompletedCrop(null)
  }

  const cancelCrop = () => {
    setCropSrc(null)
    setCrop(undefined)
    setCompletedCrop(null)
  }

  const removeAvatar = () => setAvatarDraft(null)

  const buildPayload = () => ({
    name: nameDraft.trim(),
    persona: personaDraft.trim(),
    model: modelDraft.trim() || null,
    system_prompt_override: systemPromptDraft.trim() || null,
    avatar: avatarDraft || null,
  })

  const handleCreate = async () => {
    if (!nameDraft.trim()) return
    try {
      await rpc('agent.config.create', buildPayload(), t('agent.created'))
      await refreshAgents()
      resetForm()
    } catch { /* toast already shown */ }
  }

  const handleAiGenerate = async () => {
    if (!aiDescription.trim()) return
    setAiGenerating(true)
    let lastError = ''
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        const config = await loomRpc<any>('agent.config.generate', {
          description: aiDescription.trim(),
        })
        setAiGeneratedConfig(config)
        setNameDraft(config.name || '')
        setPersonaDraft(config.persona || '')
        setModelDraft(config.model || '')
        setSystemPromptDraft(config.system_prompt_override || '')
        setAvatarDraft(config.avatar || null)
        setShowAiForm(false)
        setShowForm(true)
        setAiGenerating(false)
        return
      } catch (e: any) {
        lastError = e.message || e
        if (attempt === 0) {
          await new Promise(r => setTimeout(r, 2000))
        }
      }
    }
    useStore.getState().addToast({
      type: 'error',
      message: t('agent.aiGenerateFailed', { message: lastError }),
    })
    setAiGenerating(false)
  }

  const handleAiOptimize = async () => {
    setAiOptimizing(true)
    let lastError = ''
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        const config = await loomRpc<any>('agent.config.optimize', {
          current: buildPayload(),
        })
        setNameDraft(config.name || nameDraft)
        setPersonaDraft(config.persona || '')
        setModelDraft(config.model || modelDraft)
        setSystemPromptDraft(config.system_prompt_override || '')
        setAvatarDraft(config.avatar || avatarDraft)
        useStore.getState().addToast({ type: 'success', message: t('agent.aiOptimized') })
        setAiOptimizing(false)
        return
      } catch (e: any) {
        lastError = e.message || e
        if (attempt === 0) {
          await new Promise(r => setTimeout(r, 2000))
        }
      }
    }
    useStore.getState().addToast({
      type: 'error',
      message: t('agent.aiOptimizeFailed', { message: lastError }),
    })
    setAiOptimizing(false)
  }

  const handleRegenerate = async () => {
    setAiGeneratedConfig(null)
    setNameDraft('')
    setPersonaDraft('')
    setModelDraft('')
    setSystemPromptDraft('')
    setAvatarDraft(null)
    setShowForm(false)
    setShowAiForm(true)
    await handleAiGenerate()
  }

  const startEdit = (agent: any) => {
    const isDefault = (agent.name || agent.id) === 'default'
    setEditingId(isDefault ? 'default' : (agent.name || agent.id))
    setNameDraft(agent.name || '')
    setPersonaDraft(isDefault ? DEFAULT_PERSONA : (agent.persona || ''))
    setModelDraft(agent.model || '')
    setSystemPromptDraft(agent.system_prompt_override || '')
    setAvatarDraft(agent.avatar || null)
  }

  const handleUpdate = async () => {
    if (!editingId || !nameDraft.trim()) return
    try {
      await rpc('agent.config.update', { ...buildPayload(), prev_name: editingId }, t('agent.updated'))
      await refreshAgents()
      resetForm()
    } catch { /* toast already shown */ }
  }

  const handleDelete = async (name: string) => {
    const ok = await useStore.getState().showConfirm(t('agent.deleteConfirmTitle'), t('agent.deleteConfirmMsg', { name }), true)
    if (!ok) return
    try {
      await rpc('agent.config.delete', { name }, t('agent.deleted'))
      await refreshAgents()
    } catch { /* toast already shown */ }
  }

  const resetForm = () => {
    setShowForm(false)
    setEditingId(null)
    setNameDraft('')
    setPersonaDraft('')
    setModelDraft('')
    setSystemPromptDraft('')
    setAvatarDraft(null)
    setShowAiForm(false)
    setAiDescription('')
    setAiGenerating(false)
    setAiGeneratedConfig(null)
  }

  const isEditing = showForm || editingId !== null
  const isDefaultAgent = editingId === 'default'

  const modelOptions = useMemo(
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

  const filteredAgents = agents.filter((a) => a.name && !a.name.startsWith('__team_'))

  return (
    <div className={styles.panel}>
      {!embedded && (
      <div className={styles.header}>
        {!isEditing && (
          <div className={styles.headerButtons}>
            <button onClick={() => setShowForm(true)} className={styles.addBtn}>{t('agent.new')}</button>
            <button
              onClick={() => { setShowAiForm(true); setShowForm(false) }}
              className={styles.aiCreateBtn}
            >
              <IconSparkles size={12} /> {t('agent.aiCreate')}
            </button>
          </div>
        )}
      </div>
      )}

      {/* Avatar crop modal */}
      {cropSrc && (
        <div className={styles.cropOverlay} onClick={cancelCrop}>
          <div className={styles.cropModal} onClick={e => e.stopPropagation()}>
            <ReactCrop
              crop={crop}
              onChange={c => setCrop(c)}
              onComplete={c => setCompletedCrop(c)}
              aspect={1}
              circularCrop={false}
              minWidth={40}
              minHeight={40}
            >
              <img ref={imgRef} src={cropSrc} onLoad={onImageLoad} alt={t('agent.cropPreview')} />
            </ReactCrop>
            <div className={styles.cropActions}>
              <button onClick={cancelCrop} className={styles.cancelBtn}>{t('common.cancel')}</button>
              <button onClick={confirmCrop} className={styles.submitBtn}>{t('agent.confirmCrop')}</button>
            </div>
          </div>
        </div>
      )}

      {/* AI description input */}
      {showAiForm && !aiGeneratedConfig && (
        <div className={styles.aiSection}>
          <p className={styles.aiHint}>
            {t('agent.aiHint')}
          </p>
          <textarea
            value={aiDescription}
            onChange={(e) => setAiDescription(e.target.value)}
            placeholder={t('agent.aiPlaceholder')}
            className={styles.aiTextarea}
            disabled={aiGenerating}
          />
          <div className={styles.aiActions}>
            <button onClick={resetForm} className={styles.cancelBtn} disabled={aiGenerating}>
              {t('common.cancel')}
            </button>
            <button
              onClick={handleAiGenerate}
              disabled={!aiDescription.trim() || aiGenerating}
              className={styles.submitBtn}
            >
              {aiGenerating ? t('agent.generating') : t('agent.generateConfig')}
            </button>
          </div>
        </div>
      )}

      {/* Hidden file input for avatar */}
      <input
        ref={fileInputRef}
        type="file"
        accept="image/*"
        onChange={onSelectFile}
        style={{ display: 'none' }}
      />

      {/* Create form (shown at top when creating new agent) */}
      {showForm && !editingId && (
        <div className={styles.form}>
          {aiGeneratedConfig && (
            <div className={styles.aiGeneratedBadge}>
              <span><IconSparkles size={11} /> {t('agent.aiGenerated')}</span>
              <button onClick={handleRegenerate} className={styles.regenerateBtn}>
                {t('agent.regenerate')}
              </button>
            </div>
          )}
          <div className={styles.formRow}>
            <label className={styles.formLabel}>{t('agent.avatar')}</label>
            <div className={styles.avatarRow}>
              <div
                className={styles.avatarPreview}
                onClick={() => fileInputRef.current?.click()}
                title={t('agent.clickToUpload')}
              >
                {avatarDraft ? (
                  <img src={avatarDraft} alt="avatar" className={styles.avatarPreviewImg} />
                ) : (
                  <span className={styles.avatarPlaceholder}>+</span>
                )}
              </div>
              {avatarDraft && (
                <button onClick={removeAvatar} className={styles.avatarRemoveBtn}>{t('common.delete')}</button>
              )}
            </div>
          </div>

          <div className={styles.formRow}>
            <label className={styles.formLabel}>*</label>
            <input
              value={nameDraft}
              onChange={(e) => setNameDraft(e.target.value)}
              placeholder={t('agent.namePlaceholder')}
              className={styles.formInput}
            />
          </div>
          <div className={styles.formRow}>
            <label className={styles.formLabel}>{t('agent.model')}</label>
            <Select
              value={modelDraft}
              options={modelOptions}
              onChange={setModelDraft}
            />
          </div>
          <div className={styles.formRow}>
            <label className={styles.formLabel}>{t('agent.persona')}</label>
            <textarea
              value={personaDraft}
              onChange={(e) => setPersonaDraft(e.target.value)}
              placeholder={t('agent.personaPlaceholder')}
              className={styles.formTextarea}
            />
          </div>
          <div className={styles.formRow}>
            <label className={styles.formLabel}>{t('agent.systemPrompt')}</label>
            <textarea
              value={systemPromptDraft}
              onChange={(e) => setSystemPromptDraft(e.target.value)}
              placeholder={t('agent.systemPromptPlaceholder')}
              className={styles.formTextarea}
            />
          </div>
          <div className={styles.formActions}>
            <button onClick={resetForm} className={styles.cancelBtn}>{t('common.cancel')}</button>
            <button
              onClick={handleCreate}
              disabled={!nameDraft.trim()}
              className={styles.submitBtn}
            >
              {t('common.create')}
            </button>
          </div>
        </div>
      )}

      {/* Agent list */}
      {filteredAgents.length === 0 && !isEditing && (
        <p className={styles.empty}>{t('agent.empty')}</p>
      )}

      {(() => {
        const defaultAgent = filteredAgents.find((a) => a.name === 'default')
        const userAgents = filteredAgents.filter((a) => a.name !== 'default')

        const renderEditForm = () => (
          <div className={styles.inlineForm}>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>{t('agent.avatar')}</label>
              <div className={styles.avatarRow}>
                <div
                  className={styles.avatarPreview}
                  onClick={() => fileInputRef.current?.click()}
                  title={t('agent.clickToUpload')}
                >
                  {avatarDraft ? (
                    <img src={avatarDraft} alt="avatar" className={styles.avatarPreviewImg} />
                  ) : (
                    <span className={styles.avatarPlaceholder}>+</span>
                  )}
                </div>
                {avatarDraft && (
                  <button onClick={removeAvatar} className={styles.avatarRemoveBtn}>{t('common.delete')}</button>
                )}
              </div>
            </div>

            <div className={styles.formRow}>
              <label className={styles.formLabel}>{isDefaultAgent ? 'Loom' : '*'}</label>
              <input
                value={isDefaultAgent ? 'Loom' : nameDraft}
                onChange={(e) => setNameDraft(e.target.value)}
                placeholder={t('agent.namePlaceholder')}
                className={styles.formInput}
                disabled={isDefaultAgent}
              />
            </div>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>{t('agent.model')}</label>
              <Select
                value={modelDraft}
                options={modelOptions}
                onChange={setModelDraft}
              />
            </div>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>{t('agent.persona')}{isDefaultAgent ? t('agent.personaBuiltin') : ''}</label>
              <textarea
                value={personaDraft}
                onChange={(e) => setPersonaDraft(e.target.value)}
                placeholder={t('agent.personaPlaceholder')}
                className={styles.formTextarea}
                disabled={isDefaultAgent}
              />
            </div>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>{t('agent.systemPrompt')}{isDefaultAgent ? t('agent.systemPromptBuiltin') : ''}</label>
              <textarea
                value={systemPromptDraft}
                onChange={(e) => setSystemPromptDraft(e.target.value)}
                placeholder={t('agent.systemPromptPlaceholder')}
                className={styles.formTextarea}
                disabled={isDefaultAgent}
              />
            </div>
            <div className={styles.formActions}>
              <button onClick={resetForm} className={styles.cancelBtn}>{t('common.cancel')}</button>
              <button
                onClick={handleAiOptimize}
                disabled={aiOptimizing}
                className={styles.aiCreateBtn}
              >
                <IconSparkles size={12} /> {aiOptimizing ? t('agent.optimizing') : t('agent.aiOptimize')}
              </button>
              <button
                onClick={handleUpdate}
                disabled={!nameDraft.trim()}
                className={styles.submitBtn}
              >
                {t('common.save')}
              </button>
            </div>
          </div>
        )

        const renderItem = (a: any) => {
          const agentId = a.name === 'default' ? 'default' : (a.name || a.id)
          const isActive = editingId === agentId
          return (
            <div key={a.name} className={isActive ? styles.editGroup : ''}>
              <div className={`${styles.agentCard} ${isActive ? styles.agentCardActive : ''}`}>
                <div className={styles.agentAvatar}>
                  {a.avatar ? (
                    <img src={a.avatar} alt={a.name} className={styles.agentAvatarImg} />
                  ) : (
                    <span className={styles.agentAvatarLetter}>{a.name[0]?.toUpperCase() || '?'}</span>
                  )}
                </div>
                <div className={styles.agentCardBody}>
                  <div className={styles.agentCardHeader}>
                    <span className={styles.agentName}>{a.name === 'default' ? 'Loom' : a.name}</span>
                    <div className={styles.agentBadges}>
                      {a.name === 'default' && <span className={styles.defaultBadge}>{t('agent.default')}</span>}
                      {a.system_prompt_override && <span className={styles.customBadge}>{t('agent.customPrompt')}</span>}
                      {a.model && <span className={styles.modelBadge}>{a.model}</span>}
                    </div>
                  </div>
                  {a.persona && (
                    <p className={styles.agentDesc}>
                      {a.persona.slice(0, 80)}{a.persona.length > 80 ? '...' : ''}
                    </p>
                  )}
                </div>
                <div className={styles.agentActions}>
                  <button onClick={() => startEdit(a)} className={styles.editBtn}>{t('common.edit')}</button>
                  {a.name !== 'default' && (
                    <button onClick={() => handleDelete(a.name)} className={styles.deleteBtn}>{t('common.delete')}</button>
                  )}
                </div>
              </div>
              {isActive && renderEditForm()}
            </div>
          )
        }

        return (
          <div className={styles.list}>
            {defaultAgent && renderItem(defaultAgent)}
            {userAgents.length > 0 && (
              <>
                {defaultAgent && <div className={styles.sectionLabel}>{t('agent.userCreated')}</div>}
                {userAgents.map(renderItem)}
              </>
            )}
          </div>
        )
      })()}
    </div>
  )
}

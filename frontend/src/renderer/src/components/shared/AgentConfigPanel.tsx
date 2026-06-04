import { useState, useMemo, useEffect, useRef, useCallback } from 'react'
import ReactCrop, { type Crop, type PixelCrop, centerCrop, makeAspectCrop } from 'react-image-crop'
import 'react-image-crop/dist/ReactCrop.css'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import Select from './Select'
import { IconSparkles } from '../../utils/icons'
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

const DEFAULT_PERSONA = `你是 Loom，openLoom 的默认助手。你是一个本地优先的私人 AI 助理内核。

核心能力：
- 认知图谱记忆：通过 事件→模式→认知→人格演化 的方式存储和回忆信息，对话具有长期连续性
- 分层路由：80% 的请求无需大模型参与，关键词快速路径 + 本地模型兜底，高效节能
- 事件驱动架构：空闲时零 Token 消耗，不轮询不浪费
- 多模型接入：支持 Anthropic Claude、OpenAI GPT、DeepSeek、Ollama、LM Studio 等
- MCP 工具集成、LSP 代码理解（40+ 语言）、Skills 技能系统

你讲中文，风格简洁直接。优先使用本地工具和本地模型，注重用户隐私。回答前先确认用户环境。`

export default function AgentConfigPanel() {
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
      await rpc('agent.config.create', buildPayload(), 'Agent 已创建')
      await refreshAgents()
      resetForm()
    } catch { /* toast already shown */ }
  }

  const handleAiGenerate = async () => {
    if (!aiDescription.trim()) return
    setAiGenerating(true)
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
    } catch (e: any) {
      useStore.getState().addToast({
        type: 'error',
        message: `AI 生成失败: ${e.message || e}`,
      })
    } finally {
      setAiGenerating(false)
    }
  }

  const handleAiOptimize = async () => {
    setAiOptimizing(true)
    try {
      const config = await loomRpc<any>('agent.config.optimize', {
        current: buildPayload(),
      })
      setNameDraft(config.name || nameDraft)
      setPersonaDraft(config.persona || '')
      setModelDraft(config.model || modelDraft)
      setSystemPromptDraft(config.system_prompt_override || '')
      setAvatarDraft(config.avatar || avatarDraft)
      useStore.getState().addToast({ type: 'success', message: 'AI 优化完成' })
    } catch (e: any) {
      useStore.getState().addToast({
        type: 'error',
        message: `AI 优化失败: ${e.message || e}`,
      })
    } finally {
      setAiOptimizing(false)
    }
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
      await rpc('agent.config.update', { ...buildPayload(), prev_name: editingId }, 'Agent 已更新')
      await refreshAgents()
      resetForm()
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
      { value: '', label: '使用默认模型' },
      ...models.map((m) => ({
        value: m.name,
        label: m.name,
        group: m.backend_label || m.backend,
      })),
    ],
    [models],
  )

  const filteredAgents = agents.filter((a) => a.name)

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        {!isEditing && (
          <div className={styles.headerButtons}>
            <button onClick={() => setShowForm(true)} className={styles.addBtn}>+ 新建</button>
            <button
              onClick={() => { setShowAiForm(true); setShowForm(false) }}
              className={styles.aiCreateBtn}
            >
              <IconSparkles size={12} /> AI 创建
            </button>
          </div>
        )}
      </div>

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
              <img ref={imgRef} src={cropSrc} onLoad={onImageLoad} alt="裁剪预览" />
            </ReactCrop>
            <div className={styles.cropActions}>
              <button onClick={cancelCrop} className={styles.cancelBtn}>取消</button>
              <button onClick={confirmCrop} className={styles.submitBtn}>确认裁剪</button>
            </div>
          </div>
        </div>
      )}

      {/* AI description input */}
      {showAiForm && !aiGeneratedConfig && (
        <div className={styles.aiSection}>
          <p className={styles.aiHint}>
            描述你想要创建的 Agent，AI 将自动生成名称、人格和配置。
          </p>
          <textarea
            value={aiDescription}
            onChange={(e) => setAiDescription(e.target.value)}
            placeholder="例如：一个擅长 Python 代码审查的助手，风格严谨，只使用文件读取和 shell 工具..."
            className={styles.aiTextarea}
            disabled={aiGenerating}
          />
          <div className={styles.aiActions}>
            <button onClick={resetForm} className={styles.cancelBtn} disabled={aiGenerating}>
              取消
            </button>
            <button
              onClick={handleAiGenerate}
              disabled={!aiDescription.trim() || aiGenerating}
              className={styles.submitBtn}
            >
              {aiGenerating ? '生成中...' : '生成配置'}
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
              <span><IconSparkles size={11} /> AI 生成</span>
              <button onClick={handleRegenerate} className={styles.regenerateBtn}>
                重新生成
              </button>
            </div>
          )}
          <div className={styles.formRow}>
            <label className={styles.formLabel}>头像</label>
            <div className={styles.avatarRow}>
              <div
                className={styles.avatarPreview}
                onClick={() => fileInputRef.current?.click()}
                title="点击上传头像"
              >
                {avatarDraft ? (
                  <img src={avatarDraft} alt="avatar" className={styles.avatarPreviewImg} />
                ) : (
                  <span className={styles.avatarPlaceholder}>+</span>
                )}
              </div>
              {avatarDraft && (
                <button onClick={removeAvatar} className={styles.avatarRemoveBtn}>移除</button>
              )}
            </div>
          </div>

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
              placeholder="描述 Agent 的核心身份"
              className={styles.formTextarea}
            />
          </div>
          <div className={styles.formRow}>
            <label className={styles.formLabel}>系统提示词</label>
            <textarea
              value={systemPromptDraft}
              onChange={(e) => setSystemPromptDraft(e.target.value)}
              placeholder="自定义系统指令。留空使用默认。"
              className={styles.formTextarea}
            />
          </div>
          <div className={styles.formActions}>
            <button onClick={resetForm} className={styles.cancelBtn}>取消</button>
            <button
              onClick={handleCreate}
              disabled={!nameDraft.trim()}
              className={styles.submitBtn}
            >
              创建
            </button>
          </div>
        </div>
      )}

      {/* Agent list */}
      {filteredAgents.length === 0 && !isEditing && (
        <p className={styles.empty}>暂无 Agent 配置，点击"新建"添加</p>
      )}

      {(() => {
        const defaultAgent = filteredAgents.find((a) => a.name === 'default')
        const userAgents = filteredAgents.filter((a) => a.name !== 'default')

        const renderEditForm = () => (
          <div className={styles.inlineForm}>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>头像</label>
              <div className={styles.avatarRow}>
                <div
                  className={styles.avatarPreview}
                  onClick={() => fileInputRef.current?.click()}
                  title="点击上传头像"
                >
                  {avatarDraft ? (
                    <img src={avatarDraft} alt="avatar" className={styles.avatarPreviewImg} />
                  ) : (
                    <span className={styles.avatarPlaceholder}>+</span>
                  )}
                </div>
                {avatarDraft && (
                  <button onClick={removeAvatar} className={styles.avatarRemoveBtn}>移除</button>
                )}
              </div>
            </div>

            <div className={styles.formRow}>
              <label className={styles.formLabel}>名称 {isDefaultAgent ? '' : '*'}</label>
              <input
                value={isDefaultAgent ? 'Loom' : nameDraft}
                onChange={(e) => setNameDraft(e.target.value)}
                placeholder="输入 Agent 名称"
                className={styles.formInput}
                disabled={isDefaultAgent}
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
              <label className={styles.formLabel}>Persona{isDefaultAgent ? '（内置）' : ''}</label>
              <textarea
                value={personaDraft}
                onChange={(e) => setPersonaDraft(e.target.value)}
                placeholder="描述 Agent 的核心身份"
                className={styles.formTextarea}
                disabled={isDefaultAgent}
              />
            </div>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>系统提示词{isDefaultAgent ? '（内置）' : ''}</label>
              <textarea
                value={systemPromptDraft}
                onChange={(e) => setSystemPromptDraft(e.target.value)}
                placeholder="自定义系统指令。留空使用默认。"
                className={styles.formTextarea}
                disabled={isDefaultAgent}
              />
            </div>
            <div className={styles.formActions}>
              <button onClick={resetForm} className={styles.cancelBtn}>取消</button>
              <button
                onClick={handleAiOptimize}
                disabled={aiOptimizing}
                className={styles.aiCreateBtn}
              >
                <IconSparkles size={12} /> {aiOptimizing ? '优化中...' : 'AI 优化'}
              </button>
              <button
                onClick={handleUpdate}
                disabled={!nameDraft.trim()}
                className={styles.submitBtn}
              >
                保存
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
                      {a.name === 'default' && <span className={styles.defaultBadge}>默认</span>}
                      {a.system_prompt_override && <span className={styles.customBadge}>自定义提示词</span>}
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
                  <button onClick={() => startEdit(a)} className={styles.editBtn}>编辑</button>
                  {a.name !== 'default' && (
                    <button onClick={() => handleDelete(a.name)} className={styles.deleteBtn}>删除</button>
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
                {defaultAgent && <div className={styles.sectionLabel}>用户创建</div>}
                {userAgents.map(renderItem)}
              </>
            )}
          </div>
        )
      })()}
    </div>
  )
}

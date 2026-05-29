import { useState, useEffect, useMemo } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useStore } from '../../stores'
import { IconEye, IconWrench, IconBrain, IconX } from '../../utils/icons'
import Select from './Select'
import type { ModelConfig, ModelListItem, ModelBackend } from '../../types/bindings'
import styles from './ModelConfig.module.css'

interface ProviderEntry {
  id: string
  label: string
  backend: ModelBackend
  defaultUrl: string
  apiFormat: 'openai' | 'anthropic'
  isCustom?: boolean
  envVar?: string
}

const PRESET_PROVIDERS: ProviderEntry[] = [
  { id: 'lmstudio', label: 'LM Studio', backend: 'LmStudio', defaultUrl: 'http://localhost:1234/v1', apiFormat: 'openai' },
  { id: 'ollama', label: 'Ollama', backend: 'Ollama', defaultUrl: 'http://localhost:11434/v1', apiFormat: 'openai' },
  { id: 'anthropic', label: 'Anthropic', backend: 'Anthropic', defaultUrl: 'https://api.anthropic.com', apiFormat: 'anthropic' },
  { id: 'openai', label: 'OpenAI', backend: 'OpenAI', defaultUrl: 'https://api.openai.com/v1', apiFormat: 'openai' },
  { id: 'deepseek', label: 'DeepSeek', backend: 'DeepSeek', defaultUrl: 'https://api.deepseek.com/v1', apiFormat: 'openai' },
]

const CUSTOM_PROVIDERS_KEY = 'customProviders'

async function loadCustomProviders(): Promise<ProviderEntry[]> {
  try {
    return await window.hana.getPreference<ProviderEntry[]>(CUSTOM_PROVIDERS_KEY, [])
  } catch { return [] }
}

async function saveCustomProviders(entries: ProviderEntry[]): Promise<void> {
  const custom = entries.filter(e => e.isCustom)
  await window.hana.setPreference(CUSTOM_PROVIDERS_KEY, custom)
}

function buildProviders(customProviders: ProviderEntry[], models: ModelListItem[]): ProviderEntry[] {
  // Discover custom providers from loaded models (backend_label on Custom backend)
  const seenLabels = new Set(customProviders.map(c => c.label))
  const discovered: ProviderEntry[] = []
  for (const m of models) {
    if (m.backend === 'Custom' && m.backend_label && !seenLabels.has(m.backend_label)) {
      seenLabels.add(m.backend_label)
      discovered.push({
        id: `custom-discovered-${m.backend_label}`,
        label: m.backend_label,
        backend: 'Custom',
        defaultUrl: m.base_url || '',
        apiFormat: (m as any).api_format === 'anthropic' ? 'anthropic' : 'openai',
        isCustom: true,
      })
    }
  }
  return [...PRESET_PROVIDERS, ...customProviders, ...discovered]
}

export default function ModelConfigPanel() {
  const [models, setModels] = useState<ModelListItem[]>([])
  const [providers, setProviders] = useState<ProviderEntry[]>(PRESET_PROVIDERS)
  const [selectedId, setSelectedId] = useState<string>('deepseek')
  const [showCustomForm, setShowCustomForm] = useState(false)
  const [customName, setCustomName] = useState('')
  const [customUrl, setCustomUrl] = useState('')
  const [customFormat, setCustomFormat] = useState<'openai' | 'anthropic'>('openai')
  const [customEnvVar, setCustomEnvVar] = useState('OPENLOOM_API_KEY')

  // Per-provider state
  const [apiKey, setApiKey] = useState('')
  const [baseUrl, setBaseUrl] = useState('https://api.deepseek.com/v1')
  const [apiFormat, setApiFormat] = useState<'openai' | 'anthropic'>('openai')
  const [verifyStatus, setVerifyStatus] = useState<'idle' | 'testing' | 'ok' | 'fail'>('idle')

  // Discovered
  const [discovered, setDiscovered] = useState<Array<{ id: string; context_length?: number }>>([])
  const [discovering, setDiscovering] = useState(false)

  // Edit state
  const [editingModel, setEditingModel] = useState<string | null>(null)
  const [editForm, setEditForm] = useState<{
    name: string; model: string; backend: ModelBackend; base_url: string;
    context_size: number; max_output_tokens: string
    backend_label?: string; api_format?: string
    vision: boolean; reasoning: boolean; function_calling: boolean
  }>({ name: '', model: '', backend: 'DeepSeek', base_url: '', context_size: 4096, max_output_tokens: '', vision: false, reasoning: false, function_calling: false })

  const selected = providers.find(p => p.id === selectedId)

  const refresh = async () => {
    try {
      const [result, customProviders] = await Promise.all([
        loomRpc<{ models: ModelListItem[]; activeModel: string | null }>('model.list'),
        loadCustomProviders(),
      ])
      setModels(result.models || [])
      setProviders(buildProviders(customProviders, result.models || []))
    } catch (e) {
      console.error('Failed to list models:', e)
    }
  }

  useEffect(() => { refresh() }, [])

  const handleSelect = (p: ProviderEntry) => {
    setSelectedId(p.id)
    setBaseUrl(p.defaultUrl)
    setApiFormat(p.apiFormat)
    setApiKey('')
    setVerifyStatus('idle')
    setDiscovered([])
  }

  const handleSaveKey = async () => {
    if (!apiKey.trim() || !selected) return
    setVerifyStatus('testing')
    try {
      await rpc('model.save_key', {
        backend: selected.backend,
        api_key: apiKey.trim(),
        base_url: baseUrl.trim(),
        api_key_env: selected.isCustom ? selected.envVar : undefined,
      }, 'API Key 已保存')
      setVerifyStatus('ok')
      setApiKey('')
    } catch (e: any) {
      setVerifyStatus('fail')
    }
  }

  const handleFetchModels = async () => {
    if (!selected) return
    setDiscovering(true)
    try {
      const result = await loomRpc<{ models: Array<{ id: string; context_length?: number }> }>('model.discover', {
        backend: selected.backend,
        base_url: baseUrl.trim(),
        api_format: apiFormat,
        api_key_env: selected.isCustom ? selected.envVar : undefined,
      })
      setDiscovered(result.models || [])
    } catch (e: any) {
      console.error('Failed to discover models:', e)
      setDiscovered([])
    } finally {
      setDiscovering(false)
    }
  }

  const handleAddModel = async (model: { id: string; context_length?: number }) => {
    if (!selected) return
    const modelId = model.id
    const name = modelId.split('/').pop() || modelId
    const envName = selected.isCustom
      ? selected.envVar || 'OPENLOOM_API_KEY'
      : `${selected.backend.toUpperCase()}_API_KEY`
    try {
      await rpc('model.config.create', {
        name,
        model: modelId,
        model_type: 'Router',
        backend: selected.backend,
        backend_label: selected.isCustom ? selected.label : undefined,
        base_url: baseUrl.trim() || null,
        api_key_env: envName,
        api_format: apiFormat,
        context_size: model.context_length || 4096,
      }, `模型 "${name}" 已添加`)
      await refresh()
      setDiscovered(prev => prev.filter(m => m.id !== modelId))
    } catch (e: any) {
      console.error('Failed to add model:', e)
    }
  }

  const handleDeleteModel = async (name: string) => {
    const ok = await useStore.getState().showConfirm('删除模型', `确定删除 "${name}"？`, true)
    if (!ok) return
    try {
      await rpc('model.config.delete', { name }, `模型 "${name}" 已删除`)
      await refresh()
    } catch { /* toast already shown */ }
  }

  const handleSetActive = async (name: string) => {
    try {
      await rpc('model.config.set_active', { name }, `已切换到 "${name}"`)
      await refresh()
    } catch { /* toast already shown */ }
  }

  const handleStartEdit = (m: ModelListItem) => {
    setEditingModel(m.name)
    setEditForm({
      name: m.name,
      model: m.model || '',
      backend: m.backend as ModelBackend,
      base_url: m.base_url || '',
      context_size: m.context_size || 4096,
      max_output_tokens: '',
      backend_label: (m as any).backend_label,
      api_format: (m as any).api_format,
      vision: m.capabilities?.vision ?? false,
      reasoning: m.capabilities?.reasoning ?? false,
      function_calling: m.capabilities?.function_calling ?? false,
    })
  }

  const handleCancelEdit = () => {
    setEditingModel(null)
  }

  const handleSaveEdit = async () => {
    if (!editingModel) return
    try {
      await rpc('model.config.update', {
        name: editForm.name,
        model: editForm.model || undefined,
        backend: editForm.backend,
        base_url: editForm.base_url.trim() || undefined,
        backend_label: editForm.backend_label || undefined,
        api_format: editForm.api_format || undefined,
        context_size: editForm.context_size,
        max_output_tokens: editForm.max_output_tokens ? parseInt(editForm.max_output_tokens, 10) : undefined,
        capabilities: {
          vision: editForm.vision,
          reasoning: editForm.reasoning,
          function_calling: editForm.function_calling,
        },
      }, '模型已更新')
      setEditingModel(null)
      await refresh()
    } catch { /* toast already shown */ }
  }

  const handleDeleteCustom = async (entry: ProviderEntry) => {
    const ok = await useStore.getState().showConfirm('删除供应商', `确定删除供应商 "${entry.label}"？已配置的模型不会受影响。`, true)
    if (!ok) return
    const next = providers.filter(p => p.id !== entry.id)
    setProviders(next)
    await saveCustomProviders(next)
    if (selectedId === entry.id) {
      setSelectedId('deepseek')
      setBaseUrl('https://api.deepseek.com/v1')
      setApiFormat('openai')
      setApiKey('')
      setVerifyStatus('idle')
      setDiscovered([])
    }
  }

  const handleAddCustom = async () => {
    if (!customName.trim() || !customUrl.trim()) return
    const entry: ProviderEntry = {
      id: `custom-${Date.now()}`,
      label: customName.trim(),
      backend: 'Custom',
      defaultUrl: customUrl.trim(),
      apiFormat: customFormat,
      isCustom: true,
      envVar: customEnvVar.trim() || 'OPENLOOM_API_KEY',
    }
    const next = [...providers, entry]
    setProviders(next)
    await saveCustomProviders(next)
    setSelectedId(entry.id)
    setBaseUrl(entry.defaultUrl)
    setApiFormat(entry.apiFormat)
    setShowCustomForm(false)
    setCustomName('')
    setCustomUrl('')
    setCustomFormat('openai')
    setCustomEnvVar('OPENLOOM_API_KEY')
  }

  const providerModels = selected
    ? models.filter(m => {
        if (selected.isCustom) return (m.backend_label || '') === selected.label
        return m.backend === selected.backend
      })
    : []

  // Only dedup within the current provider — same model name across different providers is expected
  const configuredModelIds = new Set(providerModels.map(m => m.model))

  const getModelCount = (p: ProviderEntry) => {
    if (p.isCustom) return models.filter(m => (m.backend_label || '') === p.label).length
    return models.filter(m => m.backend === p.backend).length
  }

  return (
    <div className={styles.pvLayout}>
      {/* Left: provider list */}
      <div className={styles.pvList}>
        {providers.filter(p => !p.isCustom).map((p) => {
          const count = getModelCount(p)
          const hasKey = count > 0 || (verifyStatus === 'ok' && selectedId === p.id)
          return (
            <div
              key={p.id}
              className={`${styles.pvListItem} ${selectedId === p.id ? styles.pvListItemSelected : ''}`}
            >
              <button onClick={() => handleSelect(p)} className={styles.pvListItemBtn}>
                <span className={`${styles.pvStatusDot} ${hasKey ? styles.pvStatusDotOn : ''}`} />
                <span className={styles.pvListName}>{p.label}</span>
                {count > 0 && <span className={styles.pvListCount}>{count}</span>}
              </button>
            </div>
          )
        })}

        {providers.some(p => p.isCustom) && (
          <>
            <div className={styles.pvSectionHeader}>自定义供应商</div>
            {providers.filter(p => p.isCustom).map((p) => {
              const count = getModelCount(p)
              const hasKey = count > 0 || (verifyStatus === 'ok' && selectedId === p.id)
              return (
                <div
                  key={p.id}
                  className={`${styles.pvListItem} ${selectedId === p.id ? styles.pvListItemSelected : ''}`}
                >
                  <button onClick={() => handleSelect(p)} className={styles.pvListItemBtn}>
                    <span className={`${styles.pvStatusDot} ${hasKey ? styles.pvStatusDotOn : ''}`} />
                    <span className={styles.pvListName}>{p.label}</span>
                    {count > 0 && <span className={styles.pvListCount}>{count}</span>}
                  </button>
                  <button
                    onClick={(e) => { e.stopPropagation(); handleDeleteCustom(p) }}
                    className={styles.pvListDelete}
                    title="删除供应商"
                  >
                    <IconX size={10} />
                  </button>
                </div>
              )
            })}
          </>
        )}

        {showCustomForm ? (
          <div className={styles.pvCustomForm}>
            <input
              value={customName}
              onChange={e => setCustomName(e.target.value)}
              placeholder="供应商名称"
              className={styles.pvCustomInput}
              autoFocus
            />
            <input
              value={customUrl}
              onChange={e => setCustomUrl(e.target.value)}
              placeholder="Base URL"
              className={styles.pvCustomInput}
            />
            <Select
              value={customFormat}
              options={[
                { value: 'openai', label: 'OpenAI 格式' },
                { value: 'anthropic', label: 'Anthropic 格式' },
              ]}
              onChange={(v) => setCustomFormat(v as 'openai' | 'anthropic')}
            />
            <input
              value={customEnvVar}
              onChange={e => setCustomEnvVar(e.target.value)}
              placeholder="环境变量名 (如 OPENLOOM_API_KEY)"
              className={styles.pvCustomInput}
            />
            <div className={styles.pvCustomActions}>
              <button onClick={() => setShowCustomForm(false)} className={styles.pvCustomBtn}>取消</button>
              <button onClick={handleAddCustom} disabled={!customName.trim() || !customUrl.trim()} className={styles.pvCustomBtnPrimary}>添加</button>
            </div>
          </div>
        ) : (
          <button onClick={() => setShowCustomForm(true)} className={styles.pvAddBtn}>+ 添加自定义供应商</button>
        )}
      </div>

      {/* Right: provider detail */}
      <div className={styles.pvDetail}>
        {!selected ? (
          <div className={styles.pvEmpty}>选择一个供应商</div>
        ) : (
          <>
            <div className={styles.pvDetailHeader}>
              <h4 className={styles.pvDetailTitle}>{selected.label}</h4>
            </div>

            {/* Credentials */}
            <div className={styles.pvCreds}>
              <div className={styles.pvCredRow}>
                <span className={styles.pvCredLabel}>API Key</span>
                <input
                  type="password"
                  value={apiKey}
                  onChange={e => { setApiKey(e.target.value); setVerifyStatus('idle') }}
                  placeholder={verifyStatus === 'ok' ? '● 已保存' : '输入 API Key...'}
                  className={styles.pvCredInput}
                />
                <button
                  onClick={handleSaveKey}
                  disabled={!apiKey.trim()}
                  className={`${styles.pvVerifyBtn} ${verifyStatus === 'ok' ? styles.pvVerifyOk : ''} ${verifyStatus === 'fail' ? styles.pvVerifyFail : ''}`}
                  title="保存并验证"
                >
                  {verifyStatus === 'testing' ? '…' : verifyStatus === 'ok' ? '✓' : verifyStatus === 'fail' ? '✗' : '→'}
                </button>
              </div>
              <div className={styles.pvCredRow}>
                <span className={styles.pvCredLabel}>Base URL</span>
                <input
                  value={baseUrl}
                  onChange={e => setBaseUrl(e.target.value)}
                  placeholder="https://api.example.com/v1"
                  className={styles.pvCredInput}
                />
              </div>
              <div className={styles.pvCredRow}>
                <span className={styles.pvCredLabel}>API 格式</span>
                <Select
                  value={apiFormat}
                  options={[
                    { value: 'openai', label: 'OpenAI 兼容' },
                    { value: 'anthropic', label: 'Anthropic' },
                  ]}
                  onChange={(v) => setApiFormat(v as 'openai' | 'anthropic')}
                />
              </div>
            </div>

            {/* Models */}
            <div className={styles.pvModels}>
              <div className={styles.pvModelsHeader}>
                <span className={styles.pvModelsTitle}>模型 ({providerModels.length})</span>
                <button onClick={handleFetchModels} disabled={discovering || !baseUrl.trim()} className={styles.pvFetchBtn}>
                  {discovering ? '获取中...' : '获取模型'}
                </button>
              </div>

              {/* Discovered */}
              {discovered.filter(m => !configuredModelIds.has(m.id)).length > 0 && (
                <div className={styles.pvModelList}>
                  {discovered.filter(m => !configuredModelIds.has(m.id)).map(m => (
                    <div key={m.id} className={styles.pvDiscoverItem} onClick={() => handleAddModel(m)}>
                      <div className={styles.pvDiscoverMeta}>
                        <span className={styles.pvModelName}>{m.id}</span>
                        {m.context_length != null && (
                          <span className={styles.pvModelCtx}>{(m.context_length / 1000).toFixed(0)}K</span>
                        )}
                      </div>
                      <span className={styles.pvModelBtn}>+ 添加</span>
                    </div>
                  ))}
                </div>
              )}

              {/* Configured */}
              {providerModels.length === 0 && discovered.length === 0 && (
                <p className={styles.pvNoModels}>暂无模型，点击"获取模型"拉取列表</p>
              )}

              <div className={styles.pvModelList}>
                {providerModels.map(m => (
                  <div key={m.name}>
                    <div className={`${styles.pvModelItem} ${m.is_active ? styles.pvModelItemActive : ''}`}>
                      <span className={styles.pvModelName}>{m.name}</span>
                      <div className={styles.pvModelCaps}>
                        {m.capabilities?.vision && <span title="视觉"><IconEye size={12} /></span>}
                        {m.capabilities?.reasoning && <span title="推理"><IconBrain size={12} /></span>}
                        {m.capabilities?.function_calling && <span title="工具"><IconWrench size={12} /></span>}
                      </div>
                      {m.context_size && <span className={styles.pvModelCtx}>{(m.context_size / 1024).toFixed(0)}K</span>}
                      <div className={styles.pvModelActions}>
                        <button onClick={() => handleStartEdit(m)} className={styles.pvModelBtn}>编辑</button>
                        {!m.is_active && <button onClick={() => handleSetActive(m.name)} className={styles.pvModelBtn}>激活</button>}
                        {m.is_active && <span className={`${styles.pvModelBtn} ${styles.pvModelBtnActive}`}>当前</span>}
                        <button onClick={() => handleDeleteModel(m.name)} className={styles.pvModelBtnDanger}>删除</button>
                      </div>
                    </div>
                    {editingModel === m.name && (
                      <div className={styles.pvEditForm}>
                        <div className={styles.pvEditRow}>
                          <label className={styles.pvEditLabel}>名称</label>
                          <input
                            className={styles.pvEditInput}
                            value={editForm.name}
                            onChange={e => setEditForm(f => ({ ...f, name: e.target.value }))}
                            placeholder="显示名称"
                          />
                        </div>
                        <div className={styles.pvEditRow}>
                          <label className={styles.pvEditLabel}>模型 ID</label>
                          <input
                            className={styles.pvEditInput}
                            value={editForm.model}
                            onChange={e => setEditForm(f => ({ ...f, model: e.target.value }))}
                            placeholder="e.g. deepseek-v4-flash"
                          />
                        </div>
                        <div className={styles.pvEditRow}>
                          <label className={styles.pvEditLabel}>Base URL</label>
                          <input
                            className={styles.pvEditInput}
                            value={editForm.base_url}
                            onChange={e => setEditForm(f => ({ ...f, base_url: e.target.value }))}
                            placeholder="https://api.example.com/v1"
                          />
                        </div>
                        <div className={styles.pvEditRowHalf}>
                          <div className={styles.pvEditRow}>
                            <label className={styles.pvEditLabel}>上下文</label>
                            <input
                              className={styles.pvEditInput}
                              type="number"
                              value={editForm.context_size}
                              onChange={e => setEditForm(f => ({ ...f, context_size: parseInt(e.target.value, 10) || 4096 }))}
                            />
                          </div>
                          <div className={styles.pvEditRow}>
                            <label className={styles.pvEditLabel}>最大输出</label>
                            <input
                              className={styles.pvEditInput}
                              type="number"
                              value={editForm.max_output_tokens}
                              onChange={e => setEditForm(f => ({ ...f, max_output_tokens: e.target.value }))}
                              placeholder="默认"
                            />
                          </div>
                        </div>
                        <div className={styles.pvEditRow}>
                          <label className={styles.pvEditLabel}>模型能力</label>
                          <div className={styles.pvCheckboxRow}>
                            <label className={styles.pvCheckboxLabel}>
                              <input
                                type="checkbox"
                                checked={editForm.vision}
                                onChange={e => setEditForm(f => ({ ...f, vision: e.target.checked }))}
                              />
                              视觉
                            </label>
                            <label className={styles.pvCheckboxLabel}>
                              <input
                                type="checkbox"
                                checked={editForm.reasoning}
                                onChange={e => setEditForm(f => ({ ...f, reasoning: e.target.checked }))}
                              />
                              推理
                            </label>
                            <label className={styles.pvCheckboxLabel}>
                              <input
                                type="checkbox"
                                checked={editForm.function_calling}
                                onChange={e => setEditForm(f => ({ ...f, function_calling: e.target.checked }))}
                              />
                              工具调用
                            </label>
                          </div>
                        </div>
                        <div className={styles.pvEditActions}>
                          <button onClick={handleCancelEdit} className={styles.pvEditCancelBtn}>取消</button>
                          <button onClick={handleSaveEdit} disabled={!editForm.name.trim()} className={styles.pvEditSaveBtn}>保存</button>
                        </div>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  )
}

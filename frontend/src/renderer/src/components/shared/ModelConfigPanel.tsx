import { useState, useEffect, useRef } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useStore } from '../../stores'
import { IconEye, IconWrench, IconBrain, IconX, IconSearch } from '../../utils/icons'
import Select from './Select'
import { useLocale } from '../../i18n'
import type { ModelConfig, ModelListItem, ModelBackend } from '../../types/bindings'
import styles from './ModelConfig.module.css'

/** Auto-append the correct API path suffix based on the selected format:
 *  - OpenAI  → ensure /v1
 *  - Anthropic → bare host (backend appends /v1/messages itself)
 */
function normalizeBaseUrl(url: string, apiFormat: 'openai' | 'anthropic'): string {
  let u = url.trim().replace(/\/+$/, '')
  if (!u) return u
  if (apiFormat === 'openai' && !u.endsWith('/v1')) {
    u = u + '/v1'
  }
  return u
}

function getProviderModels(selected: ProviderEntry, models: ModelListItem[]): ModelListItem[] {
  if (selected.backend === 'Custom') return models.filter(m => (m.backend_label || '') === selected.label)
  return models.filter(m => m.backend === selected.backend)
}

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
  { id: 'anthropic', label: 'Anthropic', backend: 'Anthropic', defaultUrl: 'https://api.anthropic.com', apiFormat: 'anthropic' },
  { id: 'openai', label: 'OpenAI', backend: 'OpenAI', defaultUrl: 'https://api.openai.com/v1', apiFormat: 'openai' },
  { id: 'deepseek', label: 'DeepSeek', backend: 'DeepSeek', defaultUrl: 'https://api.deepseek.com/v1', apiFormat: 'openai' },
  { id: 'google', label: 'Google Gemini', backend: 'Custom', defaultUrl: 'https://generativelanguage.googleapis.com/v1beta/openai', apiFormat: 'openai' },
  { id: 'groq', label: 'Groq', backend: 'Custom', defaultUrl: 'https://api.groq.com/openai/v1', apiFormat: 'openai' },
  { id: 'zhipu', label: '智谱 GLM', backend: 'Custom', defaultUrl: 'https://open.bigmodel.cn/api/paas/v4', apiFormat: 'openai' },
  { id: 'moonshot', label: '月之暗面 Kimi', backend: 'Custom', defaultUrl: 'https://api.moonshot.cn/v1', apiFormat: 'openai' },
  { id: 'qwen', label: '通义千问', backend: 'Custom', defaultUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', apiFormat: 'openai' },
  { id: 'siliconflow', label: '硅基流动', backend: 'Custom', defaultUrl: 'https://api.siliconflow.cn/v1', apiFormat: 'openai' },
  { id: 'doubao', label: '豆包 ByteDance', backend: 'Custom', defaultUrl: 'https://ark.cn-beijing.volces.com/api/v3', apiFormat: 'openai' },
  { id: 'lmstudio', label: 'LM Studio', backend: 'LmStudio', defaultUrl: 'http://localhost:1234/v1', apiFormat: 'openai' },
  { id: 'ollama', label: 'Ollama', backend: 'Ollama', defaultUrl: 'http://localhost:11434/v1', apiFormat: 'openai' },
]

const CUSTOM_PROVIDERS_KEY = 'customProviders'

async function loadCustomProviders(): Promise<ProviderEntry[]> {
  try {
    return await window.loom.getPreference<ProviderEntry[]>(CUSTOM_PROVIDERS_KEY, [])
  } catch { return [] }
}

async function saveCustomProviders(entries: ProviderEntry[]): Promise<void> {
  const custom = entries.filter(e => e.isCustom)
  await window.loom.setPreference(CUSTOM_PROVIDERS_KEY, custom)
}

function buildProviders(customProviders: ProviderEntry[], models: ModelListItem[]): ProviderEntry[] {
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
        apiFormat: m.api_format === 'anthropic' ? 'anthropic' : 'openai',
        isCustom: true,
      })
    }
  }
  return [...PRESET_PROVIDERS, ...customProviders, ...discovered]
}

function formatContext(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`
  return `${(n / 1024).toFixed(0)}K`
}

export default function ModelConfigPanel() {
  const { t } = useLocale()
  const [models, setModels] = useState<ModelListItem[]>([])
  const [initialLoading, setInitialLoading] = useState(true)
  const mountedRef = useRef(true)
  const [providers, setProviders] = useState<ProviderEntry[]>(PRESET_PROVIDERS)
  const [selectedId, setSelectedId] = useState<string>('deepseek')
  const [showCustomForm, setShowCustomForm] = useState(false)
  const [editingCustomId, setEditingCustomId] = useState<string | null>(null)
  const [customName, setCustomName] = useState('')
  const [customUrl, setCustomUrl] = useState('')
  const [customFormat, setCustomFormat] = useState<'openai' | 'anthropic'>('openai')
  const [customEnvVar, setCustomEnvVar] = useState('OPENLOOM_API_KEY')

  // Per-provider state
  const [apiKey, setApiKey] = useState('')
  const [baseUrl, setBaseUrl] = useState('https://api.deepseek.com/v1')
  const [apiFormat, setApiFormat] = useState<'openai' | 'anthropic'>('openai')
  const [verifyStatus, setVerifyStatus] = useState<'idle' | 'testing' | 'ok' | 'fail'>('idle')
  const [urlSaveStatus, setUrlSaveStatus] = useState<'idle' | 'saving' | 'ok' | 'fail'>('idle')

  // Discovered
  const [discovered, setDiscovered] = useState<Array<{ id: string; context_length?: number }>>([])
  const [discovering, setDiscovering] = useState(false)

  // Model search
  const [modelQuery, setModelQuery] = useState('')

  // Inline rename
  const [renamingModel, setRenamingModel] = useState<string | null>(null)
  const [renameDraft, setRenameDraft] = useState('')

  // Edit state
  const [editingModel, setEditingModel] = useState<string | null>(null)
  const [editForm, setEditForm] = useState<{
    name: string; model: string; backend: ModelBackend; base_url: string;
    context_size: number; max_output_tokens?: number
    compact_mode: boolean
    backend_label?: string; api_format?: string; api_key_env?: string
    vision: boolean; reasoning: boolean; function_calling: boolean
    input_price?: number; output_price?: number; cache_read_price?: number; cache_write_price?: number
  }>({ name: '', model: '', backend: 'DeepSeek', base_url: '', context_size: 4096, max_output_tokens: undefined, compact_mode: false, vision: false, reasoning: false, function_calling: false, input_price: undefined, output_price: undefined, cache_read_price: undefined, cache_write_price: undefined })

  const selected = providers.find(p => p.id === selectedId)

  const refresh = async () => {
    try {
      const [result, customProviders] = await Promise.all([
        loomRpc<{ models: ModelListItem[]; activeModel: string | null }>('model.list'),
        loadCustomProviders(),
      ])
      if (!mountedRef.current) return
      const items = result.models || []
      setModels(items)
      setProviders(buildProviders(customProviders, items))
      useStore.getState().setModels(items)
      if (result.activeModel) useStore.getState().setCurrentModel(result.activeModel)
    } catch (e) {
      console.error('Failed to list models:', e)
    } finally {
      if (mountedRef.current) setInitialLoading(false)
    }
  }

  useEffect(() => {
    mountedRef.current = true
    refresh()
    return () => { mountedRef.current = false }
  }, [])

  const [keyAlreadySet, setKeyAlreadySet] = useState(false)

  // Check API key on initial mount for the default provider
  useEffect(() => {
    loomRpc<{ set: boolean }>('model.check_key', { backend: 'DeepSeek' })
      .then(r => { if (r && r.set && mountedRef.current) setKeyAlreadySet(true) })
      .catch(() => {})
  }, [])

  const handleSelect = async (p: ProviderEntry) => {
    setSelectedId(p.id)
    setBaseUrl(normalizeBaseUrl(p.defaultUrl, p.apiFormat))
    setApiFormat(p.apiFormat)
    setApiKey('')
    setVerifyStatus('idle')
    setUrlSaveStatus('idle')
    setDiscovered([])
    setModelQuery('')
    // Override with saved base_url / api_format from existing models
    const existing = getProviderModels(p, models)
    if (existing.length > 0) {
      if (existing[0].base_url) setBaseUrl(existing[0].base_url!)
      const fmt = existing[0].api_format
      if (fmt === 'openai' || fmt === 'anthropic') setApiFormat(fmt)
    }
    try {
      const envName = p.isCustom ? (p.envVar || 'OPENLOOM_API_KEY') : undefined
      const result = await loomRpc<{ set: boolean; env_name: string }>('model.check_key', {
        backend: p.backend,
        api_key_env: envName,
      })
      setKeyAlreadySet(result.set)
    } catch {
      setKeyAlreadySet(false)
    }
  }

  const handleSaveKey = async () => {
    if (!apiKey.trim() || !selected) return
    setVerifyStatus('testing')
    try {
      const result = await loomRpc<{ ok: boolean; env_name: string }>('model.save_key', {
        backend: selected.backend,
        api_key: apiKey.trim(),
        base_url: normalizeBaseUrl(baseUrl, apiFormat),
        api_key_env: selected.isCustom ? selected.envVar : undefined,
        backend_label: selected.backend === 'Custom' ? selected.label : undefined,
      })
      useStore.getState().addToast({ type: 'success', message: t('modelPanel.apiKeySavedMsg', { env: result.env_name }) })
      const envVarName = result.env_name
      const providerModels = getProviderModels(selected, models)
      for (const m of providerModels) {
        if (m.api_key_env !== envVarName) {
          try {
            await loomRpc('model.config.update', {
              name: m.name,
              model: m.model || undefined,
              backend: m.backend as ModelBackend,
              backend_label: m.backend_label || undefined,
              base_url: m.base_url || undefined,
              api_format: m.api_format || undefined,
              api_key_env: envVarName,
              context_size: m.context_size || 4096,
              capabilities: m.capabilities || {},
            })
          } catch { /* best-effort */ }
        }
      }
      setVerifyStatus('ok')
      setApiKey('')
      setKeyAlreadySet(true)
      await refresh()
    } catch {
      setVerifyStatus('fail')
    }
  }

  const handleSaveUrl = async () => {
    if (!selected) return
    setUrlSaveStatus('saving')
    try {
      const providerModels = getProviderModels(selected, models)
      if (providerModels.length > 0) {
        for (const m of providerModels) {
          try {
            await loomRpc('model.config.update', {
              name: m.name,
              model: m.model || undefined,
              backend: m.backend as ModelBackend,
              backend_label: m.backend_label || undefined,
              base_url: normalizeBaseUrl(baseUrl, apiFormat) || undefined,
              api_format: apiFormat,
              api_key_env: m.api_key_env || undefined,
              context_size: m.context_size || 4096,
              capabilities: m.capabilities || {},
            })
          } catch { /* best-effort per model */ }
        }
      } else if (selected.backend === 'Custom') {
        // No models yet — update the provider entry's defaultUrl
        const next = providers.map(p =>
          p.id === selected.id ? { ...p, defaultUrl: baseUrl.trim(), apiFormat } : p
        )
        setProviders(next)
        if (selected.isCustom) await saveCustomProviders(next)
      }
      setUrlSaveStatus('ok')
      useStore.getState().addToast({ type: 'success', message: t('modelPanel.baseUrlSaved') })
      await refresh()
    } catch {
      setUrlSaveStatus('fail')
    }
  }

  const handleFetchModels = async () => {
    if (!selected) return
    setDiscovering(true)
    try {
      const result = await loomRpc<{ models: Array<{ id: string; context_length?: number }> }>('model.discover', {
        backend: selected.backend,
        base_url: normalizeBaseUrl(baseUrl, apiFormat),
        api_format: apiFormat,
        api_key_env: selected.isCustom ? selected.envVar : undefined,
      })
      setDiscovered(result.models || [])
    } catch (e: any) {
      console.error('Failed to discover models:', e)
      useStore.getState().addToast({ type: 'error', message: t('modelPanel.discoverFailed', { message: e.message || e }) })
      setDiscovered([])
    } finally {
      setDiscovering(false)
    }
  }

  const handleAddModel = async (model: { id: string; context_length?: number }) => {
    if (!selected) return
    const modelId = model.id
    const name = modelId.split('/').pop() || modelId
    const ok = await useStore.getState().showConfirm(t('modelPanel.addModelTitle'), t('modelPanel.addModelConfirm', { name }), false)
    if (!ok) return
    const envName = selected.backend === 'Custom'
      ? (selected.envVar || `${(selected.label ?? '').replace(/\s+/g, '_').toUpperCase()}_API_KEY`)
      : `${selected.backend.toUpperCase()}_API_KEY`
    try {
      await rpc('model.config.create', {
        name,
        model: modelId,
        model_type: 'Router',
        backend: selected.backend,
        backend_label: selected.backend === 'Custom' ? selected.label : undefined,
        base_url: normalizeBaseUrl(baseUrl, apiFormat) || null,
        api_key_env: envName,
        api_format: apiFormat,
        context_size: model.context_length || 4096,
      }, t('modelPanel.added', { name }))
      await refresh()
      setDiscovered(prev => prev.filter(m => m.id !== modelId))
    } catch (e: any) {
      console.error('Failed to add model:', e)
    }
  }

  const handleDeleteModel = async (name: string) => {
    const ok = await useStore.getState().showConfirm(t('modelPanel.deleteModelTitle'), t('modelPanel.deleteModelConfirm', { name }), true)
    if (!ok) return
    try {
      await rpc('model.config.delete', { name }, t('modelPanel.deleted', { name }))
      await refresh()
    } catch { /* toast already shown */ }
  }

  const handleSetActive = async (name: string) => {
    try {
      await rpc('model.config.set_active', { name }, t('modelPanel.switchTo', { name }))
      await refresh()
    } catch { /* toast already shown */ }
  }

  const handleStartEdit = (m: ModelListItem) => {
    setEditingModel(m.name)
    setRenamingModel(null)
    setEditForm({
      name: m.name,
      model: m.model || '',
      backend: m.backend as ModelBackend,
      base_url: m.base_url || '',
      context_size: m.context_size || 4096,
      max_output_tokens: undefined,
      backend_label: m.backend_label,
      api_format: m.api_format,
      api_key_env: m.api_key_env || undefined,
      vision: m.capabilities?.vision ?? false,
      reasoning: m.capabilities?.reasoning ?? false,
      function_calling: m.capabilities?.function_calling ?? false,
      compact_mode: m.compact_mode ?? false,
      input_price: m.input_price ?? undefined,
      output_price: m.output_price ?? undefined,
      cache_read_price: m.cache_read_price ?? undefined,
      cache_write_price: m.cache_write_price ?? undefined,
    })
  }

  const handleCancelEdit = () => setEditingModel(null)

  const startRename = (m: ModelListItem) => {
    setRenamingModel(m.name)
    setRenameDraft(m.name)
    setEditingModel(null)
  }

  const submitRename = async (m: ModelListItem) => {
    const newName = renameDraft.trim()
    if (!newName || newName === m.name) { setRenamingModel(null); return }
    try {
      // Delete old + create new (backend doesn't support rename for models)
      await loomRpc('model.config.delete', { name: m.name })
      await loomRpc('model.config.create', {
        name: newName,
        model: m.model || undefined,
        backend: m.backend as ModelBackend,
        backend_label: m.backend_label || undefined,
        base_url: m.base_url || undefined,
        api_format: m.api_format || undefined,
        api_key_env: m.api_key_env || undefined,
        context_size: m.context_size || 4096,
        capabilities: m.capabilities || {},
      })
      await refresh()
    } catch { /* ignore */ }
    setRenamingModel(null)
  }

  const handleSaveEdit = async () => {
    if (!editingModel) return
    try {
      await rpc('model.config.update', {
        name: editForm.name,
        prev_name: editingModel,
        model: editForm.model || undefined,
        backend: editForm.backend,
        base_url: normalizeBaseUrl(editForm.base_url, (editForm.api_format as 'openai' | 'anthropic') || 'openai') || undefined,
        backend_label: editForm.backend_label || undefined,
        api_format: editForm.api_format || undefined,
        api_key_env: editForm.api_key_env || undefined,
        context_size: editForm.context_size,
        max_output_tokens: editForm.max_output_tokens,
        compact_mode: editForm.compact_mode,
        capabilities: {
          vision: editForm.vision,
          reasoning: editForm.reasoning,
          function_calling: editForm.function_calling,
        },
        input_price: editForm.input_price,
        output_price: editForm.output_price,
        cache_read_price: editForm.cache_read_price,
        cache_write_price: editForm.cache_write_price,
      }, t('modelPanel.updated'))
      setEditingModel(null)
      await refresh()
    } catch { /* toast already shown */ }
  }

  const handleDeleteCustom = async (entry: ProviderEntry) => {
    const providerModels = models.filter(m => (m.backend_label || '') === entry.label)
    const detail = providerModels.length > 0
      ? t('modelPanel.deleteProviderModels', { n: providerModels.length })
      : t('modelPanel.deleteProviderNoModels')
    const ok = await useStore.getState().showConfirm(t('modelPanel.deleteProviderTitle'), t('modelPanel.deleteProviderConfirm', { label: entry.label, detail }), true)
    if (!ok) return
    for (const m of providerModels) {
      try { await loomRpc('model.config.delete', { name: m.name }) } catch { /* ignore */ }
    }
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
    await refresh()
  }

  const handleStartEditCustom = (entry: ProviderEntry) => {
    setEditingCustomId(entry.id)
    setEditingModel(null)
    setCustomName(entry.label)
    setCustomUrl(entry.defaultUrl)
    setCustomFormat(entry.apiFormat)
    setCustomEnvVar(entry.envVar || 'OPENLOOM_API_KEY')
    setShowCustomForm(true)
    setSelectedId(entry.id)
    setBaseUrl(entry.defaultUrl)
    setApiFormat(entry.apiFormat)
  }

  const handleAddCustom = async () => {
    if (!customName.trim() || !customUrl.trim()) return

    if (editingCustomId) {
      // Update existing custom provider
      const oldEntry = providers.find(p => p.id === editingCustomId)
      const newLabel = customName.trim()
      const next = providers.map(p =>
        p.id === editingCustomId ? {
          ...p,
          label: newLabel,
          defaultUrl: customUrl.trim(),
          apiFormat: customFormat,
          envVar: customEnvVar.trim() || 'OPENLOOM_API_KEY',
        } : p
      )
      setProviders(next)
      await saveCustomProviders(next)

      // Update backend_label on all models that referenced the old label
      if (oldEntry && oldEntry.label !== newLabel) {
        const affectedModels = models.filter(m => (m.backend_label || '') === oldEntry.label)
        for (const m of affectedModels) {
          try {
            await loomRpc('model.config.update', {
              name: m.name,
              model: m.model || undefined,
              backend: m.backend as ModelBackend,
              backend_label: newLabel,
              base_url: m.base_url || undefined,
              api_format: m.api_format || undefined,
              api_key_env: m.api_key_env || undefined,
              context_size: m.context_size || 4096,
              capabilities: m.capabilities || {},
            })
          } catch { /* best-effort */ }
        }
      }

      setBaseUrl(normalizeBaseUrl(customUrl.trim(), customFormat))
      setApiFormat(customFormat)
      setShowCustomForm(false)
      setEditingCustomId(null)
      setCustomName('')
      setCustomUrl('')
      setCustomFormat('openai')
      setCustomEnvVar('OPENLOOM_API_KEY')
      await refresh()
      useStore.getState().addToast({ type: 'success', message: t('modelPanel.providerUpdated', { label: newLabel }) })
      return
    }

    // Create new custom provider
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
    setApiKey('')
    setVerifyStatus('idle')
    setDiscovered([])
    setShowCustomForm(false)
    setEditingCustomId(null)
    setCustomName('')
    setCustomUrl('')
    setCustomFormat('openai')
    setCustomEnvVar('OPENLOOM_API_KEY')
    try {
      const result = await loomRpc<{ set: boolean; env_name: string }>('model.check_key', {
        backend: 'Custom',
        api_key_env: entry.envVar,
      })
      setKeyAlreadySet(result.set)
    } catch {
      setKeyAlreadySet(false)
    }
    // Auto-fetch models for the new custom provider
    setDiscovering(true)
    try {
      const discResult = await loomRpc<{ models: Array<{ id: string; context_length?: number }> }>('model.discover', {
        backend: entry.backend,
        base_url: entry.defaultUrl,
        api_format: entry.apiFormat,
        api_key_env: entry.envVar,
      })
      setDiscovered(discResult.models || [])
    } catch {
      setDiscovered([])
    } finally {
      setDiscovering(false)
    }
  }

  const providerModels = selected
    ? models.filter(m => {
        if (selected.backend === 'Custom') return (m.backend_label || '') === selected.label
        return m.backend === selected.backend
      })
    : []

  const configuredModelIds = new Set(providerModels.map(m => m.model))

  const getModelCount = (p: ProviderEntry) => {
    if (p.backend === 'Custom') return models.filter(m => (m.backend_label || '') === p.label).length
    return models.filter(m => m.backend === p.backend).length
  }

  // Filtered model lists
  const q = modelQuery.toLowerCase().trim()
  const filteredConfigured = q
    ? providerModels.filter(m => m.name.toLowerCase().includes(q) || (m.model || '').toLowerCase().includes(q))
    : providerModels
  const newDiscovered = discovered.filter(m => !configuredModelIds.has(m.id))
  const filteredDiscovered = q
    ? newDiscovered.filter(m => m.id.toLowerCase().includes(q))
    : newDiscovered

  const envVarName = selected
    ? (selected.backend === 'Custom' ? (selected.envVar || `${(selected.label ?? '').replace(/\s+/g, '_').toUpperCase()}_API_KEY`) : `${selected.backend.toUpperCase()}_API_KEY`)
    : ''

  if (initialLoading) {
    return <div className={styles.pvLayout}><div className={styles.pvEmpty} style={{ padding: 40, textAlign: 'center' }}>{t('common.loading')}</div></div>
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
              className={styles.pvListItem + ' ' + (selectedId === p.id ? styles.pvListItemSelected : '')}
            >
              <button onClick={() => handleSelect(p)} className={styles.pvListItemBtn}>
                <span className={styles.pvStatusDot + ' ' + (hasKey ? styles.pvStatusDotOn : '')} />
                <span className={styles.pvListName}>{p.label}</span>
                {count > 0 && <span className={styles.pvListCount}>{count}</span>}
              </button>
            </div>
          )
        })}

        {providers.some(p => p.isCustom) && (
          <>
            <div className={styles.pvSectionHeader}>{t('modelPanel.customProvider')}</div>
            {providers.filter(p => p.isCustom).map((p) => {
              const count = getModelCount(p)
              const hasKey = count > 0 || (verifyStatus === 'ok' && selectedId === p.id)
              return (
                <div
                  key={p.id}
                  className={styles.pvListItem + ' ' + (selectedId === p.id ? styles.pvListItemSelected : '')}
                >
                  <button onClick={() => handleSelect(p)} className={styles.pvListItemBtn}>
                    <span className={styles.pvStatusDot + ' ' + (hasKey ? styles.pvStatusDotOn : '')} />
                    <span className={styles.pvListName}>{p.label}</span>
                    {count > 0 && <span className={styles.pvListCount}>{count}</span>}
                  </button>
                  <button
                    onClick={(e) => { e.stopPropagation(); handleStartEditCustom(p) }}
                    className={styles.pvListEdit}
                    title={t('modelPanel.editProvider')}
                  >
                    <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                      <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
                    </svg>
                  </button>
                  <button
                    onClick={(e) => { e.stopPropagation(); handleDeleteCustom(p) }}
                    className={styles.pvListDelete}
                    title={t('modelPanel.deleteProvider')}
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
              placeholder={t('modelPanel.providerName')}
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
                { value: 'openai', label: t('modelPanel.openaiFormat') },
                { value: 'anthropic', label: t('modelPanel.anthropicFormat') },
              ]}
              onChange={(v) => setCustomFormat(v as 'openai' | 'anthropic')}
            />
            <input
              value={customEnvVar}
              onChange={e => setCustomEnvVar(e.target.value)}
              placeholder={t('modelPanel.envVarPlaceholder')}
              className={styles.pvCustomInput}
            />
            <div className={styles.pvCustomActions}>
              <button onClick={() => { setShowCustomForm(false); setEditingCustomId(null); setCustomName(''); setCustomUrl(''); setCustomFormat('openai'); setCustomEnvVar('OPENLOOM_API_KEY') }} className={styles.pvCustomBtn}>{t('common.cancel')}</button>
              <button onClick={handleAddCustom} disabled={!customName.trim() || !customUrl.trim()} className={styles.pvCustomBtnPrimary}>{editingCustomId ? t('common.save') : t('common.edit')}</button>
            </div>
          </div>
        ) : (
          <button onClick={() => setShowCustomForm(true)} className={styles.pvAddBtn}>{t('modelPanel.addCustomProvider')}</button>
        )}
      </div>

      {/* Right: provider detail */}
      <div className={styles.pvDetail}>
        {!selected ? (
          <div className={styles.pvEmpty}>{t('modelPanel.selectProvider')}</div>
        ) : (
          <>
            {/* Header */}
            <div className={styles.pvDetailHeader}>
              <h4 className={styles.pvDetailTitle}>{selected.label}</h4>
              <span className={styles.pvEnvVarTag}><code>{envVarName}</code></span>
            </div>

            {/* Credentials card */}
            <div className={styles.pvCredCard}>
              <div className={styles.pvCredRow}>
                <span className={styles.pvCredLabel}>{t('modelPanel.apiKey')}</span>
                <input
                  type="password"
                  value={apiKey}
                  onChange={e => { setApiKey(e.target.value); setVerifyStatus('idle') }}
                  placeholder={verifyStatus === 'ok' ? t('modelPanel.apiKeySaved') : keyAlreadySet ? t('modelPanel.apiKeyMasked') : t('modelPanel.apiKeyPlaceholder')}
                  className={styles.pvCredInput}
                />
                <button
                  onClick={handleSaveKey}
                  disabled={!apiKey.trim()}
                  className={styles.pvSaveBtn + ' ' + (verifyStatus === 'ok' ? styles.pvSaveOk : '') + ' ' + (verifyStatus === 'fail' ? styles.pvSaveFail : '')}
                >
                  {verifyStatus === 'testing' ? '...' : verifyStatus === 'ok' ? t('modelPanel.apiKeySavedBtn') : verifyStatus === 'fail' ? t('modelPanel.apiKeyFailedBtn') : t('common.save')}
                </button>
              </div>
              <div className={styles.pvCredRow}>
                <span className={styles.pvCredLabel}>{t('modelPanel.baseUrl')}</span>
                <input
                  value={baseUrl}
                  onChange={e => { setBaseUrl(e.target.value); setUrlSaveStatus('idle') }}
                  onBlur={() => { const n = normalizeBaseUrl(baseUrl, apiFormat); if (n !== baseUrl) setBaseUrl(n) }}
                  placeholder="https://api.example.com/v1"
                  className={styles.pvCredInput}
                />
                <button
                  onClick={handleSaveUrl}
                  disabled={!baseUrl.trim() || urlSaveStatus === 'saving'}
                  className={styles.pvSaveBtn + ' ' + (urlSaveStatus === 'ok' ? styles.pvSaveOk : '') + ' ' + (urlSaveStatus === 'fail' ? styles.pvSaveFail : '')}
                >
                  {urlSaveStatus === 'saving' ? '...' : urlSaveStatus === 'ok' ? t('modelPanel.baseUrlSavedBtn') : urlSaveStatus === 'fail' ? t('modelPanel.baseUrlFailedBtn') : t('common.save')}
                </button>
              </div>
              <div className={styles.pvCredRow}>
                <span className={styles.pvCredLabel}>{t('modelPanel.apiFormat')}</span>
                <div className={styles.pvToggle}>
                  <button
                    className={styles.pvToggleBtn + ' ' + (apiFormat === 'openai' ? styles.pvToggleActive : '')}
                    onClick={() => { setApiFormat('openai'); setUrlSaveStatus('idle'); setBaseUrl(u => normalizeBaseUrl(u, 'openai')) }}
                  >
                    OpenAI
                  </button>
                  <button
                    className={styles.pvToggleBtn + ' ' + (apiFormat === 'anthropic' ? styles.pvToggleActive : '')}
                    onClick={() => { setApiFormat('anthropic'); setUrlSaveStatus('idle'); setBaseUrl(u => normalizeBaseUrl(u, 'anthropic')) }}
                  >
                    Anthropic
                  </button>
                </div>
              </div>
            </div>

            {/* Models section */}
            <div className={styles.pvModels}>
              {/* Top bar: search + fetch */}
              <div className={styles.pvModelsBar}>
                <div className={styles.pvSearchBox}>
                  <IconSearch size={12} className={styles.pvSearchIcon} />
                  <input
                    value={modelQuery}
                    onChange={e => setModelQuery(e.target.value)}
                    placeholder={t('modelPanel.searchPlaceholder', { total: String(providerModels.length + newDiscovered.length) })}
                    className={styles.pvSearchInput}
                  />
                  {modelQuery && (
                    <button onClick={() => setModelQuery('')} className={styles.pvSearchClear}>
                      <IconX size={10} />
                    </button>
                  )}
                </div>
                <button
                  onClick={handleFetchModels}
                  disabled={discovering}
                  className={styles.pvFetchBtn}
                >
                  {discovering ? t('modelPanel.fetching') : t('modelPanel.fetchModels')}
                </button>
              </div>

              {/* Configured models */}
              {filteredConfigured.length > 0 ? (
                <div className={styles.pvModelSection}>
                  <div className={styles.pvModelSectionHeader}>
                    {t('modelPanel.configured')}
                    <span className={styles.pvModelSectionCount}>{filteredConfigured.length}</span>
                    {q && <span className={styles.pvModelFilterHint}>{t('modelPanel.filterHint', { n: providerModels.length })}</span>}
                  </div>
                  <div className={styles.pvModelList}>
                    {filteredConfigured.map(m => (
                      <div key={m.name}>
                        <div className={styles.pvModelCard + ' ' + (m.is_active ? styles.pvModelCardActive : '')}>
                          <div className={styles.pvModelCardInfo}>
                            {renamingModel === m.name ? (
                              <input
                                autoFocus
                                value={renameDraft}
                                onChange={e => setRenameDraft(e.target.value)}
                                onBlur={() => submitRename(m)}
                                onKeyDown={e => { if (e.key === 'Enter') submitRename(m); if (e.key === 'Escape') setRenamingModel(null) }}
                                className={styles.pvRenameInput}
                              />
                            ) : (
                              <span
                                className={styles.pvModelCardName}
                                title={t('modelPanel.clickToRename')}
                                onDoubleClick={() => startRename(m)}
                              >
                                {m.name}
                              </span>
                            )}
                            <div className={styles.pvModelCardMeta}>
                              {m.model && <span className={styles.pvModelId} title={m.model}>{m.model}</span>}
                              {(m.context_size ?? 0) > 0 && (
                                <span className={styles.pvCtxBadge}>{formatContext(m.context_size!)}</span>
                              )}
                              <div className={styles.pvCapBadges}>
                                {m.capabilities?.vision && (
                                  <span className={styles.pvCapBadge} title={t('modelPanel.vision')}><IconEye size={11} /></span>
                                )}
                                {m.capabilities?.reasoning && (
                                  <span className={styles.pvCapBadge} title={t('modelPanel.reasoning')}><IconBrain size={11} /></span>
                                )}
                                {m.capabilities?.function_calling && (
                                  <span className={styles.pvCapBadge} title={t('modelPanel.toolCalling')}><IconWrench size={11} /></span>
                                )}
                              </div>
                            </div>
                          </div>
                          <div className={styles.pvModelCardActions}>
                            {m.is_active ? (
                              <span className={styles.pvActiveBadge}>{t('modelPanel.current')}</span>
                            ) : (
                              <button onClick={() => handleSetActive(m.name)} className={styles.pvActivateBtn}>{t('modelPanel.activate')}</button>
                            )}
                            <button onClick={() => handleStartEdit(m)} className={styles.pvCardActionBtn} title={t('common.edit')}>
                              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                                <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                                <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
                              </svg>
                            </button>
                            <button onClick={() => handleDeleteModel(m.name)} className={styles.pvCardActionBtn + ' ' + styles.pvCardActionDanger} title={t('common.delete')}>
                              <IconX size={12} />
                            </button>
                          </div>
                        </div>

                        {/* Edit panel */}
                        {editingModel === m.name && (
                          <div className={styles.pvEditPanel}>
                            <div className={styles.pvEditHeader}>
                              <span className={styles.pvEditTitle}>{t('modelPanel.editModelTitle')}</span>
                              <button onClick={handleCancelEdit} className={styles.pvEditClose}>
                                <IconX size={12} />
                              </button>
                            </div>

                            <div className={styles.pvEditSection}>
                              <div className={styles.pvEditSectionTitle}>{t('modelPanel.sectionBasic')}</div>
                              <div className={styles.pvFieldRow}>
                                <div className={styles.pvField}>
                                  <label className={styles.pvEditLabel}>{t('modelPanel.modelName')}</label>
                                  <input
                                    value={editForm.name}
                                    onChange={e => setEditForm(f => ({ ...f, name: e.target.value }))}
                                    className={styles.pvEditInput}
                                    placeholder={t('modelPanel.displayName')}
                                  />
                                </div>
                                <div className={styles.pvField}>
                                  <label className={styles.pvEditLabel}>{t('modelPanel.modelId')}</label>
                                  <input
                                    value={editForm.model}
                                    onChange={e => setEditForm(f => ({ ...f, model: e.target.value }))}
                                    className={styles.pvEditInput}
                                    placeholder="deepseek-chat"
                                  />
                                </div>
                              </div>
                              <div className={styles.pvFieldRow}>
                                <div className={styles.pvField}>
                                  <label className={styles.pvEditLabel}>{t('modelPanel.contextSize')}</label>
                                  <input
                                    type="number"
                                    min="1024"
                                    step="1024"
                                    value={editForm.context_size || ''}
                                    onChange={e => setEditForm(f => ({ ...f, context_size: parseInt(e.target.value) || 4096 }))}
                                    className={styles.pvEditInput}
                                    placeholder="4096"
                                  />
                                </div>
                                <div className={styles.pvField}>
                                  <label className={styles.pvEditLabel}>{t('modelPanel.maxOutputTokens')}</label>
                                  <input
                                    type="number"
                                    min="1"
                                    value={editForm.max_output_tokens ?? ''}
                                    onChange={e => setEditForm(f => ({ ...f, max_output_tokens: parseInt(e.target.value) || undefined }))}
                                    className={styles.pvEditInput}
                                    placeholder={t('modelPanel.defaultPlaceholder')}
                                  />
                                </div>
                              </div>
                              <div className={styles.pvField}>
                                <label className={styles.pvEditLabel}>{t('modelPanel.apiKeyEnv')}</label>
                                <input
                                  value={editForm.api_key_env || ''}
                                  onChange={e => setEditForm(f => ({ ...f, api_key_env: e.target.value }))}
                                  className={styles.pvEditInput}
                                  placeholder={t('modelPanel.autoGenPlaceholder')}
                                />
                              </div>
                            </div>

                            <div className={styles.pvEditSection}>
                              <div className={styles.pvEditSectionTitle}>{t('modelPanel.sectionCapabilities')}</div>
                              <div className={styles.pvCapRow}>
                                <label className={styles.pvCapCheck + ' ' + (editForm.vision ? styles.pvCapCheckActive : '')}>
                                  <input type="checkbox" checked={editForm.vision} onChange={e => setEditForm(f => ({ ...f, vision: e.target.checked }))} />
                                  <IconEye size={12} />
                                  <span>{t('modelPanel.vision')}</span>
                                </label>
                                <label className={styles.pvCapCheck + ' ' + (editForm.reasoning ? styles.pvCapCheckActive : '')}>
                                  <input type="checkbox" checked={editForm.reasoning} onChange={e => setEditForm(f => ({ ...f, reasoning: e.target.checked }))} />
                                  <IconBrain size={12} />
                                  <span>{t('modelPanel.reasoning')}</span>
                                </label>
                                <label className={styles.pvCapCheck + ' ' + (editForm.function_calling ? styles.pvCapCheckActive : '')}>
                                  <input type="checkbox" checked={editForm.function_calling} onChange={e => setEditForm(f => ({ ...f, function_calling: e.target.checked }))} />
                                  <IconWrench size={12} />
                                  <span>{t('modelPanel.toolCalling')}</span>
                                </label>
                                <label className={styles.pvCapCheck + ' ' + (editForm.compact_mode ? styles.pvCapCheckActive : '')}>
                                  <input type="checkbox" checked={editForm.compact_mode} onChange={e => setEditForm(f => ({ ...f, compact_mode: e.target.checked }))} />
                                  <span>{t('modelPanel.compactMode')}</span>
                                </label>
                              </div>
                            </div>

                            <div className={styles.pvEditSection}>
                              <div className={styles.pvEditSectionTitle}>{t('modelPanel.sectionPricing')}</div>
                              <div className={styles.pvPriceGrid}>
                                <div className={styles.pvPriceCell}>
                                  <label className={styles.pvPriceLabel}>{t('modelPanel.inputPriceUncached')}</label>
                                  <input
                                    type="number"
                                    min="0"
                                    step="0.01"
                                    value={editForm.input_price ?? ''}
                                    placeholder="7.26"
                                    onChange={e => setEditForm(f => ({ ...f, input_price: parseFloat(e.target.value) || 0 }))}
                                    className={styles.pvPriceInput}
                                  />
                                </div>
                                <div className={styles.pvPriceCell}>
                                  <label className={styles.pvPriceLabel}>{t('modelPanel.inputPriceCached')}</label>
                                  <input
                                    type="number"
                                    min="0"
                                    step="0.01"
                                    value={editForm.cache_read_price ?? ''}
                                    placeholder="1.45"
                                    onChange={e => setEditForm(f => ({ ...f, cache_read_price: parseFloat(e.target.value) || 0 }))}
                                    className={styles.pvPriceInput}
                                  />
                                </div>
                                <div className={styles.pvPriceCell}>
                                  <label className={styles.pvPriceLabel}>{t('modelPanel.cacheWritePrice')}</label>
                                  <input
                                    type="number"
                                    min="0"
                                    step="0.01"
                                    value={editForm.cache_write_price ?? ''}
                                    placeholder="2.50"
                                    onChange={e => setEditForm(f => ({ ...f, cache_write_price: parseFloat(e.target.value) || 0 }))}
                                    className={styles.pvPriceInput}
                                  />
                                </div>
                                <div className={styles.pvPriceCell}>
                                  <label className={styles.pvPriceLabel}>{t('modelPanel.outputPrice')}</label>
                                  <input
                                    type="number"
                                    min="0"
                                    step="0.01"
                                    value={editForm.output_price ?? ''}
                                    placeholder="10.00"
                                    onChange={e => setEditForm(f => ({ ...f, output_price: parseFloat(e.target.value) || 0 }))}
                                    className={styles.pvPriceInput}
                                  />
                                </div>
                              </div>
                            </div>

                            <div className={styles.pvEditActions}>
                              <button onClick={handleCancelEdit} className={styles.pvEditCancelBtn}>{t('common.cancel')}</button>
                              <button onClick={handleSaveEdit} disabled={!editForm.name.trim()} className={styles.pvEditSaveBtn}>{t('common.save')}</button>
                            </div>
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              ) : (
                <div className={styles.pvModelSection}>
                  <div className={styles.pvModelEmpty}>{q ? t('modelPanel.noMatching') : t('modelPanel.noModels')}</div>
                </div>
              )}

              {/* Discovered models */}
              {filteredDiscovered.length > 0 && (
                <div className={styles.pvModelSection}>
                  <div className={styles.pvModelSectionHeader}>
                    {t('modelPanel.addable')}
                    <span className={styles.pvModelSectionCount}>{filteredDiscovered.length}</span>
                    {q && <span className={styles.pvModelFilterHint}>{t('modelPanel.filterHint', { n: newDiscovered.length })}</span>}
                  </div>
                  <div className={styles.pvModelList}>
                    {filteredDiscovered.map(m => (
                      <div key={m.id} className={styles.pvDiscoverItem} onClick={() => handleAddModel(m)}>
                        <div className={styles.pvDiscoverMeta}>
                          <span className={styles.pvDiscoverName}>{m.id}</span>
                          {m.context_length != null && (
                            <span className={styles.pvModelCtx}>{formatContext(m.context_length)}</span>
                          )}
                          <div className={styles.pvCapBadges}>
                            {m.capabilities?.vision && <span className={styles.pvCapBadge} title={t('modelPanel.vision')}><IconEye size={11} /></span>}
                            {m.capabilities?.reasoning && <span className={styles.pvCapBadge} title={t('modelPanel.reasoning')}><IconBrain size={11} /></span>}
                            {m.capabilities?.function_calling && <span className={styles.pvCapBadge} title={t('modelPanel.toolCalling')}><IconWrench size={11} /></span>}
                          </div>
                        </div>
                        <span className={styles.pvAddModelBtn}>{t('modelPanel.addModel')}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  )
}

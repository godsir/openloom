import { useState, useEffect, useMemo } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useStore } from '../../stores'
import { IconEye, IconWrench, IconBrain, IconX, IconSearch } from '../../utils/icons'
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

// Pricing reference: model name substring → { input, output } USD per 1M tokens.
// Matched case-insensitively against the configured model's name or model id.
// 2026-05 定价 (¥ / 百万 Token).  国际模型按 ~7.2 汇率折算.
const PRICING_DB: Array<{ key: string; input: number; output: number; cacheRead?: number; cacheWrite?: number }> = [
  // OpenAI — USD 按 7.2 折算
  { key: 'gpt-4.1', input: 14.40, output: 57.60 },
  { key: 'gpt-4.1-mini', input: 2.88, output: 11.52 },
  { key: 'gpt-4.1-nano', input: 0.72, output: 2.88 },
  { key: 'gpt-4o', input: 18.00, output: 72.00 },
  { key: 'gpt-4o-mini', input: 1.08, output: 4.32 },
  { key: 'o4-mini', input: 7.92, output: 31.68 },
  { key: 'o3', input: 72.00, output: 288.00 },
  { key: 'o3-mini', input: 7.92, output: 31.68 },
  // Anthropic — USD 按 7.2 折算
  { key: 'claude-opus-4.6', input: 36.00, output: 180.00, cacheWrite: 9.00, cacheRead: 3.60 },
  { key: 'claude-opus-4.5', input: 36.00, output: 180.00, cacheWrite: 9.00, cacheRead: 3.60 },
  { key: 'claude-opus-4', input: 36.00, output: 180.00, cacheWrite: 9.00, cacheRead: 3.60 },
  { key: 'claude-sonnet-4.6', input: 21.60, output: 108.00, cacheWrite: 5.40, cacheRead: 2.16 },
  { key: 'claude-sonnet-4.5', input: 21.60, output: 108.00, cacheWrite: 5.40, cacheRead: 2.16 },
  { key: 'claude-sonnet-4', input: 21.60, output: 108.00, cacheWrite: 5.40, cacheRead: 2.16 },
  { key: 'claude-haiku-4.5', input: 7.20, output: 36.00, cacheWrite: 1.80, cacheRead: 0.72 },
  { key: 'claude-haiku-4', input: 7.20, output: 36.00, cacheWrite: 1.80, cacheRead: 0.72 },
  { key: 'claude-3.5', input: 21.60, output: 108.00, cacheWrite: 5.40, cacheRead: 2.16 },
  { key: 'claude-opus', input: 36.00, output: 180.00, cacheWrite: 9.00, cacheRead: 3.60 },
  { key: 'claude-sonnet', input: 21.60, output: 108.00, cacheWrite: 5.40, cacheRead: 2.16 },
  { key: 'claude-haiku', input: 7.20, output: 36.00, cacheWrite: 1.80, cacheRead: 0.72 },
  // DeepSeek — 官方 ¥ 定价 (deepseek.com)
  { key: 'deepseek-v4', input: 2.00, output: 3.50, cacheRead: 0.20, cacheWrite: 0.20 },
  { key: 'deepseek-v3', input: 2.00, output: 8.00, cacheRead: 0.50, cacheWrite: 0.50 },
  { key: 'deepseek-r1', input: 4.00, output: 16.00, cacheRead: 1.00, cacheWrite: 1.00 },
  { key: 'deepseek-chat', input: 2.00, output: 8.00, cacheRead: 0.50, cacheWrite: 0.50 },
  { key: 'deepseek-reasoner', input: 4.00, output: 16.00, cacheRead: 1.00, cacheWrite: 1.00 },
  // Qwen (通义千问百炼) — 官方 ¥ 定价
  { key: 'qwen3-vl-plus', input: 1.50, output: 6.00 },
  { key: 'qwen3-vl-flash', input: 0.00, output: 0.00 },
  { key: 'qwen3-vl', input: 0.80, output: 3.20 },
  { key: 'qwen3-plus', input: 2.00, output: 8.00 },
  { key: 'qwen3-flash', input: 0.00, output: 0.00 },
  { key: 'qwen3-235b', input: 1.50, output: 6.00 },
  { key: 'qwen3-coder', input: 2.00, output: 8.00 },
  { key: 'qwen-plus', input: 2.00, output: 8.00 },
  { key: 'qwen-turbo', input: 0.80, output: 3.20 },
  { key: 'qwen-vl', input: 1.50, output: 6.00 },
  // Google — USD 按 7.2 折算
  { key: 'gemini-2.5-pro', input: 9.00, output: 72.00 },
  { key: 'gemini-2.5-flash', input: 1.08, output: 4.32 },
  { key: 'gemma-4', input: 0.00, output: 0.00 },
  { key: 'gemma-3', input: 0.00, output: 0.00 },
]

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
        apiFormat: (m as any).api_format === 'anthropic' ? 'anthropic' : 'openai',
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
    context_size: number; max_output_tokens: string
    backend_label?: string; api_format?: string; api_key_env?: string
    vision: boolean; reasoning: boolean; function_calling: boolean
    input_price?: number; output_price?: number; cache_read_price?: number; cache_write_price?: number
  }>({ name: '', model: '', backend: 'DeepSeek', base_url: '', context_size: 4096, max_output_tokens: '', vision: false, reasoning: false, function_calling: false, input_price: undefined, output_price: undefined, cache_read_price: undefined, cache_write_price: undefined })

  const selected = providers.find(p => p.id === selectedId)

  const refresh = async () => {
    try {
      const [result, customProviders] = await Promise.all([
        loomRpc<{ models: ModelListItem[]; activeModel: string | null }>('model.list'),
        loadCustomProviders(),
      ])
      const items = result.models || []
      setModels(items)
      setProviders(buildProviders(customProviders, items))
      useStore.getState().setModels(items)
      if (result.activeModel) useStore.getState().setCurrentModel(result.activeModel)
    } catch (e) {
      console.error('Failed to list models:', e)
    }
  }

  useEffect(() => { refresh() }, [])

  const [keyAlreadySet, setKeyAlreadySet] = useState(false)

  const handleSelect = async (p: ProviderEntry) => {
    setSelectedId(p.id)
    setBaseUrl(p.defaultUrl)
    setApiFormat(p.apiFormat)
    setApiKey('')
    setVerifyStatus('idle')
    setUrlSaveStatus('idle')
    setDiscovered([])
    setModelQuery('')
    // Override with saved base_url / api_format from existing models
    const existing = models.filter(m => {
      if (p.isCustom) return (m.backend_label || '') === p.label
      return m.backend === p.backend
    })
    if (existing.length > 0) {
      if (existing[0].base_url) setBaseUrl(existing[0].base_url!)
      const fmt = (existing[0] as any).api_format
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
        base_url: baseUrl.trim(),
        api_key_env: selected.isCustom ? selected.envVar : undefined,
      })
      const { useStore } = await import('../../stores')
      useStore.getState().addToast({ type: 'success', message: `API Key 已保存 (${result.env_name})` })
      const envVarName = result.env_name
      const providerModels = models.filter(m => {
        if (selected.isCustom) return (m.backend_label || '') === selected.label
        return m.backend === selected.backend
      })
      for (const m of providerModels) {
        if (m.api_key_env !== envVarName) {
          try {
            await loomRpc('model.config.update', {
              name: m.name,
              model: m.model || undefined,
              backend: m.backend as ModelBackend,
              backend_label: (m as any).backend_label || undefined,
              base_url: m.base_url || undefined,
              api_format: (m as any).api_format || undefined,
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
      const providerModels = models.filter(m => {
        if (selected.isCustom) return (m.backend_label || '') === selected.label
        return m.backend === selected.backend
      })
      if (providerModels.length > 0) {
        for (const m of providerModels) {
          try {
            await loomRpc('model.config.update', {
              name: m.name,
              model: m.model || undefined,
              backend: m.backend as ModelBackend,
              backend_label: (m as any).backend_label || undefined,
              base_url: baseUrl.trim() || undefined,
              api_format: apiFormat,
              api_key_env: m.api_key_env || undefined,
              context_size: m.context_size || 4096,
              capabilities: m.capabilities || {},
            })
          } catch { /* best-effort per model */ }
        }
      } else if (selected.isCustom) {
        // No models yet — update the custom provider entry's defaultUrl
        const next = providers.map(p =>
          p.id === selected.id ? { ...p, defaultUrl: baseUrl.trim(), apiFormat } : p
        )
        setProviders(next)
        await saveCustomProviders(next)
      }
      setUrlSaveStatus('ok')
      useStore.getState().addToast({ type: 'success', message: 'Base URL 已保存' })
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

  const [fillingPrices, setFillingPrices] = useState(false)

  const handleAutoFillPrices = async () => {
    setFillingPrices(true)
    useStore.getState().addToast({ type: 'info', message: '正在匹配模型计费信息...' })
    let updated = 0
    let skipped = 0
    for (const m of models) {
      const existingInput = (m as any).input_price as number | undefined
      const existingOutput = (m as any).output_price as number | undefined
      if ((existingInput ?? 0) > 0 || (existingOutput ?? 0) > 0) { skipped++; continue }

      const searchStr = ((m.model || '') + ' ' + m.name).toLowerCase()
      const match = PRICING_DB.find(p => searchStr.includes(p.key.toLowerCase()))
      if (!match) continue

      try {
        await loomRpc('model.config.update', {
          name: m.name,
          prev_name: m.name,
          input_price: match.input,
          output_price: match.output,
          cache_read_price: match.cacheRead ?? 0,
          cache_write_price: match.cacheWrite ?? 0,
        })
        updated++
      } catch (e: any) {
        console.warn(`[auto-price] failed for ${m.name}:`, e.message || e)
      }
    }
    setFillingPrices(false)
    if (updated > 0) {
      const msg = `已为 ${updated} 个模型补充计费信息` + (skipped > 0 ? `，${skipped} 个已有价格跳过` : '')
      useStore.getState().addToast({ type: 'success', message: msg })
    } else if (skipped > 0) {
      useStore.getState().addToast({ type: 'info', message: `所有 ${skipped} 个模型的计费信息已设置` })
    } else {
      useStore.getState().addToast({ type: 'info', message: '未匹配到可补充的模型，请检查模型名称是否在价格表中' })
    }
    await refresh()
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
    setRenamingModel(null)
    setEditForm({
      name: m.name,
      model: m.model || '',
      backend: m.backend as ModelBackend,
      base_url: m.base_url || '',
      context_size: m.context_size || 4096,
      max_output_tokens: '',
      backend_label: (m as any).backend_label,
      api_format: (m as any).api_format,
      api_key_env: m.api_key_env || undefined,
      vision: m.capabilities?.vision ?? false,
      reasoning: m.capabilities?.reasoning ?? false,
      function_calling: m.capabilities?.function_calling ?? false,
      input_price: m.input_price ?? undefined,
      output_price: m.output_price ?? undefined,
      cache_read_price: (m as any).cache_read_price ?? undefined,
      cache_write_price: (m as any).cache_write_price ?? undefined,
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
        backend_label: (m as any).backend_label || undefined,
        base_url: m.base_url || undefined,
        api_format: (m as any).api_format || undefined,
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
        base_url: editForm.base_url.trim() || undefined,
        backend_label: editForm.backend_label || undefined,
        api_format: editForm.api_format || undefined,
        api_key_env: editForm.api_key_env || undefined,
        context_size: editForm.context_size,
        max_output_tokens: editForm.max_output_tokens ? parseInt(editForm.max_output_tokens, 10) : undefined,
        capabilities: {
          vision: editForm.vision,
          reasoning: editForm.reasoning,
          function_calling: editForm.function_calling,
        },
        input_price: editForm.input_price,
        output_price: editForm.output_price,
        cache_read_price: editForm.cache_read_price,
        cache_write_price: editForm.cache_write_price,
      }, '模型已更新')
      setEditingModel(null)
      await refresh()
    } catch { /* toast already shown */ }
  }

  const handleDeleteCustom = async (entry: ProviderEntry) => {
    const providerModels = models.filter(m => (m.backend_label || '') === entry.label)
    const detail = providerModels.length > 0
      ? `已配置的 ${providerModels.length} 个模型也将一并删除。`
      : '该供应商下暂无模型。'
    const ok = await useStore.getState().showConfirm('删除供应商', `确定删除供应商 "${entry.label}"？${detail}`, true)
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
    setApiKey('')
    setVerifyStatus('idle')
    setDiscovered([])
    setShowCustomForm(false)
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
        if (selected.isCustom) return (m.backend_label || '') === selected.label
        return m.backend === selected.backend
      })
    : []

  const configuredModelIds = new Set(providerModels.map(m => m.model))

  const getModelCount = (p: ProviderEntry) => {
    if (p.isCustom) return models.filter(m => (m.backend_label || '') === p.label).length
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
    ? (selected.isCustom ? (selected.envVar || 'OPENLOOM_API_KEY') : `${selected.backend.toUpperCase()}_API_KEY`)
    : ''

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
            {/* Header */}
            <div className={styles.pvDetailHeader}>
              <h4 className={styles.pvDetailTitle}>{selected.label}</h4>
              <span className={styles.pvEnvVarTag}><code>{envVarName}</code></span>
            </div>

            {/* Credentials card */}
            <div className={styles.pvCredCard}>
              <div className={styles.pvCredRow}>
                <span className={styles.pvCredLabel}>API Key</span>
                <input
                  type="password"
                  value={apiKey}
                  onChange={e => { setApiKey(e.target.value); setVerifyStatus('idle') }}
                  placeholder={verifyStatus === 'ok' ? '● 已保存' : keyAlreadySet ? '●●●● 环境变量已配置' : '输入 API Key...'}
                  className={styles.pvCredInput}
                />
                <button
                  onClick={handleSaveKey}
                  disabled={!apiKey.trim()}
                  className={`${styles.pvSaveBtn} ${verifyStatus === 'ok' ? styles.pvSaveOk : ''} ${verifyStatus === 'fail' ? styles.pvSaveFail : ''}`}
                >
                  {verifyStatus === 'testing' ? '…' : verifyStatus === 'ok' ? '✓ 已保存' : verifyStatus === 'fail' ? '✗ 失败' : '保存'}
                </button>
              </div>
              <div className={styles.pvCredRow}>
                <span className={styles.pvCredLabel}>Base URL</span>
                <input
                  value={baseUrl}
                  onChange={e => { setBaseUrl(e.target.value); setUrlSaveStatus('idle') }}
                  placeholder="https://api.example.com/v1"
                  className={styles.pvCredInput}
                />
                <button
                  onClick={handleSaveUrl}
                  disabled={!baseUrl.trim() || urlSaveStatus === 'saving'}
                  className={`${styles.pvSaveBtn} ${urlSaveStatus === 'ok' ? styles.pvSaveOk : ''} ${urlSaveStatus === 'fail' ? styles.pvSaveFail : ''}`}
                >
                  {urlSaveStatus === 'saving' ? '...' : urlSaveStatus === 'ok' ? '✓ 已保存' : urlSaveStatus === 'fail' ? '✗ 失败' : '保存'}
                </button>
              </div>
              <div className={styles.pvCredRow}>
                <span className={styles.pvCredLabel}>API 格式</span>
                <div className={styles.pvToggle}>
                  <button
                    className={`${styles.pvToggleBtn} ${apiFormat === 'openai' ? styles.pvToggleActive : ''}`}
                    onClick={() => { setApiFormat('openai'); setUrlSaveStatus('idle') }}
                  >
                    OpenAI
                  </button>
                  <button
                    className={`${styles.pvToggleBtn} ${apiFormat === 'anthropic' ? styles.pvToggleActive : ''}`}
                    onClick={() => { setApiFormat('anthropic'); setUrlSaveStatus('idle') }}
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
                    placeholder={`搜索模型 (${providerModels.length + newDiscovered.length} 个)...`}
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
                  disabled={discovering || !baseUrl.trim()}
                  className={styles.pvFetchBtn}
                >
                  {discovering ? '获取中...' : '获取模型'}
                </button>
              </div>

              {/* Empty state */}
              {filteredConfigured.length === 0 && filteredDiscovered.length === 0 && (
                <p className={styles.pvNoModels}>
                  {q ? '无匹配模型' : '暂无模型，点击"获取模型"拉取列表'}
                </p>
              )}

              {/* Configured models */}
              {filteredConfigured.length > 0 && (
                <div className={styles.pvModelSection}>
                  <div className={styles.pvModelSectionHeader}>
                    <span>已配置 ({filteredConfigured.length}){q && <span className={styles.pvModelFilterHint}> — 共 {providerModels.length} 个</span>}</span>
                    <button
                      onClick={handleAutoFillPrices}
                      disabled={fillingPrices}
                      className={styles.pvAutoPriceBtn}
                      title="根据内置价格表自动为模型填充计费信息"
                    >
                      {fillingPrices ? '填充中...' : '一键补充计费'}
                    </button>
                  </div>
                  <div className={styles.pvModelList}>
                    {filteredConfigured.map(m => (
                      <div key={m.name}>
                        <div className={`${styles.pvModelItem} ${m.is_active ? styles.pvModelItemActive : ''}`}>
                          <div className={styles.pvModelMain}>
                            {renamingModel === m.name ? (
                              <input
                                className={styles.pvRenameInput}
                                value={renameDraft}
                                onChange={e => setRenameDraft(e.target.value)}
                                onKeyDown={e => { if (e.key === 'Enter') submitRename(m); if (e.key === 'Escape') setRenamingModel(null) }}
                                onBlur={() => submitRename(m)}
                                onClick={e => e.stopPropagation()}
                                autoFocus
                              />
                            ) : (
                              <span
                                className={styles.pvModelName}
                                onClick={e => { e.stopPropagation(); startRename(m) }}
                                title="点击重命名"
                              >
                                {m.name}
                              </span>
                            )}
                            {m.context_size ? <span className={styles.pvModelCtx}>{formatContext(m.context_size)}</span> : null}
                            <div className={styles.pvModelCaps}>
                              {m.capabilities?.vision && <span title="视觉"><IconEye size={11} /></span>}
                              {m.capabilities?.reasoning && <span title="推理"><IconBrain size={11} /></span>}
                              {m.capabilities?.function_calling && <span title="工具"><IconWrench size={11} /></span>}
                            </div>
                          </div>
                          <div className={styles.pvModelActions}>
                            <button onClick={() => handleStartEdit(m)} className={styles.pvModelBtn}>编辑</button>
                            {!m.is_active && <button onClick={() => handleSetActive(m.name)} className={styles.pvModelBtn}>激活</button>}
                            {m.is_active && <span className={styles.pvModelActiveBadge}>当前</span>}
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
                            <div className={styles.pvEditRow}>
                              <label className={styles.pvEditLabel}>API Key 环境变量</label>
                              <input
                                className={styles.pvEditInput}
                                value={editForm.api_key_env || ''}
                                onChange={e => setEditForm(f => ({ ...f, api_key_env: e.target.value }))}
                                placeholder={selected?.isCustom ? '如 BAILIAN_API_KEY' : '自动生成（无需填写）'}
                              />
                            </div>
                            <div className={styles.pvEditRowHalf}>
                              <div className={styles.pvEditRow}>
                                <label className={styles.pvEditLabel}>上下文窗口</label>
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
                              <label className={styles.pvEditLabel}>能力</label>
                              <div className={styles.pvCheckboxRow}>
                                <label className={styles.pvCheckboxLabel}>
                                  <input type="checkbox" checked={editForm.vision} onChange={e => setEditForm(f => ({ ...f, vision: e.target.checked }))} />
                                  视觉
                                </label>
                                <label className={styles.pvCheckboxLabel}>
                                  <input type="checkbox" checked={editForm.reasoning} onChange={e => setEditForm(f => ({ ...f, reasoning: e.target.checked }))} />
                                  推理
                                </label>
                                <label className={styles.pvCheckboxLabel}>
                                  <input type="checkbox" checked={editForm.function_calling} onChange={e => setEditForm(f => ({ ...f, function_calling: e.target.checked }))} />
                                  工具调用
                                </label>
                              </div>
                            </div>
                            <div className={styles.pvField}>
                              <label className={styles.pvEditLabel}>Input 价格 (¥/百万 Token)</label>
                              <input
                                type="number"
                                min="0"
                                step="0.01"
                                value={editForm.input_price ?? ''}
                                placeholder="0.00"
                                onChange={e => setEditForm(f => ({ ...f, input_price: parseFloat(e.target.value) || 0 }))}
                                className={styles.pvEditInput}
                              />
                            </div>
                            <div className={styles.pvField}>
                              <label className={styles.pvEditLabel}>Output 价格 (¥/百万 Token)</label>
                              <input
                                type="number"
                                min="0"
                                step="0.01"
                                value={editForm.output_price ?? ''}
                                placeholder="0.00"
                                onChange={e => setEditForm(f => ({ ...f, output_price: parseFloat(e.target.value) || 0 }))}
                                className={styles.pvEditInput}
                              />
                            </div>
                            <div className={styles.pvField}>
                              <label className={styles.pvEditLabel}>Cache Read 价格 (¥/百万 Token)</label>
                              <input
                                type="number"
                                min="0"
                                step="0.01"
                                value={editForm.cache_read_price ?? ''}
                                placeholder="0.00"
                                onChange={e => setEditForm(f => ({ ...f, cache_read_price: parseFloat(e.target.value) || 0 }))}
                                className={styles.pvEditInput}
                              />
                            </div>
                            <div className={styles.pvField}>
                              <label className={styles.pvEditLabel}>Cache Write 价格 (¥/百万 Token)</label>
                              <input
                                type="number"
                                min="0"
                                step="0.01"
                                value={editForm.cache_write_price ?? ''}
                                placeholder="0.00"
                                onChange={e => setEditForm(f => ({ ...f, cache_write_price: parseFloat(e.target.value) || 0 }))}
                                className={styles.pvEditInput}
                              />
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
              )}

              {/* Discovered models */}
              {filteredDiscovered.length > 0 && (
                <div className={styles.pvModelSection}>
                  <div className={styles.pvModelSectionHeader}>
                    可添加 ({filteredDiscovered.length})
                    {q && <span className={styles.pvModelFilterHint}> — 共 {newDiscovered.length} 个</span>}
                  </div>
                  <div className={styles.pvModelList}>
                    {filteredDiscovered.map(m => (
                      <div key={m.id} className={styles.pvDiscoverItem} onClick={() => handleAddModel(m)}>
                        <div className={styles.pvDiscoverMeta}>
                          <span className={styles.pvDiscoverName}>{m.id}</span>
                          {m.context_length != null && (
                            <span className={styles.pvModelCtx}>{formatContext(m.context_length)}</span>
                          )}
                        </div>
                        <span className={styles.pvAddModelBtn}>+ 添加</span>
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

import { useState, useEffect } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import type { ModelConfig, ModelListItem } from '../../types/bindings'
import Button from './Button'
import Overlay from './Overlay'

const BACKENDS: { value: string; label: string }[] = [
  { value: 'LmStudio', label: 'LM Studio' },
  { value: 'Ollama', label: 'Ollama' },
  { value: 'Anthropic', label: 'Anthropic' },
  { value: 'OpenAI', label: 'OpenAI' },
  { value: 'DeepSeek', label: 'DeepSeek' },
]

const DEFAULT_CONFIG: ModelConfig = {
  name: '',
  model: '',
  model_type: 'Router',
  backend: 'LmStudio',
  base_url: '',
  api_key_env: '',
  context_size: 4096,
}

export default function ModelConfigPanel() {
  const [models, setModels] = useState<ModelListItem[]>([])
  const [activeModel, setActiveModel] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [form, setForm] = useState<ModelConfig>({ ...DEFAULT_CONFIG })
  const [loading, setLoading] = useState(false)

  const refresh = async () => {
    try {
      const result = await loomRpc<{ models: ModelListItem[]; activeModel: string | null }>('model.list')
      setModels(result.models || [])
      setActiveModel(result.activeModel)
    } catch (e) {
      console.error('Failed to list models:', e)
    }
  }

  useEffect(() => { refresh() }, [])

  const handleCreate = async () => {
    if (!form.name.trim() || !form.model?.trim()) return
    setLoading(true)
    try {
      await loomRpc('model.config.create', {
        name: form.name.trim(),
        model: form.model.trim(),
        model_type: 'Router',
        backend: form.backend,
        base_url: form.base_url?.trim() || null,
        api_key_env: form.api_key_env?.trim() || null,
        context_size: form.context_size,
      })
      await refresh()
      setCreating(false)
      setForm({ ...DEFAULT_CONFIG })
    } catch (e: any) {
      console.error('Failed to create model config:', e)
    } finally {
      setLoading(false)
    }
  }

  const handleDelete = async (name: string) => {
    if (!confirm(`确定删除模型 "${name}"？`)) return
    try {
      await loomRpc('model.config.delete', { name })
      await refresh()
    } catch (e: any) {
      console.error('Failed to delete model config:', e)
    }
  }

  const handleSetActive = async (name: string) => {
    try {
      await loomRpc('model.config.set_active', { name })
      await refresh()
    } catch (e: any) {
      console.error('Failed to set active model:', e)
    }
  }

  const inputClass = 'w-full bg-[var(--bg-card)] text-[var(--text)] text-sm rounded-[var(--r-sm)] px-3 py-2 outline-none border border-[var(--border)] focus:border-[var(--border-accent)] transition-colors placeholder:text-[var(--text-muted)]'
  const labelClass = 'block text-xs text-[var(--text-muted)] mb-1.5'

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-[var(--text)]">模型配置</h3>
        <Button size="sm" onClick={() => setCreating(true)}>
          + 新建
        </Button>
      </div>

      {models.length === 0 && !creating && (
        <p className="text-sm text-[var(--text-muted)]">暂无模型配置</p>
      )}

      <div className="space-y-1">
        {models.map((m) => (
          <div
            key={m.name}
            className={`flex items-center gap-2 px-3 py-2 rounded-[var(--r-sm)] text-sm border transition-colors ${
              m.is_active
                ? 'bg-[rgba(var(--accent-rgb),.05)] border-[rgba(var(--accent-rgb),.20)]'
                : 'bg-[var(--bg-card)] border-[var(--border)]'
            }`}
          >
            <span className={`w-2 h-2 rounded-full ${m.is_active ? 'bg-[var(--accent)]' : 'bg-[var(--text-muted)]'}`} />
            <span className="flex-1 text-[var(--text-light)]">
              {m.name}
              {m.model && <span className="text-[var(--text-muted)] ml-1.5 font-mono text-xs">({m.model})</span>}
            </span>
            <span className="text-[11px] font-mono px-1.5 py-0.5 rounded-md bg-[rgba(255,255,255,0.04)] text-[var(--text-muted)]">
              {m.backend}
            </span>
            {!m.is_active && (
              <button
                onClick={() => handleSetActive(m.name)}
                className="text-[11px] font-mono text-[var(--accent)] hover:text-[var(--accent)] transition-colors"
              >
                激活
              </button>
            )}
            {m.is_active && (
              <span className="text-[11px] font-mono text-[var(--accent)]">当前</span>
            )}
            <button
              onClick={() => handleDelete(m.name)}
              className="text-[11px] font-mono text-[var(--text-muted)] hover:text-[var(--red)] transition-colors"
            >
              删除
            </button>
          </div>
        ))}
      </div>

      {creating && (
        <Overlay open={creating} onClose={() => setCreating(false)} title="新建模型配置">
          <div className="space-y-3">
            <div>
              <label className={labelClass}>名称 *</label>
              <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="例如: My DeepSeek" className={inputClass} />
            </div>
            <div>
              <label className={labelClass}>Model ID *</label>
              <input value={form.model || ''} onChange={(e) => setForm({ ...form, model: e.target.value })} placeholder="例如: deepseek-v4-flash" className={inputClass} />
            </div>
            <div>
              <label className={labelClass}>后端</label>
              <select
                value={form.backend}
                onChange={(e) => setForm({ ...form, backend: e.target.value as ModelConfig['backend'] })}
                className="w-full bg-[var(--bg-card)] text-[var(--text-light)] text-sm rounded-[var(--r-sm)] px-3 py-2 outline-none border border-[var(--border)] focus:border-[var(--border-accent)] transition-colors"
              >
                {BACKENDS.map((b) => (
                  <option key={b.value} value={b.value}>{b.label}</option>
                ))}
              </select>
            </div>
            <div>
              <label className={labelClass}>Base URL（可选）</label>
              <input value={form.base_url || ''} onChange={(e) => setForm({ ...form, base_url: e.target.value })} placeholder="https://api.deepseek.com/v1" className={inputClass} />
            </div>
            <div>
              <label className={labelClass}>API Key 环境变量（可选）</label>
              <input value={form.api_key_env || ''} onChange={(e) => setForm({ ...form, api_key_env: e.target.value })} placeholder="例如: DEEPSEEK_API_KEY" className={inputClass} />
            </div>
            <div>
              <label className={labelClass}>Context Size</label>
              <input
                type="number"
                value={form.context_size}
                onChange={(e) => setForm({ ...form, context_size: parseInt(e.target.value) || 4096 })}
                className={inputClass}
              />
            </div>
            <div className="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setCreating(false)}>
                取消
              </Button>
              <Button size="sm" variant="primary" onClick={handleCreate} disabled={loading}>
                创建
              </Button>
            </div>
          </div>
        </Overlay>
      )}
    </div>
  )
}

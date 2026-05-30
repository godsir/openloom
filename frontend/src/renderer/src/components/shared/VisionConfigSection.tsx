import { useState, useEffect, useMemo } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import Select from './Select'
import styles from './VisionConfig.module.css'

interface ModelListItem {
  name: string
  model?: string
  backend: string
  backend_label?: string
  capabilities?: { vision?: boolean }
}

const BACKEND_ORDER: Record<string, number> = {
  Anthropic: 0,
  OpenAI: 1,
  DeepSeek: 2,
  LmStudio: 3,
  Ollama: 4,
  Custom: 5,
}

function sortModels(models: ModelListItem[]): ModelListItem[] {
  return [...models].sort((a, b) => {
    const ra = BACKEND_ORDER[a.backend] ?? 99
    const rb = BACKEND_ORDER[b.backend] ?? 99
    if (ra !== rb) return ra - rb
    return a.name.localeCompare(b.name)
  })
}

export default function VisionConfigSection() {
  const [enabled, setEnabled] = useState(false)
  const [visionModel, setVisionModel] = useState('')
  const [models, setModels] = useState<ModelListItem[]>([])

  useEffect(() => {
    Promise.all([
      loomRpc<{ enabled: boolean; model: string | null }>('config.get_vision'),
      loomRpc<{ models: ModelListItem[] }>('model.list'),
    ])
      .then(([visionConfig, modelResult]) => {
        setEnabled(visionConfig.enabled ?? false)
        setVisionModel(visionConfig.model ?? '')
        setModels(modelResult.models || [])
      })
      .catch(() => {})
  }, [])

  const handleToggle = async () => {
    const next = !enabled
    setEnabled(next)
    try {
      await rpc('config.set_vision', { enabled: next, model: visionModel || null }, next ? '视觉辅助已启用' : '视觉辅助已关闭')
    } catch {
      setEnabled(!next)
    }
  }

  const handleModelChange = async (model: string) => {
    setVisionModel(model)
    try {
      await rpc('config.set_vision', { enabled, model: model || null }, '视觉模型已更新')
    } catch { /* toast already shown */ }
  }

  const modelOptions = useMemo(
    () => {
      // Include models that could support vision:
      // 1. Explicitly marked with vision capability
      // 2. From cloud providers with vision APIs (OpenAI, Anthropic, DeepSeek)
      // 3. From local providers (users might configure vision models)
      const visionBackends = new Set(['OpenAI', 'Anthropic', 'DeepSeek'])
      const localBackends = new Set(['LmStudio', 'Ollama', 'Custom'])

      const filtered = models.filter(m =>
        m.capabilities?.vision ||
        visionBackends.has(m.backend) ||
        localBackends.has(m.backend)
      )

      const sorted = sortModels(filtered)

      return [
        { value: '', label: '选择视觉模型...' },
        ...sorted.map(m => ({
          value: m.name,
          label: m.name,
          group: m.backend_label || m.backend,
        })),
      ]
    },
    [models],
  )

  return (
    <div className={styles.visionSection}>
      <div className={styles.visionHeader}>
        <span className={styles.visionTitle}>视觉辅助模型</span>
      </div>
      <p className={styles.visionHint}>
        当主模型不支持图片时，使用单独的视觉模型分析图片内容并注入上下文。适用于所有不具备视觉能力的模型。
      </p>

      <div className={styles.visionToggleRow}>
        <span className={styles.visionToggleLabel}>启用视觉辅助</span>
        <button
          onClick={handleToggle}
          className={`${styles.toggle} ${enabled ? styles.toggleOn : ''}`}
          aria-label="切换视觉辅助"
        />
      </div>

      <div className={styles.visionModelRow}>
        <span className={styles.visionModelLabel}>视觉模型</span>
        <Select
          value={visionModel}
          options={modelOptions}
          onChange={handleModelChange}
          disabled={!enabled}
          className={styles.visionModelSelect}
        />
      </div>
    </div>
  )
}

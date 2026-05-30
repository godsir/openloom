import { useState, useEffect, useMemo } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import Select from './Select'
import styles from './VisionConfig.module.css'

interface AuxiliaryConfig {
  summary_model: string | null
  entity_model: string | null
}

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

export default function AuxiliaryConfigSection() {
  const [models, setModels] = useState<ModelListItem[]>([])
  const [summaryModel, setSummaryModel] = useState('')
  const [entityModel, setEntityModel] = useState('')
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    Promise.all([
      loomRpc<AuxiliaryConfig>('config.get_auxiliary'),
      loomRpc<{ models: ModelListItem[] }>('model.list'),
    ])
      .then(([config, modelResult]) => {
        setSummaryModel(config.summary_model || '')
        setEntityModel(config.entity_model || '')
        setModels(modelResult.models || [])
        setLoading(false)
      })
      .catch(() => setLoading(false))
  }, [])

  const handleSummaryChange = async (value: string) => {
    setSummaryModel(value)
    try {
      await rpc('config.set_auxiliary', {
        summary_model: value || null,
        entity_model: entityModel || null,
      }, '摘要模型已更新')
    } catch { /* toast already shown */ }
  }

  const handleEntityChange = async (value: string) => {
    setEntityModel(value)
    try {
      await rpc('config.set_auxiliary', {
        summary_model: summaryModel || null,
        entity_model: value || null,
      }, '实体提取模型已更新')
    } catch { /* toast already shown */ }
  }

  const modelOptions = useMemo(
    () => {
      const sorted = sortModels(models)
      return [
        { value: '', label: '使用主模型' },
        ...sorted.map(m => ({
          value: m.name,
          label: m.name,
          group: m.backend_label || m.backend,
        })),
      ]
    },
    [models],
  )

  if (loading) {
    return (
      <div className={styles.visionSection}>
        <div className={styles.visionHeader}>
          <span className={styles.visionTitle}>辅助模型配置</span>
        </div>
        <div className={styles.visionHint}>加载中...</div>
      </div>
    )
  }

  return (
    <div className={styles.visionSection}>
      <div className={styles.visionHeader}>
        <span className={styles.visionTitle}>辅助模型配置</span>
      </div>

      <p className={styles.visionHint}>
        为摘要生成和实体提取任务指定独立的模型，可以使用更便宜或更快的模型来降低成本。
        留空则使用当前会话的主模型。
      </p>

      <div className={styles.visionModelRow}>
        <span className={styles.visionModelLabel}>摘要模型</span>
        <Select
          value={summaryModel}
          options={modelOptions}
          onChange={handleSummaryChange}
          className={styles.visionModelSelect}
        />
      </div>

      <div className={styles.visionModelRow} style={{ marginTop: '10px' }}>
        <span className={styles.visionModelLabel}>实体提取</span>
        <Select
          value={entityModel}
          options={modelOptions}
          onChange={handleEntityChange}
          className={styles.visionModelSelect}
        />
      </div>
    </div>
  )
}

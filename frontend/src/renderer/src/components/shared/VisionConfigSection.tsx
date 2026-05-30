import { useState, useEffect, useMemo } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import Select from './Select'
import styles from './VisionConfig.module.css'

export default function VisionConfigSection() {
  const models = useStore((s) => s.models)
  const [enabled, setEnabled] = useState(false)
  const [visionModel, setVisionModel] = useState('')

  useEffect(() => {
    loomRpc<{ enabled: boolean; model: string | null }>('config.get_vision')
      .then(result => {
        setEnabled(result.enabled ?? false)
        setVisionModel(result.model ?? '')
      })
      .catch(() => {})

    // Populate global store on mount if empty (ModelConfigPanel may not have loaded yet)
    if (useStore.getState().models.length === 0) {
      loomRpc<{ models: ModelListItem[]; activeModel: string | null }>('model.list')
        .then(result => {
          useStore.getState().setModels(result.models || [])
          if (result.activeModel) useStore.getState().setCurrentModel(result.activeModel)
        })
        .catch(() => {})
    }
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
    () => [
      { value: '', label: '选择视觉模型...' },
      ...models
        .filter(m => m.capabilities?.vision || m.backend === 'OpenAI' || m.backend === 'Anthropic')
        .map(m => ({ value: m.name, label: m.name, group: m.backend_label || m.backend })),
    ],
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
        />
      </div>
    </div>
  )
}

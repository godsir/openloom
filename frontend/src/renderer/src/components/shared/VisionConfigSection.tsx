import { useState, useEffect, useMemo } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
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
  const { t } = useLocale()
  const [enabled, setEnabled] = useState(false)
  const [visionModel, setVisionModel] = useState('')
  // Subscribe to the global model list kept in sync by ModelConfigPanel
  const models = useStore(s => s.models) as ModelListItem[]

  useEffect(() => {
    loomRpc<{ enabled: boolean; model: string | null }>('config.get_vision')
      .then(vc => {
        setEnabled(vc.enabled ?? false)
        setVisionModel(vc.model ?? '')
      })
      .catch(() => {})
  }, [])

  const handleToggle = async () => {
    const next = !enabled
    setEnabled(next)
    try {
      await rpc('config.set_vision', { enabled: next, model: visionModel || null }, next ? t('vision.enabled') : t('vision.disabled'))
    } catch {
      setEnabled(!next)
    }
  }

  const handleModelChange = async (model: string) => {
    setVisionModel(model)
    try {
      await rpc('config.set_vision', { enabled, model: model || null }, t('vision.modelUpdated'))
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
        { value: '', label: t('vision.selectModel') },
        ...sorted.map(m => ({
          value: m.name,
          label: m.name,
          group: m.backend_label || m.backend,
        })),
      ]
    },
    [models, t],
  )

  return (
    <div className={styles.visionSection}>
      <div className={styles.visionHeader}>
        <span className={styles.visionTitle}>{t('vision.title')}</span>
      </div>
      <p className={styles.visionHint}>
        {t('vision.hint')}
      </p>

      <div className={styles.visionToggleRow}>
        <span className={styles.visionToggleLabel}>{t('vision.enable')}</span>
        <button
          onClick={handleToggle}
          className={`${styles.toggle} ${enabled ? styles.toggleOn : ''}`}
          aria-label={t('vision.toggle')}
        />
      </div>

      <div className={styles.visionModelRow}>
        <span className={styles.visionModelLabel}>{t('vision.model')}</span>
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

import { useState, useEffect, useMemo } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
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
  const { t } = useLocale()
  const models = useStore(s => s.models) as ModelListItem[]
  const [summaryModel, setSummaryModel] = useState('')
  const [entityModel, setEntityModel] = useState('')
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    loomRpc<AuxiliaryConfig>('config.get_auxiliary')
      .then(config => {
        setSummaryModel(config.summary_model || '')
        setEntityModel(config.entity_model || '')
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
      }, t('models.summaryModelUpdated'))
    } catch { /* toast already shown */ }
  }

  const handleEntityChange = async (value: string) => {
    setEntityModel(value)
    try {
      await rpc('config.set_auxiliary', {
        summary_model: summaryModel || null,
        entity_model: value || null,
      }, t('models.entityModelUpdated'))
    } catch { /* toast already shown */ }
  }

  const modelOptions = useMemo(
    () => {
      const sorted = sortModels(models)
      return [
        { value: '', label: t('models.usePrimaryModel') },
        ...sorted.map(m => ({
          value: m.name,
          label: m.name,
          group: m.backend_label || m.backend,
        })),
      ]
    },
    [models, t],
  )

  if (loading) {
    return (
      <div className={styles.visionSection}>
        <div className={styles.visionHeader}>
          <span className={styles.visionTitle}>{t('models.auxiliaryConfig')}</span>
        </div>
        <div className={styles.visionHint}>{t('common.loading')}</div>
      </div>
    )
  }

  return (
    <div className={styles.visionSection}>
      <div className={styles.visionHeader}>
        <span className={styles.visionTitle}>{t('models.auxiliaryConfig')}</span>
      </div>

      <p className={styles.visionHint}>
        {t('models.auxiliaryHint')}
      </p>

      <div className={styles.visionModelRow}>
        <span className={styles.visionModelLabel}>{t('models.summaryModel')}</span>
        <Select
          value={summaryModel}
          options={modelOptions}
          onChange={handleSummaryChange}
          className={styles.visionModelSelect}
        />
      </div>

      <div className={styles.visionModelRow} style={{ marginTop: '10px' }}>
        <span className={styles.visionModelLabel}>{t('models.entityExtraction')}</span>
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

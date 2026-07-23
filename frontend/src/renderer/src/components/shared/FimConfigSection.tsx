import { useState, useEffect, useMemo } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { invalidateFimCache } from '../../services/completion'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import Select from './Select'
import styles from './VisionConfig.module.css'

interface ModelListItem {
  name: string
  model?: string
  backend: string
  backend_label?: string
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

export default function FimConfigSection() {
  const { t } = useLocale()
  const [fimModel, setFimModel] = useState('')
  const [probing, setProbing] = useState(false)
  const addToast = useStore(s => s.addToast)
  // Subscribe to the global model list kept in sync by ModelConfigPanel
  const models = useStore(s => s.models) as ModelListItem[]

  useEffect(() => {
    loomRpc<{ model: string | null }>('config.get_fim')
      .then(fc => {
        setFimModel(fc.model ?? '')
      })
      .catch(() => {})
  }, [])

  const handleModelChange = async (model: string) => {
    setFimModel(model)
    try {
      await rpc('config.set_fim', { model: model || null }, t('fim.modelUpdated'))
      invalidateFimCache() // Immediately apply the new model to active FIM sessions
    } catch { /* toast already shown */ }
  }

  const modelOptions = useMemo(
    () => {
      // FIM is primarily supported by DeepSeek, but any model could potentially work.
      // Include all models so users can experiment.
      const sorted = sortModels(models.filter(m => m.backend !== 'Anthropic'))

      return [
        { value: '', label: t('fim.selectModel') },
        ...sorted.map(m => ({
          value: m.name,
          label: m.name,
          group: m.backend_label || m.backend,
        })),
      ]
    },
    [models, t],
  )

  const handleProbe = async () => {
    if (!fimModel || probing) return
    setProbing(true)
    try {
      const result = await loomRpc<{ ok: boolean; completion?: string; message?: string }>(
        'completion.fim_probe',
        { model: fimModel },
      )
      if (result.ok) {
        await rpc('config.set_fim', { model: fimModel }, t('fim.probeSuccess'))
        invalidateFimCache()
      } else {
        throw new Error(result.message || t('fim.probeFailed'))
      }
    } catch (error: any) {
      addToast({ type: 'error', message: error?.message || t('fim.probeFailed') })
    } finally {
      setProbing(false)
    }
  }

  return (
    <div className={styles.visionSection}>
      <div className={styles.visionHeader}>
        <span className={styles.visionTitle}>{t('fim.title')}</span>
      </div>
      <p className={styles.visionHint}>
        {t('fim.hint')}
      </p>

      <div className={styles.visionModelRow}>
        <span className={styles.visionModelLabel}>{t('fim.model')}</span>
        <Select
          value={fimModel}
          options={modelOptions}
          onChange={handleModelChange}
          className={styles.visionModelSelect}
        />
        <button
          type="button"
          onClick={handleProbe}
          disabled={!fimModel || probing}
          className={styles.visionTestBtn}
        >
          {probing ? t('fim.probing') : t('fim.probe')}
        </button>
      </div>
    </div>
  )
}

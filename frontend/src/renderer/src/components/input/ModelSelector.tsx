import { useEffect, useMemo } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import Select from '../shared/Select'
import type { ModelListItem } from '../../types/bindings'

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

export default function ModelSelector() {
  const models = useStore((s) => s.models)
  const currentModel = useStore((s) => s.currentModel)
  const { setModels, setCurrentModel } = useStore.getState()

  useEffect(() => {
    loomRpc<{ models: ModelListItem[]; activeModel: string | null }>('model.list')
      .then((result) => {
        if (result.models?.length) {
          setModels(result.models)
          if (result.activeModel) {
            setCurrentModel(result.activeModel)
          }
          if (!currentModel && result.models.length && !result.activeModel) {
            setCurrentModel(result.models[0].name)
          }
        }
      })
      .catch(() => {})
  }, [])

  const options = useMemo(() => {
    const sorted = sortModels(models)
    return sorted.map((m) => ({
      value: m.name,
      label: m.name,
      group: m.backend_label || m.backend,
    }))
  }, [models])

  return (
    <Select
      value={currentModel}
      options={options}
      onChange={(v) => {
        setCurrentModel(v)
        loomRpc('model.switch', { model: v }).catch(() => {})
      }}
      variant="pill"
      menuWidth={220}
    />
  )
}

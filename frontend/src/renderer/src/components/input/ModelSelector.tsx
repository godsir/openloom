import { useEffect, useMemo, useState } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { t } from '../../i18n'
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

// 全局瞬态提示（DynamicIslandCenter 渲染）
function toast(text: string) {
  ;(useStore.getState() as any).showIslandTransient?.(text, 2200)
}

export default function ModelSelector() {
  const models = useStore((s) => s.models)
  const currentModel = useStore((s) => s.currentModel)
  const [modelsLoading, setModelsLoading] = useState(true)
  const { setModels, setCurrentModel } = useStore.getState()

  useEffect(() => {
    setModelsLoading(true)
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
      .catch(() => {
        // 不再静默吞掉：加载失败给出可见反馈（A13）
        toast(t('model.loadFailed'))
      })
      .finally(() => setModelsLoading(false))
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
        loomRpc('model.switch', { model: v }).catch(() => {
          toast(t('model.switchFailed'))
        })
      }}
      variant="pill"
      menuWidth={220}
      ariaLabel={t('input.model')}
      emptyText={t('model.empty')}
      loading={modelsLoading}
    />
  )
}

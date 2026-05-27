import { useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import type { ModelListItem } from '../../types/bindings'

export default function ModelSelector() {
  const models = useStore((s) => s.models)
  const currentModel = useStore((s) => s.currentModel)
  const { setModels, setCurrentModel } = useStore.getState()

  useEffect(() => {
    loomRpc<{ models: ModelListItem[]; activeModel: string | null }>('model.list')
      .then((result) => {
        if (result.models?.length) {
          setModels(result.models.map((m) => m.model || m.name).filter(Boolean))
          if (result.activeModel) {
            const a = result.models.find((m) => m.name === result.activeModel)
            if (a?.model) setCurrentModel(a.model)
          }
          if (!currentModel && result.models.length && !result.activeModel) {
            setCurrentModel(result.models[0].model || result.models[0].name)
          }
        }
      })
      .catch(() => {})
  }, [])

  const displayModel = currentModel || 'deepseek-v4-flash'

  return (
    <select
      value={currentModel || undefined}
      onChange={(e) => {
        setCurrentModel(e.target.value)
        loomRpc('model.switch', { model: e.target.value }).catch(() => {})
      }}
      className="bg-transparent text-[11px] text-[rgba(0,227,199,0.3)] hover:text-[rgba(0,227,199,0.5)] outline-none border-0 cursor-pointer transition-colors appearance-none"
    >
      {models.map((m) => <option key={m} value={m}>{m}</option>)}
    </select>
  )
}

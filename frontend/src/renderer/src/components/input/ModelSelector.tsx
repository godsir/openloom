import { useEffect, useMemo } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import Select from '../shared/Select'
import type { ModelListItem } from '../../types/bindings'

export default function ModelSelector() {
  const models = useStore((s) => s.models)
  const currentModel = useStore((s) => s.currentModel)
  const { setModels, setCurrentModel } = useStore.getState()

  useEffect(() => {
    loomRpc<{ models: ModelListItem[]; activeModel: string | null }>('model.list')
      .then((result) => {
        if (result.models?.length) {
          setModels(result.models.map((m) => m.name).filter(Boolean))
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

  const options = useMemo(() => models.map((m) => ({ value: m, label: m })), [models])

  return (
    <Select
      value={currentModel}
      options={options}
      onChange={(v) => {
        setCurrentModel(v)
        loomRpc('model.switch', { model: v }).catch(() => {})
      }}
      variant="pill"
    />
  )
}

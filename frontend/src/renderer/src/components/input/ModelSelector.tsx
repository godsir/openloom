import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'

export default function ModelSelector() {
  const models = useStore((s) => s.models)
  const currentModel = useStore((s) => s.currentModel)
  const setCurrentModel = useStore((s) => s.setCurrentModel)

  const handleChange = async (model: string) => {
    try {
      await loomRpc('model.switch', { model })
      setCurrentModel(model)
    } catch {
      // ignore - model.switch may be a no-op in current backend
      setCurrentModel(model)
    }
  }

  // Show hardcoded models if backend returned empty
  const displayModels = models.length > 0 ? models : [
    'deepseek-v4-flash',
    'claude-sonnet-4-6',
    'gpt-4o',
  ]

  return (
    <select
      value={currentModel}
      onChange={(e) => handleChange(e.target.value)}
      className="bg-zinc-800 text-zinc-300 text-xs rounded-md px-2 py-1 outline-none focus:ring-1 focus:ring-blue-500/50 border-0 cursor-pointer"
    >
      {displayModels.map((m) => (
        <option key={m} value={m}>{m}</option>
      ))}
    </select>
  )
}

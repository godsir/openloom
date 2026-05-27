import { useState } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import Button from './Button'
import Overlay from './Overlay'

export default function AgentConfigPanel() {
  const agents = useStore((s) => s.agents)
  const [creating, setCreating] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [nameDraft, setNameDraft] = useState('')
  const [personaDraft, setPersonaDraft] = useState('')

  const handleCreate = async () => {
    if (!nameDraft.trim()) return
    try {
      await loomRpc('agent.config.create', {
        name: nameDraft.trim(),
        persona: personaDraft.trim(),
      })
      const result = await loomRpc<{ agents: unknown[] }>('agent.list')
      useStore.getState().setAgents(result.agents as any[] || [])
      setCreating(false)
      setNameDraft('')
      setPersonaDraft('')
    } catch (e: any) {
      console.error('Failed to create agent config:', e)
    }
  }

  const handleDelete = async (name: string) => {
    if (!confirm(`确定删除 Agent 配置 "${name}"？`)) return
    try {
      await loomRpc('agent.config.delete', { name })
      const result = await loomRpc<{ agents: unknown[] }>('agent.list')
      useStore.getState().setAgents(result.agents as any[] || [])
    } catch (e: any) {
      console.error('Failed to delete agent config:', e)
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-zinc-200">Agent 配置</h3>
        <Button size="sm" onClick={() => setCreating(true)}>
          + 新建
        </Button>
      </div>

      {agents.length === 0 && !creating && (
        <p className="text-sm text-zinc-500">暂无 Agent 配置</p>
      )}

      <div className="space-y-1">
        {agents.map((a) => (
          <div
            key={a.id}
            className="flex items-center gap-2 px-3 py-2 bg-zinc-800/50 rounded text-sm"
          >
            <span className="w-2 h-2 rounded-full bg-green-500" />
            <span className="flex-1 text-zinc-300">{a.name}</span>
            <span className="text-xs text-zinc-500">{a.status}</span>
            <button
              onClick={() => handleDelete(a.name || a.id)}
              className="text-xs text-zinc-500 hover:text-red-400"
            >
              删除
            </button>
          </div>
        ))}
      </div>

      {creating && (
        <Overlay open={creating} onClose={() => setCreating(false)} title="新建 Agent 配置">
          <div className="space-y-3">
            <div>
              <label className="block text-xs text-zinc-400 mb-1">名称</label>
              <input
                value={nameDraft}
                onChange={(e) => setNameDraft(e.target.value)}
                placeholder="输入 Agent 名称..."
                className="w-full bg-zinc-800 text-zinc-200 text-sm rounded-lg px-3 py-2 outline-none focus:ring-1 focus:ring-blue-500/50"
              />
            </div>
            <div>
              <label className="block text-xs text-zinc-400 mb-1">
                Persona（可选）
              </label>
              <textarea
                value={personaDraft}
                onChange={(e) => setPersonaDraft(e.target.value)}
                placeholder="描述 Agent 的行为特征..."
                rows={3}
                className="w-full bg-zinc-800 text-zinc-200 text-sm rounded-lg px-3 py-2 outline-none focus:ring-1 focus:ring-blue-500/50 resize-none"
              />
            </div>
            <div className="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setCreating(false)}>
                取消
              </Button>
              <Button size="sm" variant="primary" onClick={handleCreate}>
                创建
              </Button>
            </div>
          </div>
        </Overlay>
      )}
    </div>
  )
}

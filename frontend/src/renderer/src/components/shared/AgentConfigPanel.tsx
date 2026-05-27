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
        <h3 className="text-sm font-semibold text-[var(--text)]">Agent 配置</h3>
        <Button size="sm" onClick={() => setCreating(true)}>
          + 新建
        </Button>
      </div>

      {agents.length === 0 && !creating && (
        <p className="text-sm text-[var(--text-muted)]">暂无 Agent 配置</p>
      )}

      <div className="space-y-1">
        {agents.map((a) => (
          <div
            key={a.id}
            className="flex items-center gap-2 px-3 py-2 bg-[var(--bg-card)] rounded-[var(--r-sm)] text-sm border border-[var(--border)]"
          >
            <span className="w-2 h-2 rounded-full bg-[var(--green)]" />
            <span className="flex-1 text-[var(--text-light)]">{a.name}</span>
            <span className="text-[11px] font-mono text-[var(--text-muted)]">{a.status}</span>
            <button
              onClick={() => handleDelete(a.name || a.id)}
              className="text-[11px] font-mono text-[var(--text-muted)] hover:text-[var(--red)] transition-colors-fast"
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
              <label className="block text-xs text-[var(--text-muted)] mb-1.5">名称</label>
              <input
                value={nameDraft}
                onChange={(e) => setNameDraft(e.target.value)}
                placeholder="输入 Agent 名称..."
                className="w-full bg-[var(--bg-card)] text-[var(--text)] text-sm rounded-[var(--r-sm)] px-3 py-2 outline-none border border-[var(--border)] focus:border-[var(--border-accent)] transition-colors placeholder:text-[var(--text-muted)]"
              />
            </div>
            <div>
              <label className="block text-xs text-[var(--text-muted)] mb-1.5">
                Persona（可选）
              </label>
              <textarea
                value={personaDraft}
                onChange={(e) => setPersonaDraft(e.target.value)}
                placeholder="描述 Agent 的行为特征..."
                rows={3}
                className="w-full bg-[var(--bg-card)] text-[var(--text)] text-sm rounded-[var(--r-sm)] px-3 py-2 outline-none border border-[var(--border)] focus:border-[var(--border-accent)] transition-colors placeholder:text-[var(--text-muted)] resize-none"
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

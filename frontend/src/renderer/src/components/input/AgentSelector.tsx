import { useState, useMemo, useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import Select from '../shared/Select'

export default function AgentSelector() {
  const agents = useStore((s) => s.agents)
  const currentSessionId = useStore((s) => s.currentSessionId)
  const bindingName = useStore((s) => currentSessionId ? s.sessionAgentBindings[currentSessionId] : undefined)
  const [selected, setSelected] = useState(bindingName || '')

  useEffect(() => {
    setSelected(bindingName || '')
  }, [bindingName])

  const validAgents = agents.filter((a) => a.name && a.name !== 'default')
  const options = useMemo(
    () => [
      { value: '', label: '默认 Agent' },
      ...validAgents.map((a) => ({
        value: a.name,
        label: a.name,
        avatar: (a as any).avatar ?? null,
      })),
    ],
    [validAgents],
  )

  const handleChange = (value: string) => {
    setSelected(value)
    const sid = currentSessionId || 'default'
    useStore.getState().setSessionAgentBinding(sid, value || 'default')
    loomRpc('session.bind_agent', {
      session_id: sid,
      agent_config_name: value || 'default',
    }).catch(() => {})
  }

  return <Select value={selected} options={options} onChange={handleChange} variant="pill" menuWidth={180} />
}

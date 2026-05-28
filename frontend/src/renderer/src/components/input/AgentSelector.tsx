import { useState, useMemo } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import Select from '../shared/Select'

export default function AgentSelector() {
  const agents = useStore((s) => s.agents)
  const currentSessionId = useStore((s) => s.currentSessionId)
  const [selected, setSelected] = useState('')

  const validAgents = agents.filter((a) => a.name && a.name !== 'default')
  const options = useMemo(
    () => [
      { value: '', label: '默认 Agent' },
      ...validAgents.map((a) => ({ value: a.name, label: a.name })),
    ],
    [validAgents],
  )

  const handleChange = (value: string) => {
    setSelected(value)
    loomRpc('session.bind_agent', {
      session_id: currentSessionId || 'default',
      agent_config_name: value || 'default',
    }).catch(() => {})
  }

  return <Select value={selected} options={options} onChange={handleChange} variant="pill" menuWidth={180} />
}

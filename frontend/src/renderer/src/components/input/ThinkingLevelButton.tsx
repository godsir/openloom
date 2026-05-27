import { useStore } from '../../stores'
import type { ThinkingLevel } from '../../stores/model'

const LEVELS: { id: ThinkingLevel; label: string }[] = [
  { id: 'off', label: '思考:关' },
  { id: 'auto', label: '思考:自动' },
  { id: 'low', label: '思考:低' },
  { id: 'medium', label: '思考:中' },
  { id: 'high', label: '思考:高' },
]

export default function ThinkingLevelButton() {
  const thinkingLevel = useStore((s) => s.thinkingLevel)
  const setThinkingLevel = useStore((s) => s.setThinkingLevel)

  const cycle = () => {
    const idx = LEVELS.findIndex((l) => l.id === thinkingLevel)
    const next = LEVELS[(idx + 1) % LEVELS.length]
    setThinkingLevel(next.id)
  }

  const label = LEVELS.find((l) => l.id === thinkingLevel)?.label || '思考'

  return (
    <button
      onClick={cycle}
      className="text-xs text-zinc-400 hover:text-zinc-200 px-2 py-1 rounded hover:bg-zinc-800 transition-colors shrink-0"
    >
      {label}
    </button>
  )
}

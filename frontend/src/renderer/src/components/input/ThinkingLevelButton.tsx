import { useStore } from '../../stores'
import { IconLightbulb } from '../../utils/icons'
import type { ThinkingLevel } from '../../stores/model'

const LEVELS: { id: ThinkingLevel; label: string }[] = [
  { id: 'off', label: 'off' },
  { id: 'auto', label: 'auto' },
  { id: 'low', label: 'low' },
  { id: 'medium', label: 'mid' },
  { id: 'high', label: 'high' },
]

export default function ThinkingLevelButton() {
  const level = useStore((s) => s.thinkingLevel)
  const setLevel = useStore((s) => s.setThinkingLevel)
  const label = LEVELS.find((l) => l.id === level)?.label || 'auto'

  return (
    <button
      onClick={() => {
        const idx = LEVELS.findIndex((l) => l.id === level)
        setLevel(LEVELS[(idx + 1) % LEVELS.length].id)
      }}
      className="pill-neutral"
    >
      <IconLightbulb size={12} /> {label}
    </button>
  )
}

import { useStore } from '../../stores'
import type { PermissionMode } from '../../stores/input'

const MODES: { id: PermissionMode; label: string }[] = [
  { id: 'operate', label: '执行' },
  { id: 'ask', label: '询问' },
  { id: 'read_only', label: '只读' },
]

export default function PermissionModeButton() {
  const mode = useStore((s) => s.permissionMode)
  const setPermissionMode = useStore((s) => s.setPermissionMode)

  const cycle = () => {
    const idx = MODES.findIndex((m) => m.id === mode)
    const next = MODES[(idx + 1) % MODES.length]
    setPermissionMode(next.id)
  }

  const colors: Record<string, string> = {
    operate: 'text-green-400',
    ask: 'text-yellow-400',
    read_only: 'text-red-400',
  }

  const label = MODES.find((m) => m.id === mode)?.label || '?'

  return (
    <button
      onClick={cycle}
      className={`text-xs px-2 py-1 rounded hover:bg-zinc-800 transition-colors shrink-0 ${colors[mode]}`}
      title={`权限模式: ${label}`}
    >
      {label}
    </button>
  )
}

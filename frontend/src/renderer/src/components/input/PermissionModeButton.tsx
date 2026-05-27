import { useStore } from '../../stores'
import type { PermissionMode } from '../../stores/input'

const MODES: { id: PermissionMode; label: string }[] = [
  { id: 'operate', label: 'operate' },
  { id: 'ask', label: 'ask' },
  { id: 'read_only', label: 'read_only' },
]

export default function PermissionModeButton() {
  const mode = useStore((s) => s.permissionMode)
  const setMode = useStore((s) => s.setPermissionMode)
  const current = MODES.find((m) => m.id === mode) || MODES[0]

  return (
    <button
      onClick={() => {
        const idx = MODES.findIndex((m) => m.id === mode)
        setMode(MODES[(idx + 1) % MODES.length].id)
      }}
      className="pill"
      title={`权限: ${current.label}`}
    >
      {current.label}
    </button>
  )
}

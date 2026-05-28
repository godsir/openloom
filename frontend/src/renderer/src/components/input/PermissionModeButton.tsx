import { useStore } from '../../stores'
import { IconZap, IconShield, IconEye } from '../../utils/icons'
import type { PermissionMode } from '../../stores/input'

const ICONS: Record<PermissionMode, typeof IconZap> = {
  operate: IconZap,
  ask: IconShield,
  read_only: IconEye,
}

const LABELS: Record<PermissionMode, string> = {
  operate: 'operate',
  ask: 'ask',
  read_only: 'read_only',
}

export default function PermissionModeButton() {
  const mode = useStore((s) => s.permissionMode)
  const setMode = useStore((s) => s.setPermissionMode)
  const keys: PermissionMode[] = ['operate', 'ask', 'read_only']
  const Icon = ICONS[mode] || IconZap

  return (
    <button
      onClick={() => {
        const idx = keys.indexOf(mode)
        setMode(keys[(idx + 1) % keys.length])
      }}
      className="pill"
      title={`权限: ${LABELS[mode]}`}
    >
      <Icon size={12} /> {LABELS[mode]}
    </button>
  )
}

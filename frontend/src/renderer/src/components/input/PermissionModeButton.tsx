import { useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import { IconZap, IconShield, IconEye, IconLightbulb } from '../../utils/icons'
import type { PermissionMode } from '../../stores/input'

const OPTIONS: { id: PermissionMode; icon: typeof IconZap; label: string; desc: string }[] = [
  { id: 'operate', icon: IconZap, label: 'Operate', desc: '自动执行操作' },
  { id: 'ask', icon: IconShield, label: 'Ask', desc: '操作前需确认' },
  { id: 'read_only', icon: IconEye, label: 'Read Only', desc: '仅读取不修改' },
  { id: 'plan', icon: IconLightbulb, label: 'Plan', desc: '先分析规划，再实施' },
]

export default function PermissionModeButton() {
  const mode = useStore((s) => s.permissionMode)
  const setMode = useStore((s) => s.setPermissionMode)
  const open = useStore((s) => s.permissionDrawerOpen)
  const setPermissionDrawerOpen = useStore((s) => s.setPermissionDrawerOpen)
  const closeDrawer = () => setPermissionDrawerOpen(false)
  const current = OPTIONS.find(o => o.id === mode) || OPTIONS[0]
  const Icon = current.icon
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handleClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) closeDrawer()
    }
    document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [open])

  const handleSelect = (id: PermissionMode) => {
    setMode(id)
    closeDrawer()
  }

  return (
    <div ref={ref} style={{ position: 'relative' }}>
      <button
        onClick={() => setPermissionDrawerOpen(!open)}
        className="pill"
        title={`权限: ${current.label}`}
      >
        <Icon size={12} /> {current.label}
      </button>
      {open && (
        <div className="drawer-popover">
          {OPTIONS.map(o => (
            <button
              key={o.id}
              onClick={() => handleSelect(o.id)}
              className={`drawer-item ${mode === o.id ? 'drawer-item-active' : ''}`}
            >
              <o.icon size={14} />
              <div className="drawer-item-text">
                <span className="drawer-item-label">{o.label}</span>
                <span className="drawer-item-desc">{o.desc}</span>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

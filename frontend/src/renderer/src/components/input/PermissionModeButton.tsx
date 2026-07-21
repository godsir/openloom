import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import { IconZap, IconShield, IconEye, IconLightbulb } from '../../utils/icons'
import { useLocale } from '../../i18n'
import { useClickOutside, useMenuKeyboard } from '../shared/menu-hooks'
import type { PermissionMode } from '../../stores/input'

const OPTIONS: { id: PermissionMode; icon: typeof IconZap; label: string; descKey: string }[] = [
  { id: 'operate', icon: IconZap, label: 'Operate', descKey: 'input.autoExecute' },
  { id: 'ask', icon: IconShield, label: 'Ask', descKey: 'input.confirmBefore' },
  { id: 'read_only', icon: IconEye, label: 'Read Only', descKey: 'input.readOnly' },
  { id: 'plan', icon: IconLightbulb, label: 'Plan', descKey: 'input.planFirst' },
]

export default function PermissionModeButton() {
  const { t } = useLocale()
  const mode = useStore((s) => s.permissionMode)
  const setMode = useStore((s) => s.setPermissionMode)
  const open = useStore((s) => s.permissionDrawerOpen)
  const setPermissionDrawerOpen = useStore((s) => s.setPermissionDrawerOpen)
  const closeDrawer = () => setPermissionDrawerOpen(false)
  const current = OPTIONS.find(o => o.id === mode) || OPTIONS[0]
  const Icon = current.icon
  const [activeIndex, setActiveIndex] = useState(0)

  // 打开时把高亮复位到当前模式
  useEffect(() => {
    if (open) setActiveIndex(Math.max(0, OPTIONS.findIndex(o => o.id === mode)))
  }, [open, mode])

  useClickOutside(
    (target) => !!(target as Element).closest?.('[data-permission-drawer-root]'),
    closeDrawer,
    open,
  )

  useMenuKeyboard({
    open,
    itemCount: OPTIONS.length,
    activeIndex,
    setActiveIndex,
    onSelect: (i) => {
      const opt = OPTIONS[i]
      if (opt) {
        setMode(opt.id)
        closeDrawer()
      }
    },
    onClose: closeDrawer,
  })

  const handleSelect = (id: PermissionMode) => {
    setMode(id)
    closeDrawer()
  }

  return (
    <div data-permission-drawer-root style={{ position: 'relative' }}>
      <button
        onClick={() => setPermissionDrawerOpen(!open)}
        className={`pill ${open ? 'pill-open' : ''}`}
        title={t('input.permissionTitle', { mode: current.label })}
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        <Icon size={12} /> {current.label}
      </button>
      {open && (
        <div className="drawer-popover" role="listbox" aria-label={t('input.permissionTitle', { mode: current.label })}>
          {OPTIONS.map((o, i) => (
            <button
              key={o.id}
              role="option"
              aria-selected={mode === o.id}
              onMouseEnter={() => setActiveIndex(i)}
              onClick={() => handleSelect(o.id)}
              className={[
                'drawer-item',
                mode === o.id ? 'drawer-item-active' : '',
                i === activeIndex && mode !== o.id ? 'drawer-item-highlight' : '',
              ].filter(Boolean).join(' ')}
            >
              <o.icon size={14} />
              <div className="drawer-item-text">
                <span className="drawer-item-label">{o.label}</span>
                <span className="drawer-item-desc">{t(o.descKey)}</span>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

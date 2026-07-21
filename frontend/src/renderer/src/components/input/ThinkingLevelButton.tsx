import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import { IconLightbulb } from '../../utils/icons'
import { useLocale } from '../../i18n'
import { useClickOutside, useMenuKeyboard } from '../shared/menu-hooks'
import type { ThinkingLevel } from '../../stores/model'

const OPTIONS: { id: ThinkingLevel; labelKey: string; descKey: string }[] = [
  { id: 'off', labelKey: 'input.thinkingLabelOff', descKey: 'input.thinkingOff' },
  { id: 'auto', labelKey: 'input.thinkingLabelAuto', descKey: 'input.thinkingAuto' },
  { id: 'low', labelKey: 'input.thinkingLabelLow', descKey: 'input.thinkingLow' },
  { id: 'medium', labelKey: 'input.thinkingLabelMedium', descKey: 'input.thinkingMedium' },
  { id: 'high', labelKey: 'input.thinkingLabelHigh', descKey: 'input.thinkingHigh' },
  { id: 'xhigh', labelKey: 'input.thinkingLabelXHigh', descKey: 'input.thinkingXHigh' },
]

export default function ThinkingLevelButton() {
  const { t } = useLocale()
  const level = useStore((s) => s.thinkingLevel)
  const setLevel = useStore((s) => s.setThinkingLevel)
  const open = useStore((s) => s.thinkingDrawerOpen)
  const setOpen = useStore((s) => s.setThinkingDrawerOpen)
  const closeDrawer = () => setOpen && setOpen(false)
  const current = OPTIONS.find(o => o.id === level) || OPTIONS[1]
  const [activeIndex, setActiveIndex] = useState(0)

  // 打开时把高亮复位到当前级别
  useEffect(() => {
    if (open) setActiveIndex(Math.max(0, OPTIONS.findIndex(o => o.id === level)))
  }, [open, level])

  useClickOutside(
    (target) => !!(target as Element).closest?.('[data-thinking-drawer-root]'),
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
        setLevel(opt.id)
        closeDrawer()
      }
    },
    onClose: closeDrawer,
  })

  const handleSelect = (id: ThinkingLevel) => {
    setLevel(id)
    closeDrawer()
  }

  return (
    <div data-thinking-drawer-root style={{ position: 'relative' }}>
      <button
        onClick={() => setOpen(!open)}
        className={`pill-neutral ${open ? 'pill-neutral-open' : ''}`}
        title={t('input.thinkingTitle', { level: t(current.labelKey) })}
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        <IconLightbulb size={12} /> {t(current.labelKey)}
      </button>
      {open && (
        <div className="drawer-popover" role="listbox" aria-label={t('input.thinkingTitle', { level: t(current.labelKey) })}>
          {OPTIONS.map((o, i) => (
            <button
              key={o.id}
              role="option"
              aria-selected={level === o.id}
              onMouseEnter={() => setActiveIndex(i)}
              onClick={() => handleSelect(o.id)}
              className={[
                'drawer-item',
                level === o.id ? 'drawer-item-active' : '',
                i === activeIndex && level !== o.id ? 'drawer-item-highlight' : '',
              ].filter(Boolean).join(' ')}
            >
              <div className="drawer-item-text">
                <span className="drawer-item-label">{t(o.labelKey)}</span>
                <span className="drawer-item-desc">{t(o.descKey)}</span>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

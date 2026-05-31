import { useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import { IconLightbulb } from '../../utils/icons'
import type { ThinkingLevel } from '../../stores/model'

const OPTIONS: { id: ThinkingLevel; label: string; desc: string }[] = [
  { id: 'off', label: 'Off', desc: '不启用思考' },
  { id: 'auto', label: 'Auto', desc: '自动判断' },
  { id: 'low', label: 'Low', desc: '2K tokens' },
  { id: 'medium', label: 'Medium', desc: '8K tokens' },
  { id: 'high', label: 'High', desc: '32K tokens' },
]

export default function ThinkingLevelButton() {
  const level = useStore((s) => s.thinkingLevel)
  const setLevel = useStore((s) => s.setThinkingLevel)
  const open = useStore((s) => s.thinkingDrawerOpen)
  const setOpen = useStore((s) => s.setThinkingDrawerOpen)
  const closeDrawer = () => setOpen && setOpen(false)
  const current = OPTIONS.find(o => o.id === level) || OPTIONS[1]
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handleClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) closeDrawer()
    }
    document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [open])

  const handleSelect = (id: ThinkingLevel) => {
    setLevel(id)
    closeDrawer()
  }

  return (
    <div ref={ref} style={{ position: 'relative' }}>
      <button
        onClick={() => setOpen(!open)}
        className="pill-neutral"
        title={`思考: ${current.label}`}
      >
        <IconLightbulb size={12} /> {current.label}
      </button>
      {open && (
        <div className="drawer-popover">
          {OPTIONS.map(o => (
            <button
              key={o.id}
              onClick={() => handleSelect(o.id)}
              className={`drawer-item ${level === o.id ? 'drawer-item-active' : ''}`}
            >
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

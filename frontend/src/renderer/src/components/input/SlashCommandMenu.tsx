import { useRef, useEffect, useMemo, useState } from 'react'
import styles from './SlashCommandMenu.module.css'

export interface SlashCommand {
  name: string
  description: string
  kind: 'builtin'
  execute?: () => void
}

interface Props {
  query: string
  commands: SlashCommand[]
  onSelect: (cmd: SlashCommand) => void
  onClose: () => void
}

export default function SlashCommandMenu({
  query,
  commands,
  onSelect,
  onClose,
}: Props) {
  const menuRef = useRef<HTMLDivElement>(null)
  const itemRefs = useRef<Map<number, HTMLButtonElement>>(new Map())

  const filtered = useMemo(() => {
    if (!query) return commands
    const q = query.toLowerCase()
    return commands.filter((c) => c.name.toLowerCase().includes(q))
  }, [commands, query])

  const [activeIndex, setActiveIndex] = useState(0)

  useEffect(() => { setActiveIndex(0) }, [filtered.length])

  useEffect(() => {
    const close = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) onClose()
    }
    document.addEventListener('mousedown', close)
    return () => document.removeEventListener('mousedown', close)
  }, [onClose])

  useEffect(() => {
    const el = itemRefs.current.get(activeIndex)
    if (el) el.scrollIntoView({ block: 'nearest' })
  }, [activeIndex])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault()
          setActiveIndex(prev => (prev + 1) % Math.max(filtered.length, 1))
          break
        case 'ArrowUp':
          e.preventDefault()
          setActiveIndex(prev => (prev - 1 + filtered.length) % Math.max(filtered.length, 1))
          break
        case 'Enter':
          e.preventDefault()
          if (filtered[activeIndex]) onSelect(filtered[activeIndex])
          break
        case 'Escape':
          e.preventDefault()
          onClose()
          break
      }
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [filtered, activeIndex, onSelect, onClose])

  if (filtered.length === 0) return null

  return (
    <div ref={menuRef} className={styles.menu}>
      {filtered.map((cmd, i) => (
        <button
          key={cmd.name}
          ref={(el) => { if (el) itemRefs.current.set(i, el) }}
          className={`${styles.item} ${i === activeIndex ? styles.itemActive : ''}`}
          onClick={() => onSelect(cmd)}
          onMouseEnter={() => setActiveIndex(i)}
        >
          <span className={styles.itemName}>/{cmd.name}</span>
          <span className={styles.itemDesc}>{cmd.description}</span>
        </button>
      ))}
    </div>
  )
}

export function makeBuiltinCommands(args: {
  createSession: () => Promise<string>
  compactSession: () => Promise<void>
  t: (key: string, vars?: Record<string, string | number>) => string
}): SlashCommand[] {
  const { createSession, compactSession, t } = args
  return [
    { name: 'new', description: t('chat.newSession'), kind: 'builtin', execute: () => { createSession() } },
    { name: 'clear', description: t('chat.clearInput'), kind: 'builtin' },
    { name: 'compact', description: t('slash.compactDesc'), kind: 'builtin', execute: () => { compactSession() } },
  ]
}

export function getSlashQuery(text: string, cursorPos: number): string | null {
  const before = text.slice(0, cursorPos)
  const slashIdx = before.lastIndexOf('/')
  if (slashIdx === -1) return null
  if (slashIdx > 0 && before[slashIdx - 1] !== ' ' && before[slashIdx - 1] !== '\n') return null
  const query = before.slice(slashIdx + 1)
  if (/\s/.test(query)) return null
  return query
}

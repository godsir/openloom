import { useEffect, useRef } from 'react'

export interface SlashCommand {
  name: string
  description: string
  execute: () => void
}

interface Props {
  query: string
  commands: SlashCommand[]
  onSelect: (cmd: SlashCommand) => void
  onClose: () => void
}

export default function SlashCommandMenu({ query, commands, onSelect, onClose }: Props) {
  const filtered = commands.filter((c) =>
    c.name.toLowerCase().includes(query.toLowerCase()),
  )
  const menuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const close = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose()
      }
    }
    document.addEventListener('mousedown', close)
    return () => document.removeEventListener('mousedown', close)
  }, [onClose])

  if (filtered.length === 0) return null

  return (
    <div
      ref={menuRef}
      className="absolute bottom-full left-0 mb-1 w-56 bg-zinc-800 border border-zinc-700 rounded-lg shadow-xl overflow-hidden z-20"
    >
      {filtered.map((cmd) => (
        <button
          key={cmd.name}
          onClick={() => onSelect(cmd)}
          className="w-full text-left px-3 py-2 hover:bg-zinc-700 transition-colors"
        >
          <span className="text-sm text-zinc-200">/{cmd.name}</span>
          <span className="ml-2 text-xs text-zinc-500">{cmd.description}</span>
        </button>
      ))}
    </div>
  )
}

// Built-in slash commands
export function getBuiltinCommands(
  createSession: () => void,
  clearInput: () => void,
): SlashCommand[] {
  return [
    {
      name: 'new',
      description: '新建会话',
      execute: () => createSession(),
    },
    {
      name: 'clear',
      description: '清空输入',
      execute: () => clearInput(),
    },
  ]
}

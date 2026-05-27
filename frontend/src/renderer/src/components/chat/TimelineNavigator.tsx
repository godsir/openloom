import { useMemo } from 'react'
import type { Message } from '../../stores/chat'

interface Anchor { id: string; index: number; label: string }

export function buildTimelineAnchors(messages: Message[]): Anchor[] {
  const anchors: Anchor[] = []
  let turnCount = 0
  for (let i = 0; i < messages.length; i++) {
    if (messages[i].role === 'user') {
      turnCount++
      anchors.push({ id: messages[i].id, index: i, label: `#${turnCount}` })
    }
  }
  return anchors
}

interface TimelineNavigatorProps {
  messages: Message[]
  onScrollTo: (index: number) => void
}

export default function TimelineNavigator({ messages, onScrollTo }: TimelineNavigatorProps) {
  const anchors = useMemo(() => buildTimelineAnchors(messages), [messages])
  if (anchors.length <= 1) return null

  return (
    <div className="absolute right-1 top-0 bottom-0 w-8 flex flex-col items-center py-2 opacity-30 hover:opacity-100 transition-opacity-fast z-10">
      {anchors.map((a) => (
        <button
          key={a.id}
          onClick={() => onScrollTo(a.index)}
          className="text-[9px] font-mono text-[var(--text-muted)] hover:text-[var(--text-light)] py-0.5 leading-none transition-colors-fast"
          title={`跳转到第 ${a.label} 轮`}
        >
          {a.label}
        </button>
      ))}
    </div>
  )
}

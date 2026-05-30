export interface TimelineAnchor {
  messageId: string
  label: string
  markerWidthEm: number
}

/** Build timeline anchors from chat messages — one per user turn. */
export function buildTimelineAnchors(
  messages: Array<{ id: string; role: string; blocks?: Array<{ type: string; source?: string; [k: string]: unknown }> }>,
): TimelineAnchor[] {
  const anchors: TimelineAnchor[] = []
  for (const msg of messages) {
    if (msg.role !== 'user') continue
    const text = msg.blocks?.find(b => b.type === 'text')?.source as string || ''
    const label = formatTimelineLabel(text)
    anchors.push({
      messageId: msg.id,
      label,
      markerWidthEm: markerWidth(text.length),
    })
  }
  return anchors
}

function formatTimelineLabel(text: string, maxChars = 10): string {
  const chars = Array.from(text.trim())
  if (chars.length === 0) return '...'
  if (chars.length <= maxChars) return chars.join('')
  return chars.slice(0, maxChars).join('') + '...'
}

function markerWidth(textLen: number): number {
  if (textLen < 10) return 0.4
  if (textLen > 200) return 0.8
  return 0.4 + (Math.log10(textLen / 10) / Math.log10(20)) * 0.4
}

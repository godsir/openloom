// Screenshot pipeline — captures chat messages as rendered HTML.
// Full implementation will be wired when screenshot feature is enabled.

interface ScreenshotMessage {
  role: 'user' | 'assistant'
  text: string
}

export interface ScreenshotPayload {
  messages: ScreenshotMessage[]
  theme: string
}

export function extractScreenshotMessages(
  messages: { role: string; blocks: { type: string; html?: string; source?: string }[] }[],
): ScreenshotPayload {
  const items: ScreenshotMessage[] = []
  for (const msg of messages) {
    const textBlocks = msg.blocks.filter((b) => b.type === 'text')
    const text = textBlocks.map((b) => b.source || b.html || '').join('\n')
    if (text) {
      items.push({ role: msg.role as 'user' | 'assistant', text })
    }
  }
  return { messages: items, theme: 'dark' }
}

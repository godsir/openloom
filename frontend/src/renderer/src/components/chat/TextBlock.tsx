import type { ContentBlock } from '../../stores/chat'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'

export default function TextBlock({ block }: { block: ContentBlock }) {
  const html = (block.html as string) || ''
  const source = (block.source as string) || ''
  const displayHtml = html || sanitizeHtml(renderMarkdown(source))

  return (
    <div
      className="prose-chat max-w-none text-zinc-300 text-sm leading-relaxed"
      dangerouslySetInnerHTML={{ __html: displayHtml }}
    />
  )
}

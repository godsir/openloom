import type { Message } from '../../stores/chat'
import MessageFooterActions from './MessageFooterActions'

export default function UserMessage({ message }: { message: Message }) {
  const textBlock = message.blocks.find((b) => b.type === 'text')

  return (
    <div className="flex justify-end animate-fade-in">
      <div className="max-w-[75%] group">
        <div className="bg-[rgba(0,227,199,0.1)] backdrop-blur-[12px] border border-[rgba(0,227,199,0.15)] rounded-[var(--r-lg)_4px_var(--r-lg)_var(--r-lg)] px-3.5 py-2.5">
          {textBlock ? (
            <div
              className="text-[13.5px] text-[var(--text)] leading-[1.6]"
              dangerouslySetInnerHTML={{
                __html: (textBlock.html as string) || escapeHtml(textBlock.source as string) || '',
              }}
            />
          ) : (
            <span className="text-[var(--text-muted)] italic text-[13px]">(空)</span>
          )}
        </div>
        <MessageFooterActions messageId={message.id} role="user" timestamp={message.timestamp} />
      </div>
    </div>
  )
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

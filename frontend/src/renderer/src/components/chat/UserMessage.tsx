import type { Message } from '../../stores/chat'
import MessageFooterActions from './MessageFooterActions'

export default function UserMessage({ message }: { message: Message }) {
  const textBlock = message.blocks.find((b) => b.type === 'text')

  return (
    <div className="mb-4 max-w-3xl ml-auto group">
      <div className="text-xs text-zinc-600 mb-1 mr-1 text-right">你</div>
      <div className="bg-zinc-800 rounded-lg px-4 py-2 text-sm text-zinc-200">
        {textBlock ? (
          <div
            dangerouslySetInnerHTML={{
              __html: (textBlock.html as string) || escapeHtml(textBlock.source as string) || '',
            }}
          />
        ) : (
          <span className="text-zinc-500">(空消息)</span>
        )}
      </div>
      <MessageFooterActions
        messageId={message.id}
        role="user"
        timestamp={message.timestamp}
      />
    </div>
  )
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

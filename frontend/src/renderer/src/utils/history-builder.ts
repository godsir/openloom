import type { Message } from '../stores/chat'

// Convert server-side message format to UI Message model.
// The backend returns messages from session.messages as an array of { role, content }.
export interface ServerMessage {
  role: 'user' | 'assistant'
  content: string
  timestamp?: string
}

export function buildMessages(serverMessages: ServerMessage[]): Message[] {
  return serverMessages.map((sm, i) => ({
    id: `hist-${i}-${Date.now()}`,
    role: sm.role,
    blocks: [
      {
        type: 'text',
        html: escapeAndLinkify(sm.content),
        source: sm.content,
      },
    ],
    timestamp: sm.timestamp || new Date().toISOString(),
  }))
}

function escapeAndLinkify(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/\n/g, '<br>')
}

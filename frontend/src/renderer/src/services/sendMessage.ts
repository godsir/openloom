import { loomRpc } from './jsonrpc'
import { useStore } from '../stores'
import { streamBufferManager } from './stream-buffer'
import type { AttachedFile } from '../stores/input'

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

export interface SendMessageOptions {
  sessionId: string
  content: string
  attachedFiles?: AttachedFile[]
  skills?: string[]
}

/**
 * Send a message to the backend and manage the streaming state.
 * This is extracted from InputArea so it can be reused by resend/retry buttons.
 */
export async function sendMessage({ sessionId, content, attachedFiles = [], skills }: SendMessageOptions): Promise<void> {
  const sid = sessionId
  useStore.getState().ensureSession(sid)

  const msgId = crypto.randomUUID()
  const blocks: any[] = []
  if (content) {
    blocks.push({ type: 'text', html: escapeHtml(content), source: content })
  }
  for (const f of attachedFiles) {
    if (f.mimeType.startsWith('image/')) {
      blocks.push({ type: 'image', path: f.path, name: f.name, mimeType: f.mimeType, thumbnail: f.thumbnail })
    } else {
      blocks.push({ type: 'file', path: f.path, name: f.name, mimeType: f.mimeType, size: f.size })
    }
  }

  useStore.getState().appendMessage(sid, {
    id: msgId, role: 'user',
    blocks,
    timestamp: new Date().toISOString(),
  })

  const aiMsgId = crypto.randomUUID()
  useStore.getState().addStreamingSession(sid)
  useStore.getState().appendMessage(sid, {
    id: aiMsgId, role: 'assistant',
    blocks: [],
    timestamp: new Date().toISOString(),
  })
  streamBufferManager.startStream(sid, aiMsgId)

  const safetyTimer = setTimeout(() => {
    const buf = streamBufferManager.snapshot(sid)
    if (buf && buf.messageId === aiMsgId) {
      useStore.getState().removeStreamingSession(sid)
      streamBufferManager.clear(sid)
    }
  }, 180_000)

  // Pet: user sends message → run right excitedly → wait for response
  import('./pet-sync').then(m => m.sendPetState('runRight'))
  setTimeout(() => import('./pet-sync').then(m => m.sendPetState('wait')), 400)

  try {
    const { currentModel, thinkingLevel } = useStore.getState()
    await loomRpc('chat.send', {
      session_id: sid,
      content,
      model: currentModel || undefined,
      thinking_level: thinkingLevel || 'off',
      skills: skills && skills.length > 0 ? skills : undefined,
      attached_files: attachedFiles.map(f => ({
        path: f.path,
        name: f.name,
        size: f.size,
        mime_type: f.mimeType,
        thumbnail: f.thumbnail,
      })),
    })
  }
  catch (e: any) {
    useStore.getState().setInlineError(sid, e.message || '发送失败')
  }
  finally {
    clearTimeout(safetyTimer)
    const buf = streamBufferManager.snapshot(sid)
    if (buf && buf.messageId === aiMsgId) {
      useStore.getState().removeStreamingSession(sid)
      streamBufferManager.clear(sid)
    }
  }
}

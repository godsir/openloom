import { loomRpc } from './jsonrpc'
import { useStore } from '../stores'
import { streamBufferManager } from './stream-buffer'
import type { AttachedFile } from '../stores/input'
import type { QuotedSelection } from '../stores/selectionContext'
import { t } from '../i18n'

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

export interface SendMessageOptions {
  sessionId: string
  content: string
  attachedFiles?: AttachedFile[]
  skills?: string[]
  /** For retry: skip appending a new user message (content is already in history). */
  skipUserMessage?: boolean
  /** Quoted selections captured via Ctrl+Shift+I inline editor. */
  quotedSelections?: QuotedSelection[]
  /** Override the global permission mode (e.g. write mode always uses 'operate'). */
  permissionMode?: string
}

/**
 * Send a message to the backend and manage the streaming state.
 * This is extracted from InputArea so it can be reused by resend/retry buttons.
 */
export async function sendMessage({ sessionId, content, attachedFiles = [], skills, skipUserMessage, quotedSelections = [], permissionMode }: SendMessageOptions): Promise<void> {
  const sid = sessionId
  useStore.getState().ensureSession(sid)

  const msgId = crypto.randomUUID()
  const blocks: any[] = []

  // 1. Quoted selections first (context before instruction)
  for (const qs of quotedSelections) {
    blocks.push({
      type: 'quoted_selection',
      text: qs.text,
      filePath: qs.filePath,
      startLine: qs.startLine,
      endLine: qs.endLine,
    })
  }

  // 2. Text content
  if (content) {
    blocks.push({ type: 'text', html: escapeHtml(content).replace(/\n/g, '<br>'), source: content })
  }

  // 3. Attached files/images
  for (const f of attachedFiles) {
    if (f.mimeType.startsWith('image/')) {
      blocks.push({ type: 'image', path: f.path, name: f.name, mimeType: f.mimeType, thumbnail: f.thumbnail })
    } else {
      blocks.push({ type: 'file', path: f.path, name: f.name, mimeType: f.mimeType, size: f.size })
    }
  }

  if (!skipUserMessage) {
    useStore.getState().appendMessage(sid, {
      id: msgId, role: 'user',
      blocks,
      timestamp: new Date().toISOString(),
    })
  }

  const aiMsgId = crypto.randomUUID()
  useStore.getState().addStreamingSession(sid)
  useStore.getState().appendMessage(sid, {
    id: aiMsgId, role: 'assistant',
    blocks: [],
    timestamp: new Date().toISOString(),
  })
  streamBufferManager.startStream(sid, aiMsgId, skills)

  const safetyTimer = setTimeout(() => {
    const buf = streamBufferManager.snapshot(sid)
    if (buf && buf.messageId === aiMsgId) {
      streamBufferManager.handleStreamEnd(sid)
    }
  }, 300_000) // 5 min safety timeout for long agent loops

  // Pet: user sends message → run right excitedly → wait for response
  import('./pet-sync').then(m => m.sendPetState('runRight'))
  setTimeout(() => import('./pet-sync').then(m => m.sendPetState('wait')), 400)

  try {
    const { currentModel, thinkingLevel } = useStore.getState()
    // Validate skill names before sending
    let validSkills = skills
    if (skills && skills.length > 0) {
      try {
        const res = await loomRpc<{ skills: { name: string }[] }>('skills.list')
        const known = new Set((res.skills ?? []).map(s => s.name))
        validSkills = skills.filter(s => known.has(s))
        const filtered = skills.filter(s => !known.has(s))
        if (filtered.length > 0) {
          useStore.getState().addToast({
            type: 'warning',
            message: `Unknown skill${filtered.length > 1 ? 's' : ''} ignored: ${filtered.join(', ')}`,
          })
        }
      } catch {
        // If skills.list fails, pass skills through as-is (backend will validate)
      }
    }

    await loomRpc('chat.send', {
      session_id: sid,
      content,
      model: currentModel || undefined,
      thinking_level: thinkingLevel || 'off',
      skills: validSkills && validSkills.length > 0 ? validSkills : undefined,
      skip_user_message: skipUserMessage || undefined,
      permission_mode: permissionMode || useStore.getState().permissionMode,
      attached_files: attachedFiles.map(f => ({
        path: f.path,
        name: f.name,
        size: f.size,
        mime_type: f.mimeType,
        thumbnail: f.thumbnail,
        content: f.content,
      })),
      quoted_selections: quotedSelections.map(q => ({
        text: q.text,
        file_path: q.filePath || '',
        start_line: q.startLine || 0,
        end_line: q.endLine || 0,
      })),
    })
  }
  catch (e: any) {
    useStore.getState().setInlineError(sid, e.message || t('sessions.sendFailed'))
  }
  finally {
    clearTimeout(safetyTimer)
    const buf = streamBufferManager.snapshot(sid)
    if (buf && buf.messageId === aiMsgId) {
      streamBufferManager.handleStreamEnd(sid)
    }
    // Update current session's modified time in sidebar
    const now = new Date().toISOString()
    useStore.getState().setSessions(
      useStore.getState().sessions.map(s =>
        s.path === sid ? { ...s, modified: now } : s
      )
    )
  }
}
